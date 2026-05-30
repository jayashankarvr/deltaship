# Deltaship - Version-Aware Binary Differential Update System

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)]()
[![Crates.io](https://img.shields.io/crates/v/deltaship.svg)]()
<!-- TODO: Before public release, update to actual GitHub organization -->
[![CI](https://img.shields.io/github/actions/workflow/status/jayashankarvr/deltaship/ci.yml?branch=main)]()

A high-performance binary differential update system designed for efficient software distribution.

> **Note:** Deltaship is currently in pre-release development (v0.1.0). APIs and features may change before the stable 1.0 release.

## Features

- **Bandwidth Savings** - Transmit only the differences between versions, reducing update sizes by up to 90%
- **Cross-Platform** - Native support for Windows, macOS, and Linux
- **Cryptographic Signing** - Ed25519 signatures ensure update authenticity and integrity
- **Version-Aware Patches** - Intelligent delta generation that understands version semantics
- **Streaming Updates** - Apply patches without requiring full download completion

## Prerequisites

Before installing Deltaship, ensure you have the following dependencies:

- **Rust** - Version 1.70 or higher
- **SQLite** - Version 3.35 or higher
- **OpenSSL** - Version 1.1.1 or higher

You can verify your Rust version with:
```bash
rustc --version
```

To install or update Rust, visit [https://rustup.rs/](https://rustup.rs/)

## Quick Start

### Installation from Source

Deltaship is currently in development and not yet published to crates.io. Install from source:

```bash
# Clone the repository
# TODO: Before public release, update to actual GitHub organization
git clone https://github.com/jayashankarvr/deltaship.git
cd deltaship

# Install individual components
cargo install --locked --path crates/deltaship-publisher   # For creating and signing patches
cargo install --locked --path crates/deltaship-server      # For hosting update server
cargo install --locked --path crates/deltaship-client      # For applying updates
```

### Installation from crates.io (When Published)

Once published to crates.io, you'll be able to install directly:

```bash
# Install individual components (not yet available)
cargo install --locked deltaship-publisher
cargo install --locked deltaship-server
cargo install --locked deltaship-client
```

## Architecture

Deltaship consists of three main components:

### Publisher Toolkit

Generates binary diffs between versions and signs them for distribution.

```bash
# Generate a patch between versions
deltaship-publisher diff --old v1.0.0/app --new v1.1.0/app --output patch-1.0-1.1.deltaship

# Sign the patch
deltaship-publisher sign --patch patch-1.0-1.1.deltaship --key private.key
```

### Update Server

Serves patches to clients and manages version metadata.

```bash
# Start the update server
deltaship-server --config server.toml --port 8080
```

### Client Patcher

Downloads and applies updates on end-user systems.

```bash
# Check for updates and apply
deltaship-client update --current-version 1.0.0 --server https://updates.example.com
```

## Basic Usage

```rust
use deltaship::{DiffGenerator, Patcher};

// Generate a diff
let diff = DiffGenerator::new()
    .old_file("app-v1.0")
    .new_file("app-v1.1")
    .generate()?;

// Apply a patch
let patcher = Patcher::new()
    .source("app-v1.0")
    .patch(&diff)
    .apply("app-v1.1")?;
```

## Documentation

For detailed documentation, see the [`/docs`](./docs/) directory:

- **[Integration & Adoption Guide](./docs/integration/INTEGRATION_GUIDE.md)** - Start here to adopt Deltaship: deploy → publish → integrate, with platform notes and the HTTP API
- **[Language Examples](./docs/integration/LANGUAGE_EXAMPLES.md)** - Embed the updater from C, C++, C#, Go, Python, Node/Electron, Java, Rust, and shell
- [System Design](./docs/architecture/SYSTEM_DESIGN.md) - Architecture overview and design decisions
- [Protocol Specification](./docs/architecture/PROTOCOL_SPECIFICATION.md) - Wire protocol and API specification
- [Security Model](./docs/security/SECURITY_MODEL.md) - Threat model and cryptographic design
- [Complete Flow](./docs/COMPLETE_FLOW.md) - End-to-end workflow guide
- [FAQ](./docs/FAQ.md) - Frequently asked questions
- [Roadmap](./docs/ROADMAP.md) - Future plans and features

## Dependency Management

Deltaship uses `Cargo.lock` to ensure reproducible builds and consistent dependency versions across all environments. This file is committed to version control and should not be deleted.

**Why we commit Cargo.lock:**

- **Reproducible builds**: Ensures all developers and CI builds use identical dependency versions
- **Security**: Makes it easier to audit and verify exact dependency versions
- **Stability**: Prevents unexpected breakage from transitive dependency updates

When updating dependencies, run `cargo update` and commit the resulting `Cargo.lock` changes. For security patches, update specific dependencies with `cargo update -p <package-name>`.

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE) or
<http://www.apache.org/licenses/LICENSE-2.0>).

Copyright © 2026 Jayashankar. The author retains copyright; Apache-2.0 grants use
with required attribution and an explicit patent grant.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.
