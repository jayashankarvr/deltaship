# Update Server

**Component:** Centralized server storing versions and serving diffs to clients
**Audience:** System administrators, DevOps engineers, platform operators
**Last Updated:** 2026-01-07

---

## Overview

The Update Server is the central distribution point for software updates. It stores full binaries, pre-computed diffs, signatures, and metadata. Clients query the server to check for updates and download only the necessary differential data.

**Key Characteristics:**
- **Stateless:** Each request independent, horizontally scalable
- **Cacheable:** All responses cache-friendly (CDN compatible)
- **Secure:** All content cryptographically signed
- **Efficient:** Serves minimal data (diffs) instead of full binaries
- **Smart:** Can compute diffs on-demand if not cached

---

## Design Principles

### Scalability
- Horizontal scaling through load balancing
- CDN-friendly (all responses have cache headers)
- Database-less option (file-based storage)
- Stateless request handling

### Reliability
- No single point of failure
- Graceful degradation (fall back to full downloads)
- Automatic retry mechanisms
- Health monitoring

### Performance
- Diff caching reduces computation
- Pre-computed common version transitions
- Efficient storage (deduplication)
- Parallel request handling

### Security
- All content signed and verified
- API key authentication for publishers
- Rate limiting to prevent abuse
- Audit logging

---

## Core Components

### 1. Storage Layer

**Purpose:** Store binaries, diffs, signatures, and metadata

**Storage Options:**

**Option A: File-Based Storage (Simple)**
- Binaries, diffs, signatures stored as files
- Directory structure organizes content
- Metadata in JSON files
- Works with any file system or object storage (S3, Azure Blob, GCS)

**Advantages:**
- Simple to deploy
- CDN-friendly (serve static files)
- No database required
- Easy backup/restore

**Storage Structure:**
```
/storage/
├── apps/
│   └── my-app/
│       ├── catalog.json (all versions index)
│       ├── versions/
│       │   ├── 1.0.1/
│       │   │   ├── manifest.json
│       │   │   ├── binaries/
│       │   │   │   ├── x86_64-linux
│       │   │   │   ├── aarch64-linux
│       │   │   │   └── x86_64-windows.exe
│       │   │   ├── signatures/
│       │   │   │   ├── binary-x86_64-linux.sig
│       │   │   │   └── ...
│       │   │   └── diffs/
│       │   │       ├── from-1.0.0.diff
│       │   │       ├── from-1.0.0.sig
│       │   │       └── ...
│       │   └── 1.0.0/
│       │       └── ... (previous versions)
│       └── public-key.pem (publisher's public key)
```

**Option B: Database + Object Storage (Advanced)**
- Metadata in database (PostgreSQL, MySQL)
- Binaries/diffs in object storage
- Better querying, analytics
- More complex deployment

**Advantages:**
- Fast metadata queries
- Better analytics
- Transaction support
- Easier version queries

**Hybrid Approach (Recommended):**
- Metadata in lightweight database (SQLite for single server, PostgreSQL for multi-server)
- Binaries/diffs in object storage or file system
- Best of both worlds

### 2. API Layer

**Purpose:** RESTful API for clients and publishers

**Endpoints:**

**For Clients (Public, Read-Only):**

```
GET /api/v1/apps/{app_name}/check-update
  Query params:
    - current_version (required)
    - architecture (required)
    - device_id_hash (optional, for rollout groups)
  Response:
    - Latest version available
    - Diff URL or full binary URL
    - Checksum, signature URL
    - Rollout status

GET /api/v1/apps/{app_name}/versions/{version}/manifest
  Response: JSON manifest with version metadata

GET /api/v1/apps/{app_name}/diffs/{from_version}-to-{to_version}
  Response: Binary diff file

GET /api/v1/apps/{app_name}/binaries/{version}/{architecture}
  Response: Full binary file

GET /api/v1/apps/{app_name}/signatures/{version}/{file_name}
  Response: Signature file

GET /api/v1/apps/{app_name}/public-key
  Response: Publisher's public key (PEM format)
```

**For Publishers (Authenticated, Write):**

```
POST /api/v1/apps/{app_name}/versions/{version}
  Auth: API key required
  Body: Multipart form with binary, manifest, signatures
  Response: Upload confirmation

PUT /api/v1/apps/{app_name}/versions/{version}/activate
  Auth: API key required
  Body: Rollout configuration (percentage, canary groups)
  Response: Activation status

DELETE /api/v1/apps/{app_name}/versions/{version}
  Auth: API key required
  Response: Deletion confirmation (soft delete, can restore)

GET /api/v1/apps/{app_name}/stats
  Auth: API key required
  Query: date range, metrics
  Response: Analytics data
```

