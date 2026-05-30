# Compression Handling Specification

**Version:** 1.0
**Status:** Design Phase
**Last Updated:** 2026-01-07

**Audience:** Developers, system architects, performance engineers

---

## Overview

One of the **critical challenges** identified in the original BDP (Binary Differential Patching) analysis is that **non-deterministic compression breaks chunk-based deduplication**.

### The Problem

**Scenario: npm packages**

1. Developer builds `mypackage-1.0.0.tgz` (gzip-compressed tarball)
2. Developer makes tiny change (one character in README)
3. Developer builds `mypackage-1.0.1.tgz`

**Expected:** Diff should be ~few bytes (one character change)

**Actual:** Diff is ~entire package size because:
- gzip compression is **non-deterministic** (compression timestamps, slight algorithm variations)
- Entire compressed stream changes even if content is 99.9% identical
- Chunk-based deduplication fails (chunks don't match)

**Impact:**
- npm packages: 1MB → 1MB diff (instead of ~1KB)
- Docker images: 500MB → 500MB diff (instead of ~10MB for layer changes)
- Compressed executables: Full download instead of small diff

---

## Table of Contents

- [Problem Analysis](#problem-analysis)
- [VBDP Solution Strategy](#vbdp-solution-strategy)
- [Pre-Decompression Approach](#pre-decompression-approach)
- [Content-Defined Chunking](#content-defined-chunking)
- [Compression Format Support](#compression-format-support)
- [npm Package Handling](#npm-package-handling)
- [Docker Image Handling](#docker-image-handling)
- [Implementation](#implementation)
- [Performance Considerations](#performance-considerations)
- [Trade-offs](#trade-offs)

---

## Problem Analysis

### Why Compression Breaks Diffing

**Compressed data characteristics:**
1. **High entropy**: Looks random, no repeating patterns
2. **Cascade effect**: One byte change → entire block changes
3. **Non-deterministic**: Same content → different compressed output
4. **No alignment**: Chunk boundaries don't align across versions

**Example:**

```
# Version 1.0.0
Original:   "Hello World\nThis is a test\n"
Compressed: 0x1f8b0800... (gzip)

# Version 1.0.1 (changed "test" → "demo")
Original:   "Hello World\nThis is a demo\n"
Compressed: 0x1f8b0801... (completely different compressed stream)
```

**Binary diff result:**
- **Without decompression**: ~100% of file size (entire compressed stream different)
- **With decompression**: ~0.01% of file size (4-byte change in decompressed data)

### Affected Formats

**Archives:**
- `.tgz` / `.tar.gz` (npm packages, source distributions)
- `.zip` (JAR files, ZIP archives)
- `.tar.bz2`, `.tar.xz`
- `.7z`, `.rar`

**Container images:**
- Docker images (layer tarballs)
- OCI images

**Compressed executables:**
- UPX-compressed binaries
- Installers (`.exe` with embedded compressed data)

**Game assets:**
- Unity AssetBundles (compressed)
- Unreal Engine PAK files

---

## VBDP Solution Strategy

VBDP uses a **multi-strategy approach**:

### Strategy 1: Pre-Decompression

**For archives and known formats:**
1. **Detect compression** format (gzip, zstd, bzip2, etc.)
2. **Decompress** both versions to temporary files
3. **Compute diff** on decompressed data
4. **Compress diff** for transmission
5. **Client decompresses diff**, applies to decompressed original, **recompresses** result

**Pros:**
- Maximum deduplication (diff on actual content)
- Works with any compression format

**Cons:**
- Higher CPU usage (decompress, diff, recompress)
- Storage overhead (store decompressed versions temporarily)
- Client must recompress (may not match original)

---

### Strategy 2: Content-Defined Chunking (FastCDC)

**For large binaries with some uncompressed sections:**
1. **Use FastCDC** (Fast Content-Defined Chunking) to find natural boundaries
2. **Chunk deduplication** even if some chunks are compressed
3. **Hybrid approach**: Decompress compressible chunks, diff uncompressed chunks normally

**Pros:**
- Works on mixed compressed/uncompressed data
- Better than fixed-size chunking

**Cons:**
- Doesn't solve fully compressed archives
- More complex implementation

---

### Strategy 3: Smart Format Detection

**Detect and handle specific formats:**
- **npm packages**: Decompress `.tgz`, diff tarball, recompress
- **Docker images**: Decompress layers, diff, recompress
- **ZIP files**: Extract, diff contents, repack

**Pros:**
- Optimal for each format
- Can preserve metadata (timestamps, permissions)

**Cons:**
- Format-specific code for each type
- Maintenance burden

---

### VBDP Recommended Strategy

**Default behavior:**

1. **Detect if file is compressed** (magic bytes, extension)
2. **If compressed archive** (tar.gz, zip, etc.):
   - Use **Pre-Decompression** approach
3. **If compressed executable** or **mixed compression**:
   - Use **Content-Defined Chunking** (FastCDC)
4. **If uncompressed**:
   - Use standard binary diffing (bsdiff, courgette)

**Publisher can override** via configuration:
```toml
[binary.compression]
strategy = "pre-decompress"  # or "fastcdc" or "standard"
format = "tar.gz"  # or "zip", "docker", "auto"
```

---

## Pre-Decompression Approach

### Algorithm

**Publisher side (diff computation):**

```
1. Detect compression format (gzip, zstd, bzip2, xz, zip)
2. Decompress version A → temp_A
3. Decompress version B → temp_B
4. Compute diff: bsdiff(temp_A, temp_B) → diff_raw
5. Compress diff: zstd(diff_raw) → diff_compressed
6. Upload diff_compressed to server
7. Delete temp_A, temp_B, diff_raw
```

**Client side (diff application):**

```
1. Download diff_compressed
2. Decompress diff_compressed → diff_raw
3. Decompress current version → temp_current
4. Apply diff: bspatch(temp_current, diff_raw) → temp_new
5. Recompress temp_new → new_version (if archive format)
6. Verify signature and hash
7. Replace current version with new_version
8. Delete temp files
```

### Compression Format Detection

**Magic bytes:**

| Format | Magic Bytes | Extension |
|--------|-------------|-----------|
| gzip | `1f 8b` | `.gz`, `.tgz` |
| bzip2 | `42 5a 68` | `.bz2`, `.tar.bz2` |
| xz | `fd 37 7a 58 5a 00` | `.xz`, `.tar.xz` |
| zstd | `28 b5 2f fd` | `.zst` |
| ZIP | `50 4b 03 04` | `.zip`, `.jar` |
| 7z | `37 7a bc af 27 1c` | `.7z` |

**Detection code (Rust):**

```rust
fn detect_compression(file_path: &Path) -> Result<CompressionFormat> {
    let mut file = File::open(file_path)?;
    let mut magic = [0u8; 6];
    file.read_exact(&mut magic)?;

    match &magic[..] {
        [0x1f, 0x8b, ..] => Ok(CompressionFormat::Gzip),
        [0x42, 0x5a, 0x68, ..] => Ok(CompressionFormat::Bzip2),
        [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00] => Ok(CompressionFormat::Xz),
        [0x28, 0xb5, 0x2f, 0xfd, ..] => Ok(CompressionFormat::Zstd),
        [0x50, 0x4b, 0x03, 0x04, ..] => Ok(CompressionFormat::Zip),
        [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c] => Ok(CompressionFormat::SevenZip),
        _ => Ok(CompressionFormat::None),
    }
}
```

### Decompression

**Using external tools:**

```bash
# gzip
gunzip -c version.tar.gz > version.tar

# bzip2
bunzip2 -c version.tar.bz2 > version.tar

# xz
unxz -c version.tar.xz > version.tar

# zstd
zstd -d version.tar.zst -o version.tar

# zip
unzip -q version.zip -d ./temp/
```

**Using Rust libraries:**

```rust
use flate2::read::GzDecoder;
use std::io::Read;

fn decompress_gzip(input_path: &Path, output_path: &Path) -> Result<()> {
    let input = File::open(input_path)?;
    let mut decoder = GzDecoder::new(input);
    let mut output = File::create(output_path)?;
    std::io::copy(&mut decoder, &mut output)?;
    Ok(())
}
```

### Storage Implications

**Temporary storage needed:**

| Binary Size | Decompressed Size | Diff Size | Total Storage |
|-------------|-------------------|-----------|---------------|
| 1MB (tar.gz) | 10MB | 100KB | ~20MB temp |
| 10MB (tar.gz) | 100MB | 500KB | ~200MB temp |
| 100MB (zip) | 500MB | 5MB | ~1GB temp |

**Mitigation:**
- Use streaming decompression (don't store full decompressed file)
- Chunk-based processing (decompress, diff, delete chunks)
- Configurable temp directory (use fast SSD)

---

## Content-Defined Chunking

### FastCDC Algorithm

**FastCDC** (Fast Content-Defined Chunking) creates variable-size chunks based on content, not fixed boundaries.

**Key idea:** Find chunk boundaries based on content hash (rolling hash)

**Advantages:**
- Chunks remain aligned across versions (even if data shifted)
- Efficient deduplication
- Fast (optimized for modern CPUs)

### Algorithm

```
1. Sliding window of N bytes
2. Compute rolling hash for window
3. If hash & MASK == PATTERN:
   - Chunk boundary found
   - Emit chunk
   - Reset window
4. Move window forward
5. Repeat
```

**Parameters:**
- **Average chunk size**: 64KB (configurable: 8KB-1MB)
- **Min chunk size**: 16KB (prevent too-small chunks)
- **Max chunk size**: 256KB (prevent too-large chunks)

### Implementation (Rust)

```rust
use fastcdc::FastCDC;

fn chunk_file(file_path: &Path, avg_chunk_size: usize) -> Vec<Chunk> {
    let data = std::fs::read(file_path)?;
    let chunker = FastCDC::new(&data, avg_chunk_size);

    let mut chunks = Vec::new();
    for chunk in chunker {
        chunks.push(Chunk {
            offset: chunk.offset,
            length: chunk.length,
            hash: blake3::hash(&data[chunk.offset..chunk.offset+chunk.length]),
        });
    }
    chunks
}
```

### Deduplication

**Version A:**
```
Chunk A1 (hash: abc123, size: 64KB)
Chunk A2 (hash: def456, size: 64KB)
Chunk A3 (hash: ghi789, size: 64KB)
```

**Version B** (modified chunk A2):
```
Chunk B1 (hash: abc123, size: 64KB)  ← SAME as A1
Chunk B2 (hash: xyz999, size: 64KB)  ← DIFFERENT
Chunk B3 (hash: ghi789, size: 64KB)  ← SAME as A3
```

**Diff result:**
- Store only B2 (64KB)
- Refer to A1 and A3 (metadata only)
- **Total diff size**: ~64KB instead of ~192KB

---

## Compression Format Support

### Supported Formats (v1.0)

| Format | Read | Write | Priority |
|--------|------|-------|----------|
| **gzip** | ✅ | ✅ | High (npm, tarballs) |
| **zstd** | ✅ | ✅ | High (modern, fast) |
| **bzip2** | ✅ | ✅ | Medium (legacy) |
| **xz** | ✅ | ✅ | Medium (high compression) |
| **ZIP** | ✅ | ⚠️ | High (JAR, archives) |
| **7z** | ⚠️ | ❌ | Low (complex) |
| **LZ4** | ✅ | ✅ | Medium (fast) |
| **Brotli** | ✅ | ✅ | Medium (web) |

**Legend:**
- ✅ Full support
- ⚠️ Read-only or limited support
- ❌ Not supported

### Format-Specific Handling

**gzip (.gz, .tgz):**
- Decompress with `flate2`
- Recompress with `flate2` (level 6 default)
- **Issue**: Recompressed file may differ slightly (timestamps, metadata)
- **Solution**: Strip gzip headers, use deterministic compression

**zstd (.zst):**
- Decompress with `zstd` library
- Recompress with `zstd` (level 3 default, fast)
- **Advantage**: Faster than gzip, better compression

**ZIP (.zip, .jar):**
- Extract with `zip` crate
- Diff each file individually
- Repack with `zip` crate
- **Challenge**: Preserve metadata (timestamps, permissions, comments)
- **Solution**: Store metadata separately, restore on repack

**7z (.7z):**
- **Read-only** using `7z` command-line tool
- Cannot reliably recompress (complex format)
- **Workaround**: Publisher converts to `.tar.zst` before registering

---

## npm Package Handling

### Problem

npm packages are `.tgz` files (gzip-compressed tarballs):
```
mypackage-1.0.0.tgz  (1.2MB)
  ├── package.json
  ├── index.js
  ├── lib/...
  └── README.md
```

Tiny change in `README.md` → entire `.tgz` changes due to gzip non-determinism.

### VBDP Solution

**Publisher workflow:**

```bash
# Publish version 1.0.0
npm pack  # Creates mypackage-1.0.0.tgz
vbdp-register --version 1.0.0 --binary mypackage.tgz

# Publish version 1.0.1 (tiny change)
npm pack  # Creates mypackage-1.0.1.tgz
vbdp-register --version 1.0.1 --binary mypackage.tgz
```

**VBDP processing:**

```
1. Detect: "mypackage-1.0.0.tgz" is gzip-compressed tarball
2. Decompress both versions:
   - mypackage-1.0.0.tgz → mypackage-1.0.0.tar (10MB)
   - mypackage-1.0.1.tgz → mypackage-1.0.1.tar (10MB)
3. Compute diff on .tar files:
   - bsdiff(1.0.0.tar, 1.0.1.tar) → diff.patch (5KB)
4. Compress diff:
   - zstd(diff.patch) → diff.patch.zst (2KB)
5. Store diff.patch.zst (2KB)
```

**Client workflow:**

```
1. Client has: mypackage-1.0.0.tgz (1.2MB)
2. Download: diff.patch.zst (2KB)
3. Decompress:
   - mypackage-1.0.0.tgz → mypackage-1.0.0.tar (10MB)
   - diff.patch.zst → diff.patch (5KB)
4. Apply patch:
   - bspatch(1.0.0.tar, diff.patch) → mypackage-1.0.1.tar (10MB)
5. Recompress:
   - gzip(mypackage-1.0.1.tar) → mypackage-1.0.1.tgz (1.2MB)
6. Verify hash and signature
7. Replace old version
```

**Bandwidth savings:**
- **Without VBDP**: 1.2MB (full download)
- **With VBDP**: 2KB (diff)
- **Savings**: 99.8%

### Deterministic Recompression

**Problem:** Recompressed `.tgz` may differ from original (timestamps in gzip header)

**Solution:**

```rust
use flate2::write::GzEncoder;
use flate2::Compression;

fn compress_deterministic(input: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(6));
    encoder.set_mtime(0);  // Strip timestamp
    encoder.write_all(input)?;
    encoder.finish()?
}
```

**Alternative:** Publisher provides original compression settings (level, timestamp) as metadata

---

## Docker Image Handling

### Problem

Docker images are collections of compressed tarballs (layers):

```
image.tar
├── layer1.tar.gz  (base OS layer, 100MB)
├── layer2.tar.gz  (dependencies, 50MB)
├── layer3.tar.gz  (application code, 10MB)
└── manifest.json
```

Tiny code change → `layer3.tar.gz` entirely different due to gzip non-determinism.

### VBDP Solution

**Layer-aware diffing:**

```
1. Extract Docker image layers:
   docker save myapp:1.0.0 > image-1.0.0.tar
   docker save myapp:1.0.1 > image-1.0.1.tar

2. Identify changed layers (by comparing manifests):
   layer1.tar.gz: UNCHANGED (same hash)
   layer2.tar.gz: UNCHANGED (same hash)
   layer3.tar.gz: CHANGED (different hash)

3. Diff only changed layers:
   - Decompress layer3-1.0.0.tar.gz → layer3-1.0.0.tar
   - Decompress layer3-1.0.1.tar.gz → layer3-1.0.1.tar
   - Compute diff: bsdiff(layer3-1.0.0.tar, layer3-1.0.1.tar) → diff.patch
   - Compress: zstd(diff.patch) → diff.patch.zst

4. Store diff.patch.zst (tiny, maybe 100KB)
```

**Client workflow:**

```
1. Client has: myapp:1.0.0 (full image, 160MB)
2. Download: diff.patch.zst (100KB)
3. Apply diff to layer3 only
4. Reconstruct image with updated layer3
5. Load into Docker: docker load < image-1.0.1.tar
```

**Bandwidth savings:**
- **Without VBDP**: 160MB (full image)
- **With VBDP**: 100KB (diff for changed layer)
- **Savings**: 99.9%

### Integration with Docker Registry

**Future enhancement:** VBDP server acts as Docker registry proxy

```bash
# Configure Docker to use VBDP proxy
export DOCKER_REGISTRY=vbdp-proxy.example.com

# Pull image (VBDP applies diff transparently)
docker pull myapp:1.0.1  # Downloads only diff, not full image
```

---

## Implementation

### Publisher Toolkit

**Compression detection and handling:**

```rust
pub struct CompressionHandler {
    strategy: CompressionStrategy,
}

impl CompressionHandler {
    pub fn handle(&self, from_path: &Path, to_path: &Path) -> Result<Diff> {
        // Detect compression
        let format = detect_compression(from_path)?;

        match self.strategy {
            CompressionStrategy::PreDecompress => {
                self.pre_decompress_diff(from_path, to_path, format)
            }
            CompressionStrategy::FastCDC => {
                self.fastcdc_diff(from_path, to_path)
            }
            CompressionStrategy::Standard => {
                // Standard binary diff (no special handling)
                binary_diff(from_path, to_path)
            }
        }
    }

    fn pre_decompress_diff(&self, from: &Path, to: &Path, format: CompressionFormat) -> Result<Diff> {
        // 1. Decompress both files
        let from_decompressed = temp_file()?;
        let to_decompressed = temp_file()?;

        decompress(from, &from_decompressed, format)?;
        decompress(to, &to_decompressed, format)?;

        // 2. Compute diff on decompressed data
        let diff_raw = binary_diff(&from_decompressed, &to_decompressed)?;

        // 3. Compress the diff
        let diff_compressed = compress(&diff_raw, CompressionFormat::Zstd)?;

        // 4. Cleanup temp files
        cleanup(&[from_decompressed, to_decompressed])?;

        Ok(Diff {
            data: diff_compressed,
            algorithm: DiffAlgorithm::BsdiffPreDecompressed,
            source_format: format,
        })
    }
}
```

### Client Patcher

**Diff application with recompression:**

```rust
pub fn apply_compressed_diff(
    current_path: &Path,
    diff: &Diff,
    output_path: &Path,
) -> Result<()> {
    // 1. Decompress diff
    let diff_raw = decompress_diff(&diff.data, diff.compression)?;

    // 2. Decompress current version
    let current_decompressed = temp_file()?;
    decompress(current_path, &current_decompressed, diff.source_format)?;

    // 3. Apply patch
    let new_decompressed = temp_file()?;
    apply_patch(&current_decompressed, &diff_raw, &new_decompressed)?;

    // 4. Recompress (if needed)
    if diff.source_format != CompressionFormat::None {
        compress_deterministic(&new_decompressed, output_path, diff.source_format)?;
    } else {
        std::fs::copy(&new_decompressed, output_path)?;
    }

    // 5. Cleanup
    cleanup(&[current_decompressed, new_decompressed])?;

    Ok(())
}
```

---

## Performance Considerations

### CPU Usage

**Decompression/recompression overhead:**

| Operation | Time (100MB file) | CPU |
|-----------|-------------------|-----|
| Decompress gzip | ~2s | Single-core |
| Decompress zstd | ~0.5s | Single-core |
| Compress gzip (level 6) | ~10s | Single-core |
| Compress zstd (level 3) | ~1s | Multi-core |
| Binary diff (bsdiff) | ~30s | Single-core |

**Total time for pre-decompress strategy:**
```
Decompress A: 2s
Decompress B: 2s
Diff: 30s
Compress diff: 1s
Total: ~35s (for 100MB compressed → 1GB decompressed)
```

**Optimization:**
- Use faster compression (zstd level 1-3)
- Parallel processing (chunk-based)
- Dedicated diff server with high CPU

### Storage Usage

**Temporary storage:**

| Compressed Size | Decompressed Size | Temp Storage |
|-----------------|-------------------|--------------|
| 1MB | 10MB | 20MB (2x decompress) |
| 10MB | 100MB | 200MB |
| 100MB | 1GB | 2GB |

**Mitigation:**
- Stream processing (don't store full decompressed files)
- Cleanup temp files immediately
- Use fast local SSD for temp storage

### Network Bandwidth

**Diff sizes (example: 100MB compressed tarball, 1KB actual change):**

| Strategy | Diff Size | Bandwidth Savings |
|----------|-----------|-------------------|
| Standard (no decompress) | ~100MB | 0% |
| Pre-decompress | ~10KB | 99.99% |
| FastCDC | ~50KB | 99.95% |

---

## Trade-offs

### Pre-Decompression

**Pros:**
- Maximum bandwidth savings
- Works with any compression format

**Cons:**
- High CPU usage (decompress, diff, recompress)
- High temporary storage
- Client must recompress (format compatibility)

**Best for:**
- npm packages, Docker images, source tarballs
- Slow networks, fast CPUs
- Large files with small changes

---

### Content-Defined Chunking (FastCDC)

**Pros:**
- Works on mixed compressed/uncompressed data
- Lower CPU than full decompression
- Moderate bandwidth savings

**Cons:**
- Doesn't solve fully compressed archives
- More complex implementation
- Chunk storage overhead

**Best for:**
- Large binaries with some compressed sections
- Executables with embedded compressed data
- When decompression not possible

---

### Standard Binary Diff

**Pros:**
- Simple, fast
- No temporary storage
- No recompression needed

**Cons:**
- Poor results on compressed data
- Large diffs for small changes in compressed files

**Best for:**
- Uncompressed binaries
- Executables without compression
- When compression format unknown

---

## Summary

VBDP solves the **compression problem** through:

1. **Pre-decompression strategy** for archives (npm, Docker)
   - Decompress → diff → recompress
   - Maximum bandwidth savings (~99.9%)

2. **Content-defined chunking** (FastCDC) for mixed compression
   - Chunk-level deduplication
   - Works on compressed executables

3. **Format-specific handling** for popular formats
   - npm: Handle `.tgz` intelligently
   - Docker: Layer-aware diffing
   - ZIP: File-level diffing

4. **Configurable strategy** per binary
   - Publisher chooses approach
   - Server auto-detects format

**Result:** Efficient updates for compressed artifacts that previously required full re-downloads.

---

**Next Steps:**
1. Implement compression detection in publisher toolkit
2. Add decompression/recompression logic
3. Benchmark performance on real npm packages and Docker images
4. Document best practices for publishers
5. Add compression strategy to database schema

---

**References:**
- [FastCDC Paper](https://www.usenix.org/system/files/conference/atc16/atc16-paper-xia.pdf)
- [gzip RFC 1952](https://datatracker.ietf.org/doc/html/rfc1952)
- [zstd](https://facebook.github.io/zstd/)
- [BDP Analysis (original problem)](https://github.com/magv/bdp/blob/master/README.md)
