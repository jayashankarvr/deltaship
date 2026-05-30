# Deltaship Examples

This directory contains example scripts demonstrating the Deltaship (Verified Binary Distribution Protocol) workflow.

## demo.sh - End-to-End Workflow Demo

The `demo.sh` script demonstrates the complete publisher -> server -> client flow in a self-contained environment.

### Prerequisites

1. **Build the Deltaship binaries:**

   ```bash
   cd /path/to/deltaship
   cargo build --release
   ```

   The script will look for binaries in `target/release/` first, then fall back to `target/debug/`.

2. **Required tools:**
   - `curl` - for API testing
   - `python3` - for JSON formatting (optional, falls back gracefully)

### Running the Demo

```bash
./examples/demo.sh
```

The script is fully automated and will:
- Create a temporary demo environment in `/tmp/deltaship-demo`
- Run all steps of the Deltaship workflow
- Clean up automatically on exit (including Ctrl+C)

### What Each Step Does

| Step | Description |
|------|-------------|
| 1 | **Setup** - Creates demo directory structure for publisher, server, and client |
| 2 | **Create Binaries** - Generates sample "application" files (v1.0.0 and v1.1.0) |
| 3 | **Publisher Init** - Initializes Deltaship project, generates Ed25519 keypair |
| 4 | **Register v1.0.0** - Registers first version, computes hashes |
| 5 | **Register v1.1.0** - Registers second version, auto-generates diff from v1.0.0 |
| 6 | **List Versions** - Shows all registered versions in the database |
| 7 | **Sign Versions** - Creates cryptographic signatures for both versions |
| 8 | **Start Server** - Launches the update server in the background |
| 9 | **Publish** - Uploads binaries, signatures, and diffs to the server |
| 10 | **Setup Client** - Configures client with v1.0.0 "installed" |
| 11 | **Add Binary** - Registers the binary with the client for management |
| 12 | **List Managed** - Shows binaries being tracked by the client |
| 13 | **Check Updates** - Client checks server for available updates |
| 14 | **Show Status** - Displays current update status |
| 15 | **API Test** - Directly queries the update API endpoint |

### Expected Output

A successful run will show output similar to:

```
=== Step 1: Setting up demo environment ===
Creating directory structure at /tmp/deltaship-demo

=== Step 3: Initializing Deltaship publisher project ===
Initializing Deltaship project...
  Created .deltaship/
  Created .deltaship/keys/
  Generating Ed25519 keypair...

=== Step 4: Registering version 1.0.0 ===
Creating new binary: demo-app (linux-x86_64)
Computing file hashes...
Version registered successfully!
  Binary:     demo-app (linux-x86_64)
  Version:    1.0.0

=== Step 7: Signing versions with Ed25519 key ===
Signing version manifest...
Version signed successfully!

=== Step 9: Publishing versions to server ===
Uploading to http://127.0.0.1:8080...
Binary uploaded successfully!
Version published successfully!

=== Step 15: Testing update API directly ===
{
    "update_available": true,
    "latest_version": "1.1.0",
    "full_binary_url": "/api/v1/apps/demo-app/versions/1.1.0/binary?platform=linux-x86_64",
    ...
}

=== Demo Complete! ===
```

### Configuration

You can modify these variables at the top of `demo.sh`:

```bash
DEMO_DIR="/tmp/deltaship-demo"    # Where to create demo files
SERVER_PORT=8080              # Server port
SERVER_HOST="127.0.0.1"       # Server bind address
APP_NAME="demo-app"           # Application name
PLATFORM="linux-x86_64"       # Target platform
```

### Manual Testing

After the demo starts the server, you can manually test the API:

```bash
# Health check
curl http://127.0.0.1:8080/health

# Check for updates
curl "http://127.0.0.1:8080/api/v1/apps/demo-app/check-update?current_version=1.0.0&platform=linux-x86_64"
```

### Troubleshooting

**"Binary not found" error:**
Ensure you've built the project with `cargo build --release` or `cargo build`.

**Server fails to start:**
Check if port 8080 is already in use. Modify `SERVER_PORT` in the script.

**Passphrase prompts:**
The demo uses empty passphrases for convenience. In production, always use strong passphrases for signing keys.

### Security Notes

This demo uses **empty passphrases** for the signing key to enable non-interactive execution. In production:

1. Always use strong passphrases for signing keys
2. Store signing keys securely (HSM, encrypted storage)
3. Use HTTPS for server communication
4. Implement proper API authentication

## Component Overview

### Publisher (`deltaship-publisher`)

The publisher toolkit manages binary releases:

```bash
deltaship-publisher init                    # Initialize project
deltaship-publisher keygen                  # Generate signing keypair
deltaship-publisher register --name APP --version X.Y.Z --file PATH --platform PLATFORM
deltaship-publisher sign --name APP --version X.Y.Z
deltaship-publisher publish --name APP --version X.Y.Z --server-url URL
deltaship-publisher list                    # List registered versions
```

### Server (`deltaship-server`)

The update server hosts binaries and serves the update API:

```bash
deltaship-server --host 0.0.0.0 --port 8080 --data-dir ./data
```

### Client (`deltaship-client`)

The client daemon manages automatic updates:

```bash
deltaship-client add --name APP --path PATH --public-key-file KEY
deltaship-client list                       # List managed binaries
deltaship-client status                     # Show update status
deltaship-client --check-now                # One-time update check
deltaship-client --daemon                   # Run as background daemon
```
