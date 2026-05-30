/// gen-delta: Generate a binary delta between OLD and NEW.
///
/// Usage: gen-delta <old-file> <new-file> <delta-output>
///
/// Produces a bsdiff patch that, when applied to OLD, yields NEW exactly.

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

    let delta = vbdp_diff::generate_diff(&old, &new)
        .unwrap_or_else(|e| { eprintln!("Diff failed: {}", e); std::process::exit(1); });

    std::fs::write(out_path, &delta)
        .unwrap_or_else(|e| { eprintln!("Cannot write {}: {}", out_path, e); std::process::exit(1); });

    let ratio = delta.len() as f64 / new.len() as f64 * 100.0;
    eprintln!("done.");
    eprintln!("  old size  : {} bytes", old.len());
    eprintln!("  new size  : {} bytes", new.len());
    eprintln!("  delta size: {} bytes  ({:.1}% of new)", delta.len(), ratio);
    eprintln!("  saved     : {} bytes  (would skip {:.1}% of a full download)", new.len().saturating_sub(delta.len()), 100.0 - ratio);
    eprintln!("  written to: {}", out_path);
}
