# VBDP Integration & Adoption Guide

This guide is everything you need to adopt **VBDP** (Version-aware Binary Differential
Patcher) to keep an application up to date in the field. It is the canonical,
code-verified reference; the top-level `README.md` only links here.

> **One-line mental model:** VBDP is an **auto-updater**, not an installer. Your
> normal installer puts the app on disk the first time; VBDP then keeps that
> already-installed binary current by shipping **signed binary diffs** (falling
> back to a full download when a diff isn't smaller or isn't available).

- [1. Architecture: the three components](#1-architecture-the-three-components)
- [2. End-to-end update flow](#2-end-to-end-update-flow)
- [3. Supported platforms](#3-supported-platforms)
- [4. Quick start (deploy → publish → integrate)](#4-quick-start)
- [5. The `vbdp-updater` sidecar contract](#5-the-vbdp-updater-sidecar-contract)
- [6. Integrating from your application](#6-integrating-from-your-application)
- [7. Per-platform integration notes](#7-per-platform-integration-notes)
- [8. Publisher: full release workflow](#8-publisher-full-release-workflow)
- [9. Server: configuration & operations](#9-server-configuration--operations)
- [10. HTTP API reference (for custom clients)](#10-http-api-reference)
- [11. Security model](#11-security-model)
- [12. Rust in-process library API](#12-rust-in-process-library-api)
- [13. Troubleshooting & FAQ](#13-troubleshooting--faq)
- [14. Known gaps & caveats](#14-known-gaps--caveats)

---

## 1. Architecture: the three components

VBDP has **three roles**. The vendor runs the first two; the third ships inside
your application.

| Component | Binary | Who runs it | Job |
|---|---|---|---|
| **Publisher** | `vbdp-publisher` | Vendor (build/CI machine) | Register a new version, diff it against prior versions, **sign** it with the vendor's Ed25519 key, upload to the server. |
| **Server** | `vbdp-server` | Vendor (self-hosted) | Dumb host: stores binaries/diffs/signatures on disk, answers "is there an update?". File-based storage, no database. |
| **Client / Updater** | `vbdp-updater` (sidecar) or `vbdp-client` (CLI/daemon) | End-user machine, bundled with your app | Check the server, download a diff (or full binary), **verify hash + signature**, atomically replace the installed binary, keep a backup for rollback. |

```
   VENDOR SIDE                         NETWORK                  END-USER MACHINE
   ───────────                         ───────                  ────────────────
 ┌───────────────┐                                          ┌────────────────────┐
 │ vbdp-publisher│  register / diff / sign / publish        │  your application  │
 │  (in CI)      │ ───────────────┐                         │  + vbdp-updater    │
 └───────────────┘                │                         │  (bundled sidecar) │
                                   ▼                         │  + publisher.pub   │
                            ┌─────────────┐    HTTPS         │    (pinned key)    │
                            │ vbdp-server │◀──(Caddy TLS)──▶ │                    │
                            │ files on    │   diff or full   │  on launch:        │
                            │ disk        │ ───────────────▶ │   spawn updater →  │
                            └─────────────┘                  │   verify → replace │
                                                             └────────────────────┘
```

**Trust boundary:** the server is untrusted for *authenticity*. It only stores and
serves bytes. Authenticity comes from the **publisher's Ed25519 signature**, which
the client verifies against a public key **pinned at integration time** — never a
key from the server. A compromised server (or a network attacker) cannot push a
tampered binary without the vendor's private key.

---

## 2. End-to-end update flow

What happens on a single `vbdp-updater` run:

1. **Check.** `GET /api/v1/apps/{app}/check-update?current_version=X&platform=P`.
   The server replies whether an update exists and, if so, the `target_version`,
   a `diff_url` (only if a diff from *your exact current version* exists), a
   `full_binary_url`, a `signature_url`, and the expected **BLAKE3 checksum** of
   the final binary.
2. **Download.** Prefer the diff when offered; otherwise download the full binary.
   All downloads are over HTTPS for non-loopback hosts (enforced).
3. **Reconstruct.** If a diff was used, apply it (bsdiff + zstd) to the currently
   installed binary to rebuild the new binary in a temp file.
4. **Verify — _before_ touching the installed file:**
   - the reconstructed binary's **BLAKE3 hash** must equal the expected checksum, **and**
   - the **Ed25519 signature** (downloaded from `signature_url`) must verify against
     the canonical signed payload using the **pinned** public key.
5. **Atomically replace.** Write to a temp file in the same directory, `fsync`,
   then `rename()` over the target (atomic on the same filesystem). The previous
   binary is saved as a **backup** first.
6. **Re-verify & record.** Re-hash the installed file, drop a `.sig` sidecar next
   to it (so a later rollback can re-check authenticity), and record the install
   for rollback.
7. **Exit code** tells your app what happened: `0` up-to-date, `2` updated
   (restart me), `1` error. See [§5](#5-the-vbdp-updater-sidecar-contract).

**Diff vs. full — when does each happen?**

- A diff is generated by the publisher from the **most recent 3** prior versions to
  the new one (fixed `N = 3`). It is **skipped** when the compressed diff is *not
  smaller* than the full binary.
- The client gets a `diff_url` only when a diff from *its exact current version*
  exists. Otherwise (first install, version older than the last 3 releases, or
  no worthwhile diff) it downloads the **full** binary. Either way the result is
  verified identically.

---

## 3. Supported platforms

VBDP recognizes exactly **five** platform identifiers (a closed set — anything else
is rejected). The updater auto-detects its own at runtime via `#[cfg(target_os/arch)]`.

| Platform identifier | OS / Arch | Rust target triple (suggested build) |
|---|---|---|
| `linux-x86_64`  | Linux, x86-64        | `x86_64-unknown-linux-gnu` (or `-musl` for static) |
| `linux-aarch64` | Linux, ARM64         | `aarch64-unknown-linux-gnu` |
| `windows-x86_64`| Windows, x86-64      | `x86_64-pc-windows-msvc` |
| `macos-x86_64`  | macOS, Intel         | `x86_64-apple-darwin` |
| `macos-aarch64` | macOS, Apple Silicon | `aarch64-apple-darwin` |

You build a separate `vbdp-updater` binary per target you ship, and you publish a
separate VBDP "version" per platform (each platform has its own binary, hash, and
signature). The `platform` string is used everywhere: publishing, the
`?platform=` query on checks/downloads, and the on-disk layout.

---

## 4. Quick start

This takes you from nothing to a working update pipeline. Commands are
**code-verified** against the CLIs (the older `README.md` examples use stale flags —
use the ones here).

### 4.1 Build the tools

```bash
# From the repo root
cargo build --release -p vbdp-server -p vbdp-publisher -p vbdp-client
# Binaries: target/release/{vbdp-server, vbdp-publisher, vbdp-client, vbdp-updater}
```

### 4.2 Deploy the server (Docker Compose + automatic HTTPS)

The `deploy/` directory ships a turnkey stack: the server behind **Caddy** (which
obtains a Let's Encrypt certificate automatically).

```bash
cd deploy
cp .env.example .env          # set DOMAIN=updates.example.com and ACME_EMAIL=ops@example.com
mkdir -p data

# Generate a publisher API key, hash it, and register it (keep the plaintext KEY safe!)
KEY=$(docker compose run --rm --no-deps vbdp-server --generate-api-key)
echo "$KEY" | docker compose run --rm --no-deps -T vbdp-server hash-key >> data/api_keys.txt
chmod 600 data/api_keys.txt

docker compose up -d
# Verify (auth required):
curl -H "X-API-Key: $KEY" https://$DOMAIN/api/v1/admin/stats
```

> **Important:** to make per-client rate limiting / auth-backoff work behind Caddy,
> add `--trust-proxy` to the server command (or `VBDP_TRUST_PROXY=1` in the compose
> `environment:`). The shipped compose does **not** set it — see
> [§9](#9-server-configuration--operations) and [§14](#14-known-gaps--caveats).

Server defaults if you run the binary directly (no Docker): `--host 127.0.0.1
--port 8080 --data-dir ./data`. There is **no built-in TLS** — always terminate
HTTPS at a reverse proxy in production.

### 4.3 Cut your first release (publisher)

Run these on your build/CI machine, in the directory where you want the `.vbdp/`
project workspace to live.

```bash
# Once per project: create .vbdp/ workspace + Ed25519 keypair + local DB.
vbdp-publisher init --passphrase "$SIGNING_PASSPHRASE"
#   -> .vbdp/keys/signing.key (encrypted, 0600) and .vbdp/keys/public.key

# Once: point the publisher at your server.
vbdp-publisher config set server_url https://updates.example.com

# Per release & per platform: register the built binary (auto-diffs vs up to 3 prior versions).
vbdp-publisher register \
  --name myapp --version 1.0.0 --platform linux-x86_64 \
  --description "Release 1.0.0" --file ./build/linux-x86_64/myapp

# Sign it (unlocks the encrypted signing key).
vbdp-publisher sign --name myapp --version 1.0.0 --passphrase "$SIGNING_PASSPHRASE"

# Publish: uploads the binary + signature + checksum, then any generated diffs.
vbdp-publisher publish \
  --name myapp --version 1.0.0 \
  --server-url https://updates.example.com --api-key "$KEY" --yes
```

Distribute `.vbdp/keys/public.key` to your application build — it gets **bundled and
pinned** by the updater. **Never** distribute `signing.key`.

### 4.4 Bundle and run the updater in your app

Ship the per-platform `vbdp-updater` binary and `public.key` alongside your app.
On launch (or on a schedule), spawn it and act on the exit code:

```bash
vbdp-updater \
  --name        myapp \
  --install-path /opt/myapp/bin/myapp \
  --server-url  https://updates.example.com \
  --public-key  /opt/myapp/publisher.pub
# exit 0 = up to date | 2 = updated, restart app | 1 = error
```

Language-specific spawn code (C/C++, C#, Go, Python, Node/Electron, Java, Rust, shell)
is in **[LANGUAGE_EXAMPLES.md](./LANGUAGE_EXAMPLES.md)**.

---

## 5. The `vbdp-updater` sidecar contract

The sidecar is the **primary, language-agnostic integration surface**. Your app
spawns it as a child process; integration is just *args in, exit code out* — no
linking, no FFI.

### Flags

| Flag | Required | Meaning |
|---|---|---|
| `--name <NAME>` | ✅ | App identifier (must match what you published; `[A-Za-z0-9_-]`, ≤64 chars). |
| `--install-path <PATH>` | ✅ | The binary to keep updated (replaced in place). May not exist on first run — it'll be created from a full download. |
| `--server-url <URL>` | ✅ | Base URL of your `vbdp-server` (use `https://` in production). |
| `--public-key <PATH>` | ✅ | The pinned `public.key` you bundled. Verifies every update. |
| `--data-dir <PATH>` | — | State dir (DB, backups, downloads). Default: `~/.local/share/vbdp/<name>/`. |
| `--check-only` | — | Check but don't apply: exit `0` if up to date, `2` if an update is available. |
| `-q, --quiet` | — | Suppress progress output. |

### Exit codes (the contract)

| Code | Constant | Meaning | What your app should do |
|---|---|---|---|
| `0` | `EXIT_UP_TO_DATE` | Already on the latest version. | Continue starting up. |
| `2` | `EXIT_UPDATED` | An update was applied (or, with `--check-only`, is available). | **Re-launch** so the user runs the new binary. |
| `1` | `EXIT_ERROR` | Something failed; details on **stderr**. | Continue on the old version; log/telemetry the failure. Updates are safe to retry later. |

### State directory

`--data-dir` holds a small SQLite DB (`client.db`) tracking the installed version,
a `backups/` directory (for rollback), and a `downloads/` directory. On first run
the updater auto-registers the binary and pins the public key, then performs a full
download if `--install-path` doesn't exist yet.

> The lightweight sidecar applies whatever the server advertises as the latest
> version. The server **only advertises an update when the latest published version
> is newer** than the client's `current_version`, so normal operation never
> downgrades. The CLI `vbdp-client update` and the daemon add an *extra* explicit
> client-side monotonicity guard (`--allow-downgrade` to override) — use those if
> you want defense-in-depth beyond the server's newer-only logic.

---

## 6. Integrating from your application

There are three integration styles, in increasing order of coupling:

1. **Spawn the sidecar (recommended, any language/platform).** Bundle
   `vbdp-updater`, run it on startup, branch on the exit code. Works identically
   for C/C++, C#, Go, Python, Node/Electron, Java, etc. See
   **[LANGUAGE_EXAMPLES.md](./LANGUAGE_EXAMPLES.md)**.
2. **Run it as a background service/daemon.** Use `vbdp-client --daemon` (it polls
   on `check_interval_secs`) or your OS service manager. Good for always-on agents.
3. **In-process (Rust only).** Call the `vbdp-client` library API directly for
   progress callbacks and custom UI. See [§12](#12-rust-in-process-library-api).

There is **no C ABI / shared library**, so non-Rust apps use option 1 or 2.

**Typical startup pattern (pseudocode):**

```
on app launch:
    status = spawn_and_wait("vbdp-updater", [--name, --install-path, --server-url, --public-key])
    if status == 2:        # updated
        relaunch_self()    # exec the freshly-written binary
        exit
    else:                  # 0 = up to date, 1 = error (non-fatal)
        continue_startup()
```

Run the updater **before** loading the app's main binary into memory where possible,
and design for "update applies on next launch" — you cannot replace a running
executable's open image on Windows (see [§7](#7-per-platform-integration-notes)).

---

## 7. Per-platform integration notes

### Linux (`linux-x86_64`, `linux-aarch64`)
- Replacing a binary while it runs is fine (the inode stays open until the process
  exits), so the simplest pattern is: updater runs → exits `2` → your launcher
  `exec`s the new binary.
- Service integration: `vbdp-client service install [--user]` generates a systemd
  unit; or schedule `vbdp-updater` from a `systemd.timer`/cron.
- Prefer `x86_64-unknown-linux-musl` for a static updater with no glibc dependency.

### Windows (`windows-x86_64`)
- **You cannot overwrite a running `.exe`'s image.** Use a small **launcher** model:
  a tiny `myapp-launcher.exe` runs `vbdp-updater` first, and only then starts
  `myapp.exe`. The updater replaces `myapp.exe` while it is *not* running.
- Path handling: the updater's privileged-directory denylist is case-insensitive,
  separator-insensitive, and drive-agnostic, and it **refuses installs under
  `C:\Windows`, `C:\Program Files`, `C:\Program Files (x86)`** (on any drive). Install
  your app under a user-writable location (e.g. `%LOCALAPPDATA%\MyApp\`).
- Code signing: keep your existing Authenticode signing in your build; VBDP's
  Ed25519 signature is independent and verifies *transport/authenticity*, not the
  OS trust chain. Sign each platform binary before `register`.

### macOS (`macos-x86_64`, `macos-aarch64`)
- For app bundles, point `--install-path` at the actual Mach-O inside
  `MyApp.app/Contents/MacOS/myapp`. Replacing a single binary in place is fine.
- **Notarization/Gatekeeper:** if you ship a notarized `.app`, replacing the inner
  binary can invalidate the bundle's notarization/codesign seal. For notarized
  distribution, prefer updating a self-contained helper or re-stapling; for
  unsigned/internal tools, in-place replacement works directly.
- Ship both `macos-x86_64` and `macos-aarch64` (or a universal binary built per
  arch and published under each platform id).

---

## 8. Publisher: full release workflow

### Command reference (`vbdp-publisher`)

Global flags: `-v/--verbose` (repeatable), `-q/--quiet`.

| Command | Key flags | Purpose |
|---|---|---|
| `init` | `-p/--passphrase` | Create `.vbdp/` workspace, keypair, DB (run once). |
| `keygen` | `-o/--output-dir`, `-p/--passphrase` | Generate a keypair separately (not needed if you ran `init`). |
| `register` | `-n/--name`, `-V/--version`, `-f/--file`, `-p/--platform`, `-d/--description`, `--no-diff` | Register a new version; auto-generates diffs unless `--no-diff`. |
| `sign` | `-n/--name` + `-V/--version` (or `--version-id`), `-k/--key-file`, `-p/--passphrase` | Sign a registered version. |
| `publish` | `-n/--name`, `-V/--version`, `-s/--server-url`, `-a/--api-key`, `-y/--yes` | Upload binary + signature + diffs to the server. |
| `diff` | `-n/--name`, `--from-version`, `--to-version`, `-p/--platform`, `-o/--output` | Manually generate a diff. |
| `diff-list` | `-n/--name`, `-s/--status` | List diff jobs. |
| `verify` | `-n/--name`, `-V/--version`, `--fix`, `--json` | Verify stored signatures + checksums. |
| `list` / `info` / `stats` | `-n/--name`, `--json` | Inspect binaries/versions/savings. |
| `config get/set/list/reset` | `<key> <value>` | Manage `server_url`, `publisher_name` (`public_key` is read-only). |
| `export` / `import` | `-o/--output` / `-i/--input`, `--merge`, `--overwrite`, `--include-keys`, `--include-binaries` | Move a project between machines. |
| `cleanup` / `prune` | `--dry-run`, `--max-age-days`, `-k/--keep` | Housekeeping. |

### What `publish` sends

- **Binary:** `POST /api/v1/apps/{app}/versions/{version}` — multipart fields
  `platform`, `signature` (hex/base64 Ed25519, 64 bytes), `checksum` (hex **BLAKE3**
  — note: the wire field is named `checksum` and is BLAKE3, despite a stale
  "SHA-256" code comment), and `binary`.
- **Diffs:** `POST /api/v1/apps/{app}/diffs/{from}/to/{to}` — fields `platform`,
  `checksum` (BLAKE3 of the compressed diff), `diff`. If any diff upload fails, the
  version is **not** marked published.
- **Auth:** header `X-API-Key: <plaintext key>`. The publisher reads it **only** from
  `--api-key` (no env-var or config fallback).

### State & distribution
- Workspace: `.vbdp/` in the current directory — `keys/{signing.key,public.key}`,
  `publisher.db` (SQLite), `diffs/<name>/<from>-to-<to>.patch`. Config lives in the
  DB, not a file.
- Install: `cargo install --locked --path crates/vbdp-publisher` (binary
  `vbdp-publisher`).

### CI/CD
Wire `register → sign → publish` into your release pipeline. Provide the signing
passphrase and the API key as CI secrets (`--passphrase "$SIGNING_PASSPHRASE"`,
`--api-key "$VBDP_API_KEY"`). See `docs/integration/CI_CD_INTEGRATION.md` and
`docs/ci/publish-template.yml` for a starting template.

---

## 9. Server: configuration & operations

### Run-the-server flags (`vbdp-server`)

| Flag / env | Default | Purpose |
|---|---|---|
| `--host` | `127.0.0.1` | Bind address (`0.0.0.0` in containers). |
| `--port` | `8080` | Listen port. |
| `--data-dir` | `./data` | Storage root (catalogs, binaries, diffs, `api_keys.txt`). |
| `--generate-api-key` | — | Print a fresh 256-bit hex key and exit. |
| `--trust-proxy` / `VBDP_TRUST_PROXY=1` | off | Derive client IP from the right-most `X-Forwarded-For` hop. **Enable this behind Caddy.** |
| `VBDP_CORS_ORIGINS` | unset = allow all (warns) | Comma-separated allowed origins. |
| `hash-key [KEY]` (subcommand) | — | Argon2id-hash a plaintext key (stdin if omitted). |

Global body limit: **600 MB** (per-field caps: **512 MB** binary, **256 MB** diff).
No built-in TLS — front it with Caddy/nginx.

### API-keys file (`{data_dir}/api_keys.txt`)

One key per line: an **Argon2id PHC hash** followed by optional scoping metadata.
`chmod 600` it (the server warns otherwise). `#` comments and blank lines are ignored.

```text
# Bare hash = all-apps admin (backward-compatible; server logs a warning).
$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>

# Scoped publisher: may only publish to apps "foo" and "bar".
$argon2id$v=19$...$...   owner=acme   apps=foo,bar

# All-apps for a named owner, with a pinned pubkey so the server also
# verifies upload signatures over the canonical payload.
$argon2id$v=19$...$...   owner=ops    apps=*    pubkey=<64-hex-chars>
```

- `apps=*` or omitting `apps=` ⇒ all apps; `apps=` (empty) ⇒ locked out.
- `pubkey=` (32-byte Ed25519, 64 hex) ⇒ the server verifies the uploaded signature
  at publish time; without it, signature verification is skipped server-side (the
  client still verifies, always).
- Generate + register a key:
  ```bash
  KEY=$(vbdp-server --generate-api-key)
  echo "$KEY" | vbdp-server hash-key >> ./data/api_keys.txt
  ```

### Rate limits (per IP, per 60 s window)

| Endpoint group | Limit |
|---|---|
| check-update | 100/min |
| publish + diff upload | 10/min |
| admin | 30/min |
| download (binary/diff/signature) | 200/min |
| health | 60/min |

Auth backoff: 5 consecutive failures per IP → exponential backoff (base 60 s, cap
600 s), `429` while blocked. These buckets key on the socket peer IP unless
`--trust-proxy` is set.

### On-disk layout

```
{data_dir}/
├── api_keys.txt
└── apps/{app}/
    ├── catalog.json                         # versions[], latest_version
    ├── versions/{version}/
    │   ├── manifest.json                     # per-platform checksum/size/diffs_from
    │   ├── binary-{platform}                  # full binary, e.g. binary-linux-x86_64
    │   ├── signature.sig                      # 64-byte Ed25519 signature
    │   ├── rollout.json                       # rollout config
    │   └── .deleted                           # present ⇒ soft-deleted (files kept)
    └── diffs/{from}-to-{to}-{platform}        # diff blob
```

All JSON writes are atomic (temp + rename) under a file lock + per-app mutex. See
`docs/deployment/SERVER_DEPLOYMENT.md` and `docs/operations/*` for full ops detail.

---

## 10. HTTP API reference

For building a custom client or debugging. Base prefix `/api/v1`. "Auth" = requires
`X-API-Key`.

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/health` | no | Liveness: `{"status":"ok","version":"…"}`. |
| GET | `/api/v1/apps/{app}/check-update?current_version=&platform=&device_id_hash=` | no | Update check. |
| GET | `/api/v1/apps/{app}/versions/{version}/binary?platform=` | no | Download full binary (streamed, BLAKE3-verified). |
| GET | `/api/v1/apps/{app}/diffs/{from}-to-{to}?platform=` | no | Download a diff blob. |
| GET | `/api/v1/apps/{app}/versions/{version}/signature` | no | Download the 64-byte Ed25519 signature. |
| POST | `/api/v1/apps/{app}/versions/{version}` | yes | Publish a binary (multipart). |
| PUT | `/api/v1/apps/{app}/versions/{version}/activate` | yes | Make a version the latest. |
| PUT/GET | `/api/v1/apps/{app}/versions/{version}/rollout` | yes | Set/get staged-rollout percentage. |
| POST | `/api/v1/apps/{app}/diffs/{from}/to/{to}` | yes | Upload a diff (multipart). |
| GET | `/api/v1/admin/apps` · `/{app}` · `/{app}/versions` | yes | List apps / details / versions. |
| DELETE | `/api/v1/admin/apps/{app}/versions/{version}` | yes | Soft-delete a version. |
| GET | `/api/v1/admin/stats` | yes | Server stats. |

### `check-update` response

```jsonc
{
  "update_available": true,
  "target_version": "1.1.0",
  "diff_url": "/api/v1/apps/myapp/diffs/1.0.0-to-1.1.0?platform=linux-x86_64", // null if no diff for your version
  "diff_checksum": "<blake3-hex>",          // null when no diff
  "diff_size": 20480,                        // null when no diff
  "full_binary_url": "/api/v1/apps/myapp/versions/1.1.0/binary?platform=linux-x86_64",
  "full_binary_size": 5242880,
  "checksum": "<blake3-hex of final binary>",
  "signature_url": "/api/v1/apps/myapp/versions/1.1.0/signature",
  "release_notes": "…",                     // nullable
  "forced": false,
  "rollout_percentage": 100,                 // omitted if null
  "in_rollout": true                         // omitted if null
}
```

- URLs are **relative** to the server base — resolve them against `--server-url`.
- `current_version` omitted ⇒ treated as `0.0.0` (always offers the latest).
- `device_id_hash` (a stable per-device hash) drives consistent **staged rollouts**
  (BLAKE3 mod 100 < rollout percentage ⇒ in rollout).
- Checks/downloads are unauthenticated by design (public update channel); the
  signature is what guarantees integrity. Publishing/admin require `X-API-Key`.

Full request/response field tables live in `docs/api/API_SPECIFICATION.md`.

---

## 11. Security model

- **What's signed:** the publisher signs the canonical payload
  `b"VBDP-sig-v1\x00"` ++ the raw 32-byte BLAKE3 hash of the binary ++ the UTF-8
  version string. Binding the version means a signature for one version **cannot be
  replayed** for another. One implementation of this payload (`vbdp_crypto::signing_payload`)
  is shared by publisher, server, and client and is locked by a golden-vector test.
- **One keypair, many signatures:** a single project Ed25519 keypair signs every
  version. Each version has its own BLAKE3 hash and its own signature. The client
  pins the **one public key** at integration time.
- **Verify-then-replace:** hash + signature are checked **before** the installed
  binary is touched; replacement is atomic; the old binary is backed up; the
  installed file is re-hashed afterward and a `.sig` sidecar is written for
  rollback re-verification.
- **Downgrade protection:** the server only advertises *newer* versions; the CLI/
  daemon additionally enforce semver monotonicity (override with `--allow-downgrade`,
  never in the daemon).
- **Transport:** HTTPS is enforced by the client for non-loopback hosts; signature
  verification is **mandatory and not configurable**.
- **Install-path guard:** the client refuses to update binaries in privileged
  system directories (`/etc`, `/usr`, … and `C:\Windows`, `C:\Program Files[*]` on
  any drive, case/separator-insensitive), and rejects symlinked/root-owned targets.
- **Server keys:** API keys are stored as Argon2id hashes; verification is
  constant-time; keys can be scoped per publisher/app.

See `docs/security/SECURITY_MODEL.md` and `SECURITY.md` for the full threat model.

---

## 12. Rust in-process library API

If your host application is Rust, you can drive updates in-process (progress
callbacks, custom UI) instead of spawning the sidecar. Add the `vbdp-client` crate
and use:

```rust
use vbdp_client::{load_config, ClientConfig, UpdateChecker, apply_update};
// also available: run_daemon, run_check_once, rollback_to_backup, UpdateInfo

# async fn demo() -> anyhow::Result<()> {
// Construct config (or load_config(None) to read the default config file).
let config = ClientConfig {
    server_url: "https://updates.example.com".into(),
    ..ClientConfig::default()
};

// Open the client DB (from vbdp-db) at config.db_path(); register the managed
// binary with its pinned public key (mirrors what `vbdp-client add` does), then:
let checker = UpdateChecker::new(config.clone())?;
// let managed = /* DbManagedBinary loaded from the client DB */;
// if let Some(update) = checker.check_for_updates(&managed).await? {
//     apply_update(&config, &db, &managed, &update, None).await?; // verify + atomic replace
// }
# Ok(()) }
```

Key signatures (exact):

```rust
pub fn load_config(path: Option<&std::path::Path>) -> anyhow::Result<ClientConfig>;
impl UpdateChecker {
    pub fn new(config: ClientConfig) -> anyhow::Result<Self>;
    pub async fn check_for_updates(&self, binary: &DbManagedBinary)
        -> anyhow::Result<Option<UpdateInfo>>;
    pub async fn health_check(&self) -> anyhow::Result<bool>;
}
pub async fn apply_update(
    config: &ClientConfig, db: &ClientDb, managed_binary: &DbManagedBinary,
    update: &UpdateInfo, progress_bar: Option<&indicatif::ProgressBar>,
) -> anyhow::Result<()>;
pub async fn run_daemon(config: ClientConfig, db: ClientDb) -> anyhow::Result<()>;
pub async fn run_check_once(config: &ClientConfig, db: &ClientDb) -> anyhow::Result<()>;
```

`ClientConfig` fields: `server_url` (default `http://localhost:3000` — set yours),
`check_interval_secs` (3600), `data_dir`, `disk_check_mode`. Signature verification
is always on; there is no toggle.

---

## 13. Troubleshooting & FAQ

| Symptom | Likely cause / fix |
|---|---|
| Updater exits `1`, stderr mentions HTTPS | `--server-url` is `http://` for a non-loopback host. Use `https://` (only loopback may use http). |
| "Security error: cannot update binary in privileged location" | `--install-path` is under a system dir. Install the app somewhere user-writable. |
| Update never offered though a new version exists | The published version isn't *activated* as latest, the `platform` doesn't match, or `current_version` ≥ latest. Check `GET /admin/apps/{app}/versions`. |
| Signature verification fails on the client | Wrong/old `public.key` pinned, or the version wasn't signed before publish. Re-`sign` then re-`publish`; re-bundle `public.key`. |
| Always downloads full binary, never a diff | The client's current version is older than the last 3 releases, or the diff wasn't smaller than the full binary (expected fallback). |
| Publish returns 403 | The API key is scoped (`apps=`) and not allowed for this app. Use an admin key or add the app to its scope. |
| Rate-limit `429`s behind a proxy hit everyone | Server isn't trusting the proxy. Add `--trust-proxy` / `VBDP_TRUST_PROXY=1`. |
| Windows: "file in use"/can't replace exe | You're updating a running `.exe`. Use the launcher pattern in [§7](#7-per-platform-integration-notes). |

---

## 14. Known gaps & caveats

These are honest, code-verified rough edges to plan around:

1. **Diff depth is fixed at 3.** Clients more than 3 releases behind always get a
   full download. There is no CLI flag to raise it today (the builder exists in
   source but isn't wired up).
2. **`--trust-proxy` is not enabled in the shipped `deploy/docker-compose.yml`.**
   Add it, or rate-limit/auth-backoff buckets collapse onto Caddy's IP.
3. **Caddyfile body-size comment is stale.** It mentions 2 GB; the server actually
   caps bodies at 600 MB (512 MB binary / 256 MB diff). Uploads above the server cap
   are rejected by the server even if Caddy lets them through.
4. **No C FFI / shared library.** Non-Rust apps must use the sidecar or daemon, not
   in-process linking. (If you need tight in-process control from C/C++/C#, an
   `extern "C"` `cdylib` layer would be the natural addition.)
5. **The sidecar has no client-side `--allow-downgrade` guard;** it trusts the
   server's newer-only logic. Use `vbdp-client`/daemon if you want the extra
   monotonicity check on the client.
6. **Server default port (8080) ≠ client default `server_url` (`localhost:3000`).**
   Always pass explicit URLs; don't rely on defaults lining up.
7. **macOS notarization** of an `.app` can be invalidated by in-place binary
   replacement — plan your update target accordingly.
8. **`published_at` is not tracked** (always null in admin version listings).

For the full architecture and protocol, see `docs/architecture/SYSTEM_DESIGN.md`
and `docs/architecture/PROTOCOL_SPECIFICATION.md`.
