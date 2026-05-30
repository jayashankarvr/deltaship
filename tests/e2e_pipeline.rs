//! End-to-end pipeline test (G3 + G7)
//!
//! Real integration test that exercises the full Deltaship pipeline:
//!
//!   1. Spawn `deltaship-server` on a random free TCP port.
//!   2. Initialize a publisher workspace + signing key.
//!   3. Build a tiny `myapp` v1 binary, register/sign/publish it.
//!   4. Spawn `deltaship-updater` -- assert exit code 2 (first install)
//!      and verify the installed binary's BLAKE3 matches the published checksum.
//!   5. Build `myapp` v2, register/sign/publish.
//!   6. Spawn `deltaship-updater` again -- assert exit code 2 (updated)
//!      and verify the install path now contains v2.
//!
//! Run with: `cargo test --test e2e_pipeline -p deltaship-integration-tests -- --ignored`
//!
//! This test is `#[ignore]` because it spawns multiple subprocesses, builds
//! Rust binaries with `rustc`, and takes ~10-20 seconds even on a fast box.
//! It is too slow for the default `cargo test` loop but is reproducible and
//! self-contained.
//!
//! The test relies on the workspace having been built (so the
//! `deltaship-server`, `deltaship-publisher`, and `deltaship-updater` binaries exist under
//! `target/debug/` or `target/release/`). It does NOT call `cargo build`
//! itself; the assumption is the developer ran `cargo build` (or the test
//! harness did) before invoking it. This keeps the test focused on behavior
//! rather than re-doing what cargo would already do for a co-located test.

use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

const PASSPHRASE: &str = "e2e-pipeline-test-passphrase";
const PLATFORM: &str = "linux-x86_64";
const APP_NAME: &str = "myapp";

/// Locate the workspace root (directory containing the top-level `Cargo.toml`
/// with `[workspace]`). Walks up from `CARGO_MANIFEST_DIR`.
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut cur: &Path = &manifest;
    loop {
        let candidate = cur.join("Cargo.toml");
        if candidate.exists() {
            let contents = std::fs::read_to_string(&candidate).unwrap_or_default();
            if contents.contains("[workspace]") {
                return cur.to_path_buf();
            }
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => panic!("could not find workspace root from {}", manifest.display()),
        }
    }
}

/// Resolve a binary by name, preferring debug over release. Falls back to
/// `cargo build -p <pkg>` if neither is found, but normally the developer or
/// the harness has already built the workspace.
fn locate_binary(name: &str) -> PathBuf {
    let root = workspace_root();
    let debug = root.join("target").join("debug").join(name);
    if debug.exists() {
        return debug;
    }
    let release = root.join("target").join("release").join(name);
    if release.exists() {
        return release;
    }
    panic!(
        "binary `{}` not found under target/debug or target/release. \
         Run `cargo build` in the workspace root before running this test.",
        name
    );
}

/// Bind a TCP listener on a random port, read the port, and drop the listener
/// so the OS frees it for the server to bind to. There is a tiny TOCTOU window
/// here, but it is acceptable for a test.
fn pick_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let port = listener.local_addr().expect("local_addr").port();
    drop(listener);
    port
}

