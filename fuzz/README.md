# Deltaship Fuzzing Targets

This directory contains fuzz testing targets for Deltaship using cargo-fuzz (libFuzzer).

## Prerequisites

Install cargo-fuzz:
```bash
cargo install cargo-fuzz
```

## Running Fuzz Tests

### Fuzz Version String Parsing
Tests semver version string parsing with arbitrary inputs:
```bash
cargo fuzz run fuzz_version_string
```

### Fuzz URL Parsing
Tests URL parsing and validation with malformed inputs:
```bash
cargo fuzz run fuzz_url_parsing
```

### Fuzz Binary Name Validation
Tests binary name validation, path traversal detection:
```bash
cargo fuzz run fuzz_binary_name
```

### Fuzz Diff/Patch Operations
Tests diff generation and patch application with arbitrary binary data:
```bash
cargo fuzz run fuzz_diff_patch
```

## Running with Options

### Set time limit (e.g., 60 seconds)
```bash
cargo fuzz run fuzz_version_string -- -max_total_time=60
```

### Set iteration limit
```bash
cargo fuzz run fuzz_version_string -- -runs=1000000
```

### Use multiple workers
```bash
cargo fuzz run fuzz_version_string -- -workers=4
```

## Reproducing Crashes

If fuzzing finds a crash, the input is saved to `fuzz/artifacts/`:
```bash
# Reproduce a specific crash
cargo fuzz run fuzz_version_string fuzz/artifacts/fuzz_version_string/crash-<hash>
```

## Corpus Management

Fuzzing builds a corpus of interesting inputs in `fuzz/corpus/`:
- These inputs achieve new code coverage
- Commit interesting corpus additions to help future fuzzing
- Clear corpus to start fresh: `rm -rf fuzz/corpus/fuzz_*/`

## Integration with CI

These fuzz targets can run in CI with time limits:
```bash
# Run each target for 60 seconds in CI
cargo fuzz run fuzz_version_string -- -max_total_time=60
cargo fuzz run fuzz_url_parsing -- -max_total_time=60
cargo fuzz run fuzz_binary_name -- -max_total_time=60
cargo fuzz run fuzz_diff_patch -- -max_total_time=60
```

## Coverage

Check code coverage from fuzzing:
```bash
cargo fuzz coverage fuzz_version_string
```

## Targets Overview

| Target | Purpose | Priority |
|--------|---------|----------|
| fuzz_version_string | Version parsing robustness | High |
| fuzz_url_parsing | URL validation security | High |
| fuzz_binary_name | Path traversal prevention | Critical |
| fuzz_diff_patch | Diff/patch memory safety | Critical |

## Notes

- Fuzz targets are designed to never panic on any input
- Memory limits prevent OOM in diff/patch fuzzing (1MB max)
- All inputs are validated for UTF-8 where appropriate
- Path operations check for traversal attempts