**For Monitoring (Internal):**

```
GET /health
  Response: Server health status

GET /metrics
  Response: Prometheus-format metrics
```

### 3. Diff Computation Engine

**Purpose:** Compute diffs on-demand when not cached

**When Triggered:**
- Client requests uncommon version transition
- Diff not pre-computed by publisher
- Cache miss for requested diff

**Process:**
1. Load source version binary from storage
2. Load target version binary from storage
3. Select diff algorithm (based on file type)
4. Compute diff
5. Compress diff
6. Cache result
7. Serve to client

**Algorithm Selection:**
- Detect binary format (ELF, PE, Mach-O, other)
- Choose best algorithm:
  - **Courgette:** For executables (best compression)
  - **bsdiff:** For other binaries (general purpose)
  - **xdelta3:** For very large files (faster, larger diffs)

**Resource Management:**
- Limit concurrent diff computations (CPU-bound)
- Queue requests if overloaded
- Time limit per computation (prevent DoS)
- Memory limit per computation

**Caching:**
- Store computed diffs in cache
- LRU eviction when cache full
- Track hit rate for optimization
- Pre-compute popular transitions based on analytics

### 4. Rollout Manager

**Purpose:** Control gradual rollout of new versions

**Strategies:**

**1. Percentage-Based Rollout**
- Start at configured percentage (e.g., 10%)
- Increase incrementally (e.g., +20% daily)
- Reach 100% over time (e.g., 5 days)
- Deterministic user selection (hash-based)

**Logic:**
- Hash device ID to number 0-99
- If hash < current_percentage: show update
- Ensures same user always in same group
- No server-side state needed

**2. Canary Deployment**
- Deploy to specific user groups first
- Examples: internal users, beta testers, specific regions
- Require user group membership (tracked server-side or client-provided)
- Monitor for errors before wider rollout

**3. Geographic Rollout**
- Deploy by region (e.g., US-West → US-East → EU → Asia)
- Based on client IP or self-reported location
- Useful for region-specific testing

**4. Time-Based Rollout**
- Scheduled activation at specific time
- All users get update simultaneously
- Useful for coordinated releases

**Emergency Controls:**
- **Pause rollout:** Stop at current percentage
- **Rollback:** Deactivate version, revert to previous
- **Force update:** Override normal cadence (security patches)

### 5. Analytics & Monitoring

**Purpose:** Track update adoption, errors, performance

**Metrics Collected:**

**Update Metrics:**
- Total updates per version
- Success/failure rates
- Error types and frequencies
- Update duration (percentiles)
- Bandwidth saved vs full downloads

**Version Distribution:**
- Current version breakdown (pie chart)
- Version adoption over time (line chart)
- Fragmentation score (how many old versions still active)

**Performance Metrics:**
- API response times
- Diff computation times
- Download speeds (by client region)
- Server resource usage (CPU, memory, disk, network)

**Error Tracking:**
- Signature verification failures
- Checksum mismatches
- Network errors
- Patch application failures
- Categorization by error type

**Geographic Distribution:**
- Updates by country/region
- Bandwidth usage by region
- Error rates by region

**Platform Distribution:**
- Updates by OS (Linux, Windows, macOS)
- Updates by architecture (x86_64, aarch64)

**Data Privacy:**
- All metrics anonymized
- Device ID hashed before storage
- No personally identifiable information
- Compliance with GDPR, CCPA

### 6. Security Layer

**Purpose:** Protect server and ensure only authentic updates distributed

**Authentication:**

**Publisher Authentication:**
- API key required for write operations
- Keys stored hashed (bcrypt, scrypt)
- Key rotation supported
- Per-app or global keys (configurable)

**Client Authentication (Optional):**
- No authentication for public software
- Optional license key validation for commercial software
- Device registration for enterprise deployments

**Authorization:**
- Publishers can only modify their own apps
- Read-only access to public endpoints
- Admin-only access to sensitive operations

**Rate Limiting:**
- Prevent abuse and DoS attacks
- Separate limits for read vs write operations
- Burst allowance for legitimate traffic spikes
- IP-based and API-key-based limits

**Example Limits:**
- Public reads: 1000 requests/hour per IP
- Publisher writes: 100 requests/hour per API key
- Diff computation: 10 concurrent per server

