# Deltaship - Version-Aware Binary Differential Update System

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)]()
[![Crates.io](https://img.shields.io/crates/v/deltaship.svg)]()
[![CI](https://img.shields.io/github/actions/workflow/status/jayashankarvr/deltaship/ci.yml?branch=main)]()

A high-performance binary differential update system designed for efficient software distribution.

> **Note:** Deltaship is currently in pre-release development (v0.2.0). APIs and features may change before the stable 1.0 release.

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

Registers, signs, and publishes versions; binary diffs against recent versions are generated automatically.

```bash
# One-time: create the project workspace + Ed25519 keypair
deltaship-publisher init --passphrase "$SIGNING_PASSPHRASE"

# Per release & platform: register the built binary (auto-generates diffs)
deltaship-publisher register --name myapp --version 1.1.0 \
  --platform linux-x86_64 --file ./build/myapp

# Sign it, then publish binary + signature + diffs to the server
deltaship-publisher sign    --name myapp --version 1.1.0 --passphrase "$SIGNING_PASSPHRASE"
deltaship-publisher publish --name myapp --version 1.1.0 \
  --server-url https://updates.example.com --api-key "$DELTASHIP_API_KEY"
```

### Update Server

Hosts binaries, diffs, and signatures (file-based storage, no database) and answers update checks.

```bash
# Generate + register an API key, then start the server
KEY=$(deltaship-server --generate-api-key)
echo "$KEY" | deltaship-server hash-key >> ./data/api_keys.txt

deltaship-server --host 0.0.0.0 --port 8080 --data-dir ./data
# No built-in TLS — terminate HTTPS at a reverse proxy (see deploy/Caddyfile)
```

### Client / Updater

Downloads a diff (or full binary), verifies hash + Ed25519 signature against a pinned key, then atomically replaces the installed binary.

```bash
# Register the installed binary once, pinning the publisher's public key
deltaship-client add --name myapp \
  --path /opt/myapp/bin/myapp --public-key-file publisher.pub

# Apply updates (verify-then-replace, with backup for rollback)
deltaship-client update --name myapp
```

## Embedding the Updater

To keep an application up to date, bundle the `deltaship-updater` sidecar alongside it and run it on startup. Branch on the exit code — `0` = up to date, `2` = updated (restart your app), `1` = error:

```bash
deltaship-updater \
  --name         myapp \
  --install-path /opt/myapp/bin/myapp \
  --server-url   https://updates.example.com \
  --public-key   /opt/myapp/publisher.pub
```

Integration is just *spawn a process + read the exit code*, so it works from any language. See the [Integration & Adoption Guide](docs/integration/INTEGRATION_GUIDE.md) and [Language Examples](docs/integration/LANGUAGE_EXAMPLES.md) (C, C++, C#, Go, Python, Node/Electron, Java, Rust, shell) for complete, copy-paste examples.

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
