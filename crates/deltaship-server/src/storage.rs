//! File-based storage helpers for reading catalogs and artifacts.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use crate::models::{
    AppCatalog, AppDetails, AppListItem, PlatformArtifact, RolloutConfig, VersionInfo,
    VersionListItem, VersionManifest,
};
use std::path::Path;
use deltaship_core::Platform;

// Global per-app mutexes to prevent thread pool exhaustion from concurrent
// spawn_blocking calls that all contend on the same per-app file lock.
static CATALOG_MUTEXES: OnceLock<StdMutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    OnceLock::new();

/// Soft cap on the number of distinct per-app mutexes retained (FIX G).
///
/// The map previously grew unbounded (one entry per app name, forever). We cap
/// it and, when over the cap, evict entries whose `Arc` is not currently held by
/// anyone (`strong_count == 1`, i.e. only the map holds it). Evicting an unheld
/// mutex is safe: a later request for that app simply creates a fresh mutex, and
/// since no operation is in flight there is nothing to be mutually-excluded from.
/// Entries that ARE in use are never evicted, so correctness is preserved.
const MAX_CATALOG_MUTEXES: usize = 4096;

fn get_catalog_mutex(app_name: &str) -> Arc<tokio::sync::Mutex<()>> {
    let map = CATALOG_MUTEXES.get_or_init(|| StdMutex::new(HashMap::new()));
    let mut guard = map.lock().expect("catalog mutex poisoned");

    // Bound growth: if over capacity and this is a new key, drop idle entries.
    if guard.len() >= MAX_CATALOG_MUTEXES && !guard.contains_key(app_name) {
        guard.retain(|_, m| Arc::strong_count(m) > 1);
    }

    guard
        .entry(app_name.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

/// Storage error type
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
}

/// Get the app catalog for a given application
#[tracing::instrument(skip(data_dir), fields(app = %app_name))]
pub async fn get_app_catalog(data_dir: &Path, app_name: &str) -> Option<AppCatalog> {
    let catalog_path = data_dir.join("apps").join(app_name).join("catalog.json");

    if !catalog_path.exists() {
        tracing::debug!("Catalog not found");
        return None;
    }

    let content = tokio::fs::read_to_string(&catalog_path).await.ok()?;
    serde_json::from_str(&content).ok()
}

/// Get the version manifest for a specific version
#[tracing::instrument(skip(data_dir), fields(app = %app_name, version = %version))]
pub async fn get_version_manifest(
    data_dir: &Path,
    app_name: &str,
    version: &str,
) -> Option<VersionManifest> {
    let manifest_path = data_dir
        .join("apps")
        .join(app_name)
        .join("versions")
        .join(version)
        .join("manifest.json");

    if !manifest_path.exists() {
        tracing::debug!("Manifest not found");
        return None;
    }

    let content = tokio::fs::read_to_string(&manifest_path).await.ok()?;
    serde_json::from_str(&content).ok()
}

/// Get the path to a binary file for a specific version and platform
pub fn get_binary_path(
    data_dir: &Path,
    app_name: &str,
    version: &str,
    platform: &Platform,
) -> PathBuf {
    let path = data_dir
        .join("apps")
        .join(app_name)
        .join("versions")
        .join(version)
        .join(format!("binary-{}", platform.as_str()));

    // Validate path length for Windows MAX_PATH compatibility
    if let Err(e) = crate::validation::validate_path_length(&path) {
        tracing::warn!(
            path = %path.display(),
            error = %e,
            "Path length validation failed for binary path"
        );
    }

    path
}

/// Get the path to a diff file between two versions
pub fn get_diff_path(
    data_dir: &Path,
    app_name: &str,
    from_version: &str,
    to_version: &str,
    platform: &Platform,
) -> PathBuf {
    let path = data_dir
        .join("apps")
        .join(app_name)
        .join("diffs")
        .join(format!(
            "{}-to-{}-{}",
            from_version,
            to_version,
            platform.as_str()
        ));

    // Validate path length for Windows MAX_PATH compatibility
    if let Err(e) = crate::validation::validate_path_length(&path) {
        tracing::warn!(
            path = %path.display(),
            error = %e,
            "Path length validation failed for diff path"
        );
    }

    path
}

/// Get the path to a signature file for a specific version
pub fn get_signature_path(data_dir: &Path, app_name: &str, version: &str) -> PathBuf {
    let path = data_dir
        .join("apps")
        .join(app_name)
        .join("versions")
        .join(version)
        .join("signature.sig");

    // Validate path length for Windows MAX_PATH compatibility
    if let Err(e) = crate::validation::validate_path_length(&path) {
        tracing::warn!(
            path = %path.display(),
            error = %e,
            "Path length validation failed for signature path"
        );
    }

    path
}

/// Ensure the directory structure exists for an app
pub async fn ensure_app_dirs(data_dir: &Path, app_name: &str) -> StorageResult<()> {
    let app_dir = data_dir.join("apps").join(app_name);
    let versions_dir = app_dir.join("versions");
    let diffs_dir = app_dir.join("diffs");

    tokio::fs::create_dir_all(&versions_dir).await?;
    tokio::fs::create_dir_all(&diffs_dir).await?;

    Ok(())
}

/// Ensure the version directory exists
pub async fn ensure_version_dir(
    data_dir: &Path,
    app_name: &str,
    version: &str,
) -> StorageResult<PathBuf> {
    let version_dir = data_dir
        .join("apps")
        .join(app_name)
        .join("versions")
        .join(version);

    tokio::fs::create_dir_all(&version_dir).await?;

    Ok(version_dir)
}

/// Save a binary file for a specific version and platform.
///
/// Retained for the library API / tests; the publish path now uses
/// [`save_binary_from_temp`] to avoid buffering the whole binary (FIX A).
#[allow(dead_code)]
#[tracing::instrument(skip(data_dir, data), fields(app = %app_name, version = %version, platform = %platform.as_str(), size = data.len()))]
pub async fn save_binary(
    data_dir: &Path,
    app_name: &str,
    version: &str,
    platform: &Platform,
    data: &[u8],
) -> StorageResult<()> {
    let version_dir = ensure_version_dir(data_dir, app_name, version).await?;
    let binary_path = version_dir.join(format!("binary-{}", platform.as_str()));

    tokio::fs::write(&binary_path, data).await?;
    tracing::info!(
        "Saved binary for {}/{} ({}): {} bytes",
        app_name,
        version,
        platform.as_str(),
        data.len()
    );

    Ok(())
}

/// Atomically move a streamed temp file into place as the binary for a version
/// and platform (FIX A). The temp file is expected to already live in the
/// version directory (same filesystem) so the rename is atomic.
pub async fn save_binary_from_temp(
    data_dir: &Path,
    app_name: &str,
    version: &str,
    platform: &Platform,
    temp_path: &Path,
) -> StorageResult<()> {
    let version_dir = ensure_version_dir(data_dir, app_name, version).await?;
    let binary_path = version_dir.join(format!("binary-{}", platform.as_str()));
    tokio::fs::rename(temp_path, &binary_path).await?;
    tracing::info!(
        "Saved binary (streamed) for {}/{} ({})",
        app_name,
        version,
        platform.as_str()
    );
    Ok(())
}

/// Atomically move a streamed temp file into place as a diff file (FIX A).
pub async fn save_diff_from_temp(
    data_dir: &Path,
    app_name: &str,
    from_version: &str,
    to_version: &str,
    platform: &Platform,
    temp_path: &Path,
) -> StorageResult<std::path::PathBuf> {
    ensure_diffs_dir(data_dir, app_name).await?;
    let diff_path = get_diff_path(data_dir, app_name, from_version, to_version, platform);
    tokio::fs::rename(temp_path, &diff_path).await?;
    tracing::info!(
        "Saved diff (streamed) for {}: {} -> {} ({})",
        app_name,
        from_version,
        to_version,
        platform.as_str()
    );
    Ok(diff_path)
}

/// Save a signature file for a specific version
pub async fn save_signature(
    data_dir: &Path,
    app_name: &str,
    version: &str,
    data: &[u8],
) -> StorageResult<()> {
    let version_dir = ensure_version_dir(data_dir, app_name, version).await?;
    let sig_path = version_dir.join("signature.sig");

    tokio::fs::write(&sig_path, data).await?;
    tracing::info!("Saved signature for {}/{}", app_name, version);

    Ok(())
}

/// Update or create the catalog with a new version.
///
/// **DEPRECATED**: This function has a race condition in its read-modify-write operation.
/// Use `update_catalog_atomic()` instead for safe concurrent updates.
///
/// # Race Condition
///
/// This function performs an unlocked read-modify-write:
/// 1. Read catalog.json
/// 2. Modify in memory
/// 3. Write catalog.json
///
/// If two updates happen concurrently, one can overwrite the other's changes.
#[deprecated(
    since = "0.1.0",
    note = "Use update_catalog_atomic() to prevent race conditions"
)]
#[allow(dead_code)]
pub async fn update_catalog(
    data_dir: &Path,
    app_name: &str,
    version_info: VersionInfo,
) -> StorageResult<()> {
    ensure_app_dirs(data_dir, app_name).await?;

    let catalog_path = data_dir.join("apps").join(app_name).join("catalog.json");

    let mut catalog = if catalog_path.exists() {
        let content = tokio::fs::read_to_string(&catalog_path).await?;
        serde_json::from_str::<AppCatalog>(&content)?
    } else {
        AppCatalog {
            app_name: app_name.to_string(),
            versions: Vec::new(),
            latest_version: version_info.version.clone(),
        }
    };

    // Check if version already exists and update it, or add new
    if let Some(existing) = catalog
        .versions
        .iter_mut()
        .find(|v| v.version == version_info.version)
    {
        // Merge platforms if version exists
        for platform in &version_info.platforms {
            if !existing.platforms.contains(platform) {
                existing.platforms.push(*platform);
            }
        }
        if version_info.release_notes.is_some() {
            existing.release_notes = version_info.release_notes.clone();
        }
    } else {
        // Use binary search to find insertion position (versions sorted oldest first)
        // This is O(log n) instead of O(n log n) for sorting after insert
        let insert_pos = catalog
            .versions
            .binary_search_by(|v| version_info.version.cmp(&v.version))
            .unwrap_or_else(|pos| pos);
        catalog.versions.insert(insert_pos, version_info);
    }

    let content = serde_json::to_string_pretty(&catalog)?;
    tokio::fs::write(&catalog_path, content).await?;

    tracing::info!("Updated catalog for {}", app_name);

    Ok(())
}

