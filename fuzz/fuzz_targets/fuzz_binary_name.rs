#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    // Try to convert bytes to a string
    if let Ok(s) = std::str::from_utf8(data) {
        // Test path validation logic that might be used for binary names
        // Should handle path traversal attempts, special characters, etc.
        let path = Path::new(s);

        // These operations should never panic
        let _ = path.file_name();
        let _ = path.extension();
        let _ = path.parent();

        // Check for path traversal components
        let has_traversal = path.components().any(|c| {
            matches!(c, std::path::Component::ParentDir)
        });

        // Validate alphanumeric with allowed chars (_, -, .)
        let is_valid_name = s.chars().all(|c| {
            c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
        });

        // Both checks should complete without panic
        let _ = (has_traversal, is_valid_name);
    }
});
