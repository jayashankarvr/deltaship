# API Specification

**Document:** Complete REST API specification for Deltaship Update Server
**Audience:** Developers, integrators, API consumers
**Last Updated:** 2026-01-07

---

## Overview

The Deltaship Update Server provides a RESTful HTTP API for:
- Client patchers checking for and downloading updates
- Publishers uploading new versions
- Administrators managing the system
- Analytics and reporting

**Base URL:** `https://updates.example.com/api`

**API Version:** v1

**Protocol:** HTTPS only (TLS 1.2+)

**Data Format:** JSON

**Authentication:** API keys (HMAC-signed requests for publishers)

---

## API Endpoints

### Public Endpoints (No Authentication)

#### GET /health

**Description:** Health check endpoint for monitoring

**Request:**
```
GET /health
```

**Response:**

**Success (200 OK):**
```json
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime_seconds": 3600,
  "timestamp": "2026-01-07T12:34:56Z"
}
```

**Unhealthy (503 Service Unavailable):**
```json
{
  "status": "unhealthy",
  "errors": [
    "database_connection_failed",
    "storage_unavailable"
  ],
  "timestamp": "2026-01-07T12:34:56Z"
}
```

**Use Case:** Load balancer health checks, monitoring systems

---

#### GET /api/check-update

**Description:** Check if update is available for application

**Request:**

```
GET /api/check-update?app={app_name}&version={current_version}&platform={platform}&arch={architecture}&device_id={device_id}
```

**Query Parameters:**

| Parameter | Type | Required | Description | Example |
|-----------|------|----------|-------------|---------|
| app | string | Yes | Application name | `myapp` |
| version | string | Yes | Current version (semver) | `1.0.0` |
| platform | string | Yes | Operating system | `linux`, `windows`, `macos` |
| arch | string | Yes | CPU architecture | `x86_64`, `arm64` |
| device_id | string | No | Anonymized device identifier | `abc123...` (SHA-256 hash) |

**Response:**

**Update Available (200 OK):**
```json
{
  "update_available": true,
  "target_version": "1.1.0",
  "release_date": "2026-01-05T10:00:00Z",
  "diff_available": true,
  "diff": {
    "url": "https://cdn.example.com/diffs/myapp-1.0.0-to-1.1.0-linux-x86_64.diff",
    "size_bytes": 1200000,
    "hash": "blake3:a1b2c3d4e5f6...",
    "signature_url": "https://cdn.example.com/diffs/myapp-1.0.0-to-1.1.0.sig"
  },
  "full_binary": {
    "url": "https://cdn.example.com/binaries/myapp-1.1.0-linux-x86_64.bin",
    "size_bytes": 85400000,
    "hash": "blake3:f7g8h9i0j1k2...",
    "signature_url": "https://cdn.example.com/binaries/myapp-1.1.0.sig"
  },
  "changelog_url": "https://example.com/changelog/1.1.0",
  "rollout_percentage": 50,
  "rollout_group": "stable",
  "force_update": false,
  "min_supported_version": "0.9.0"
}
```

**No Update Available (200 OK):**
```json
{
  "update_available": false,
  "current_version": "1.1.0",
  "message": "You are on the latest version"
}
```

**Rollout Not Active for Device (200 OK):**
```json
{
  "update_available": true,
  "target_version": "1.1.0",
  "diff_available": false,
  "message": "Update available but not yet rolled out to this device",
  "rollout_percentage": 10,
  "estimated_availability": "2026-01-08T00:00:00Z"
}
```

**Invalid Request (400 Bad Request):**
```json
{
  "error": "invalid_request",
  "message": "Missing required parameter: version"
}
```

**Notes:**
- Server determines if device is in rollout group based on device_id hash and rollout_percentage
- If diff not available for this version transition, diff_available = false (client must use full binary)

---

#### GET /api/download-diff/{diff_id}

**Description:** Download differential update

**Request:**
```
GET /api/download-diff/myapp-1.0.0-to-1.1.0-linux-x86_64
```

**Response:**