**Input Validation:**
- All inputs sanitized
- Version numbers validated (semver format)
- File sizes limited (prevent upload bombs)
- Content-type verification

**Audit Logging:**
- All write operations logged
- Who, what, when, from where
- Tamper-proof logs (append-only)
- Retention policy (e.g., 1 year)

---

## Deployment Models

### Model 1: Single Server (Small Scale)

**Characteristics:**
- One server instance
- File-based storage (local disk or S3)
- SQLite for metadata (optional)
- Suitable for: <1000 clients, <10 apps

**Advantages:**
- Simple deployment
- Low cost
- Easy to maintain

**Limitations:**
- Single point of failure
- Limited scalability
- No geographic distribution

**Resource Requirements:**
- 2 CPU cores
- 4 GB RAM
- 100 GB storage (scales with apps)
- 10 Mbps network

### Model 2: Load-Balanced Multi-Server (Medium Scale)

**Characteristics:**
- Multiple server instances behind load balancer
- Shared object storage (S3, Azure Blob, GCS)
- Database for metadata (PostgreSQL)
- Suitable for: 1,000-100,000 clients, 10-100 apps

**Advantages:**
- High availability
- Horizontal scalability
- Better performance

**Components:**
- Load balancer (HAProxy, Nginx, ALB, etc.)
- 2-5 application servers
- Object storage (S3, GCS, Azure Blob)
- Database (RDS, managed PostgreSQL)

**Resource Requirements:**
- 4 CPU cores per app server
- 8 GB RAM per app server
- Shared storage (scales with apps)
- 100+ Mbps network

### Model 3: CDN-Backed Global Distribution (Large Scale)

**Characteristics:**
- Application servers in multiple regions
- CDN for static content (binaries, diffs, signatures)
- Database with read replicas
- Suitable for: >100,000 clients, 100+ apps, global users

**Advantages:**
- Global distribution (low latency)
- Massive scalability
- Cost-effective bandwidth (CDN caching)

**Components:**
- Multi-region application servers
- CDN (CloudFlare, Fastly, CloudFront, Akamai)
- Database with read replicas (multi-region)
- Object storage (multi-region)

**Architecture:**
- Static content (binaries, diffs) served via CDN
- Dynamic content (API, diff computation) via app servers
- Metadata queries served by nearest replica

**Resource Requirements:**
- Auto-scaling app servers (10-100+)
- CDN bandwidth budget
- Multi-region storage
- High-availability database

### Model 4: Self-Hosted On-Premise (Enterprise)

**Characteristics:**
- Deployed within enterprise network
- No external dependencies
- Air-gapped option available
- Suitable for: High-security environments, regulated industries

**Advantages:**
- Full control
- Data sovereignty
- Customizable
- No cloud vendor lock-in

**Limitations:**
- Higher operational overhead
- Manual scaling
- Requires expertise

---

## Integration Points

### Object Storage Integration

**Supported Backends:**
- **AWS S3:** boto3 SDK, S3 API
- **Google Cloud Storage:** GCS SDK, S3-compatible API
- **Azure Blob Storage:** Azure SDK, Blob API
- **MinIO:** S3-compatible, self-hosted
- **Local filesystem:** For development/testing

**Storage Features Used:**
- Object storage (put, get, delete, list)
- Versioning (optional, for safety)
- Lifecycle policies (auto-delete old versions)
- Access control (private by default, signed URLs for downloads)

### CDN Integration

**Supported CDNs:**
- **CloudFlare:** Cache-Control headers, purge API
- **AWS CloudFront:** Origin configuration, invalidation API
- **Fastly:** VCL configuration, instant purge
- **Akamai:** Cache configuration, purge API
- **Generic:** Any CDN supporting standard HTTP cache headers

**CDN Configuration:**
- **Static content (binaries, diffs, signatures):** Cache for 1 year (immutable)
- **Metadata (manifests):** Cache for 5 minutes (frequent updates)
- **API responses:** Cache for 1 minute with Vary header
- **Purge on publish:** Invalidate cache when new version published

### Database Integration

**Supported Databases:**
- **SQLite:** Single server, file-based, no separate service
- **PostgreSQL:** Multi-server, full-featured, recommended
- **MySQL/MariaDB:** Alternative to PostgreSQL
- **MongoDB:** NoSQL option (less recommended)

