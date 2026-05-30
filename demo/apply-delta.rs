/// apply-delta: Reconstruct NEW by applying a delta to OLD.
///
/// Usage: apply-delta <old-file> <delta-file> <output-file>
///
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
    let delta = std::fs::read(delta_path)
        .unwrap_or_else(|e| { eprintln!("Cannot read {}: {}", delta_path, e); std::process::exit(1); });

    eprint!("Applying delta ({} MB old + {} MB delta)... ", old.len() / 1_048_576, delta.len() / 1_048_576);

    let new = vbdp_diff::apply_patch(&old, &delta)
        .unwrap_or_else(|e| { eprintln!("Patch failed: {}", e); std::process::exit(1); });

    eprintln!("done  →  {} bytes", new.len());

    // Optional checksum verification
    if let Some(expected_hex) = expected {
        let hash = blake3::hash(&new);
        let got_hex = hash.to_hex().to_string();
        if got_hex == expected_hex {
            eprintln!("  checksum OK: {}", got_hex);
        } else {
            eprintln!("  CHECKSUM MISMATCH!");
            eprintln!("  expected: {}", expected_hex);
            eprintln!("  got:      {}", got_hex);
            std::process::exit(1);
        }
    }

    std::fs::write(out_path, &new)
        .unwrap_or_else(|e| { eprintln!("Cannot write {}: {}", out_path, e); std::process::exit(1); });

    eprintln!("  written to: {}", out_path);
}