**Success (200 OK):**
```
Content-Type: application/octet-stream
Content-Length: 1200000
Content-Disposition: attachment; filename="myapp-1.0.0-to-1.1.0-linux-x86_64.diff"
ETag: "abc123..."
Cache-Control: public, max-age=2592000

[Binary diff data]
```

**Headers:**
- `X-Deltaship-Hash`: `blake3:a1b2c3d4e5f6...` (for integrity verification)
- `X-Deltaship-Size`: `1200000` (for progress tracking)

**Not Found (404 Not Found):**
```json
{
  "error": "diff_not_found",
  "message": "Diff not found for specified version transition"
}
```

**Notes:**
- Supports HTTP Range requests (for resume capability)
- Served from CDN for best performance
- Client should verify hash after download

---

#### GET /api/download-binary/{binary_id}

**Description:** Download full binary (fallback if diff not available or diff application fails)

**Request:**
```
GET /api/download-binary/myapp-1.1.0-linux-x86_64
```

**Response:**

**Success (200 OK):**
```
Content-Type: application/octet-stream
Content-Length: 85400000
Content-Disposition: attachment; filename="myapp-1.1.0-linux-x86_64.bin"
ETag: "def456..."
Cache-Control: public, max-age=2592000

[Binary data]
```

**Headers:**
- `X-Deltaship-Hash`: `blake3:f7g8h9i0j1k2...`
- `X-Deltaship-Size`: `85400000`
- `X-Deltaship-Version`: `1.1.0`

**Not Found (404 Not Found):**
```json
{
  "error": "binary_not_found",
  "message": "Binary not found for specified version"
}
```

---

#### GET /api/download-signature/{signature_id}

**Description:** Download cryptographic signature for verification

**Request:**
```
GET /api/download-signature/myapp-1.0.0-to-1.1.0-linux-x86_64
```

**Response:**

**Success (200 OK):**
```json
{
  "signed_data": {
    "version": "1.1.0",
    "app": "myapp",
    "platform": "linux",
    "architecture": "x86_64",
    "from_version": "1.0.0",
    "diff_hash": "blake3:a1b2c3d4e5f6...",
    "diff_size": 1200000,
    "target_binary_hash": "blake3:f7g8h9i0j1k2...",
    "target_binary_size": 85400000,
    "timestamp": "2026-01-05T10:00:00Z",
    "publisher": "Example Inc."
  },
  "signature": "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXowMTIzNDU2Nzg5...",
  "public_key_fingerprint": "sha256:9f86d081884c7d659a2feaa0c55ad015...",
  "algorithm": "ed25519",
  "signature_version": 1
}
```

---

#### POST /api/report-status

**Description:** Client reports update result (optional, for analytics)

**Request:**

```
POST /api/report-status
Content-Type: application/json

{
  "app": "myapp",
  "device_id": "abc123...",
  "old_version": "1.0.0",
  "new_version": "1.1.0",
  "platform": "linux",
  "architecture": "x86_64",
  "success": true,
  "update_method": "diff",
  "duration_seconds": 15,
  "diff_size_bytes": 1200000,
  "bandwidth_saved_bytes": 84200000,
  "timestamp": "2026-01-07T12:34:56Z",
  "error_message": null
}
```

**Response:**

**Success (200 OK):**
```json
{
  "status": "recorded",
  "message": "Thank you for the feedback"
}
```

**Notes:**
- No authentication required (data is anonymized)
- Used for analytics and monitoring
- Helps publisher track update success rates

---

### Publisher Endpoints (Requires Authentication)

#### Authentication

**All publisher endpoints require authentication:**

**Headers:**
```
X-Deltaship-API-Key: deltaship_publisher_a1b2c3d4e5f6...
X-Deltaship-Timestamp: 1704628496
X-Deltaship-Signature: hmac_sha256(api_secret, "{timestamp}:{request_body}")
```

**Signature Computation:**