/// Atomically update or create the catalog with a new version.
///
/// This function performs a read-modify-write operation under a single exclusive lock,
/// preventing race conditions when multiple requests try to update the catalog simultaneously.
///
/// # Atomicity
///
/// The entire read-modify-write operation is performed under a single exclusive lock:
/// 1. Acquire exclusive file lock on catalog.json
/// 2. Read current catalog (or create default if doesn't exist)
/// 3. Apply version update (merge or insert)
/// 4. Write new catalog to a temporary file
/// 5. Atomically rename temp file to catalog.json
/// 6. Release lock
///
/// This ensures that concurrent updates don't lose data due to the read-modify-write race.
#[tracing::instrument(skip(data_dir, version_info), fields(app = %app_name, version = %version_info.version))]
pub async fn update_catalog_atomic(
    data_dir: &Path,
    app_name: &str,
    version_info: VersionInfo,
) -> StorageResult<()> {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    ensure_app_dirs(data_dir, app_name).await?;

    let catalog_path = data_dir.join("apps").join(app_name).join("catalog.json");
    let version_info_clone = version_info.clone();
    let app_name_owned = app_name.to_string();
    let app_name_for_log = app_name.to_string();

    // Acquire async mutex to prevent thread pool exhaustion
    // Without this, concurrent publishes for the same app could spawn multiple blocking tasks,
    // all waiting for the file lock, exhausting the thread pool and causing deadlocks
    let mutex = get_catalog_mutex(app_name);
    let _guard = mutex.lock().await;

    // Use blocking I/O for file locking since fs4 doesn't have async support
    tokio::task::spawn_blocking(move || -> StorageResult<()> {
        let app_name = app_name_owned;
        // Create or open the catalog file with exclusive lock
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&catalog_path)?;

        // Acquire exclusive lock - this blocks until lock is available
        file.lock_exclusive()?;

        // Read current catalog while holding lock (or create default)
        let mut catalog = if catalog_path.exists() {
            std::fs::read_to_string(&catalog_path)
                .ok()
                .and_then(|content| serde_json::from_str(&content).ok())
                .unwrap_or_else(|| AppCatalog {
                    app_name: app_name.to_string(),
                    versions: Vec::new(),
                    latest_version: version_info_clone.version.clone(),
                })
        } else {
            AppCatalog {
                app_name: app_name.to_string(),
                versions: Vec::new(),
                latest_version: version_info_clone.version.clone(),
            }
        };

        // Check if version already exists and update it, or add new
        if let Some(existing) = catalog
            .versions
            .iter_mut()
            .find(|v| v.version == version_info_clone.version)
        {
            // Merge platforms if version exists
            for platform in &version_info_clone.platforms {
                if !existing.platforms.contains(platform) {
                    existing.platforms.push(*platform);
                }
            }
            if version_info_clone.release_notes.is_some() {
                existing.release_notes = version_info_clone.release_notes.clone();
            }
        } else {
            // Use binary search to find insertion position (versions sorted newest first)
            let insert_pos = catalog
                .versions
                .binary_search_by(|v| version_info_clone.version.cmp(&v.version))
                .unwrap_or_else(|pos| pos);
            catalog.versions.insert(insert_pos, version_info_clone.clone());
        }

        // Advance latest_version if this version is newer
        if version_info_clone.version > catalog.latest_version {
            catalog.latest_version = version_info_clone.version;
        }

        // Write to a temporary file first for atomic operation
        let temp_path = catalog_path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(&catalog)?;
        std::fs::write(&temp_path, &content)?;

        // Atomically rename temp file to target (atomic on POSIX systems)
        std::fs::rename(&temp_path, &catalog_path)?;

        // Lock is released when file is dropped
        drop(file);

        Ok(())
    })
    .await
    .map_err(|e| StorageError::Io(std::io::Error::other(e)))??;

    tracing::info!("Atomically updated catalog for {}", app_name_for_log);

    Ok(())
}

