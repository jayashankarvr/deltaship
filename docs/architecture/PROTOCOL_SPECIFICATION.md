# Protocol Specification

**Document:** Communication protocols for Deltaship components
**Audience:** Developers, integrators, protocol implementers
**Last Updated:** 2026-01-07

---

## Overview

This document specifies the communication protocols used between Deltaship components: Publisher Toolkit, Update Server, and Client Patcher.

**Protocol Layers:**
1. **Transport:** HTTP/HTTPS (TLS 1.2+)
2. **Application:** REST API with JSON payloads
3. **Security:** Ed25519 signatures, Blake3 hashes

---

## Protocol Principles

### Stateless Communication
- Each request is independent
- Server doesn't maintain client session state
- Enables horizontal scaling and load balancing

### Idempotency
- Publishing same version twice has same effect as once
- GET operations are always idempotent
- POST operations use idempotency keys where needed

### Versioning
- API version in URL: `/api/v1/...`
- Backward compatibility maintained within major version
- Deprecation warnings in response headers

---

## Client-Server Protocol

### 1. Update Check

**Request:**
```
GET /api/v1/check-update HTTP/1.1
Host: updates.example.com
User-Agent: deltaship-client/1.0.0
Accept: application/json

Query Parameters:
  app={app_name}
  version={current_version}
  platform={os}
  arch={architecture}
  device_id={hash}  # Optional, for rollout targeting
```

**Response (Update Available):**
```
HTTP/1.1 200 OK
Content-Type: application/json
Cache-Control: no-cache
X-Deltaship-Server-Version: 1.0.0

{
  "update_available": true,
  "target_version": "1.1.0",
  "diff": {
    "url": "https://cdn.example.com/diffs/...",
    "size": 1200000,
    "hash": "blake3:abc123...",
    "signature_url": "https://cdn.example.com/sigs/..."
  },
  "full_binary": {
    "url": "https://cdn.example.com/binaries/...",
    "size": 85400000,
    "hash": "blake3:def456..."
  },
  "rollout_percentage": 50
}
```

**Response (No Update):**
```
HTTP/1.1 200 OK
Content-Type: application/json

{
  "update_available": false,
  "message": "Already on latest version"
}
```

### 2. Diff Download

**Request:**
```
GET /api/v1/download-diff/{diff_id} HTTP/1.1
Host: updates.example.com
Range: bytes=0-1000000  # Optional, for resume
Accept: application/octet-stream
```

**Response:**
```
HTTP/1.1 200 OK  # Or 206 Partial Content if Range requested
Content-Type: application/octet-stream
Content-Length: 1200000
ETag: "abc123"
X-Deltaship-Hash: blake3:abc123...
X-Deltaship-From-Version: 1.0.0
X-Deltaship-To-Version: 1.1.0
Cache-Control: public, max-age=2592000

[Binary diff data]
```

### 3. Status Report

**Request:**
```
POST /api/v1/report-status HTTP/1.1
Host: updates.example.com
Content-Type: application/json

{
  "app": "myapp",
  "device_id": "abc123",
  "old_version": "1.0.0",
  "new_version": "1.1.0",
  "success": true,
  "method": "diff",
  "duration_seconds": 15,
  "timestamp": "2026-01-07T12:34:56Z"
}
```

**Response:**
```
HTTP/1.1 200 OK
Content-Type: application/json

{
  "status": "recorded"
}
```

---

## Publisher-Server Protocol

### 1. Publish Initialization

**Request:**
```
POST /api/v1/publish/init HTTP/1.1
Host: updates.example.com
Content-Type: application/json
X-Deltaship-API-Key: deltaship_publisher_abc123...
X-Deltaship-Timestamp: 1704628496
X-Deltaship-Signature: hmac_sha256(secret, "timestamp:body")

{
  "app": "myapp",
  "version": "1.1.0",
  "platform": "linux",
  "architecture": "x86_64",
  "binary_size": 85400000,
  "binary_hash": "blake3:def456...",
  "diffs": [
    {
      "from_version": "1.0.0",
      "size": 1200000,
      "hash": "blake3:abc123..."
    }
  ]
}
```

**Response:**
```
HTTP/1.1 200 OK
Content-Type: application/json

{
  "publication_id": "pub_xyz789",
  "upload_urls": {
    "binary": "https://s3.../upload?signature=...",
    "diffs": [{
      "from_version": "1.0.0",
      "url": "https://s3.../upload?signature=..."
    }]
  }
}
```

### 2. Upload Files

**Request (to S3 pre-signed URL):**
```
PUT https://s3.amazonaws.com/bucket/path?signature=... HTTP/1.1
Content-Type: application/octet-stream
Content-Length: 1200000

[Binary data]
```

