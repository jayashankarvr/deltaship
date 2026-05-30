/// gen-delta: Generate a compressed binary delta between OLD and NEW.
///
/// Usage: gen-delta <old-file> <new-file> <delta-output>
///
/// Produces a zstd-compressed bsdiff patch that, when applied to OLD, yields NEW exactly.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: {} <old-file> <new-file> <delta-output>", args[0]);
        std::process::exit(1);
    }

    let old_path = &args[1];
    let new_path = &args[2];
    let out_path = &args[3];

    let old = std::fs::read(old_path)
        .unwrap_or_else(|e| { eprintln!("Cannot read {}: {}", old_path, e); std::process::exit(1); });
    let new = std::fs::read(new_path)
        .unwrap_or_else(|e| { eprintln!("Cannot read {}: {}", new_path, e); std::process::exit(1); });

    eprint!("Generating delta ({} MB old, {} MB new)... ", old.len() / 1_048_576, new.len() / 1_048_576);

    let delta = deltaship_diff::generate_diff(&old, &new)
        .unwrap_or_else(|e| { eprintln!("Diff failed: {}", e); std::process::exit(1); });

    let raw_size = delta.len();

    eprint!("compressing... ");

    let compressed = deltaship_diff::compress_diff(&delta)
        .unwrap_or_else(|e| { eprintln!("Compression failed: {}", e); std::process::exit(1); });

    std::fs::write(out_path, &compressed)
        .unwrap_or_else(|e| { eprintln!("Cannot write {}: {}", out_path, e); std::process::exit(1); });

    let pct = |n: usize, d: usize| if d == 0 { "N/A".to_string() } else { format!("{:.1}%", n as f64 / d as f64 * 100.0) };
    eprintln!("done.");
    eprintln!("  old size        : {} bytes", old.len());
    eprintln!("  new size        : {} bytes", new.len());
    eprintln!("  raw delta       : {} bytes", raw_size);
    eprintln!("  compressed delta: {} bytes  ({} of raw, {} of new)", compressed.len(), pct(compressed.len(), raw_size), pct(compressed.len(), new.len()));
    eprintln!("  saved vs full DL: {} bytes  ({})", new.len().saturating_sub(compressed.len()), pct(new.len().saturating_sub(compressed.len()), new.len()));
    eprintln!("  written to: {}", out_path);
}