/// Update or create the version manifest.
///
/// Uses file-level locking and atomic write operations to prevent race conditions
/// when multiple requests try to update the manifest simultaneously.
///
/// # Atomicity
///
/// The entire read-modify-write operation is performed under a single exclusive lock:
/// 1. Acquire exclusive file lock on manifest.json
/// 2. Read current manifest (or create default if doesn't exist)
/// 3. Apply platform update
/// 4. Write new manifest to a temporary file
/// 5. Atomically rename temp file to manifest.json
/// 6. Release lock
///
/// This ensures that concurrent updates don't lose data due to the read-modify-write race.
pub async fn update_manifest(
    data_dir: &Path,
    app_name: &str,
    version: &str,
    platform: &Platform,
    checksum: &str,
    size: u64,
) -> StorageResult<()> {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let version_dir = ensure_version_dir(data_dir, app_name, version).await?;
    let manifest_path = version_dir.join("manifest.json");
    let platform = *platform;
    let checksum = checksum.to_string();
    let version_str = version.to_string();
    let app_name_for_log = app_name.to_string();
    let version_for_log = version.to_string();

    // FIX G: hold the same per-app async mutex used by the catalog paths so that
    // concurrent manifest updates for one app don't each spawn a blocking task
    // that piles up waiting on the file lock (thread-pool pileup). Serializing
    // here keeps at most one blocking task per app in flight.
    let mutex = get_catalog_mutex(app_name);
    let _guard = mutex.lock().await;

    // Use blocking I/O for file locking since fs4 doesn't have async support
    tokio::task::spawn_blocking(move || -> StorageResult<()> {
        // Create or open the manifest file with exclusive lock
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&manifest_path)?;

        // Acquire exclusive lock - this blocks until lock is available
        file.lock_exclusive()?;

        // Read current manifest while holding lock (or create default)
        let mut manifest = if manifest_path.exists() {
            std::fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|content| serde_json::from_str::<VersionManifest>(&content).ok())
                .unwrap_or_else(|| VersionManifest {
                    version: version_str
                        .parse()
                        .expect("Version should be parseable at this point"),
                    platforms: std::collections::HashMap::new(),
                })
        } else {
            VersionManifest {
                version: version_str
                    .parse()
                    .map_err(|_| StorageError::NotFound(format!("Invalid version: {}", version_str)))?,
                platforms: std::collections::HashMap::new(),
            }
        };

        manifest.platforms.insert(
            platform,
            PlatformArtifact {
                checksum,
                size,
                diffs_from: Vec::new(),
            },
        );

        // Write to a temporary file first for atomic operation
        let temp_path = manifest_path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(&temp_path, &content)?;

        // Atomically rename temp file to target (atomic on POSIX systems)
        std::fs::rename(&temp_path, &manifest_path)?;

        // Lock is released when file is dropped
        drop(file);

        Ok(())
    })
    .await
    .map_err(|e| StorageError::Io(std::io::Error::other(e)))??;

    tracing::info!("Updated manifest for {}/{}", app_name_for_log, version_for_log);

    Ok(())
}

