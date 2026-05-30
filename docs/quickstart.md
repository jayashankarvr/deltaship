# VBDP Quickstart

Ship your first end-to-end binary update in under ten minutes. This guide walks
through standing up an update server, registering a publisher, publishing two
versions of a toy app, and watching the `vbdp-updater` sidecar pull and apply
the delta.

All commands assume Linux x86_64. Adjust `--platform` if you are on macOS or
Windows. The workspace lives at `~/vbdp-quickstart/` and the server runs at
`http://127.0.0.1:8080`.

## Prerequisites

- Rust toolchain (1.70+)
- A clone of this repository, with the workspace built once:

```bash
git clone https://github.com/jayashankarvr/vbdp.git
cd vbdp
cargo build --release -p vbdp-server -p vbdp-publisher -p vbdp-client
export PATH="$PWD/target/release:$PATH"
```

Verify the three binaries are on your `PATH`:

```bash
vbdp-server --version && vbdp-publisher --version && vbdp-updater --version
```

Create the demo workspace:

```bash
mkdir -p ~/vbdp-quickstart && cd ~/vbdp-quickstart
```

## 1. Start the update server

The server is HTTP-only and expects a reverse proxy for TLS in production. For
local development, bind to loopback. Authenticated endpoints (publish, admin)
require an API key whose Argon2id hash lives in `<data-dir>/api_keys.txt`.

Generate a plaintext key, hash it, and write the hash to the server's data
directory:

```bash
mkdir -p ~/vbdp-quickstart/server-data
export VBDP_API_KEY=$(vbdp-server --generate-api-key)
echo "$VBDP_API_KEY" | vbdp-server hash-key > ~/vbdp-quickstart/server-data/api_keys.txt
chmod 600 ~/vbdp-quickstart/server-data/api_keys.txt
```

Start the server in another terminal (it stays in the foreground):

```bash
vbdp-server --host 127.0.0.1 --port 8080 --data-dir ~/vbdp-quickstart/server-data -v
```

Sanity check from the original terminal:

```bash
curl -s http://127.0.0.1:8080/health
```

## 2. Initialize a publisher workspace

`init` creates `.vbdp/` (containing the publisher database) and generates an
Ed25519 signing keypair under `.vbdp/keys/`. The signing key is encrypted with
a passphrase (minimum 12 characters); use the `--passphrase` flag for
non-interactive setup.

```bash
mkdir -p ~/vbdp-quickstart/publisher && cd ~/vbdp-quickstart/publisher
vbdp-publisher init --passphrase 'quickstart-demo-passphrase'
```

The command prints the publisher's public key in hex. The matching public-key
file (which is what clients consume) is written to:

```
~/vbdp-quickstart/publisher/.vbdp/keys/public.key
```

Point the publisher at the local server (this is the default, but setting it
explicitly avoids the "non-default URL" confirmation prompt later):

```bash
vbdp-publisher config set server_url http://127.0.0.1:8080
```

## 3. Build and publish v1.0.0

Create the toy app:

```bash
mkdir -p ~/vbdp-quickstart/myapp-src/src
cat > ~/vbdp-quickstart/myapp-src/Cargo.toml <<'EOF'
[package]
name = "myapp"
version = "1.0.0"
edition = "2021"

[[bin]]
name = "myapp"
path = "src/main.rs"
EOF
cat > ~/vbdp-quickstart/myapp-src/src/main.rs <<'EOF'
fn main() { println!("hello v1"); }
EOF

cargo build --release --manifest-path ~/vbdp-quickstart/myapp-src/Cargo.toml
```

Register the binary, sign it, then publish. All publisher commands run from
`~/vbdp-quickstart/publisher` so they pick up the local `.vbdp/` workspace.

```bash
cd ~/vbdp-quickstart/publisher

vbdp-publisher register \
    --name myapp \
    --version 1.0.0 \
    --file ~/vbdp-quickstart/myapp-src/target/release/myapp \
    --platform linux-x86_64 \
    --description "Quickstart v1"

vbdp-publisher sign \
    --name myapp \
    --version 1.0.0 \
    --passphrase 'quickstart-demo-passphrase'

vbdp-publisher publish \
    --name myapp \
    --version 1.0.0 \
    --server-url http://127.0.0.1:8080 \
    --api-key "$VBDP_API_KEY" \
    --yes
```

Confirm the version is live:

```bash
curl -s "http://127.0.0.1:8080/v1/check-update?app=myapp&platform=linux-x86_64&current_version=0.0.0" | head
```

## 4. Bundle the app for distribution

A "release bundle" for an end user is just three files in one directory: the
app binary, the `vbdp-updater` sidecar, and the publisher's public key. Anyone
running the bundle should be able to spawn `vbdp-updater` and have it talk to
your server.

