/// apply-delta: Reconstruct NEW by applying a delta to OLD.
///
/// Usage: apply-delta <old-file> <delta-file> <output-file> [expected-blake3-hex]
///
/// Accepts both raw and zstd-compressed delta files (auto-detected).
/// Verifies the result against the expected BLAKE3 checksum (if provided as 4th arg).

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 || args.len() > 5 {
        eprintln!("Usage: {} <old-file> <delta-file> <output-file> [expected-blake3-hex]", args[0]);
        std::process::exit(1);
    }

    let old_path   = &args[1];
    let delta_path = &args[2];
    let out_path   = &args[3];
    let expected   = args.get(4).map(|s| s.as_str());

    let old = std::fs::read(old_path)
        .unwrap_or_else(|e| { eprintln!("Cannot read {}: {}", old_path, e); std::process::exit(1); });
    let delta_raw = std::fs::read(delta_path)
        .unwrap_or_else(|e| { eprintln!("Cannot read {}: {}", delta_path, e); std::process::exit(1); });

    // zstd magic: 0xFD2FB528 stored little-endian → bytes [28 B5 2F FD]
    let is_compressed = delta_raw.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]);

    let delta = if is_compressed {
        eprint!("Decompressing delta ({} MB)... ", delta_raw.len() / 1_048_576);
        let d = deltaship_diff::decompress_diff(&delta_raw)
            .unwrap_or_else(|e| { eprintln!("Decompression failed: {}", e); std::process::exit(1); });
        eprintln!("{} MB raw", d.len() / 1_048_576);
        d
    } else {
        delta_raw
    };

    eprint!("Applying delta ({} MB old + {} MB delta)... ", old.len() / 1_048_576, delta.len() / 1_048_576);

    let new = deltaship_diff::apply_patch(&old, &delta)
        .unwrap_or_else(|e| { eprintln!("Patch failed: {}", e); std::process::exit(1); });

    eprintln!("done  →  {} bytes", new.len());

    // Verify checksum before writing — don't persist a corrupt result.
    if let Some(expected_hex) = expected {
        let hash = blake3::hash(&new);
        let got_hex = hash.to_hex().to_string();
        if got_hex == expected_hex {
            eprintln!("  checksum OK: {}", got_hex);
        } else {
            eprintln!("  CHECKSUM MISMATCH — output NOT written.");
            eprintln!("  expected: {}", expected_hex);
            eprintln!("  got:      {}", got_hex);
            std::process::exit(1);
        }
    }

    std::fs::write(out_path, &new)
        .unwrap_or_else(|e| { eprintln!("Cannot write {}: {}", out_path, e); std::process::exit(1); });

    eprintln!("  written to: {}", out_path);
}