/// Set the latest version for an app.
///
/// Uses file-level locking and atomic write operations to prevent race conditions
/// when multiple requests try to update the catalog simultaneously.
///
/// # Atomicity
///
/// The entire read-modify-write operation is performed under a single exclusive lock:
/// 1. Acquire exclusive file lock on catalog.json
/// 2. Read current catalog
/// 3. Verify version exists and update latest_version
/// 4. Write new catalog to a temporary file
/// 5. Atomically rename temp file to catalog.json
/// 6. Release lock
pub async fn set_latest_version(
    data_dir: &Path,
    app_name: &str,
    version: &str,
) -> StorageResult<()> {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let catalog_path = data_dir.join("apps").join(app_name).join("catalog.json");
    let version_owned = version.to_string();
    let app_name_for_closure = app_name.to_string();
    let app_name_for_log = app_name.to_string();
    let version_for_log = version.to_string();

    // Acquire async mutex to prevent thread pool exhaustion
    let mutex = get_catalog_mutex(app_name);
    let _guard = mutex.lock().await;

    // Use blocking I/O for file locking since fs4 doesn't have async support
    tokio::task::spawn_blocking(move || -> StorageResult<()> {
        if !catalog_path.exists() {
            return Err(StorageError::NotFound(format!(
                "Catalog not found for app: {}",
                app_name_for_closure
            )));
        }

        // Open the catalog file with exclusive lock
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&catalog_path)?;

        // Acquire exclusive lock - this blocks until lock is available
        file.lock_exclusive()?;

        // Read current catalog while holding lock
        let content = std::fs::read_to_string(&catalog_path)?;
        let mut catalog: AppCatalog = serde_json::from_str(&content)?;

        // Verify the version exists
        let version_exists = catalog
            .versions
            .iter()
            .any(|v| v.version.to_string() == version_owned);
        if !version_exists {
            return Err(StorageError::NotFound(format!(
                "Version {} not found in catalog",
                version_owned
            )));
        }

        catalog.latest_version = version_owned
            .parse()
            .map_err(|_| StorageError::NotFound(format!("Invalid version: {}", version_owned)))?;

        // Write to a temporary file first for atomic operation
        let temp_path = catalog_path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(&catalog)?;
        std::fs::write(&temp_path, &content)?;

        // Atomically rename temp file to target (atomic on POSIX systems)
        std::fs::rename(&temp_path, &catalog_path)?;

        // Lock is released when file is dropped
        drop(file);

        Ok(())
    })
    .await
    .map_err(|e| StorageError::Io(std::io::Error::other(e)))??;

    tracing::info!("Set latest version for {} to {}", app_name_for_log, version_for_log);

    Ok(())
}

