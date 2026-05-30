# Publishing to Deltaship from GitHub Actions

`publish-template.yml` is a copy-paste workflow that builds your application
on Linux, macOS (Intel + Apple Silicon), and Windows, then registers, signs,
and publishes each binary to a Deltaship server when you push a `vX.Y.Z` tag.

## Install

1. Copy `publish-template.yml` into your application repo at
   `.github/workflows/publish.yml`.
2. Set `APP_NAME` (top of the file) to your binary name. Update the build
   step if your repo isn't a single `cargo build --release --bin $APP_NAME`.
3. Configure the four secrets below.
4. Push a tag like `v1.0.0`.

## Required secrets

Set these under **Settings -> Secrets and variables -> Actions** in your
application repo.

| Secret | What it is |
| --- | --- |
| `DELTASHIP_SERVER_URL` | Base URL of your `deltaship-server`, e.g. `https://updates.example.com`. |
| `DELTASHIP_API_KEY` | Plaintext API key issued by your server admin. The server stores its Argon2id hash in `<data-dir>/api_keys.txt`; the client sends the plaintext in the `X-API-Key` header. |
| `DELTASHIP_SIGNING_KEY` | Full contents of `.deltaship/keys/signing.key` (the encrypted Ed25519 PEM file produced by `deltaship-publisher init` or `keygen`). Paste the file verbatim including the `-----BEGIN DELTASHIP ENCRYPTED SIGNING KEY-----` headers. |
| `DELTASHIP_SIGNING_PASSPHRASE` | Passphrase that decrypts `DELTASHIP_SIGNING_KEY`. Minimum 12 characters. |

## How signing keys flow through CI

`deltaship-publisher` keeps signing keys at `.deltaship/keys/signing.key`, encrypted
with a passphrase. The workflow does **not** generate a fresh key per run —
that would make every release verify under a different public key, breaking
clients. Instead:

1. **Locally, once:** run `deltaship-publisher init --passphrase '<passphrase>'`
   on your workstation (or a one-shot trusted machine). This produces:
   - `.deltaship/keys/signing.key` — the encrypted private key. Paste its full
     contents into the `DELTASHIP_SIGNING_KEY` GitHub secret.
   - `.deltaship/keys/public.key` — the public key. Ship this with your app
     bundle so `deltaship-updater` can verify downloads.
2. **In CI:** the workflow runs `deltaship-publisher init` with the real
   passphrase to bootstrap `.deltaship/publisher.db` (which `register` requires),
   then overwrites the generated `signing.key` with the secret. The result
   is a workspace whose database is fresh per run but whose signing identity
   is the persistent one from your secret.
3. `deltaship-publisher sign --passphrase "$DELTASHIP_SIGNING_PASSPHRASE"` decrypts
   the key in-process and signs the BLAKE3 hash recorded by `register`.

Never commit `.deltaship/keys/signing.key` or the passphrase to your repo. Keep
the passphrase out of build logs (`${{ secrets.* }}` is masked by Actions).

## Customisation points

Search the workflow for `# CHANGE ME:` markers. The common ones:

- `APP_NAME` — your binary name.
- The **Build application binary** step — replace `cargo build --release
  --bin "${APP_NAME}"` with whatever your project uses (Make, Bazel, npm
  build, multiple binaries, etc.). The output must end up at
  `target/release/${APP_NAME}${ext}` or you must adjust the staging step.
- The **build matrix** — add/remove platforms. If you change the matrix,
  also update the `PLATFORMS` map in the publish job. Valid platform
  identifiers (from `deltaship-core`): `linux-x86_64`, `linux-aarch64`,
  `windows-x86_64`, `macos-x86_64`, `macos-aarch64`.
- `DELTASHIP_REF` — pin to a specific commit/tag of the `jayashankarvr/deltaship`
  repo to make CI reproducible. Defaults to `main` because there are no
  binary releases of `deltaship-publisher` yet.

## Known gaps in the publisher CLI

- **No release of `deltaship-publisher`.** The workflow builds it from source on
  every run (cached by `Swatinem/rust-cache@v2`). Once the upstream project
  ships binary releases, replace the build step with a `gh release download`
  call against `jayashankarvr/deltaship`.
- **`init` is mandatory before `register`.** There is no
  `--bootstrap-from-existing-key` flag, so the workflow runs `init` and
  then overwrites the generated signing key. If `init` is ever changed to
  refuse running when keys would be discarded, this template will need
  updating.
- **No environment-variable fallback for `--api-key` or `--passphrase`.**
  Both must be passed as flags. The workflow expands secrets directly into
  the command line; GitHub masks them in logs, but be aware that any
  process running inside the same job can see the value via `/proc`.