```
timestamp = current_unix_timestamp()
payload = json.dumps(request_body, separators=(',', ':'))  # Compact JSON, sorted keys
signature = hmac_sha256(api_secret, f"{timestamp}:{payload}")
```

**Server Verification:**
- Check timestamp within 5 minutes (prevents replay attacks)
- Compute expected signature
- Compare with provided signature (constant-time comparison)

**Errors:**

**401 Unauthorized:**
```json
{
  "error": "unauthorized",
  "message": "Invalid API key or signature"
}
```

**403 Forbidden:**
```json
{
  "error": "forbidden",
  "message": "API key does not have permission for this operation"
}
```

---

#### POST /api/publish/init

**Description:** Initialize publication of new version (registers version, creates upload URLs)

**Authentication:** Required (Publisher)

**Request:**

```
POST /api/publish/init
Content-Type: application/json

{
  "app": "myapp",
  "version": "1.1.0",
  "platform": "linux",
  "architecture": "x86_64",
  "binary_size": 85400000,
  "binary_hash": "blake3:f7g8h9i0j1k2...",
  "changelog": "## Version 1.1.0\n- New feature X\n- Bug fix Y",
  "release_notes_url": "https://example.com/releases/1.1.0",
  "diffs": [
    {
      "from_version": "1.0.0",
      "size": 1200000,
      "hash": "blake3:a1b2c3d4e5f6..."
    },
    {
      "from_version": "1.0.1",
      "size": 800000,
      "hash": "blake3:g7h8i9j0k1l2..."
    }
  ]
}
```

**Response:**

**Success (200 OK):**
```json
{
  "publication_id": "pub_abc123def456",
  "status": "initialized",
  "upload_urls": {
    "binary": {
      "url": "https://s3.amazonaws.com/deltaship-updates-prod/upload/...",
      "method": "PUT",
      "headers": {
        "Content-Type": "application/octet-stream"
      },
      "expires_at": "2026-01-07T13:34:56Z"
    },
    "diffs": [
      {
        "from_version": "1.0.0",
        "url": "https://s3.amazonaws.com/deltaship-updates-prod/upload/...",
        "method": "PUT",
        "expires_at": "2026-01-07T13:34:56Z"
      },
      {
        "from_version": "1.0.1",
        "url": "https://s3.amazonaws.com/deltaship-updates-prod/upload/...",
        "method": "PUT",
        "expires_at": "2026-01-07T13:34:56Z"
      }
    ],
    "signature": {
      "url": "https://s3.amazonaws.com/deltaship-updates-prod/upload/...",
      "method": "PUT",
      "expires_at": "2026-01-07T13:34:56Z"
    }
  },
  "next_step": "Upload files to provided URLs, then call /api/publish/finalize"
}
```

**Notes:**
- Upload URLs are pre-signed S3 URLs (temporary, expires in 1 hour)
- Publisher uploads files directly to S3 (faster, more efficient)
- After uploads complete, call /api/publish/finalize

---

#### POST /api/publish/finalize

**Description:** Finalize publication after files uploaded

**Authentication:** Required (Publisher)

**Request:**

```
POST /api/publish/finalize
Content-Type: application/json

{
  "publication_id": "pub_abc123def456"
}
```

**Response:**

**Success (200 OK):**
```json
{
  "status": "published",
  "version": "1.1.0",
  "published_at": "2026-01-07T12:45:00Z",
  "cdn_urls": {
    "binary": "https://cdn.example.com/binaries/myapp-1.1.0-linux-x86_64.bin",
    "diffs": [
      "https://cdn.example.com/diffs/myapp-1.0.0-to-1.1.0-linux-x86_64.diff",
      "https://cdn.example.com/diffs/myapp-1.0.1-to-1.1.0-linux-x86_64.diff"
    ]
  },
  "rollout_status": {
    "percentage": 10,
    "strategy": "gradual",
    "started_at": "2026-01-07T12:45:00Z"
  },
  "estimated_reach": {
    "immediate": 543,
    "24_hours": 5430,
    "7_days": 50000
  }
}
```

**Errors:**