/// Ensure the diffs directory exists for an app
pub async fn ensure_diffs_dir(data_dir: &Path, app_name: &str) -> StorageResult<()> {
    let diffs_dir = data_dir.join("apps").join(app_name).join("diffs");
    tokio::fs::create_dir_all(&diffs_dir).await?;
    Ok(())
}

/// Save a diff file between two versions.
///
/// Retained for the library API / tests; the publish path now uses
/// [`save_diff_from_temp`] to avoid buffering the whole diff (FIX A).
#[allow(dead_code)]
pub async fn save_diff(
    data_dir: &Path,
    app_name: &str,
    from_version: &str,
    to_version: &str,
    platform: &Platform,
    data: &[u8],
) -> StorageResult<std::path::PathBuf> {
    ensure_diffs_dir(data_dir, app_name).await?;

    let diff_path = get_diff_path(data_dir, app_name, from_version, to_version, platform);
    tokio::fs::write(&diff_path, data).await?;

    tracing::info!(
        "Saved diff for {}: {} -> {} ({}): {} bytes",
        app_name,
        from_version,
        to_version,
        platform.as_str(),
        data.len()
    );

    Ok(diff_path)
}

/// Add diff info to the version manifest.
///
/// Uses file-level locking and atomic write operations to prevent race conditions
/// when multiple requests try to update the manifest simultaneously.
///
/// # Atomicity
///
/// The entire read-modify-write operation is performed under a single exclusive lock:
/// 1. Acquire exclusive file lock on manifest.json
/// 2. Read current manifest
/// 3. Add or update diff info
/// 4. Write new manifest to a temporary file
/// 5. Atomically rename temp file to manifest.json
/// 6. Release lock
pub async fn add_diff_to_manifest(
    data_dir: &Path,
    app_name: &str,
    to_version: &str,
    platform: &Platform,
    diff_info: crate::models::DiffInfo,
) -> StorageResult<()> {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let version_dir = data_dir
        .join("apps")
        .join(app_name)
        .join("versions")
        .join(to_version);
    let manifest_path = version_dir.join("manifest.json");
    let platform = *platform;
    let app_name_for_log = app_name.to_string();
    let to_version_for_log = to_version.to_string();
    let to_version_for_closure = to_version.to_string();

    // FIX G: serialize per-app via the shared async mutex (see update_manifest).
    let mutex = get_catalog_mutex(app_name);
    let _guard = mutex.lock().await;

    // Use blocking I/O for file locking since fs4 doesn't have async support
    tokio::task::spawn_blocking(move || -> StorageResult<()> {
        if !manifest_path.exists() {
            return Err(StorageError::NotFound(format!(
                "Manifest not found for version: {}",
                to_version_for_closure
            )));
        }

        // Open the manifest file with exclusive lock
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&manifest_path)?;

        // Acquire exclusive lock - this blocks until lock is available
        file.lock_exclusive()?;

        // Read current manifest while holding lock
        let content = std::fs::read_to_string(&manifest_path)?;
        let mut manifest: VersionManifest = serde_json::from_str(&content)?;

        // Get the platform artifact, or return error if platform not found
        let artifact = manifest.platforms.get_mut(&platform).ok_or_else(|| {
            StorageError::NotFound(format!(
                "Platform {} not found in manifest for version {}",
                platform.as_str(),
                to_version_for_closure
            ))
        })?;

        // Check if diff from this version already exists, update if so
        if let Some(existing) = artifact
            .diffs_from
            .iter_mut()
            .find(|d| d.from_version == diff_info.from_version)
        {
            *existing = diff_info;
        } else {
            artifact.diffs_from.push(diff_info);
        }

        // Write to a temporary file first for atomic operation
        let temp_path = manifest_path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(&temp_path, &content)?;

        // Atomically rename temp file to target (atomic on POSIX systems)
        std::fs::rename(&temp_path, &manifest_path)?;

        // Lock is released when file is dropped
        drop(file);

        Ok(())
    })
    .await
    .map_err(|e| StorageError::Io(std::io::Error::other(e)))??;

    tracing::info!(
        "Added diff info to manifest for {}/{} ({})",
        app_name_for_log,
        to_version_for_log,
        platform.as_str()
    );

    Ok(())
}

/// Get the path to the rollout config file for a specific version
pub fn get_rollout_path(data_dir: &Path, app_name: &str, version: &str) -> PathBuf {
    data_dir
        .join("apps")
        .join(app_name)
        .join("versions")
        .join(version)
        .join("rollout.json")
}

/// Get the rollout configuration for a specific version.
///
/// Uses shared file-level locking to ensure atomic reads when concurrent writes
/// may be happening via `update_rollout_config_atomic` or `set_rollout_config`.
pub async fn get_rollout_config(data_dir: &Path, app_name: &str, version: &str) -> Option<RolloutConfig> {
    let rollout_path = get_rollout_path(data_dir, app_name, version);

    if !rollout_path.exists() {
        return None;
    }

    // Use blocking I/O for file locking since fs4 doesn't have async support
    tokio::task::spawn_blocking(move || -> Option<RolloutConfig> {
        #[allow(unused_imports)] // FileExt provides lock_shared() method
        use fs4::fs_std::FileExt;
        use std::fs::OpenOptions;

        // Open file with shared (read) lock
        let file = OpenOptions::new()
            .read(true)
            .open(&rollout_path)
            .ok()?;

        // Acquire shared lock - multiple readers can hold this simultaneously,
        // but exclusive locks (from writers) will block until all readers release
        file.lock_shared().ok()?;

        // Read the content while holding the lock
        let content = std::fs::read_to_string(&rollout_path).ok()?;
        let config = serde_json::from_str(&content).ok();

        // Lock is released when file is dropped
        drop(file);

        config
    })
    .await
    .ok()
    .flatten()
}

