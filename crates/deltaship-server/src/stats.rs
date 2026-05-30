//! Statistics collection for the Deltaship server.

use std::path::Path;

/// Calculate total storage usage in bytes for the data directory
pub fn calculate_storage_usage(data_dir: &Path) -> u64 {
    let apps_dir = data_dir.join("apps");
    if !apps_dir.exists() {
        return 0;
    }

    calculate_dir_size(&apps_dir)
}

/// Recursively calculate directory size
fn calculate_dir_size(path: &Path) -> u64 {
    let mut size: u64 = 0;

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Use symlink_metadata to avoid following symlinks (security)
            if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                // FIX F: saturating arithmetic so attacker-influenced sizes can't
                // overflow (panic in debug / wrap in release).
                if metadata.is_file() {
                    size = size.saturating_add(metadata.len());
                } else if metadata.is_dir() {
                    size = size.saturating_add(calculate_dir_size(&path));
                }
                // Symlinks are ignored for size calculation
            }
        }
    }

    size
}

/// Count the number of apps in the data directory
pub fn count_apps(data_dir: &Path) -> usize {
    let apps_dir = data_dir.join("apps");
    if !apps_dir.exists() {
        return 0;
    }

    std::fs::read_dir(&apps_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

/// Count total versions across all apps
pub fn count_versions(data_dir: &Path) -> usize {
    let apps_dir = data_dir.join("apps");
    if !apps_dir.exists() {
        return 0;
    }

    let mut count = 0;

    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let versions_dir = entry.path().join("versions");
            if versions_dir.is_dir() {
                if let Ok(versions) = std::fs::read_dir(&versions_dir) {
                    count += versions
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().is_dir())
                        .count();
                }
            }
        }
    }

    count
}
