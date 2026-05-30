#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the diff/patch functionality
    // Split input into source and target portions
    if data.len() < 2 {
        return;
    }

    let split_point = data.len() / 2;
    let source = &data[..split_point];
    let target = &data[split_point..];

    // Try to generate a diff - should handle any binary input
    // This will test bsdiff's robustness
    if source.len() > 0 && target.len() > 0 && source.len() < 1024 * 1024 && target.len() < 1024 * 1024 {
        // Limit sizes to 1MB to avoid OOM in fuzzing environments.
        // Note: Real binaries can be hundreds of MB, but fuzzing with larger inputs
        // causes memory exhaustion. For large-input testing, use integration tests
        // or manual testing with realistic binary sizes.
        let _ = deltaship_diff::generate::generate_diff(source, target);
    }

    // Also test patching with random patch data
    if data.len() > 10 {
        // Try to apply arbitrary data as a patch - should fail gracefully
        let _ = deltaship_diff::apply::apply_patch(source, data);
    }
});
