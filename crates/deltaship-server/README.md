# Deltaship Update Server

A REST API server for serving software updates using the Deltaship protocol.

## Quick Start

```bash
# Start the server
deltaship-server --host 127.0.0.1 --port 8080 --data-dir ./data

# Generate an API key for publishers
deltaship-server --generate-api-key
```

## TLS/HTTPS Requirements

**IMPORTANT:** This server does NOT handle TLS/HTTPS encryption directly. It binds to a plain HTTP socket and expects to be deployed behind a reverse proxy that handles TLS termination.

### Why No Built-in TLS?

Following the Unix philosophy and best practices for production deployments:

- **Separation of Concerns:** TLS termination is better handled by specialized tools (nginx, Caddy, Traefik)
- **Certificate Management:** Reverse proxies provide automatic certificate renewal (Let's Encrypt)
- **Performance:** Dedicated reverse proxies are optimized for TLS handling
- **Flexibility:** Easy to add load balancing, caching, and other features

### Production Deployment

In production, you MUST use HTTPS to protect:
- API keys transmitted in authentication headers
- Client privacy (version info, device IDs)
- Metadata about available updates

**Note:** Update content itself is cryptographically signed, preventing tampering even over HTTP. However, metadata and authentication still require HTTPS protection.

### Example: Nginx Reverse Proxy

```nginx
server {
    listen 80;
    server_name updates.example.com;
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name updates.example.com;

    # TLS certificate (managed by certbot)
    ssl_certificate /etc/letsencrypt/live/updates.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/updates.example.com/privkey.pem;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-Frame-Options "DENY" always;

    # Proxy to Deltaship server
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Example: Caddy (Automatic HTTPS)

Caddy automatically obtains and renews TLS certificates:

```
updates.example.com {
    reverse_proxy localhost:8080
}
```

### Local Development

For local development and testing, you can run without TLS:

```bash
deltaship-server --host 127.0.0.1 --port 8080
```

The server will detect localhost binding and won't show TLS warnings.

## Configuration

```bash
# Basic options
--host <IP>          # Host address to bind to (default: 127.0.0.1)
--port <PORT>        # Port to listen on (default: 8080)
--data-dir <PATH>    # Directory for storing data (default: ./data)

# Verbosity
-v, -vv, -vvv        # Increase logging verbosity
--quiet              # Suppress all output except errors

# API key management
--generate-api-key   # Generate a new API key and exit
```

## Environment Variables

```bash
# API authentication
export DELTASHIP_API_KEY="your-generated-api-key-here"

# CORS configuration (production)
export DELTASHIP_CORS_ORIGINS="https://example.com,https://api.example.com"
```

## Security Best Practices

1. **Use HTTPS in production** - Deploy behind a reverse proxy with TLS
2. **Protect API keys** - Set `DELTASHIP_API_KEY` environment variable, never commit to code
3. **Configure CORS** - Set `DELTASHIP_CORS_ORIGINS` to restrict allowed origins
4. **Run as non-root** - Use a dedicated service account
5. **Keep updated** - Regularly update dependencies and server software

## Deployment

See the [Server Deployment Guide](../../docs/deployment/SERVER_DEPLOYMENT.md) for detailed production deployment instructions including:

- Small deployment (single server)
- Medium deployment (load balanced with PostgreSQL and S3)
- Large deployment (Kubernetes with auto-scaling)
- Monitoring and alerting setup
- Backup strategies

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