```bash
mkdir -p ~/vbdp-quickstart/bundle
cp ~/vbdp-quickstart/myapp-src/target/release/myapp           ~/vbdp-quickstart/bundle/myapp
cp "$(command -v vbdp-updater)"                                ~/vbdp-quickstart/bundle/vbdp-updater
cp ~/vbdp-quickstart/publisher/.vbdp/keys/public.key           ~/vbdp-quickstart/bundle/publisher.pub

ls -l ~/vbdp-quickstart/bundle
```

`~/vbdp-quickstart/bundle/` is what you would zip up and ship.

## 5. Run the sidecar — should report "up to date"

The updater stores its state (SQLite DB, backups, downloads) under
`--data-dir`. We pin it inside the bundle directory so the demo is
self-contained.

```bash
cd ~/vbdp-quickstart/bundle

./vbdp-updater \
    --name myapp \
    --install-path "$PWD/myapp" \
    --server-url http://127.0.0.1:8080 \
    --public-key "$PWD/publisher.pub" \
    --data-dir "$PWD/.updater-state"

echo "exit code: $?"
```

The first run auto-registers `myapp` in the local updater DB and downloads
v1.0.0 (because the local DB has no recorded version yet), so you will see
exit code `2` (an update was applied). Run it a second time:

```bash
./vbdp-updater \
    --name myapp \
    --install-path "$PWD/myapp" \
    --server-url http://127.0.0.1:8080 \
    --public-key "$PWD/publisher.pub" \
    --data-dir "$PWD/.updater-state"

echo "exit code: $?"
./myapp
```

This time the response is `myapp: up to date (1.0.0)` and exit code `0`. The
binary still prints `hello v1`.

Exit codes (from `vbdp-updater --help`):

- `0` — already on the latest version
- `2` — an update was applied (your app should restart itself)
- `1` — error; details on stderr

## 6. Build and publish v2.0.0

Bump the source, rebuild, and publish exactly the same way as v1:

```bash
sed -i 's/hello v1/hello v2/' ~/vbdp-quickstart/myapp-src/src/main.rs
sed -i 's/version = "1.0.0"/version = "2.0.0"/' ~/vbdp-quickstart/myapp-src/Cargo.toml
cargo build --release --manifest-path ~/vbdp-quickstart/myapp-src/Cargo.toml

cd ~/vbdp-quickstart/publisher

vbdp-publisher register \
    --name myapp \
    --version 2.0.0 \
    --file ~/vbdp-quickstart/myapp-src/target/release/myapp \
    --platform linux-x86_64 \
    --description "Quickstart v2"

vbdp-publisher sign \
    --name myapp \
    --version 2.0.0 \
    --passphrase 'quickstart-demo-passphrase'

vbdp-publisher publish \
    --name myapp \
    --version 2.0.0 \
    --server-url http://127.0.0.1:8080 \
    --api-key "$VBDP_API_KEY" \
    --yes
```

`register` automatically generates a binary delta from v1.0.0 to v2.0.0 (skip
this with `--no-diff` if you want a full-binary-only release). `publish`
uploads both the new full binary and any matching diffs in one step.

## 7. Re-run the sidecar — should download and apply the diff

From the bundle directory, point at the same data-dir as before so the updater
sees its previous state:

```bash
cd ~/vbdp-quickstart/bundle

./vbdp-updater \
    --name myapp \
    --install-path "$PWD/myapp" \
    --server-url http://127.0.0.1:8080 \
    --public-key "$PWD/publisher.pub" \
    --data-dir "$PWD/.updater-state"

echo "exit code: $?"
./myapp
```

Expected output:

```
myapp: updating 1.0.0 -> 2.0.0
myapp: updated to 2.0.0
exit code: 2
hello v2
```

The updater downloaded the v1->v2 delta, verified the publisher's signature
against `publisher.pub`, applied the patch in place, and exited with code `2`
to tell the host application to restart. The on-disk `myapp` binary is now v2.

## Caveats and known limitations

- The server speaks plain HTTP. In production, terminate TLS in front of it
  with nginx, Caddy, or a cloud load balancer (see `crates/vbdp-server/src/main.rs`
  for example reverse-proxy configs).
- Do not commit `.vbdp/keys/signing.key` or your plaintext API key to source
  control. The signing key should always be passphrase-encrypted in CI.
- Review the operational considerations (rollback path validation, diff upload
  edge cases) before relying on this in production.

## Where to go next

- `SECURITY.md` — threat model, key-handling guidance, vulnerability reporting.
- `GOVERNANCE.md` — project governance, release authority, decision-making.
- Per-tool help: every subcommand documents its own flags. Useful starting
  points:
  - `vbdp-server --help`
  - `vbdp-publisher --help`, `vbdp-publisher init --help`,
    `vbdp-publisher register --help`, `vbdp-publisher sign --help`,
    `vbdp-publisher publish --help`, `vbdp-publisher diff --help`
  - `vbdp-updater --help`
- `docs/architecture/` and `docs/COMPLETE_FLOW.md` for protocol-level detail.