**Schema:**
- **apps table:** App metadata (name, description, publisher)
- **versions table:** Version metadata (number, timestamp, checksums)
- **diffs table:** Diff metadata (source, target, size, algorithm)
- **analytics table:** Update events (time-series data)
- **rollout table:** Rollout configurations per version

### Monitoring Integration

**Prometheus:**
- Metrics endpoint: `/metrics`
- Standard metrics (request count, duration, errors)
- Custom metrics (diff computation time, cache hit rate, etc.)

**Grafana:**
- Pre-built dashboards provided
- Real-time visualization
- Alerting integration

**Log Aggregation:**
- **ELK Stack:** Elasticsearch, Logstash, Kibana
- **Loki:** Grafana's log aggregation
- **CloudWatch Logs:** AWS native
- **Splunk:** Enterprise option

**Alerting:**
- High error rate (>1%)
- Slow response times (p95 > threshold)
- Diff computation failures
- Storage capacity warnings
- Security events (failed auth attempts)

---

## API Specification

### Check Update Endpoint

**Purpose:** Client checks if update available

**Request:**
```
GET /api/v1/apps/my-app/check-update
  ?current_version=1.0.0
  &architecture=x86_64-linux
  &device_id_hash=abc123def456

Headers:
  User-Agent: Deltaship-Client/1.0
  Accept: application/json
```

**Response (Update Available):**
```json
HTTP/1.1 200 OK
Content-Type: application/json
Cache-Control: public, max-age=60

{
  "update_available": true,
  "target_version": "1.0.1",
  "update_type": "differential",
  "forced": false,
  "rollout_included": true,
  "diff": {
    "url": "/api/v1/apps/my-app/diffs/1.0.0-to-1.0.1",
    "size": 38912,
    "checksum": "blake3:abc123...",
    "algorithm": "bsdiff",
    "compression": "zstd"
  },
  "full_binary": {
    "url": "/api/v1/apps/my-app/binaries/1.0.1/x86_64-linux",
    "size": 5242880,
    "checksum": "blake3:def456..."
  },
  "signature": {
    "url": "/api/v1/apps/my-app/signatures/1.0.1/diff-from-1.0.0.sig",
    "algorithm": "Ed25519"
  },
  "release_notes": "Bug fixes and performance improvements",
  "published_at": "2026-01-07T10:00:00Z"
}
```

**Response (No Update):**
```json
HTTP/1.1 200 OK
Content-Type: application/json
Cache-Control: public, max-age=300

{
  "update_available": false,
  "current_version_latest": true,
  "message": "You are running the latest version"
}
```

**Response (Not in Rollout Group):**
```json
HTTP/1.1 200 OK
Content-Type: application/json
Cache-Control: public, max-age=60

{
  "update_available": false,
  "current_version_latest": false,
  "rollout_included": false,
  "message": "Update available but not yet released to your device group",
  "latest_version": "1.0.1",
  "rollout_percentage": 10
}
```

### Download Diff Endpoint

**Request:**
```
GET /api/v1/apps/my-app/diffs/1.0.0-to-1.0.1

Headers:
  User-Agent: Deltaship-Client/1.0
  Accept: application/octet-stream
```

**Response:**
```
HTTP/1.1 200 OK
Content-Type: application/octet-stream
Content-Length: 38912
Content-Disposition: attachment; filename="1.0.0-to-1.0.1.diff"
Cache-Control: public, max-age=31536000, immutable
X-Checksum-Blake3: abc123def456...
X-Diff-Algorithm: bsdiff
X-Compression: zstd

[binary diff data]
```

---

## Performance Optimization

### Caching Strategy

**Level 1: CDN Cache (Edge)**
- Binaries, diffs, signatures: Cache for 1 year
- Hit rate target: >95%
- Reduces server load dramatically
- Serves users from nearest edge location

**Level 2: Server Cache (Memory)**
- Metadata (manifests): In-memory cache, 5-minute TTL
- Frequently accessed diffs: In-memory, LRU eviction
- Reduces database queries
- Reduces object storage reads

**Level 3: Computed Diff Cache (Disk)**
- On-demand computed diffs cached to disk
- Persistent across server restarts
- LRU eviction when disk space limited
- Reduces expensive computation

### Database Optimization

**Indexes:**
- apps(name) - unique
- versions(app_id, version_number) - composite unique
- diffs(app_id, source_version, target_version) - composite unique
- analytics(timestamp) - for time-range queries

**Query Optimization:**
- Use prepared statements (prevent SQL injection, faster execution)
- Connection pooling (reduce connection overhead)
- Read replicas for analytics queries
- Partition analytics table by month (faster queries)