/// Set the rollout configuration for a specific version.
///
/// Uses file-level locking and atomic write operations to prevent race conditions
/// when multiple requests try to update the rollout config simultaneously.
///
/// # Atomicity
///
/// The update is performed atomically using the following steps:
/// 1. Acquire exclusive file lock on rollout.json (or create if doesn't exist)
/// 2. Write new config to a temporary file (rollout.json.tmp)
/// 3. Atomically rename temp file to rollout.json
/// 4. Release lock
///
/// This ensures that concurrent updates don't corrupt the file or lose data.
///
/// # Note
///
/// For read-modify-write operations, prefer `update_rollout_config_atomic` which
/// performs the entire operation under a single lock.
// Kept for simple write-only callers (e.g., initial setup) where no read-modify-write is needed.
#[allow(dead_code)]
pub async fn set_rollout_config(
    data_dir: &Path,
    app_name: &str,
    version: &str,
    config: RolloutConfig,
) -> StorageResult<()> {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    // Ensure version directory exists
    ensure_version_dir(data_dir, app_name, version).await?;

    let rollout_path = get_rollout_path(data_dir, app_name, version);
    let config_clone = config.clone();

    // Use blocking I/O for file locking since fs4 doesn't have async support
    tokio::task::spawn_blocking(move || -> StorageResult<()> {
        // Create or open the rollout file with exclusive lock
        // Using create(true) ensures the file exists for locking
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&rollout_path)?;

        // Acquire exclusive lock - this blocks until lock is available
        // This prevents race conditions from concurrent read-modify-write operations
        file.lock_exclusive()?;

        // Write to a temporary file first for atomic operation
        let temp_path = rollout_path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(&config_clone)?;
        std::fs::write(&temp_path, &content)?;

        // Atomically rename temp file to target (atomic on POSIX systems)
        std::fs::rename(&temp_path, &rollout_path)?;

        // Lock is released when file is dropped
        drop(file);

        Ok(())
    })
    .await
    .map_err(|e| StorageError::Io(std::io::Error::other(e)))??;

    tracing::info!(
        "Updated rollout config for {}/{}: {}%",
        app_name,
        version,
        config.percentage
    );

    Ok(())
}

/// Atomically update the rollout configuration for a specific version.
///
/// This function performs a read-modify-write operation under a single exclusive lock,
/// preventing race conditions when multiple requests try to update the config simultaneously.
///
/// # Arguments
///
/// * `data_dir` - The data directory path
/// * `app_name` - The application name
/// * `version` - The version string
/// * `update_fn` - A closure that takes the current config (or default) and returns the updated config
///
/// # Atomicity
///
/// The entire read-modify-write operation is performed under a single exclusive lock:
/// 1. Acquire exclusive file lock on rollout.json
/// 2. Read current config (or use default if file doesn't exist)
/// 3. Apply the update function to get the new config
/// 4. Write new config to a temporary file
/// 5. Atomically rename temp file to rollout.json
/// 6. Release lock
///
/// This ensures that concurrent updates don't lose data due to the read-modify-write race.
pub async fn update_rollout_config_atomic<F>(
    data_dir: &Path,
    app_name: &str,
    version: &str,
    update_fn: F,
) -> StorageResult<RolloutConfig>
where
    F: FnOnce(RolloutConfig) -> RolloutConfig + Send + 'static,
{
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    // Ensure version directory exists
    ensure_version_dir(data_dir, app_name, version).await?;

    let rollout_path = get_rollout_path(data_dir, app_name, version);

    // Use blocking I/O for file locking since fs4 doesn't have async support
    let result = tokio::task::spawn_blocking(move || -> StorageResult<RolloutConfig> {
        // Create or open the rollout file with exclusive lock
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&rollout_path)?;

        // Acquire exclusive lock - this blocks until lock is available
        file.lock_exclusive()?;

        // Read current config while holding lock (or use default)
        let current_config = if rollout_path.exists() {
            std::fs::read_to_string(&rollout_path)
                .ok()
                .and_then(|content| serde_json::from_str(&content).ok())
                .unwrap_or_default()
        } else {
            RolloutConfig::default()
        };

        // Apply the update function
        let new_config = update_fn(current_config);

        // Write to a temporary file first for atomic operation
        let temp_path = rollout_path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(&new_config)?;
        std::fs::write(&temp_path, &content)?;

        // Atomically rename temp file to target (atomic on POSIX systems)
        std::fs::rename(&temp_path, &rollout_path)?;

        // Lock is released when file is dropped
        drop(file);

        Ok(new_config)
    })
    .await
    .map_err(|e| StorageError::Io(std::io::Error::other(e)))??;

    tracing::info!(
        "Atomically updated rollout config for {}/{}: {}%",
        app_name,
        version,
        result.percentage
    );

    Ok(result)
}

