//! Integration tests for the update check and health endpoints.
//!
//! Each test creates a temporary directory, writes the catalog / rollout /
//! manifest files the server expects, builds an axum router backed by that
//! directory, and fires requests through it using `tower::ServiceExt::oneshot`.
//!
//! # Platform string note
//!
//! `deltaship_core::Platform` serialises with `#[serde(rename_all = "kebab-case")]`,
//! so `LinuxX86_64` → `"linux-x86-64"` on disk and in URL query params when
//! going through Platform::from_str (which accepts both hyphen and underscore
//! forms). The catalog JSON files therefore use the kebab-case strings.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::{body::Body, http::{Request, StatusCode}, Router};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt; // provides `.oneshot()`

use deltaship_server::{routes, state::AppState};

// ─── router builder ─────────────────────────────────────────────────────────

/// Build a minimal test router: health + update-check routes, no auth / rate-limiting.
fn build_router(data_dir: std::path::PathBuf) -> Router {
    let state = Arc::new(AppState::new(data_dir));
    Router::new()
        .merge(routes::health::router())
        .merge(routes::update::router())
        .merge(routes::download::router())
        .with_state(state)
}

// ─── fixture helpers ─────────────────────────────────────────────────────────

/// Write `apps/<app_name>/catalog.json`.
///
/// `versions` entries: `(version_str, platform_serde_strings, forced)`.
/// Use the serde-serialised platform form: `"linux-x86-64"`, `"windows-x86-64"`, etc.
fn write_catalog(
    root: &Path,
    app_name: &str,
    latest_version: &str,
    versions: &[(&str, &[&str], bool)],
) {
    let app_dir = root.join("apps").join(app_name);
    std::fs::create_dir_all(&app_dir).unwrap();

    let versions_json: Vec<Value> = versions
        .iter()
        .map(|(ver, plats, forced)| {
            json!({
                "version": ver,
                "platforms": plats,
                "forced": forced,
                "release_notes": null
            })
        })
        .collect();

    let catalog = json!({
        "app_name": app_name,
        "latest_version": latest_version,
        "versions": versions_json
    });

    std::fs::write(
        app_dir.join("catalog.json"),
        serde_json::to_string_pretty(&catalog).unwrap(),
    )
    .unwrap();
}

/// Write `apps/<app_name>/versions/<version>/rollout.json`.
fn write_rollout(root: &Path, app_name: &str, version: &str, percentage: u8, enabled: bool) {
    let dir = root
        .join("apps")
        .join(app_name)
        .join("versions")
        .join(version);
    std::fs::create_dir_all(&dir).unwrap();

    let rollout = json!({
        "percentage": percentage,
        "enabled": enabled,
        "started_at": null,
        "completed_at": null
    });

    std::fs::write(
        dir.join("rollout.json"),
        serde_json::to_string_pretty(&rollout).unwrap(),
    )
    .unwrap();
}

/// Write `apps/<app_name>/versions/<version>/manifest.json`.
///
/// `artifacts` entries: `(platform_serde_str, checksum, size_bytes)`.
fn write_manifest(
    root: &Path,
    app_name: &str,
    version: &str,
    artifacts: &[(&str, &str, u64)],
) {
    let dir = root
        .join("apps")
        .join(app_name)
        .join("versions")
        .join(version);
    std::fs::create_dir_all(&dir).unwrap();

    let mut platforms_map: HashMap<&str, Value> = HashMap::new();
    for (platform, checksum, size) in artifacts {
        platforms_map.insert(
            platform,
            json!({
                "checksum": checksum,
                "size": size,
                "diffs_from": []
            }),
        );
    }

    let manifest = json!({
        "version": version,
        "platforms": platforms_map
    });

    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

/// Fire a GET request at the update-check endpoint and return `(StatusCode, body_json)`.
async fn check_update(
    router: &Router,
    app_name: &str,
    current_version: &str,
    platform: &str,
    device_id_hash: Option<&str>,
) -> (StatusCode, Value) {
    let mut url = format!(
        "/api/v1/apps/{}/check-update?current_version={}&platform={}",
        app_name, current_version, platform
    );
    if let Some(hash) = device_id_hash {
        url.push_str(&format!("&device_id_hash={}", hash));
    }

    let req = Request::builder()
        .method("GET")
        .uri(&url)
        .body(Body::empty())
        .unwrap();

    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

// ─── tests ───────────────────────────────────────────────────────────────────

// ── health ──

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let dir = TempDir::new().unwrap();
    let router = build_router(dir.path().to_path_buf());

    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["status"], "ok");
}

// ── no update ──

#[tokio::test]
async fn update_check_no_update_when_client_is_on_latest() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "1.2.3",
        &[("1.2.3", &["linux-x86-64"], false)],
    );

    let router = build_router(dir.path().to_path_buf());
    // Platform::from_str accepts "linux-x86_64" and "linux-x86-64"
    let (status, body) =
        check_update(&router, "myapp", "1.2.3", "linux-x86_64", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], false);
    assert!(body["full_binary_url"].is_null());
}

