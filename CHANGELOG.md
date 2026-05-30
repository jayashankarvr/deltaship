# Changelog

> **Note:** Repository URLs in this file use the placeholder
> `REPLACE_ME_ORG`. Before the first public release, replace every
> occurrence of `REPLACE_ME_ORG` with the final GitHub organization or
> user that hosts the canonical VBDP repository.

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Versioning policy

VBDP follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html)
with the following clarifications for the pre-1.0 phase:

- The project is currently in the `0.x` series. While in `0.x`:
  - **MINOR** version bumps (`0.1.0` -> `0.2.0`) MAY contain breaking
    changes. Any breaking change is called out explicitly in the
    `Changed` or `Removed` section of the release entry.
  - **PATCH** version bumps (`0.2.0` -> `0.2.1`) are bug-fix and
    security-fix only. They MUST NOT introduce breaking changes.
- The first stable release will be tagged `1.0.0`. There is no
  committed date for `1.0.0`; it ships when the protocol, on-disk
  format, and CLI surface have stabilized.
- Once `1.0.0` is released, normal semver rules apply: breaking
  changes require a MAJOR bump.

A change is considered **breaking** if it does any of the following:

- Renames, removes, or repurposes a CLI flag or subcommand on
  `vbdp-publisher`, `vbdp-server`, `vbdp-client`, or `vbdp-updater`.
- Changes the on-disk layout of the publisher database, the client
  database, or the server catalog/storage tree in a way that an older
  binary cannot read.
- Changes the wire protocol of the update server REST API in a
  non-additive way (renaming or removing endpoints, request fields,
  or response fields).
- Changes the diff or signature format such that artifacts produced
  by the previous version can no longer be applied or verified.

Purely additive changes (new endpoints, new optional fields, new
subcommands, new diff algorithms behind a flag) are not breaking.

## [Unreleased]

Nothing yet.

## [0.2.0] - 2026-04-28

First tagged development release. The project is still pre-1.0 and the
protocol/storage formats are not yet frozen; see the versioning policy
above.

### Added

- **zstd-compressed binary deltas.** The diff pipeline produces
  `bsdiff` patches and compresses them with `zstd` before upload.
  Compression is applied transparently in `vbdp-diff` and unwrapped on
  the client during patch application.
- **`vbdp-updater` sidecar binary.** A small companion binary
  (`crates/vbdp-client/src/bin/vbdp-updater.rs`) intended to be spawned
  synchronously by a parent application at startup. It uses an
  exit-code protocol so the parent can decide whether to restart:
  `0` = up to date, `2` = update applied (parent should restart),
  non-zero other = error.
- **Diff size guard in the publisher.** When generating a delta,
  `generate_single_diff` now returns `None` if the compressed diff is
  not smaller than the full new binary. The diff job is recorded as
  failed in the publisher database with a clear reason, both call
  sites handle the skip gracefully, and the client falls back to a
  full binary download when no diff is published for a transition.
- **End-to-end integration test (`tests/end_to_end.rs`).** Spawns a
  real `vbdp-server`, drives `vbdp-publisher` to publish two versions
  with a delta between them, and exercises the client update path.
  Runnable as part of the workspace test suite.
- **Concurrent locking integration test
  (`tests/concurrent_locking.rs`).** Exercises the file-locked
  catalog/manifest update paths under concurrent writers.
- **Docker Compose deployment for the update server
  (`deploy/docker-compose.yml`, `deploy/Dockerfile.server`).** Provides
  a starting point for running `vbdp-server` with a persistent volume
  for the catalog and binary storage.
- **Quickstart guide (`docs/quickstart.md`).** End-to-end walkthrough
  from `cargo build` to a working delta update, with every command
  cross-referenced against the actual `clap` definitions in the
  publisher, server, and updater binaries.
- **Multi-platform CI publish template
  (`docs/ci/publish-template.yml` + `docs/ci/README.md`).** Copyable
  GitHub Actions workflow showing a 4-platform matrix build
  (linux-x86_64, macos-x86_64, macos-aarch64, windows-x86_64) that
  feeds into a fan-in `vbdp-publisher register/sign/publish` step
  using GitHub Secrets. README documents required secrets and known
  gaps in the publisher CLI for non-interactive use.
- **End-to-end pipeline test exercising `vbdp-updater`
  (`tests/e2e_pipeline.rs`).** Spawns the server, runs the publisher
  for v1+v2, runs `vbdp-updater` as a child process, and asserts on
  exit codes (`2` first install, `2` after v2 publish, `0` when up
  to date) and BLAKE3 hashes of the installed binary. Gated
  `#[ignore]` to keep the default test loop fast.

### Changed

- **Workspace version bumped to `0.2.0`** in the root `Cargo.toml`.
  All workspace crates inherit this version via
  `workspace.package.version`.
- **Publisher upload checksum is now BLAKE3** to match what the server
  computes and validates (`crates/vbdp-publisher/src/commands/publish.rs`).
  Previously the publisher sent SHA-256, which the server rejected.
  This is a behavior change for anyone running an older publisher
  against a `0.2.0` server.