// ============== Admin Storage Functions ==============

/// List all registered applications
pub fn list_apps(data_dir: &Path) -> StorageResult<Vec<AppListItem>> {
    let apps_dir = data_dir.join("apps");
    if !apps_dir.exists() {
        return Ok(Vec::new());
    }

    let mut apps = Vec::new();

    for entry in std::fs::read_dir(&apps_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let app_name = entry.file_name().to_string_lossy().to_string();

            // Try to read catalog for this app
            let catalog_path = path.join("catalog.json");
            let (versions_count, latest_version) = if catalog_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&catalog_path) {
                    if let Ok(catalog) = serde_json::from_str::<AppCatalog>(&content) {
                        (catalog.versions.len(), catalog.latest_version.to_string())
                    } else {
                        (0, "unknown".to_string())
                    }
                } else {
                    (0, "unknown".to_string())
                }
            } else {
                // Count versions from directory
                let versions_dir = path.join("versions");
                let count = if versions_dir.exists() {
                    std::fs::read_dir(&versions_dir)
                        .map(|e| {
                            e.filter_map(|x| x.ok())
                                .filter(|x| x.path().is_dir())
                                .count()
                        })
                        .unwrap_or(0)
                } else {
                    0
                };
                (count, "unknown".to_string())
            };

            apps.push(AppListItem {
                name: app_name,
                versions_count,
                latest_version,
            });
        }
    }

    // Sort by name
    apps.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(apps)
}

/// Get detailed information about a specific application
pub fn get_app_info(data_dir: &Path, app_name: &str) -> StorageResult<AppDetails> {
    let app_dir = data_dir.join("apps").join(app_name);

    if !app_dir.exists() {
        return Err(StorageError::NotFound(format!(
            "Application not found: {}",
            app_name
        )));
    }

    // Get versions from catalog or directory
    let catalog_path = app_dir.join("catalog.json");
    let (versions, platforms_from_catalog) = if catalog_path.exists() {
        let content = std::fs::read_to_string(&catalog_path)?;
        let catalog: AppCatalog = serde_json::from_str(&content)?;
        let versions: Vec<String> = catalog
            .versions
            .iter()
            .map(|v| v.version.to_string())
            .collect();
        let platforms: std::collections::HashSet<String> = catalog
            .versions
            .iter()
            .flat_map(|v| v.platforms.iter().map(|p| p.as_str().to_string()))
            .collect();
        (versions, platforms.into_iter().collect::<Vec<_>>())
    } else {
        // Fall back to reading versions directory
        let versions_dir = app_dir.join("versions");
        let versions = if versions_dir.exists() {
            std::fs::read_dir(&versions_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        } else {
            Vec::new()
        };
        (versions, Vec::new())
    };

    // Calculate total size
    let total_size_bytes = calculate_app_size(&app_dir);

    Ok(AppDetails {
        name: app_name.to_string(),
        versions,
        total_size_bytes,
        platforms: platforms_from_catalog,
    })
}

/// Calculate total size of an app directory
fn calculate_app_size(app_dir: &Path) -> u64 {
    let mut size: u64 = 0;

    if let Ok(entries) = std::fs::read_dir(app_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Use symlink_metadata to avoid following symlinks (security)
            if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                // FIX F: saturating arithmetic to avoid overflow on hostile sizes.
                if metadata.is_file() {
                    size = size.saturating_add(metadata.len());
                } else if metadata.is_dir() {
                    size = size.saturating_add(calculate_app_size(&path));
                }
                // Symlinks are ignored for size calculation
            }
        }
    }

    size
}