**400 Bad Request (files not uploaded):**
```json
{
  "error": "upload_incomplete",
  "message": "Not all files have been uploaded",
  "missing": ["binary", "diff:1.0.0->1.1.0"]
}
```

**400 Bad Request (hash mismatch):**
```json
{
  "error": "hash_mismatch",
  "message": "Uploaded binary hash does not match declared hash",
  "expected": "blake3:f7g8h9i0j1k2...",
  "actual": "blake3:xxxxxxxxxxxxxxx..."
}
```

---

#### GET /api/apps

**Description:** List all applications for publisher

**Authentication:** Required (Publisher)

**Request:**
```
GET /api/apps
```

**Response:**

**Success (200 OK):**
```json
{
  "apps": [
    {
      "name": "myapp",
      "display_name": "My Application",
      "platforms": ["linux", "windows", "macos"],
      "architectures": ["x86_64", "arm64"],
      "latest_version": "1.1.0",
      "total_installations": 50000,
      "created_at": "2025-06-01T00:00:00Z"
    },
    {
      "name": "otherapp",
      "display_name": "Other App",
      "platforms": ["linux"],
      "architectures": ["x86_64"],
      "latest_version": "2.5.1",
      "total_installations": 12000,
      "created_at": "2024-03-15T00:00:00Z"
    }
  ],
  "total": 2
}
```

---

#### GET /api/apps/{app_name}/versions

**Description:** List all versions for application

**Authentication:** Required (Publisher)

**Request:**
```
GET /api/apps/myapp/versions?platform=linux&limit=10&offset=0
```

**Query Parameters:**

| Parameter | Type | Required | Description | Default |
|-----------|------|----------|-------------|---------|
| platform | string | No | Filter by platform | all |
| limit | integer | No | Results per page | 50 |
| offset | integer | No | Pagination offset | 0 |

**Response:**

**Success (200 OK):**
```json
{
  "app": "myapp",
  "versions": [
    {
      "version": "1.1.0",
      "platform": "linux",
      "architecture": "x86_64",
      "binary_size": 85400000,
      "binary_hash": "blake3:f7g8h9i0j1k2...",
      "published_at": "2026-01-07T12:45:00Z",
      "installations": 25000,
      "rollout_percentage": 50,
      "diffs": [
        {
          "from_version": "1.0.0",
          "size": 1200000,
          "installations_using": 18000
        },
        {
          "from_version": "1.0.1",
          "size": 800000,
          "installations_using": 7000
        }
      ]
    },
    {
      "version": "1.0.1",
      "platform": "linux",
      "architecture": "x86_64",
      "binary_size": 83800000,
      "binary_hash": "blake3:m3n4o5p6q7r8...",
      "published_at": "2025-12-15T10:00:00Z",
      "installations": 15000,
      "rollout_percentage": 100
    }
  ],
  "total": 12,
  "limit": 10,
  "offset": 0
}
```

---

#### GET /api/apps/{app_name}/analytics

**Description:** Get analytics for application

**Authentication:** Required (Publisher)

**Request:**
```
GET /api/apps/myapp/analytics?start_date=2026-01-01&end_date=2026-01-07&granularity=day
```

**Query Parameters:**

| Parameter | Type | Required | Description | Example |
|-----------|------|----------|-------------|---------|
| start_date | string (ISO 8601) | Yes | Start date | `2026-01-01` |
| end_date | string (ISO 8601) | Yes | End date | `2026-01-07` |
| granularity | string | No | Aggregation level | `hour`, `day`, `week` (default: `day`) |

**Response:**

