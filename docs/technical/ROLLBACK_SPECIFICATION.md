# Rollback Specification

**Version:** 1.0
**Status:** Design Phase - **UNIFIED SPECIFICATION**
**Last Updated:** 2026-01-07

**Audience:** Developers, system architects, client implementers

---

## Overview

This document **unifies** the rollback mechanism specification for Deltaship, resolving contradictions found in earlier documentation.

**Previous confusion:**
- CLIENT_PATCHER.md mentioned: "Restore backup" (backup-based rollback)
- PUBLISHER_TOOLKIT.md mentioned: "Generate reverse diff" (diff-based rollback)
- These appeared contradictory

**Unified approach:**
Deltaship supports **BOTH mechanisms**, used in different scenarios:

1. **Backup-based rollback**: For automatic recovery from failed updates
2. **Reverse diff-based rollback**: For deliberate downgrades (optional)

---

## Table of Contents

- [Rollback Scenarios](#rollback-scenarios)
- [Backup-Based Rollback](#backup-based-rollback)
- [Reverse Diff-Based Rollback](#reverse-diff-based-rollback)
- [Decision Matrix](#decision-matrix)
- [Implementation](#implementation)
- [Configuration](#configuration)
- [Security Considerations](#security-considerations)

---

## Rollback Scenarios

### Scenario 1: Failed Update (Automatic Rollback)

**Situation:** Update from 1.0.0 → 1.1.0 fails during patching

**Causes:**
- Corrupted diff download
- Signature verification failure
- Patch application error
- Insufficient disk space
- Power loss during update

**User expectation:** System automatically restores to working state (1.0.0)

**Solution:** **Backup-based rollback**

**Why:**
- Fast (no network required)
- Always available (backup exists before update)
- Guaranteed to work (exact copy of working version)
- No server dependency

---

### Scenario 2: Deliberate Downgrade

**Situation:** User wants to rollback 1.1.0 → 1.0.0 after successful update

**Causes:**
- New version has regression/bugs
- Compatibility issues with other software
- User preference (old UI preferred)
- Organization policy (rollback deployment)

**User expectation:** Download minimal data to revert to previous version

**Solution:** **Reverse diff-based rollback** (if backup unavailable)

**Why:**
- Backup may be deleted (retention policy)
- Efficient (download only diff, not full binary)
- Works across multiple versions (1.2.0 → 1.0.0)

---

### Scenario 3: Emergency Rollback (Publisher-Initiated)

**Situation:** Publisher discovers critical bug in 1.1.0, wants all clients to revert to 1.0.0

**Causes:**
- Security vulnerability found
- Data corruption bug
- Critical crash

**Publisher action:** Issue signed rollback command

**Solution:** **Either mechanism** (prefer backup if available, else reverse diff)

**Why:**
- Speed critical (backup faster)
- Fallback to reverse diff if backup unavailable
- Signed command ensures authenticity

---

## Backup-Based Rollback

### How It Works

**Before update:**
```
1. Client creates backup of current version
   /usr/bin/myapp (1.0.0) → /var/lib/deltaship/backups/myapp-1.0.0.backup

2. Client downloads and applies diff
   /usr/bin/myapp.new (1.1.0 candidate)

3. Client verifies new version
   - Signature check
   - Hash check
   - Basic functionality test (optional)

4. If successful:
   - Atomically replace: myapp.new → myapp
   - Delete backup (or retain per policy)

5. If failed:
   - ROLLBACK: Restore backup
     /var/lib/deltaship/backups/myapp-1.0.0.backup → /usr/bin/myapp
   - Delete myapp.new
   - Log error
```

### Backup Storage

**Location:**
- Linux: `/var/lib/deltaship/backups/`
- Windows: `%PROGRAMDATA%\Deltaship\backups\`
- macOS: `/Library/Application Support/Deltaship/backups/`

**Naming convention:**
```
{binary_name}-{version_string}-{timestamp}.backup

Examples:
myapp-1.0.0-20270115T123045.backup
game-client-2.3.1-20270115T150000.backup
```

**Retention policy (configurable):**

| Policy | Description | Default |
|--------|-------------|---------|
| **keep_last_n** | Keep last N successful versions | 3 |
| **keep_days** | Keep backups for N days | 30 |
| **max_total_size** | Delete oldest if total size exceeds | 1GB |
| **always_keep_current** | Never delete backup of current version | true |

**Example configuration:**
```toml
[rollback]
backup_enabled = true
backup_retention_count = 3
backup_retention_days = 30
backup_max_total_size_mb = 1024
backup_compression = "zstd"  # Compress backups to save space
```

### Backup Compression

**Problem:** Backups consume disk space (e.g., 100MB binary × 3 backups = 300MB)

**Solution:** Compress backups

**Implementation:**
```rust
fn create_backup(binary_path: &Path, backup_dir: &Path) -> Result<PathBuf> {
    let version = get_current_version(binary_path)?;
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let backup_name = format!("{}-{}-{}.backup.zst", binary_name, version, timestamp);
    let backup_path = backup_dir.join(&backup_name);

    // Compress with zstd (fast, good compression)
    let input = File::open(binary_path)?;
    let output = File::create(&backup_path)?;
    let mut encoder = zstd::Encoder::new(output, 3)?;  // Level 3: fast
    std::io::copy(&mut input, &mut encoder)?;
    encoder.finish()?;

    Ok(backup_path)
}

fn restore_backup(backup_path: &Path, binary_path: &Path) -> Result<()> {
    let input = File::open(backup_path)?;
    let mut decoder = zstd::Decoder::new(input)?;
    let output_tmp = binary_path.with_extension("tmp");
    let mut output = File::create(&output_tmp)?;
    std::io::copy(&mut decoder, &mut output)?;

    // Atomic replace
    std::fs::rename(&output_tmp, binary_path)?;
    Ok(())
}
```

**Compression savings:**
- Typical binary: 100MB → 30-40MB compressed
- Disk usage for 3 backups: ~120MB instead of 300MB

---

## Reverse Diff-Based Rollback

### How It Works

**Deliberate downgrade (no backup available):**
```
1. Client wants to rollback: 1.1.0 → 1.0.0

2. Client requests reverse diff from server:
   GET /api/v1/diffs?from=1.1.0&to=1.0.0

3. Server checks if reverse diff exists:
   - If exists: Return diff (1.1.0 → 1.0.0)
   - If not exists: Compute on-demand or return full download

4. Client downloads reverse diff

5. Client applies reverse diff:
   bspatch(current-1.1.0, reverse-diff) → previous-1.0.0

6. Client verifies result:
   - Check hash matches expected 1.0.0 hash
   - Verify signature

7. Replace current binary
```

### Publisher Generates Reverse Diffs

**Publisher workflow:**
```bash
# Register versions
deltaship-register --version 1.0.0 --binary ./myapp-1.0.0
deltaship-register --version 1.1.0 --binary ./myapp-1.1.0

# Publish with bidirectional diffs
deltaship-publish --version 1.1.0 --with-reverse-diffs
```

**Server stores:**
- Forward diff: 1.0.0 → 1.1.0 (for updates)
- Reverse diff: 1.1.0 → 1.0.0 (for rollbacks)

**Storage cost:**
- Doubles diff storage (forward + reverse)
- Acceptable for critical binaries
- Optional (can compute on-demand)

### On-Demand Reverse Diff Computation

**Problem:** Storing both forward and reverse diffs doubles storage cost

**Solution:** Compute reverse diff on-demand when requested

**Trade-off:**
- **Saves storage**: Don't pre-compute reverse diffs
- **Slower rollback**: Compute diff when requested (30s-2min for large binaries)
- **Higher server CPU**: Diff computation on live servers

**Configuration (server):**
```toml
[diffs]
# Pre-compute reverse diffs for recent versions only
precompute_reverse_diffs = true
reverse_diff_retention_versions = 5  # Last 5 versions

# For older versions: compute on-demand
reverse_diff_compute_on_demand = true
reverse_diff_cache_ttl_hours = 24  # Cache for 24h
```

**Implementation:**
```rust
async fn get_reverse_diff(from: Version, to: Version) -> Result<Diff> {
    // Check cache first
    if let Some(diff) = cache.get_diff(&from, &to).await? {
        return Ok(diff);
    }

    // Check if pre-computed
    if let Some(diff) = db.get_diff(&from, &to).await? {
        cache.put_diff(&from, &to, &diff).await?;
        return Ok(diff);
    }

    // Compute on-demand
    let from_binary = storage.download_version(&from).await?;
    let to_binary = storage.download_version(&to).await?;
    let diff = compute_diff(&from_binary, &to_binary).await?;

    // Cache result
    cache.put_diff(&from, &to, &diff).await?;

    Ok(diff)
}
```

### Signed Rollback Commands

**For publisher-initiated emergency rollbacks:**

**Publisher workflow:**
```bash
# Issue emergency rollback from 1.1.0 to 1.0.0
deltaship-rollback --from 1.1.0 --to 1.0.0 --reason "Critical security fix"
```

**Server creates signed rollback command:**
```json
{
  "action": "rollback",
  "from_version": "1.1.0",
  "to_version": "1.0.0",
  "reason": "Critical security fix",
  "timestamp": "2027-01-15T12:00:00Z",
  "signature": "ed25519_signature_here"
}
```

**Client behavior:**
```
1. Client checks for updates (periodic or on-demand)
2. Server returns: "Rollback required"
3. Client verifies signature (publisher's public key)
4. Client initiates rollback:
   - Prefer backup if available (fast)
   - Else download reverse diff
5. Apply rollback
6. Verify result
7. Report success/failure to server
```

---

## Decision Matrix

**When to use which rollback mechanism:**

| Scenario | Backup Available? | Mechanism | Rationale |
|----------|-------------------|-----------|-----------|
| **Failed update (automatic)** | Yes | Backup | Fast, reliable, no network |
| **Failed update (automatic)** | No | Abort, retry | Can't rollback without backup |
| **Deliberate downgrade** | Yes | Backup | Fastest option |
| **Deliberate downgrade** | No | Reverse diff | Download diff, bandwidth-efficient |
| **Multi-version rollback (1.3.0 → 1.0.0)** | No 1.0.0 backup | Reverse diff or full download | Diff may be smaller |
| **Emergency publisher rollback** | Yes | Backup | Fastest |
| **Emergency publisher rollback** | No | Reverse diff | Fallback |
| **Backup deleted (retention policy)** | No | Reverse diff or full download | Only option |

---

## Implementation

### Client Rollback Logic

```rust
pub enum RollbackMethod {
    Backup,
    ReverseDiff,
    FullDownload,
}

pub struct RollbackManager {
    backup_dir: PathBuf,
    config: RollbackConfig,
}

impl RollbackManager {
    /// Rollback binary to previous version
    pub async fn rollback(
        &self,
        binary_id: &str,
        target_version: &Version,
        reason: RollbackReason,
    ) -> Result<()> {
        // Determine rollback method
        let method = self.select_rollback_method(binary_id, target_version)?;

        match method {
            RollbackMethod::Backup => {
                self.rollback_from_backup(binary_id, target_version).await
            }
            RollbackMethod::ReverseDiff => {
                self.rollback_with_reverse_diff(binary_id, target_version).await
            }
            RollbackMethod::FullDownload => {
                self.rollback_with_full_download(binary_id, target_version).await
            }
        }
    }

    fn select_rollback_method(
        &self,
        binary_id: &str,
        target_version: &Version,
    ) -> Result<RollbackMethod> {
        // 1. Check if backup exists
        if let Some(backup) = self.find_backup(binary_id, target_version)? {
            return Ok(RollbackMethod::Backup);
        }

        // 2. Check if reverse diff available
        if self.config.enable_reverse_diffs {
            if self.server.has_reverse_diff(binary_id, target_version).await? {
                return Ok(RollbackMethod::ReverseDiff);
            }
        }

        // 3. Fallback to full download
        Ok(RollbackMethod::FullDownload)
    }

    async fn rollback_from_backup(
        &self,
        binary_id: &str,
        target_version: &Version,
    ) -> Result<()> {
        let backup_path = self.find_backup(binary_id, target_version)?
            .ok_or(RollbackError::BackupNotFound)?;

        let binary_path = self.get_binary_path(binary_id)?;

        // Restore backup
        restore_backup(&backup_path, &binary_path)?;

        // Verify
        self.verify_version(&binary_path, target_version)?;

        log::info!("Rollback successful: {} → {}", binary_id, target_version);
        Ok(())
    }

    async fn rollback_with_reverse_diff(
        &self,
        binary_id: &str,
        target_version: &Version,
    ) -> Result<()> {
        let current_version = self.get_current_version(binary_id)?;
        let binary_path = self.get_binary_path(binary_id)?;

        // Download reverse diff
        let diff = self.server.get_reverse_diff(
            binary_id,
            &current_version,
            target_version,
        ).await?;

        // Apply diff
        let new_binary_path = binary_path.with_extension("new");
        apply_diff(&binary_path, &diff, &new_binary_path)?;

        // Verify
        self.verify_version(&new_binary_path, target_version)?;

        // Atomic replace
        std::fs::rename(&new_binary_path, &binary_path)?;

        log::info!("Rollback successful: {} → {}", binary_id, target_version);
        Ok(())
    }
}

pub enum RollbackReason {
    UpdateFailed,
    UserRequested,
    PublisherCommand,
    AutomaticRecovery,
}
```

### Server API Endpoints

**Get reverse diff:**
```http
GET /api/v1/diffs/reverse?binary_id={id}&from_version=1.1.0&to_version=1.0.0
```

**Response:**
```json
{
  "diff_id": "uuid-here",
  "from_version": "1.1.0",
  "to_version": "1.0.0",
  "diff_size_bytes": 524288,
  "diff_algorithm": "bsdiff",
  "download_url": "https://cdn.example.com/diffs/reverse-xyz.diff",
  "signature": "ed25519_signature",
  "hash_blake3": "abc123..."
}
```

---

## Configuration

### Client Configuration

```toml
[rollback]
# Backup-based rollback
backup_enabled = true
backup_dir = "/var/lib/deltaship/backups"
backup_retention_count = 3
backup_retention_days = 30
backup_max_total_size_mb = 1024
backup_compression = "zstd"
backup_compression_level = 3

# Reverse diff rollback
enable_reverse_diffs = true
prefer_backup_over_diff = true  # Use backup if available

# Automatic rollback on update failure
auto_rollback_on_failure = true
rollback_timeout_seconds = 300

# Verification
verify_after_rollback = true
run_post_rollback_tests = false  # Optional functionality tests
```

### Publisher Configuration

```toml
[rollback_policy]
# Generate reverse diffs when publishing
generate_reverse_diffs = true

# Reverse diff retention (how many versions back)
reverse_diff_retention = 5  # Last 5 versions

# Storage optimization
compress_reverse_diffs = true
deduplicate_diffs = true  # If forward and reverse similar
```

### Server Configuration

```toml
[diffs.reverse]
# Pre-compute reverse diffs
precompute_enabled = true
precompute_retention_versions = 5

# On-demand computation
compute_on_demand = true
on_demand_cache_ttl_hours = 24
on_demand_max_compute_time_seconds = 300

# Resource limits
max_concurrent_computations = 2
computation_cpu_limit_percent = 50
```

---

## Security Considerations

### Signed Rollback Commands

**Threat:** Attacker forces clients to downgrade to vulnerable version

**Mitigation:**
1. **Require signed rollback command** (publisher's private key)
2. **Client verifies signature** before accepting rollback
3. **Timestamp check**: Reject old rollback commands (replay attack)
4. **Rollback window**: Limit how far back clients can rollback (e.g., max 10 versions)

**Example verification:**
```rust
fn verify_rollback_command(cmd: &RollbackCommand, public_key: &PublicKey) -> Result<()> {
    // 1. Verify signature
    verify_signature(&cmd.signature, &cmd.data, public_key)?;

    // 2. Check timestamp (must be recent)
    let age = Utc::now() - cmd.timestamp;
    if age > Duration::hours(24) {
        return Err(RollbackError::CommandExpired);
    }

    // 3. Check rollback is allowed (version monotonicity)
    let current = get_current_version()?;
    let target = &cmd.to_version;

    if target >= current {
        return Err(RollbackError::NotADowngrade);
    }

    // 4. Check rollback window
    let version_distance = current.version_number - target.version_number;
    if version_distance > config.max_rollback_distance {
        return Err(RollbackError::TooFarBack);
    }

    Ok(())
}
```

### Backup Integrity

**Threat:** Attacker modifies backup files

**Mitigation:**
1. **Store backup hash** in database
2. **Verify hash before restore**
3. **Encrypt backups** (optional, if binary contains secrets)

**Implementation:**
```rust
fn create_backup_with_hash(binary_path: &Path) -> Result<BackupInfo> {
    let backup_path = create_backup(binary_path)?;
    let hash = compute_blake3(&backup_path)?;

    let info = BackupInfo {
        backup_path: backup_path.clone(),
        hash_blake3: hash,
        created_at: Utc::now(),
    };

    // Store in database
    db.insert_backup(&info)?;

    Ok(info)
}

fn restore_backup_verified(backup_info: &BackupInfo) -> Result<()> {
    // Verify integrity before restore
    let current_hash = compute_blake3(&backup_info.backup_path)?;
    if current_hash != backup_info.hash_blake3 {
        return Err(RollbackError::BackupCorrupted);
    }

    restore_backup(&backup_info.backup_path, &target_path)?;
    Ok(())
}
```

---

## Summary

Deltaship uses a **hybrid rollback approach**:

### 1. Backup-Based Rollback
- **Primary mechanism** for automatic recovery from failed updates
- **Fastest** (no network, exact copy)
- **Most reliable** (always available during update)
- **Configurable retention** (balance disk usage vs rollback capability)
- **Compression** to reduce disk usage

### 2. Reverse Diff-Based Rollback
- **Secondary mechanism** for deliberate downgrades
- **Bandwidth-efficient** (download only diff)
- **Optional** (can be disabled if storage/CPU constrained)
- **On-demand computation** to reduce storage costs
- **Signed commands** for publisher-initiated rollbacks

### 3. Decision Logic
- **Prefer backup** if available (fastest)
- **Fallback to reverse diff** if backup unavailable
- **Last resort: full download** if neither available

**Result:** Reliable automatic recovery + efficient deliberate downgrades

---

**Next Steps:**
1. Implement backup management in client patcher
2. Add reverse diff generation to publisher toolkit
3. Implement server-side on-demand diff computation
4. Add rollback command signing/verification
5. Document rollback best practices for publishers
6. Test rollback scenarios (failure recovery, deliberate downgrade)

---

**References:**
- [Atomic Operations](https://en.wikipedia.org/wiki/Atomicity_(database_systems))
- [Binary Diff Algorithms](https://en.wikipedia.org/wiki/Delta_encoding)
- [Signed Updates](https://theupdateframework.io/)