/// Get list of versions for an application with details
pub fn list_versions(data_dir: &Path, app_name: &str) -> StorageResult<Vec<VersionListItem>> {
    let app_dir = data_dir.join("apps").join(app_name);

    if !app_dir.exists() {
        return Err(StorageError::NotFound(format!(
            "Application not found: {}",
            app_name
        )));
    }

    let versions_dir = app_dir.join("versions");
    if !versions_dir.exists() {
        return Ok(Vec::new());
    }

    // Read catalog for version info
    let catalog_path = app_dir.join("catalog.json");
    let catalog: Option<AppCatalog> = if catalog_path.exists() {
        std::fs::read_to_string(&catalog_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
    } else {
        None
    };

    let mut versions = Vec::new();

    for entry in std::fs::read_dir(&versions_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let version_str = entry.file_name().to_string_lossy().to_string();

            // Check if deleted
            let deleted_marker = path.join(".deleted");
            let deleted = deleted_marker.exists();

            // Get manifest for size and platforms
            let manifest_path = path.join("manifest.json");
            let (size_bytes, platforms) = if manifest_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                    if let Ok(manifest) = serde_json::from_str::<VersionManifest>(&content) {
                        // FIX F: saturating fold over attacker-controlled manifest
                        // sizes instead of `.sum()` (which panics in debug / wraps).
                        let size: u64 = manifest
                            .platforms
                            .values()
                            .fold(0u64, |acc, p| acc.saturating_add(p.size));
                        let plats: Vec<String> = manifest
                            .platforms
                            .keys()
                            .map(|p| p.as_str().to_string())
                            .collect();
                        (size, plats)
                    } else {
                        (0, Vec::new())
                    }
                } else {
                    (0, Vec::new())
                }
            } else {
                (0, Vec::new())
            };

            // Get rollout percentage from catalog or rollout config
            let rollout_percentage = if let Some(ref cat) = catalog {
                cat.versions
                    .iter()
                    .find(|v| v.version.to_string() == version_str)
                    .and_then(|v| v.rollout.as_ref())
                    .map(|r| r.percentage)
                    .unwrap_or(100)
            } else {
                // Check rollout.json
                let rollout_path = path.join("rollout.json");
                if rollout_path.exists() {
                    std::fs::read_to_string(&rollout_path)
                        .ok()
                        .and_then(|c| serde_json::from_str::<RolloutConfig>(&c).ok())
                        .map(|r| r.percentage)
                        .unwrap_or(100)
                } else {
                    100
                }
            };

            // published_at is not tracked for MVP - would require additional metadata storage
            let published_at = None;

            versions.push(VersionListItem {
                version: version_str,
                size_bytes,
                platforms,
                rollout_percentage,
                deleted,
                published_at,
            });
        }
    }

    // Sort by version (descending)
    versions.sort_by(|a, b| b.version.cmp(&a.version));

    Ok(versions)
}

/// Soft-delete a version (mark as unavailable without removing files)
///
/// Uses file-level locking and atomic operations to prevent race conditions.
#[tracing::instrument(skip(data_dir), fields(app = %app_name, version = %version))]
pub async fn delete_version(data_dir: &Path, app_name: &str, version: &str) -> StorageResult<()> {
    use fs4::fs_std::FileExt;
    use std::fs::OpenOptions;

    let version_dir = data_dir
        .join("apps")
        .join(app_name)
        .join("versions")
        .join(version);

    if !version_dir.exists() {
        return Err(StorageError::NotFound(format!(
            "Version not found: {}/{}",
            app_name, version
        )));
    }

    // Create .deleted marker file
    let deleted_marker = version_dir.join(".deleted");
    tokio::fs::write(&deleted_marker, "").await?;

    // Also remove from catalog's latest_version if it matches
    let catalog_path = data_dir.join("apps").join(app_name).join("catalog.json");
    if catalog_path.exists() {
        // Use file locking to prevent race conditions
        // We need to use blocking I/O for file locking, so spawn_blocking
        let catalog_path_clone = catalog_path.clone();
        let version_clone = version.to_string();
        let versions_dir = data_dir.join("apps").join(app_name).join("versions");

        tokio::task::spawn_blocking(move || -> StorageResult<()> {
            // Open catalog file with exclusive lock for atomic read-modify-write
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&catalog_path_clone)?;

            // Acquire exclusive lock - this blocks until lock is available
            file.lock_exclusive()?;

            // Read catalog while holding lock
            let content = std::fs::read_to_string(&catalog_path_clone)?;
            let mut catalog: AppCatalog = serde_json::from_str(&content)?;

            // If deleted version is the latest, find the next available version
            if catalog.latest_version.to_string() == version_clone {
                // Find next non-deleted version by checking the catalog's version list
                // and only checking filesystem for .deleted markers
                // Collect version strings from catalog first to minimize filesystem access
                let catalog_versions: std::collections::HashSet<String> = catalog
                    .versions
                    .iter()
                    .map(|v| v.version.to_string())
                    .collect();

                let mut available_versions: Vec<String> = catalog_versions
                    .iter()
                    .filter(|v| {
                        if **v == version_clone {
                            return false;
                        }
                        // Only check filesystem if not in catalog (edge case)
                        // Prefer catalog data to avoid filesystem race conditions
                        true
                    })
                    .cloned()
                    .collect();

                // Remove versions with .deleted marker (filesystem check only as fallback)
                available_versions.retain(|v| {
                    let v_dir = versions_dir.join(v);
                    !v_dir.join(".deleted").exists()
                });

                available_versions.sort_by(|a, b| b.cmp(a)); // Sort descending

                if let Some(new_latest) = available_versions.first() {
                    if let Ok(parsed) = new_latest.parse() {
                        catalog.latest_version = parsed;
                    }
                }
            }

            // Write back atomically - write to temp file then rename
            let temp_path = catalog_path_clone.with_extension("json.tmp");
            let content = serde_json::to_string_pretty(&catalog)?;
            std::fs::write(&temp_path, &content)?;
            std::fs::rename(&temp_path, &catalog_path_clone)?;

            // Lock is released when file is dropped
            drop(file);

            Ok(())
        })
        .await
        .map_err(|e| StorageError::Io(std::io::Error::other(e)))??;
    }

    tracing::info!("Soft-deleted version: {}/{}", app_name, version);

    Ok(())
}