**Success (200 OK):**
```json
{
  "app": "myapp",
  "period": {
    "start": "2026-01-01T00:00:00Z",
    "end": "2026-01-07T23:59:59Z",
    "granularity": "day"
  },
  "summary": {
    "total_updates": 35000,
    "successful_updates": 34650,
    "failed_updates": 350,
    "success_rate": 0.99,
    "bandwidth_saved_bytes": 2940000000,
    "bandwidth_saved_percentage": 0.986,
    "average_update_duration_seconds": 14.5
  },
  "timeseries": [
    {
      "date": "2026-01-01",
      "updates": 5000,
      "successful": 4950,
      "failed": 50,
      "bandwidth_saved_bytes": 420000000
    },
    {
      "date": "2026-01-02",
      "updates": 5200,
      "successful": 5148,
      "failed": 52,
      "bandwidth_saved_bytes": 436800000
    }
    // ... more days
  ],
  "version_distribution": {
    "1.1.0": 25000,
    "1.0.1": 15000,
    "1.0.0": 8000,
    "0.9.5": 2000
  },
  "update_methods": {
    "diff": 32900,
    "full_binary": 2100
  },
  "error_breakdown": {
    "signature_verification_failed": 150,
    "network_timeout": 100,
    "disk_space_insufficient": 50,
    "diff_application_failed": 50
  }
}
```

---

#### POST /api/rollout/update

**Description:** Update rollout percentage for version

**Authentication:** Required (Publisher)

**Request:**

```
POST /api/rollout/update
Content-Type: application/json

{
  "app": "myapp",
  "version": "1.1.0",
  "platform": "linux",
  "rollout_percentage": 50,
  "rollout_groups": ["stable"],
  "pause": false
}
```

**Response:**

**Success (200 OK):**
```json
{
  "status": "updated",
  "app": "myapp",
  "version": "1.1.0",
  "rollout_percentage": 50,
  "estimated_devices": 25000,
  "updated_at": "2026-01-07T14:00:00Z"
}
```

---

#### POST /api/rollout/rollback

**Description:** Rollback version (pause or downgrade users)

**Authentication:** Required (Publisher)

**Request:**

```
POST /api/rollout/rollback
Content-Type: application/json

{
  "app": "myapp",
  "version": "1.1.0",
  "action": "rollback",
  "rollback_to_version": "1.0.1",
  "reason": "Critical bug discovered in payment processing"
}
```

**Actions:**
- `pause`: Stop new users from receiving this version
- `rollback`: Tell existing users to downgrade to specified version

**Response:**

**Success (200 OK):**
```json
{
  "status": "rollback_initiated",
  "app": "myapp",
  "version": "1.1.0",
  "action": "rollback",
  "rollback_to_version": "1.0.1",
  "affected_devices": 25000,
  "estimated_completion": "2026-01-07T20:00:00Z",
  "initiated_at": "2026-01-07T14:30:00Z"
}
```

---

### Admin Endpoints (Requires Admin Authentication)

#### GET /api/admin/stats

**Description:** Global statistics (all apps, all publishers)

**Authentication:** Required (Admin)

**Request:**
```
GET /api/admin/stats
```

**Response:**

**Success (200 OK):**
```json
{
  "total_apps": 150,
  "total_publishers": 42,
  "total_installations": 1200000,
  "updates_last_24h": 85000,
  "success_rate_last_24h": 0.992,
  "bandwidth_saved_last_30d_gb": 12500,
  "storage_used_gb": 450,
  "api_requests_per_second": 1250
}
```

---

## Rate Limits

**Client Endpoints:**

| Endpoint | Rate Limit | Window |
|----------|------------|--------|
| /api/check-update | 100 requests | per device per hour |
| /api/download-diff | 10 downloads | per device per hour |
| /api/download-binary | 5 downloads | per device per hour |
| /api/report-status | 50 requests | per device per hour |

**Publisher Endpoints:**

