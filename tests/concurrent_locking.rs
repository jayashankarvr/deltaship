//! Stress tests for concurrent locking mechanisms.
//!
//! Tests the robustness of file-based locking in the following scenarios:
//! - Multiple daemon instances attempting to start
//! - Concurrent update operations
//! - Catalog/manifest updates during concurrent publishes
//! - Rollout config updates during concurrent reads

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use fs2::FileExt;
use tempfile::TempDir;

/// Test that only one daemon can acquire the PID lock at a time.
#[test]
fn test_single_daemon_instance() {
    let temp_dir = TempDir::new().unwrap();
    let pid_path = temp_dir.path().join("deltaship-daemon.pid");

    // First instance should succeed
    let file1 = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&pid_path)
        .unwrap();

    file1.try_lock_exclusive().unwrap();

    // Second instance should fail
    let file2 = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&pid_path)
        .unwrap();

    assert!(
        file2.try_lock_exclusive().is_err(),
        "Second daemon instance should not acquire lock"
    );

    // After dropping the first lock, second should succeed
    drop(file1);
    file2.try_lock_exclusive().unwrap();
}

/// Test multiple threads attempting to acquire daemon lock concurrently.
#[test]
fn test_concurrent_daemon_lock_acquisition() {
    let temp_dir = Arc::new(TempDir::new().unwrap());
    let pid_path = temp_dir.path().join("deltaship-daemon.pid");

    let num_threads = 10;
    let barrier = Arc::new(Barrier::new(num_threads));
    let success_count = Arc::new(std::sync::Mutex::new(0));

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let pid_path = pid_path.clone();
            let barrier = Arc::clone(&barrier);
            let success_count = Arc::clone(&success_count);

            thread::spawn(move || {
                // Wait for all threads to be ready
                barrier.wait();

                // Try to acquire the lock
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(&pid_path)
                    .unwrap();

                if file.try_lock_exclusive().is_ok() {
                    let mut count = success_count.lock().unwrap();
                    *count += 1;

                    // Hold lock briefly
                    thread::sleep(Duration::from_millis(10));
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Exactly one thread should have acquired the lock
    let count = *success_count.lock().unwrap();
    assert_eq!(count, 1, "Exactly one thread should acquire daemon lock");
}

/// Test concurrent update lock acquisitions.
#[test]
fn test_concurrent_update_locks() {
    let temp_dir = Arc::new(TempDir::new().unwrap());
    let lock_path = temp_dir.path().join("deltaship-update.lock");

    let num_threads = 20;
    let barrier = Arc::new(Barrier::new(num_threads));
    let active_locks = Arc::new(std::sync::Mutex::new(0));
    let max_concurrent = Arc::new(std::sync::Mutex::new(0));

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let lock_path = lock_path.clone();
            let barrier = Arc::clone(&barrier);
            let active_locks = Arc::clone(&active_locks);
            let max_concurrent = Arc::clone(&max_concurrent);

            thread::spawn(move || {
                // Wait for all threads to be ready
                barrier.wait();

                // Try to acquire the lock
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(&lock_path)
                    .unwrap();

                if file.try_lock_exclusive().is_ok() {
                    // Track concurrent lock holders
                    {
                        let mut active = active_locks.lock().unwrap();
                        *active += 1;
                        let current = *active;

                        let mut max = max_concurrent.lock().unwrap();
                        if current > *max {
                            *max = current;
                        }
                    }

                    // Simulate update operation
                    thread::sleep(Duration::from_millis(5));

                    {
                        let mut active = active_locks.lock().unwrap();
                        *active -= 1;
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Should never have more than 1 concurrent exclusive lock holder
    let max = *max_concurrent.lock().unwrap();
    assert_eq!(
        max, 1,
        "Should never have more than 1 concurrent update lock holder"
    );
}

/// Test concurrent file operations with proper locking.
///
/// Locks a stable sidecar lockfile (NOT catalog.json itself) — locking
/// catalog.json directly is meaningless under read-modify-rename, because the
/// rename swaps the inode and a waiting thread wakes up holding a lock on the
/// unlinked old inode. A separate lockfile path is the standard fix.
#[test]
fn test_concurrent_catalog_updates() {
    let temp_dir = Arc::new(TempDir::new().unwrap());
    let catalog_path = temp_dir.path().join("catalog.json");
    let lock_path = temp_dir.path().join("catalog.lock");

    fs::write(&catalog_path, r#"{"versions":[]}"#).unwrap();
    fs::write(&lock_path, b"").unwrap();

    let num_writers = 10;
    let barrier = Arc::new(Barrier::new(num_writers));

    let handles: Vec<_> = (0..num_writers)
        .map(|i| {
            let catalog_path = catalog_path.clone();
            let lock_path = lock_path.clone();
            let barrier = Arc::clone(&barrier);

            thread::spawn(move || {
                barrier.wait();

                let lock_file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&lock_path)
                    .unwrap();
                lock_file.lock_exclusive().unwrap();

                let content = fs::read_to_string(&catalog_path).unwrap();
                let mut catalog: serde_json::Value = serde_json::from_str(&content).unwrap();

                if let Some(versions) = catalog.get_mut("versions").and_then(|v| v.as_array_mut())
                {
                    versions.push(serde_json::json!({"id": i}));
                }

                // Per-thread temp filename so two threads' renames can never collide.
                let temp_path = catalog_path.with_extension(format!("tmp.{i}"));
                fs::write(&temp_path, serde_json::to_string_pretty(&catalog).unwrap()).unwrap();
                fs::rename(&temp_path, &catalog_path).unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all versions were added
    let final_content = fs::read_to_string(&catalog_path).unwrap();
    let final_catalog: serde_json::Value = serde_json::from_str(&final_content).unwrap();
    let versions = final_catalog["versions"].as_array().unwrap();

    assert_eq!(
        versions.len(),
        num_writers,
        "All concurrent writes should be preserved"
    );
}

/// Test shared vs exclusive locking behavior.
#[test]
fn test_shared_vs_exclusive_locks() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.lock");

    // Create file
    let _file = File::create(&file_path).unwrap();

    // Multiple shared locks should coexist
    let reader1 = File::open(&file_path).unwrap();
    let reader2 = File::open(&file_path).unwrap();
    let reader3 = File::open(&file_path).unwrap();

    reader1.lock_shared().unwrap();
    reader2.lock_shared().unwrap();
    reader3.lock_shared().unwrap();

    // Exclusive lock should fail while shared locks are held
    let writer = OpenOptions::new()
        .write(true)
        .open(&file_path)
        .unwrap();

    assert!(
        writer.try_lock_exclusive().is_err(),
        "Exclusive lock should fail while shared locks exist"
    );

    // Release shared locks
    drop(reader1);
    drop(reader2);
    drop(reader3);

    // Now exclusive lock should succeed
    writer.try_lock_exclusive().unwrap();
}

/// Stress test: rapid lock/unlock cycles.
#[test]
fn test_rapid_lock_unlock_cycles() {
    let temp_dir = Arc::new(TempDir::new().unwrap());
    let lock_path = temp_dir.path().join("rapid.lock");

    let num_threads = 5;
    let iterations_per_thread = 100;

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let lock_path = lock_path.clone();

            thread::spawn(move || {
                for _ in 0..iterations_per_thread {
                    let file = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .open(&lock_path)
                        .unwrap();

                    file.lock_exclusive().unwrap();
                    // Immediately unlock by dropping
                    drop(file);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Test should complete without deadlocks or panics
}

/// Test lock timeout behavior (using try_lock).
#[test]
fn test_lock_timeout_behavior() {
    let temp_dir = TempDir::new().unwrap();
    let lock_path = temp_dir.path().join("timeout.lock");

    let file1 = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&lock_path)
        .unwrap();

    file1.lock_exclusive().unwrap();

    // Spawn thread that will try to acquire lock
    let lock_path_clone = lock_path.clone();
    let handle = thread::spawn(move || {
        let file2 = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path_clone)
            .unwrap();

        // Should fail immediately with try_lock
        let start = std::time::Instant::now();
        let result = file2.try_lock_exclusive();
        let elapsed = start.elapsed();

        assert!(result.is_err(), "try_lock should fail when lock is held");
        assert!(
            elapsed < Duration::from_millis(100),
            "try_lock should fail quickly, not block"
        );
    });

    handle.join().unwrap();

    // Release first lock
    drop(file1);

    // Now acquiring should succeed
    let file3 = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    file3.try_lock_exclusive().unwrap();
}
