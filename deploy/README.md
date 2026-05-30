# Deltaship Update Server - Docker Deployment

Turnkey deployment of `deltaship-server` behind Caddy with automatic TLS.
The server is built from the workspace at the repo root; this directory only
holds the deployment glue.

## Layout

| File | Purpose |
| --- | --- |
| `Dockerfile.server` | Multi-stage build of `deltaship-server` (Debian slim runtime, non-root user). |
| `docker-compose.yml` | `deltaship-server` + `caddy` services, internal-only network. |
| `Caddyfile` | TLS termination + reverse proxy to `deltaship-server:8080`. |
| `.env.example` | The two values you must fill in: `DOMAIN`, `ACME_EMAIL`. |

The server listens on `0.0.0.0:8080` inside its container and is **not**
published to the host - only Caddy on `:80` / `:443` is reachable from outside.

## One-time setup

1. **Configure the domain.** Point an A/AAAA record at this host, then:

   ```sh
   cd deploy
   cp .env.example .env
   $EDITOR .env   # set DOMAIN and ACME_EMAIL
   ```

2. **Create the data directory and seed API keys.** The server reads
   `${data_dir}/api_keys.txt` at startup and expects one Argon2id PHC hash
   per line (plaintext keys are rejected). The repo ships two helpers on
   the `deltaship-server` binary itself:

   - `deltaship-server --generate-api-key` - prints a fresh 64-char hex key.
   - `deltaship-server hash-key <plaintext>` - prints the Argon2id hash to store.

   Build the image and use it to generate a key without installing anything
   on the host:

   ```sh
   mkdir -p data
   docker compose build deltaship-server

   # Generate a plaintext key. Save this somewhere safe - publishers will
   # send it as the X-API-Key header. It is NOT recoverable from the hash.
   PLAINTEXT=$(docker compose run --rm --no-deps deltaship-server --generate-api-key)
   echo "$PLAINTEXT"  # <-- copy to your password manager

   # Append the corresponding Argon2id hash to api_keys.txt.
   echo "$PLAINTEXT" \
     | docker compose run --rm --no-deps -T deltaship-server hash-key \
     >> data/api_keys.txt

   chmod 600 data/api_keys.txt
   ```

   Repeat the last two commands once per publisher. Lines starting with `#`
   are treated as comments.

3. *(Optional)* Edit `Caddyfile` if you are not using public Let's Encrypt -
   for example, uncomment the `acme_ca` staging line while testing, or replace
   the site block with a `tls /path/to/cert /path/to/key` directive for a
   private CA.

## Run

```sh
docker compose up -d
docker compose logs -f
```

Caddy will request a certificate on first start; this can take 10-60s. Once
the `deltaship-server` healthcheck reports healthy, traffic is live.

## Verify

```sh
# Public health endpoint - no auth required.
curl https://$DOMAIN/health

# Authenticated endpoint - replace with a key you minted above.
curl -H "X-API-Key: $PLAINTEXT" https://$DOMAIN/api/v1/admin/stats
```

The server's health route is `GET /health` (not `/api/v1/health`); the
docker healthcheck and the command above both target it directly.

## Backup

Two paths hold all stateful data:

- `./data/` - catalogs, binaries, diffs, signatures, and `api_keys.txt`.
  This is the only thing you need for disaster recovery of update content.
- The named docker volume `caddy_data` - ACME account key and issued
  certificates. Losing it just forces Caddy to re-issue certs on next start
  (subject to Let's Encrypt rate limits), so it is nice-to-have, not critical.

A simple offline snapshot:

```sh
docker compose stop deltaship-server
tar -czf deltaship-data-$(date +%F).tgz data/
docker compose start deltaship-server
```

For online backups, `data/` is safe to copy with `rsync` while the server is
running - writes are atomic per-file, but you may capture a publish in
progress; re-run the backup until it converges.

## Upgrade

```sh
git pull
docker compose build deltaship-server
docker compose up -d
```

`docker compose pull` only helps once you publish the image to a registry;
for source builds, `build` is the equivalent step.

## Caveats

- The server has **no built-in migrations** for stored content. Major version
  upgrades may require manual data migration - check `CHANGELOG.md` before
  upgrading across minor versions.
- The container runs as UID 10001 (`deltaship`). If you bind-mount `./data` from
  a host directory owned by another user, fix ownership with
  `sudo chown -R 10001:10001 data/` or the server will fail to write.
- `DELTASHIP_CORS_ORIGINS` is preset in `docker-compose.yml` to your `DOMAIN`.
  Edit it there if browsers from other origins also need to call the API.
- The `--generate-api-key` and `hash-key` subcommands are documented in
  `crates/deltaship-server/src/main.rs`. There is no separate admin CLI.