#[tokio::test]
async fn update_check_no_update_when_client_is_ahead_of_latest() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "1.0.0",
        &[("1.0.0", &["linux-x86-64"], false)],
    );

    let router = build_router(dir.path().to_path_buf());
    let (status, body) =
        check_update(&router, "myapp", "2.0.0", "linux-x86_64", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], false);
}

// ── update available ──

#[tokio::test]
async fn update_check_update_available_returns_urls() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "2.0.0",
        &[
            ("1.0.0", &["linux-x86-64"], false),
            ("2.0.0", &["linux-x86-64"], false),
        ],
    );

    let router = build_router(dir.path().to_path_buf());
    let (status, body) =
        check_update(&router, "myapp", "1.0.0", "linux-x86_64", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], true);
    assert_eq!(body["target_version"], "2.0.0");

    // Both binary and signature URLs must be present and reference the new version
    let binary_url = body["full_binary_url"].as_str().expect("full_binary_url missing");
    assert!(binary_url.contains("2.0.0"), "binary URL should contain version");

    let sig_url = body["signature_url"].as_str().expect("signature_url missing");
    assert!(sig_url.contains("2.0.0"), "signature URL should contain version");
}

#[tokio::test]
async fn update_check_update_available_includes_checksum_from_manifest() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "2.0.0",
        &[
            ("1.0.0", &["linux-x86-64"], false),
            ("2.0.0", &["linux-x86-64"], false),
        ],
    );
    write_manifest(dir.path(), "myapp", "2.0.0", &[("linux-x86-64", "deadbeef01", 4096)]);

    let router = build_router(dir.path().to_path_buf());
    let (status, body) =
        check_update(&router, "myapp", "1.0.0", "linux-x86_64", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], true);
    assert_eq!(body["checksum"], "deadbeef01");
}

#[tokio::test]
async fn update_check_forced_update_flag_propagates() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "2.0.0",
        &[
            ("1.0.0", &["linux-x86-64"], false),
            ("2.0.0", &["linux-x86-64"], true), // forced
        ],
    );

    let router = build_router(dir.path().to_path_buf());
    let (status, body) =
        check_update(&router, "myapp", "1.0.0", "linux-x86_64", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], true);
    assert_eq!(body["forced"], true);
}

// ── 404 app not found ──

#[tokio::test]
async fn update_check_app_not_found_returns_404() {
    let dir = TempDir::new().unwrap();
    let router = build_router(dir.path().to_path_buf());

    let (status, _) =
        check_update(&router, "nonexistent-app", "1.0.0", "linux-x86_64", None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── 400 bad request ──

#[tokio::test]
async fn update_check_invalid_app_name_path_traversal_returns_400() {
    let dir = TempDir::new().unwrap();
    let router = build_router(dir.path().to_path_buf());

    // "..dangerous" contains ".." — should be rejected by validate_name
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/apps/..dangerous/check-update?current_version=1.0.0&platform=linux-x86_64")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_check_invalid_app_name_special_chars_returns_400() {
    let dir = TempDir::new().unwrap();
    let router = build_router(dir.path().to_path_buf());

    // "@" is not in [a-zA-Z0-9._-]
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/apps/bad@name/check-update?current_version=1.0.0&platform=linux-x86_64")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_check_invalid_platform_returns_400() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "2.0.0",
        &[("2.0.0", &["linux-x86-64"], false)],
    );

    let router = build_router(dir.path().to_path_buf());
    let (status, _) =
        check_update(&router, "myapp", "1.0.0", "amiga-500", None).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_check_invalid_version_returns_400() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "2.0.0",
        &[("2.0.0", &["linux-x86-64"], false)],
    );

    let router = build_router(dir.path().to_path_buf());
    let (status, _) =
        check_update(&router, "myapp", "not-a-version", "linux-x86_64", None).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ── platform not supported by latest version ──

#[tokio::test]
async fn update_check_platform_not_in_latest_returns_no_update() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "2.0.0",
        &[
            ("1.0.0", &["linux-x86-64", "windows-x86-64"], false),
            ("2.0.0", &["linux-x86-64"], false), // no windows build for 2.0.0
        ],
    );

    let router = build_router(dir.path().to_path_buf());
    let (status, body) =
        check_update(&router, "myapp", "1.0.0", "windows-x86_64", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], false);
}

// ── rollout ──