### Storage Optimization

**Deduplication:**
- Identical chunks across versions stored once
- Content-addressable storage (hash-based naming)
- Significant savings for binaries with common sections

**Compression:**
- Diffs compressed with zstd (good compression, fast)
- Binaries served compressed (gzip, brotli via CDN)
- Metadata JSON minified

**Lifecycle Policies:**
- Old versions (>6 months) moved to cheaper storage tier
- Very old versions (>1 year) archived or deleted
- LTS versions exempt from deletion

---

## Security Best Practices

### Server Hardening

**Network Security:**
- HTTPS only (TLS 1.2+, strong ciphers)
- Firewall (allow only HTTPS 443, SSH 22 from admin IPs)
- DDoS protection (rate limiting, CDN-level protection)
- VPN for admin access (optional)

**Application Security:**
- No default credentials
- API keys rotated regularly
- Input validation on all endpoints
- CORS headers configured
- Security headers (CSP, HSTS, X-Frame-Options, etc.)

**System Security:**
- Regular OS updates
- Minimal installed software
- Non-root user for application
- Log aggregation for security monitoring
- Intrusion detection (fail2ban, etc.)

### Data Protection

**At Rest:**
- Encrypted storage (filesystem encryption or storage service encryption)
- Database encryption (if sensitive data stored)
- Key management (AWS KMS, Azure Key Vault, etc.)

**In Transit:**
- TLS for all connections
- Certificate pinning (optional, for clients)
- Signed downloads (all content signed by publisher)

**Backup & Recovery:**
- Regular backups (daily minimum)
- Off-site backup storage
- Tested restore procedures
- Disaster recovery plan

---

## Operational Procedures

### Deployment

**Steps:**
1. Provision infrastructure (servers, storage, database)
2. Install server software
3. Configure environment variables
4. Load initial data (if migrating)
5. Start services
6. Verify health endpoints
7. Configure monitoring & alerting
8. Route traffic (DNS, load balancer)

**Zero-Downtime Deployment:**
- Blue-green deployment
- Rolling updates (one server at a time)
- Health checks before routing traffic
- Automatic rollback on failures

### Monitoring

**Key Metrics:**
- Request rate (requests/second)
- Error rate (errors/second, percentage)
- Response time (p50, p95, p99)
- Diff computation time
- Cache hit rate
- Database query time
- Disk usage
- Network bandwidth

**Alerts:**
- Error rate >1%: Warning
- Error rate >5%: Critical
- p95 response time >1s: Warning
- Disk usage >80%: Warning
- Disk usage >90%: Critical

### Maintenance

**Regular Tasks:**
- **Daily:** Check error logs, review metrics
- **Weekly:** Review storage usage, analyze slow queries
- **Monthly:** Update security patches, review analytics, optimize caching
- **Quarterly:** Capacity planning, disaster recovery test, security audit

**Database Maintenance:**
- Vacuum (PostgreSQL) - weekly
- Reindex - monthly
- Backup verification - weekly
- Replica lag monitoring - continuous

**Storage Cleanup:**
- Delete diffs not accessed in 6 months
- Archive old versions to cold storage
- Remove temp files from failed uploads

---

## Troubleshooting

### Common Issues

**Issue:** High error rate on diff downloads
- **Symptoms:** 404 errors, missing diff files
- **Diagnosis:** Check storage for missing files, review publisher logs
- **Solution:** Regenerate missing diffs, fix publisher pipeline

**Issue:** Slow response times
- **Symptoms:** p95 >5 seconds
- **Diagnosis:** Check database query times, diff computation load
- **Solution:** Add caching, optimize queries, scale horizontally

**Issue:** Storage full
- **Symptoms:** Upload failures, disk usage alerts
- **Diagnosis:** Check storage usage breakdown
- **Solution:** Clean up old versions, increase storage capacity

**Issue:** Database connection errors
- **Symptoms:** 500 errors, connection refused
- **Diagnosis:** Check database status, connection pool
- **Solution:** Increase connection pool size, scale database

---

## Next Steps

- **For deployment:** Read [Server Deployment](../deployment/SERVER_DEPLOYMENT.md)
- **For architecture:** Read [System Design](../architecture/SYSTEM_DESIGN.md)
- **For security:** Read [Security Model](../security/SECURITY_MODEL.md)
- **For operations:** Read [Monitoring](../operations/MONITORING.md)

---

**End of Update Server Specification**