### 3. Finalize Publication

**Request:**
```
POST /api/v1/publish/finalize HTTP/1.1
Host: updates.example.com
Content-Type: application/json
X-Deltaship-API-Key: deltaship_publisher_abc123...
X-Deltaship-Timestamp: 1704628596
X-Deltaship-Signature: hmac_sha256(secret, "timestamp:body")

{
  "publication_id": "pub_xyz789"
}
```

**Response:**
```
HTTP/1.1 200 OK
Content-Type: application/json

{
  "status": "published",
  "version": "1.1.0",
  "cdn_urls": {
    "binary": "https://cdn.example.com/binaries/...",
    "diffs": ["https://cdn.example.com/diffs/..."]
  }
}
```

---

## Binary Format Specifications

### Diff File Format

**Structure:**
```
[Deltaship Header]  # 64 bytes
[bsdiff data]  # Variable length
```

**Deltaship Header (64 bytes):**
```
Bytes 0-3:   Magic number: "Deltaship" (0x56 0x42 0x44 0x50)
Bytes 4-7:   Format version: uint32 (current: 1)
Bytes 8-15:  Diff algorithm: "bsdiff\0\0" or "courgtt" (8 bytes, null-padded)
Bytes 16-23: From version hash: first 8 bytes of Blake3
Bytes 24-31: To version hash: first 8 bytes of Blake3
Bytes 32-39: Diff size: uint64 (little-endian)
Bytes 40-47: Target binary size: uint64
Bytes 48-63: Reserved (zero-filled)
```

**Followed by:** Algorithm-specific diff data (bsdiff, Courgette, etc.)

### Signature File Format

**JSON structure:**
```json
{
  "version": 1,
  "algorithm": "ed25519",
  "signed_data": {
    "app": "myapp",
    "version": "1.1.0",
    "platform": "linux",
    "architecture": "x86_64",
    "from_version": "1.0.0",  # For diffs
    "diff_hash": "blake3:abc123...",
    "binary_hash": "blake3:def456...",
    "timestamp": "2026-01-07T10:00:00Z"
  },
  "signature": "base64_encoded_signature",
  "public_key_fingerprint": "sha256:fingerprint"
}
```

**Canonical JSON:** RFC 8785 (deterministic JSON serialization)

---

## Error Codes

### HTTP Status Codes

| Code | Meaning | Use Case |
|------|---------|----------|
| 200 | OK | Successful request |
| 206 | Partial Content | Range request served |
| 400 | Bad Request | Invalid parameters |
| 401 | Unauthorized | Invalid API key/signature |
| 403 | Forbidden | Insufficient permissions |
| 404 | Not Found | Resource doesn't exist |
| 409 | Conflict | Version already exists |
| 429 | Too Many Requests | Rate limit exceeded |
| 500 | Internal Server Error | Server-side error |
| 503 | Service Unavailable | Maintenance or overload |

### Application Error Codes

**Format:**
```json
{
  "error": "error_code",
  "message": "Human-readable description",
  "details": {
    "field": "additional_context"
  },
  "request_id": "req_abc123"
}
```

**Error Codes:**
- `invalid_request` - Malformed request
- `invalid_version` - Version format invalid
- `version_exists` - Version already published
- `diff_not_found` - Diff unavailable for version transition
- `signature_invalid` - Signature verification failed
- `hash_mismatch` - File hash doesn't match declared
- `rate_limit_exceeded` - Too many requests
- `rollout_not_active` - Device not in rollout group

---

## Authentication

### API Key Authentication (Publishers)

**Header Format:**
```
X-Deltaship-API-Key: deltaship_publisher_{64_hex_chars}
X-Deltaship-Timestamp: {unix_timestamp}
X-Deltaship-Signature: {hmac_sha256_signature}
```

**Signature Computation:**
```
timestamp = current_unix_time()
payload = canonical_json(request_body)
signature = hmac_sha256(api_secret, f"{timestamp}:{payload}")
```

**Server Verification:**
1. Check timestamp within 5 minutes (prevents replay)
2. Compute expected signature
3. Constant-time comparison with provided signature
4. Reject if mismatch

### No Authentication (Clients)

- Download endpoints require no authentication (updates are public)
- Optional device_id for analytics and rollout targeting
- Signature verification provides authenticity

---

## Compression and Encoding

### Content Encoding

**Request:**
```
Accept-Encoding: gzip, deflate, br
```

**Response:**
```
Content-Encoding: br  # Brotli compression (best for JSON)
```

**Supported:** gzip, deflate, brotli (br)

### Transfer Encoding