| Endpoint | Rate Limit | Window |
|----------|------------|--------|
| /api/publish/* | 100 requests | per API key per hour |
| /api/apps/* | 1000 requests | per API key per hour |
| /api/rollout/* | 50 requests | per API key per hour |

**Rate Limit Headers:**

```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 45
X-RateLimit-Reset: 1704632096
```

**Rate Limit Exceeded (429 Too Many Requests):**
```json
{
  "error": "rate_limit_exceeded",
  "message": "Rate limit exceeded. Retry after 3600 seconds.",
  "retry_after": 3600
}
```

---

## Error Responses

**Standard Error Format:**

```json
{
  "error": "error_code",
  "message": "Human-readable error message",
  "details": {
    "field": "additional context"
  },
  "request_id": "req_abc123def456",
  "timestamp": "2026-01-07T12:34:56Z"
}
```

**HTTP Status Codes:**

| Code | Meaning | Use Case |
|------|---------|----------|
| 200 | OK | Successful request |
| 201 | Created | Resource created successfully |
| 400 | Bad Request | Invalid request parameters |
| 401 | Unauthorized | Authentication failed |
| 403 | Forbidden | Insufficient permissions |
| 404 | Not Found | Resource not found |
| 409 | Conflict | Resource already exists |
| 429 | Too Many Requests | Rate limit exceeded |
| 500 | Internal Server Error | Server-side error |
| 503 | Service Unavailable | Server maintenance or overloaded |

---

## Webhooks (Optional)

**Publisher Webhooks:**

**Publishers can configure webhook URLs to receive notifications:**

**Events:**
- `version.published` - New version published
- `update.success` - Update applied successfully (aggregate, hourly)
- `update.failed` - Update failed (real-time if high rate)
- `rollout.completed` - Rollout reached 100%

**Webhook Payload Example:**

```json
{
  "event": "update.success",
  "timestamp": "2026-01-07T15:00:00Z",
  "data": {
    "app": "myapp",
    "version": "1.1.0",
    "count": 1250,
    "success_rate": 0.995
  },
  "signature": "hmac_sha256(webhook_secret, payload)"
}
```

**Verification:**
```
expected_signature = hmac_sha256(webhook_secret, request_body)
if expected_signature != received_signature:
  reject (potential spoofing)
```

---

## Versioning

**API Version:** v1

**URL Format:** `/api/{endpoint}` (implicit v1)

**Future versions:** `/api/v2/{endpoint}`

**Deprecation Policy:**
- 6 months notice before deprecating endpoint
- Deprecated endpoints return warning header:
  ```
  X-Deltaship-Deprecation: This endpoint is deprecated. Use /api/v2/check-update instead. Removal date: 2026-07-01
  ```

---

## SDKs and Libraries

**Official SDKs (Planned for Phase 3):**

**JavaScript/TypeScript:**
```javascript
import { DeltashipClient } from '@deltaship/client';

const client = new DeltashipClient({
  updateServerUrl: 'https://updates.example.com',
  appName: 'myapp',
  currentVersion: '1.0.0'
});

const update = await client.checkForUpdate();
if (update.available) {
  await client.applyUpdate(update);
}
```

**Python:**
```python
from deltaship import DeltashipClient

client = DeltashipClient(
    update_server_url='https://updates.example.com',
    app_name='myapp',
    current_version='1.0.0'
)

update = client.check_for_update()
if update.available:
    client.apply_update(update)
```

**Go:**
```go
import "github.com/deltaship/go-client"

client := deltaship.NewClient(deltaship.Config{
    UpdateServerURL: "https://updates.example.com",
    AppName:         "myapp",
    CurrentVersion:  "1.0.0",
})

update, _ := client.CheckForUpdate(ctx)
if update.Available {
    client.ApplyUpdate(ctx, update)
}
```

---

## OpenAPI Specification

**Full OpenAPI 3.0 specification available at:**

```
https://updates.example.com/openapi.yaml
https://updates.example.com/openapi.json
```

**Swagger UI:**

```
https://updates.example.com/api-docs
```

**Interactive API explorer for testing**

---

## Next Steps

**For Developers:**
- Review API endpoints for your use case
- Test with curl or Postman
- Integrate into your application

**For Publishers:**
- Obtain API key from administrator
- Test publish workflow with staging server
- Automate with CI/CD

**For More Information:**
- Read: [Publisher Setup](../deployment/PUBLISHER_SETUP.md)
- Read: [System Design](../architecture/SYSTEM_DESIGN.md)
- Read: [Security Model](../security/SECURITY_MODEL.md)

---

**End of API Specification**