/// RAII guard that kills the server child process on drop.
struct ServerGuard {
    child: Option<Child>,
    port: u16,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl ServerGuard {
    /// Start the server and wait for `/health` to return 200. Bails after
    /// `timeout` with a panic that includes captured stderr where possible.
    fn start(server_bin: &Path, data_dir: &Path, port: u16, timeout: Duration) -> Self {
        let mut cmd = Command::new(server_bin);
        cmd.arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--data-dir")
            .arg(data_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let child = cmd.spawn().expect("spawn deltaship-server");
        let mut guard = ServerGuard {
            child: Some(child),
            port,
        };

        let url = format!("http://127.0.0.1:{}/health", port);
        let deadline = Instant::now() + timeout;
        let agent = ureq_like_get;
        loop {
            if Instant::now() >= deadline {
                guard.kill_with_diag("server health check timed out");
            }

            // Server may not be bound yet; ignore connection errors and retry.
            match agent(&url) {
                Ok(()) => return guard,
                Err(_) => {
                    // Has the child died? Surface its exit + stderr if so.
                    if let Some(child) = guard.child.as_mut() {
                        if let Ok(Some(status)) = child.try_wait() {
                            let mut buf = String::new();
                            if let Some(mut stderr) = child.stderr.take() {
                                use std::io::Read;
                                let _ = stderr.read_to_string(&mut buf);
                            }
                            panic!(
                                "deltaship-server exited early with {:?}; stderr:\n{}",
                                status, buf
                            );
                        }
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }

    fn kill_with_diag(&mut self, msg: &str) -> ! {
        let mut buf = String::new();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(mut stderr) = child.stderr.take() {
                use std::io::Read;
                let _ = stderr.read_to_string(&mut buf);
            }
        }
        panic!("{} (port={}); server stderr:\n{}", msg, self.port, buf);
    }
}

/// Tiny HTTP GET that succeeds on 2xx. Avoids an extra dependency by speaking
/// HTTP/1.1 directly. Returns Ok(()) on 2xx, Err otherwise.
fn ureq_like_get(url: &str) -> Result<(), String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    // Parse very simple URL: http://host:port/path
    let rest = url.strip_prefix("http://").ok_or("expected http://")?;
    let slash = rest.find('/').ok_or("expected path")?;
    let host_port = &rest[..slash];
    let path = &rest[slash..];
    let mut stream = TcpStream::connect_timeout(
        &host_port.parse().map_err(|e: std::net::AddrParseError| e.to_string())?,
        Duration::from_millis(500),
    )
    .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(1000)))
        .map_err(|e| e.to_string())?;
    write!(
        stream,
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host_port
    )
    .map_err(|e| e.to_string())?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    let status_line = buf.lines().next().ok_or("empty response")?;
    if status_line.contains(" 2") {
        Ok(())
    } else {
        Err(format!("non-2xx: {}", status_line))
    }
}

/// Run a command and return (stdout, stderr) on success. On failure, panic
/// with the captured output so debugging the test is feasible.
fn run_ok(cmd: &mut Command, label: &str) -> (String, String) {
    let output = cmd.output().unwrap_or_else(|e| panic!("spawn {}: {}", label, e));
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        panic!(
            "{} failed: status={:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            label, output.status, stdout, stderr
        );
    }
    (stdout, stderr)
}

/// Compile a tiny Rust source string into an executable at `out`. Uses `rustc`
/// directly to avoid the cost of spawning cargo for a one-file program.
fn compile_myapp(src: &str, out: &Path) {
    let dir = out.parent().expect("out has parent");
    std::fs::create_dir_all(dir).expect("mkdir out parent");
    let src_path = dir.join(format!(
        "{}.rs",
        out.file_name().unwrap().to_string_lossy()
    ));
    let mut f = std::fs::File::create(&src_path).expect("write myapp src");
    f.write_all(src.as_bytes()).expect("write myapp src bytes");
    drop(f);

    let mut cmd = Command::new("rustc");
    cmd.arg(&src_path)
        .arg("-O")
        .arg("--edition=2021")
        .arg("-o")
        .arg(out);
    run_ok(&mut cmd, "rustc compile myapp");
    assert!(out.exists(), "rustc did not produce {}", out.display());
}

/// BLAKE3-hash a file's contents.
fn blake3_file(path: &Path) -> [u8; 32] {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    *blake3::hash(&bytes).as_bytes()
}

#[test]
#[ignore]
fn e2e_pipeline_full() {
    let started = Instant::now();

    let server_bin = locate_binary("deltaship-server");
    let publisher_bin = locate_binary("deltaship-publisher");
    let updater_bin = locate_binary("deltaship-updater");

    // ---------- workspace layout ----------
    let work = TempDir::new().expect("tempdir");
    // Keep the temp dir on failure for post-mortem; we manually remove on success.
    let work = work.keep();
    let server_data = work.join("server-data");
    let publisher_dir = work.join("publisher");
    let myapp_src_dir = work.join("myapp-src");
    let bundle_dir = work.join("bundle");
    let myapp_v1_path = myapp_src_dir.join("myapp-v1");
    let myapp_v2_path = myapp_src_dir.join("myapp-v2");
    let install_path = bundle_dir.join("myapp");
    let updater_state = bundle_dir.join(".updater-state");
    let public_key_dst = bundle_dir.join("publisher.pub");

    std::fs::create_dir_all(&server_data).unwrap();
    std::fs::create_dir_all(&publisher_dir).unwrap();
    std::fs::create_dir_all(&myapp_src_dir).unwrap();
    std::fs::create_dir_all(&bundle_dir).unwrap();
    std::fs::create_dir_all(&updater_state).unwrap();

    // ---------- API key: generate, hash, write to api_keys.txt ----------
    let api_key = {
        let out = Command::new(&server_bin)
            .arg("--generate-api-key")
            .output()
            .expect("generate-api-key");
        assert!(out.status.success(), "generate-api-key failed: {:?}", out);
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    assert_eq!(api_key.len(), 64, "expected 64-char hex API key");

    let hashed = {
        let out = Command::new(&server_bin)
            .arg("hash-key")
            .arg(&api_key)
            .output()
            .expect("hash-key");
        assert!(out.status.success(), "hash-key failed: {:?}", out);
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    let api_keys_file = server_data.join("api_keys.txt");
    std::fs::write(&api_keys_file, format!("{}\n", hashed)).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&api_keys_file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    // ---------- start server ----------
    let port = pick_free_port();
    let _server = ServerGuard::start(&server_bin, &server_data, port, Duration::from_secs(10));
    let server_url = format!("http://127.0.0.1:{}", port);

    // ---------- init publisher ----------
    run_ok(
        Command::new(&publisher_bin)
            .current_dir(&publisher_dir)
            .arg("init")
            .arg("--passphrase")
            .arg(PASSPHRASE),
        "publisher init",
    );

    // We pass `--server-url` and `--yes` to every `publisher publish` invocation
    // below, so we skip persisting the server URL in the publisher config.
    // (The config key is `server_url`, not `server.url` as the quickstart shows --
    // a small docs nit, not a blocker for this test.)

    // Copy public key into bundle directory (clients consume it via --public-key).
    let pubkey_src = publisher_dir.join(".deltaship/keys/public.key");
    assert!(pubkey_src.exists(), "publisher public.key missing");
    std::fs::copy(&pubkey_src, &public_key_dst).expect("copy public key");

    // ============================================================
    // v1 -- build, register, sign, publish, install via updater
    // ============================================================
    compile_myapp(
        "fn main() { println!(\"hello v1\"); }\n",
        &myapp_v1_path,
    );
    let v1_hash = blake3_file(&myapp_v1_path);

    run_ok(
        Command::new(&publisher_bin)
            .current_dir(&publisher_dir)
            .args([
                "register",
                "--name",
                APP_NAME,
                "--version",
                "1.0.0",
                "--platform",
                PLATFORM,
                "--description",
                "e2e v1",
                "--file",
            ])
            .arg(&myapp_v1_path),
        "publisher register v1",
    );

    run_ok(
        Command::new(&publisher_bin)
            .current_dir(&publisher_dir)
            .args([
                "sign",
                "--name",
                APP_NAME,
                "--version",
                "1.0.0",
                "--passphrase",
                PASSPHRASE,
            ]),
        "publisher sign v1",
    );

    run_ok(
        Command::new(&publisher_bin)
            .current_dir(&publisher_dir)
            .args([
                "publish",
                "--name",
                APP_NAME,
                "--version",
                "1.0.0",
                "--server-url",
                &server_url,
                "--api-key",
                &api_key,
                "--yes",
            ]),
        "publisher publish v1",
    );

    // ---------- updater: first invocation, expect exit code 2 (first install) ----------
    let out = Command::new(&updater_bin)
        .args(["--name", APP_NAME, "--server-url", &server_url])
        .arg("--install-path")
        .arg(&install_path)
        .arg("--public-key")
        .arg(&public_key_dst)
        .arg("--data-dir")
        .arg(&updater_state)
        .output()
        .expect("spawn updater v1");
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 on first install; stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(install_path.exists(), "install_path was not created");
    let installed_v1_hash = blake3_file(&install_path);
    assert_eq!(
        installed_v1_hash, v1_hash,
        "installed v1 hash does not match published v1 hash"
    );

    // Sanity: running the installed binary prints v1.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&install_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&install_path, perms).unwrap();
    }
    let exec = Command::new(&install_path).output().expect("run myapp v1");
    assert!(exec.status.success(), "running installed v1 failed: {:?}", exec);
    assert!(
        String::from_utf8_lossy(&exec.stdout).contains("hello v1"),
        "installed v1 did not print 'hello v1' (got: {:?})",
        String::from_utf8_lossy(&exec.stdout)
    );

    // ============================================================
    // v2 -- build, register, sign, publish, update via updater
    // ============================================================
    compile_myapp(
        "fn main() { println!(\"hello v2\"); }\n",
        &myapp_v2_path,
    );
    let v2_hash = blake3_file(&myapp_v2_path);
    assert_ne!(v1_hash, v2_hash, "v1 and v2 should hash differently");

    run_ok(
        Command::new(&publisher_bin)
            .current_dir(&publisher_dir)
            .args([
                "register",
                "--name",
                APP_NAME,
                "--version",
                "2.0.0",
                "--platform",
                PLATFORM,
                "--description",
                "e2e v2",
                "--file",
            ])
            .arg(&myapp_v2_path),
        "publisher register v2",
    );

    run_ok(
        Command::new(&publisher_bin)
            .current_dir(&publisher_dir)
            .args([
                "sign",
                "--name",
                APP_NAME,
                "--version",
                "2.0.0",
                "--passphrase",
                PASSPHRASE,
            ]),
        "publisher sign v2",
    );

    run_ok(
        Command::new(&publisher_bin)
            .current_dir(&publisher_dir)
            .args([
                "publish",
                "--name",
                APP_NAME,
                "--version",
                "2.0.0",
                "--server-url",
                &server_url,
                "--api-key",
                &api_key,
                "--yes",
            ]),
        "publisher publish v2",
    );

    // ---------- updater: second invocation, expect exit 2 (updated) ----------
    let out = Command::new(&updater_bin)
        .args(["--name", APP_NAME, "--server-url", &server_url])
        .arg("--install-path")
        .arg(&install_path)
        .arg("--public-key")
        .arg(&public_key_dst)
        .arg("--data-dir")
        .arg(&updater_state)
        .output()
        .expect("spawn updater v2");
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2 after v2 update; stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let installed_v2_hash = blake3_file(&install_path);
    assert_eq!(
        installed_v2_hash, v2_hash,
        "installed v2 hash does not match published v2 hash (delta apply broken?)"
    );

    // Run installed binary to confirm it really is v2.
    let exec = Command::new(&install_path).output().expect("run myapp v2");
    assert!(exec.status.success(), "running installed v2 failed: {:?}", exec);
    assert!(
        String::from_utf8_lossy(&exec.stdout).contains("hello v2"),
        "installed v2 did not print 'hello v2' (got: {:?})",
        String::from_utf8_lossy(&exec.stdout)
    );

    // ---------- updater: third invocation, expect exit 0 (already up to date) ----------
    let out = Command::new(&updater_bin)
        .args(["--name", APP_NAME, "--server-url", &server_url])
        .arg("--install-path")
        .arg(&install_path)
        .arg("--public-key")
        .arg(&public_key_dst)
        .arg("--data-dir")
        .arg(&updater_state)
        .output()
        .expect("spawn updater v2 idempotent");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0 on idempotent run; stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    eprintln!(
        "e2e_pipeline_full: total elapsed = {:.2}s",
        started.elapsed().as_secs_f64()
    );

    // Best-effort cleanup of the temp tree.
    let _ = std::fs::remove_dir_all(&work);
}
