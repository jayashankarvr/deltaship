#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try to convert bytes to a string
    if let Ok(s) = std::str::from_utf8(data) {
        // Test semver parsing - this should never panic
        // Only parse strings of reasonable length to avoid excessive memory usage
        if s.len() < 256 {
            let _ = semver::Version::parse(s);
        }
    }
});