- **Diff upload multipart form now includes the `platform` field**
  (`crates/vbdp-publisher/src/api_client.rs`). The server has always
  required this field; older publishers failed every diff upload with
  "Missing required field: platform".
- **Public verifying-key files are written with mode `0644`** instead
  of `0600`, matching their public nature
  (`crates/vbdp-crypto/src/keys.rs`). Signing keys remain `0600`.

### Fixed

- **CLIENT-P0-1: Path validation on rollback.** The rollback command
  now calls `validate_install_path()` before the atomic rename,
  closing a TOCTOU/symlink-substitution gap that previously existed
  only on the forward-update path
  (`crates/vbdp-client/src/commands/rollback.rs`).
- **CLIENT-P0-2: Signature verification in `download_update_only`.**
  Manually downloaded full-binary updates are now signature- and
  checksum-verified before being written to the destination, with an
  explicit warning emitted for diffs (which are not individually
  signed under the current protocol)
  (`crates/vbdp-client/src/commands/update.rs`).
- **CRYPTO-P1-1: Decoded signing-key bytes are now wrapped in
  `Zeroizing`** so plaintext key material does not linger in memory
  after `SigningKey::from_bytes()`
  (`crates/vbdp-crypto/src/keys.rs`).
- **CRYPTO-P1-2: `load_verifying_key` no longer makes an unnecessary
  clone** of the decoded key bytes; uses `try_into()` directly.
- **DB-P1-1: `diff_algorithm` values are now validated** against the
  allowed set (`bsdiff`, `courgette`, `xdelta3`) on insert via a
  shared helper (`crates/vbdp-db/src/publisher.rs`).
- **DB-P1-2: Schema initialization is transactional.** Both
  `publisher.rs::init()` and `client.rs::init()` now wrap their DDL
  in a single transaction so a partial failure leaves no half-created
  schema behind.
- **PROJECT-P0-1 / PROJECT-P0-2: License files in place at the
  project root.** `LICENSE`, `LICENSE-MIT`, and `LICENSE-APACHE` exist
  and match the filenames referenced by `.github/workflows/release.yml`,
  unblocking the release archive build.
- **CLIENT-P1-1: Backup signature verification on rollback.** The
  publisher's Ed25519 signature is now persisted alongside the
  installed binary as `<install>.sig` at install time, copied with
  the backup, and verified against the BLAKE3 hash of the backup file
  before any rollback restore. Backups created before this change
  fail loudly rather than restoring unverified bytes
  (`crates/vbdp-client/src/patcher.rs`,
  `crates/vbdp-client/src/commands/rollback.rs`).
- **PROJECT-P1-1 / P1-2: Empty security-test stubs replaced with real
  assertions** for consecutive-dot, leading-dot, and trailing-dot
  rejection in `validate_version_format`. Tests that genuinely
  require fixtures (multi-thread TOCTOU race, end-to-end diff
  checksum enforcement, signature-bypass scenarios) are explicitly
  `#[ignore]`-d with concrete TODOs describing what's needed
  (`crates/vbdp-client/tests/security_tests.rs`,
  `crates/vbdp-client/tests/version_validation_tests.rs`).
- **PROJECT-P1-3 / P1-4: Placeholder contact addresses standardized**
  on `<role>@vbdp.example.com` (RFC 2606 reserved domain) across the
  documentation set, with a banner note on each affected page making
  it explicit that real contacts must replace these before a public
  release.

### Security

The following items in the **Fixed** section are security fixes and
are repeated here for visibility:

- CLIENT-P0-1: rollback path validation closes a symlink-substitution
  vector against `install_path`.
- CLIENT-P0-2: `download_update_only` now performs signature and
  checksum verification on full binaries.
- CLIENT-P1-1: backup signatures are persisted, propagated, and
  verified on rollback so a tampered backup cannot be restored.
- CRYPTO-P1-1 / CRYPTO-P1-2: tighter handling of in-memory key
  material.
- CRYPTO-P3-3: public verifying keys saved with `0644` permissions
  (corrects an inconsistency, not an exploitable bug).
- DB-P1-1: `diff_algorithm` validation prevents unknown algorithm
  strings from reaching the database.
- DB-P1-2: transactional schema initialization prevents partial
  schema states on crash.

## [0.1.0] - Unreleased

Internal pre-tag development snapshot. The feature surface listed
under the original `Unreleased` section was developed against this
version but `0.1.0` was never tagged or published. The first tagged
release is `0.2.0` above.

<!-- TODO: Replace REPLACE_ME_ORG with the real GitHub organization before public release. -->
[Unreleased]: https://github.com/REPLACE_ME_ORG/vbdp/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/REPLACE_ME_ORG/vbdp/releases/tag/v0.2.0
[0.1.0]: https://github.com/REPLACE_ME_ORG/vbdp/releases/tag/v0.1.0