/// `device-7` hashes to bucket 55 (>= 50), so it is excluded from a 50% rollout.
#[tokio::test]
async fn update_check_rollout_excluded_device_gets_not_in_rollout_response() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "2.0.0",
        &[
            ("1.0.0", &["linux-x86-64"], false),
            ("2.0.0", &["linux-x86-64"], false),
        ],
    );
    write_rollout(dir.path(), "myapp", "2.0.0", 50, true);

    let router = build_router(dir.path().to_path_buf());
    let (status, body) =
        check_update(&router, "myapp", "1.0.0", "linux-x86_64", Some("device-7")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], false, "excluded device must not get update");
    assert_eq!(body["in_rollout"], false, "in_rollout must be false for excluded device");
    assert_eq!(body["rollout_percentage"], 50);
    // target_version present so client knows an update exists (just not for it yet)
    assert_eq!(body["target_version"], "2.0.0");
}

/// `device-0` hashes to bucket 33 (< 50), so it IS included in a 50% rollout.
#[tokio::test]
async fn update_check_rollout_included_device_gets_update() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "2.0.0",
        &[
            ("1.0.0", &["linux-x86-64"], false),
            ("2.0.0", &["linux-x86-64"], false),
        ],
    );
    write_rollout(dir.path(), "myapp", "2.0.0", 50, true);

    let router = build_router(dir.path().to_path_buf());
    let (status, body) =
        check_update(&router, "myapp", "1.0.0", "linux-x86_64", Some("device-0")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], true, "included device must get update");
    assert_eq!(body["target_version"], "2.0.0");
    assert_eq!(body["in_rollout"], true);
    assert_eq!(body["rollout_percentage"], 50);
}

#[tokio::test]
async fn update_check_rollout_100_percent_all_devices_get_update() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "2.0.0",
        &[
            ("1.0.0", &["linux-x86-64"], false),
            ("2.0.0", &["linux-x86-64"], false),
        ],
    );
    // 100% rollout — everyone in, regardless of device hash
    write_rollout(dir.path(), "myapp", "2.0.0", 100, true);

    let router = build_router(dir.path().to_path_buf());
    // Use device-7 which was excluded at 50% — must be included at 100%
    let (status, body) =
        check_update(&router, "myapp", "1.0.0", "linux-x86_64", Some("device-7")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], true);
}

#[tokio::test]
async fn update_check_rollout_0_percent_no_device_gets_update() {
    let dir = TempDir::new().unwrap();
    write_catalog(
        dir.path(),
        "myapp",
        "2.0.0",
        &[
            ("1.0.0", &["linux-x86-64"], false),
            ("2.0.0", &["linux-x86-64"], false),
        ],
    );
    // 0% rollout — no one gets it
    write_rollout(dir.path(), "myapp", "2.0.0", 0, true);

    let router = build_router(dir.path().to_path_buf());
    // Use device-0 which was included at 50% — must be excluded at 0%
    let (status, body) =
        check_update(&router, "myapp", "1.0.0", "linux-x86_64", Some("device-0")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["update_available"], false);
    assert_eq!(body["in_rollout"], false);
}

// ─── unit tests for validate_name() via the real validation module ──────────

#[cfg(test)]
mod validation_unit_tests {
    use deltaship_server::validation::{validate_name, ValidationError};

    #[test]
    fn valid_names_are_accepted() {
        for name in &["my-app", "my_app", "my.app", "MyApp123", "a", "app.v2_beta"] {
            assert!(validate_name(name).is_ok(), "expected ok for {:?}", name);
        }
    }

    #[test]
    fn empty_name_is_rejected() {
        assert_eq!(validate_name(""), Err(ValidationError::Empty));
    }

    #[test]
    fn path_traversal_is_rejected() {
        assert_eq!(validate_name("../etc"), Err(ValidationError::PathTraversal));
        assert_eq!(validate_name("foo/bar"), Err(ValidationError::PathTraversal));
        assert_eq!(
            validate_name("foo\\bar"),
            Err(ValidationError::PathTraversal)
        );
        assert_eq!(
            validate_name(".."),
            Err(ValidationError::PathTraversal)
        );
    }

    #[test]
    fn null_byte_is_rejected() {
        assert_eq!(validate_name("foo\0bar"), Err(ValidationError::NullByte));
    }

    #[test]
    fn invalid_characters_are_rejected() {
        assert_eq!(
            validate_name("foo bar"),
            Err(ValidationError::InvalidCharacters)
        );
        assert_eq!(
            validate_name("foo@bar"),
            Err(ValidationError::InvalidCharacters)
        );
        assert_eq!(
            validate_name("foo!"),
            Err(ValidationError::InvalidCharacters)
        );
        assert_eq!(
            validate_name("foo#bar"),
            Err(ValidationError::InvalidCharacters)
        );
    }

    #[test]
    fn name_at_max_length_255_is_accepted() {
        let max = "a".repeat(255);
        assert!(validate_name(&max).is_ok());
    }

    #[test]
    fn name_exceeding_255_chars_is_rejected() {
        let too_long = "a".repeat(256);
        assert_eq!(validate_name(&too_long), Err(ValidationError::TooLong));
    }
}
