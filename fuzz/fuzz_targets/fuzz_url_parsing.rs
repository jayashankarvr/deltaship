#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try to convert bytes to a string
    if let Ok(s) = std::str::from_utf8(data) {
        // Test URL parsing - should handle malformed URLs gracefully
        if let Ok(url) = url::Url::parse(s) {
            // If parsing succeeds, try various operations
            let _ = url.scheme();
            let _ = url.host_str();
            let _ = url.port();
            let _ = url.path();
            let _ = url.query();
        }

        // Also test with "http://" prefix
        let prefixed = format!("http://{}", s);
        let _ = url::Url::parse(&prefixed);
    }
});
