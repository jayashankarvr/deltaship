//! Benchmarks for deltaship-diff operations
//!
//! # P3 Issue #109 Fix: Benchmark Suite for Diff Operations
//!
//! This benchmark suite measures the performance of diff generation and patching
//! operations across different file sizes and change patterns.
//!
//! Run with: `cargo bench --package deltaship-diff`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use deltaship_diff::{apply_patch, generate_diff};

/// Generate test data with a specific size
fn generate_test_data(size: usize) -> Vec<u8> {
    // Create somewhat realistic binary data (not purely random to allow diff to work)
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        // Pattern that has some structure (repeating blocks)
        data.push(((i / 64) % 256) as u8);
    }
    data
}

/// Modify test data slightly to simulate a realistic binary update
fn modify_data(data: &[u8], change_percent: f64) -> Vec<u8> {
    let mut modified = data.to_vec();
    let num_changes = (data.len() as f64 * change_percent / 100.0) as usize;

    // Make localized changes (realistic for code updates)
    for i in 0..num_changes {
        let pos = i % data.len();
        modified[pos] = modified[pos].wrapping_add(1);
    }
    modified
}

fn bench_diff_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_generation");

    // Test different file sizes
    for size in [1024, 10 * 1024, 100 * 1024, 1024 * 1024].iter() {
        let old_data = generate_test_data(*size);
        // 1% change (realistic for small updates)
        let new_data = modify_data(&old_data, 1.0);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let diff = generate_diff(black_box(&old_data), black_box(&new_data)).unwrap();
                black_box(diff);
            });
        });
    }
    group.finish();
}

fn bench_patch_application(c: &mut Criterion) {
    let mut group = c.benchmark_group("patch_application");

    // Test different file sizes
    for size in [1024, 10 * 1024, 100 * 1024, 1024 * 1024].iter() {
        let old_data = generate_test_data(*size);
        let new_data = modify_data(&old_data, 1.0);
        let diff = generate_diff(&old_data, &new_data).unwrap();

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let result = apply_patch(black_box(&old_data), black_box(&diff)).unwrap();
                black_box(result);
            });
        });
    }
    group.finish();
}

fn bench_diff_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_size_analysis");

    let size = 100 * 1024; // 100KB
    let old_data = generate_test_data(size);

    // Test different change percentages
    for change_pct in [0.1, 1.0, 5.0, 10.0].iter() {
        let new_data = modify_data(&old_data, *change_pct);

        group.bench_with_input(
            BenchmarkId::new("diff_generation", format!("{}%_change", change_pct)),
            change_pct,
            |b, _| {
                b.iter(|| {
                    let diff = generate_diff(black_box(&old_data), black_box(&new_data)).unwrap();
                    black_box(diff);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_diff_generation,
    bench_patch_application,
    bench_diff_sizes
);
criterion_main!(benches);