**Chunked Transfer (for large files):**
```
Transfer-Encoding: chunked
```

Enables streaming without knowing total size upfront.

---

## Caching

### Cache Headers

**Immutable Resources (diffs, binaries):**
```
Cache-Control: public, max-age=31536000, immutable
ETag: "blake3_hash_of_content"
```

**Versioned URLs:** `https://cdn.example.com/diffs/myapp-1.0.0-to-1.1.0.diff`
- Version in URL ensures uniqueness
- Never changes, safe to cache forever

**Mutable Resources (check-update):**
```
Cache-Control: no-cache
Vary: User-Agent
```

### Conditional Requests

**Client sends:**
```
If-None-Match: "blake3_hash"
```

**Server responds:**
```
HTTP/1.1 304 Not Modified
```

(Saves bandwidth if client already has correct version)

---

## Rate Limiting

### Rate Limit Headers

**Included in all responses:**
```
X-RateLimit-Limit: 1000  # Requests allowed per window
X-RateLimit-Remaining: 847  # Requests remaining
X-RateLimit-Reset: 1704632096  # Unix timestamp when limit resets
```

### Rate Limit Algorithm

**Token Bucket:**
- Bucket size: 1000 requests
- Refill rate: 1000 requests per hour
- Burst allowed up to bucket size

**Limits by Endpoint:**
- check-update: 100/hour per device
- download-diff: 10/hour per device
- publish: 100/hour per API key

---

## Idempotency

### Idempotent Operations

**GET (always idempotent):**
- check-update
- download-diff
- download-binary

**POST (conditionally idempotent):**
- publish/init: Idempotent if same version re-published (returns existing publication_id)
- publish/finalize: Idempotent if called multiple times for same publication_id
- report-status: Idempotent (duplicate reports ignored)

**Idempotency Key (optional):**
```
Idempotency-Key: uuid_v4
```

Server stores result for 24 hours, returns cached result if same key sent again.

---

## Protocol Extensions (Future)

### Webhooks (Phase 2)

**Publisher registers webhook URL:**
```
POST /api/v1/webhooks
{
  "url": "https://publisher.com/webhook",
  "events": ["update.success", "update.failed", "rollout.completed"]
}
```

**Server sends events:**
```
POST https://publisher.com/webhook
X-Deltaship-Event: update.success
X-Deltaship-Signature: hmac_sha256(webhook_secret, body)

{
  "event": "update.success",
  "timestamp": "2026-01-07T15:00:00Z",
  "data": {
    "app": "myapp",
    "version": "1.1.0",
    "count": 1250
  }
}
```

### WebSocket Support (Phase 3)

**For real-time updates:**
```
ws://updates.example.com/ws/subscribe?app=myapp&device_id=abc123
```

**Server pushes update notifications:**
```json
{
  "type": "update_available",
  "version": "1.1.0",
  "diff_url": "..."
}
```

---

## Security Considerations

### Transport Security

- **TLS 1.2+ mandatory** (no plain HTTP)
- **Certificate validation required** on client
- **Optional certificate pinning** for high-security deployments

### Request Signing

- **HMAC-SHA256** for publisher authentication
- **Ed25519** for content signatures
- **Constant-time comparison** to prevent timing attacks

### Replay Protection

- **Timestamp validation** (5-minute window)
- **Nonce support** (optional, for paranoid security)

### Content Validation

- **Blake3 hashes** for all binaries and diffs
- **Signature verification** before applying updates
- **Size limits** to prevent DoS (max diff size: 500MB)

---

## Compatibility

### Version Negotiation

**Client sends:**
```
X-Deltaship-Client-Version: 1.0.0
Accept: application/vnd.deltaship.v1+json
```

**Server responds:**
```
X-Deltaship-Server-Version: 1.0.0
Content-Type: application/vnd.deltaship.v1+json
```

**If client version too old:**
```
HTTP/1.1 426 Upgrade Required
X-Deltaship-Minimum-Client-Version: 1.2.0

{
  "error": "client_too_old",
  "message": "Please upgrade to client version 1.2.0 or higher",
  "upgrade_url": "https://deltaship.io/download"
}
```

---

## Next Steps

**For Implementers:**
- Implement client protocol in your language
- Follow authentication requirements exactly
- Validate all responses (status codes, hashes, signatures)

**For Server Operators:**
- Implement rate limiting
- Configure caching headers correctly
- Monitor for protocol violations

**Related Documents:**
- [API Specification](../api/API_SPECIFICATION.md) - Full API reference
- [Security Model](../security/SECURITY_MODEL.md) - Security architecture
- [System Design](SYSTEM_DESIGN.md) - Overall system architecture

---

**End of Protocol Specification**
