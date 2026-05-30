# System Design

**Document:** Overall system architecture and design decisions
**Audience:** Architects, senior engineers, technical decision makers
**Last Updated:** 2026-01-07

---

## Overview

VBDP (Version-Aware Binary Differential Update System) is a distributed system enabling efficient software updates through binary differential compression. This document describes the overall architecture, component interactions, and key design decisions.

**Core Philosophy:**
- **Version-Aware:** Explicit version tracking enables optimal differential computation
- **Distributed:** Scales horizontally across multiple servers and regions
- **Secure by Default:** Cryptographic verification at every step
- **Fault-Tolerant:** Graceful degradation and automatic recovery
- **Platform-Agnostic:** Works across Linux, Windows, macOS with consistent behavior

---

## System Architecture

### High-Level Overview

```
┌─────────────────┐
│   Publisher     │
│  (Developer)    │
└────────┬────────┘
         │
         │ Builds, Signs, Publishes
         ▼
┌─────────────────┐      ┌──────────────────┐
│ Publisher       │─────▶│  Update Server   │
│ Toolkit         │      │  (API + Storage) │
│ (vbdp-*)        │      └────────┬─────────┘
└─────────────────┘               │
                                  │ Serves Updates
                                  ▼
                         ┌─────────────────┐
                         │  CDN (Optional) │
                         └────────┬────────┘
                                  │
                                  │ Downloads
                                  ▼
                         ┌─────────────────┐
                         │ Client Patcher  │
                         │  (End-User      │
                         │   Device)       │
                         └─────────────────┘
```

### Component Responsibilities

**Publisher Toolkit:**
- Version management
- Binary diff generation
- Cryptographic signing
- Publishing to update server

**Update Server:**
- Version registry
- Diff storage and retrieval
- API endpoints for clients
- Rollout management
- Analytics collection

**CDN (Optional):**
- Global content distribution
- Bandwidth optimization
- DDoS protection
- Cache management

**Client Patcher:**
- Update checking
- Diff downloading
- Signature verification
- Atomic patch application

---

## Architecture Patterns

### 1. Three-Tier Architecture

**Tier 1: Presentation (Client)**
- Minimal UI (system tray, notifications)
- CLI for advanced users
- Configuration management

**Tier 2: Application (Update Server)**
- REST API
- Business logic (rollout rules, analytics)
- Authentication and authorization

**Tier 3: Data (Storage)**
- Object storage (diffs, binaries)
- Database (metadata, versions, analytics)
- Cache layer (Redis/Memcached)

**Benefits:**
- Clear separation of concerns
- Independent scaling of each tier
- Technology flexibility per tier

### 2. Microservices-Inspired Design

While not full microservices, VBDP uses service-oriented principles:

**API Service:**
- Handles client requests
- Stateless (scales horizontally)
- Load-balanced

**Diff Computation Service:**
- On-demand or pre-computed diffs
- CPU-intensive operations
- Can be scaled independently
- Queue-based processing

**Analytics Service:**
- Collects telemetry
- Processes metrics
- Separate from critical path

**Rollout Service:**
- Manages gradual rollouts
- Feature flags
- A/B testing logic

**Benefits:**
- Independent deployment
- Targeted scaling
- Failure isolation

### 3. Event-Driven Architecture

**Key Events:**
- `VersionPublished` - New version available
- `UpdateRequested` - Client checks for update
- `DiffDownloaded` - Client retrieved diff
- `UpdateApplied` - Client successfully updated
- `UpdateFailed` - Client encountered error
- `RollbackTriggered` - Emergency rollback initiated

**Event Flow:**
```
Publisher → VersionPublished → [Diff Computation Queue]
                              ↓
                         [Diff Generated Event]
                              ↓
                         [Make Available for Download]

Client → UpdateRequested → [Check Rollout Rules]
                         → [Log Analytics Event]
                         → [Return Update Response]

Client → UpdateApplied → [Analytics Event]
                       → [Update Version Stats]
                       → [Check Rollout Progress]
```

**Benefits:**
- Asynchronous processing
- Loose coupling
- Scalable analytics
- Audit trail

---

## Component Deep Dive

### Publisher Toolkit Architecture

**Design Pattern:** Command Pattern

Each tool (`vbdp-init`, `vbdp-register`, etc.) is a separate command with:
- Single Responsibility
- Clear input/output
- Composable (can be chained)

**Core Modules:**

**Version Manager:**
- SQLite database for version registry
- Schema: versions table with metadata
- Queries: list versions, get version details, register new version

**Diff Generator:**
- Algorithm abstraction layer (supports bsdiff, Courgette, xdelta3)
- Strategy pattern for algorithm selection
- Input: old_binary, new_binary, algorithm
- Output: diff_file, metadata (compression ratio, generation time)

**Crypto Module:**
- Ed25519 key management
- Sign operation: hash(diff + metadata) → signature
- Verify operation: validate signature with public key

**Publisher Module:**
- HTTP client for update server API
- Upload: multipart/form-data for large diffs
- Retry logic with exponential backoff

### Update Server Architecture

**Design Pattern:** Layered Architecture

**Layer 1: API Layer (REST)**
- Routes: `/check-update`, `/download-diff/{id}`, `/download-binary/{id}`, `/report-status`
- Input validation
- Rate limiting
- Authentication (API keys, JWT)

**Layer 2: Business Logic**
- Rollout manager: determine if client should receive update
- Version resolver: find optimal diff path
- Analytics aggregator: collect metrics

**Layer 3: Data Access**
- Repository pattern for database access
- Storage abstraction for object storage (S3, Azure Blob, filesystem)

**Layer 4: Infrastructure**
- Caching (Redis for metadata, CDN for files)
- Monitoring (Prometheus metrics export)
- Logging (structured JSON logs)

**Scalability Approach:**

**Horizontal Scaling:**
- Stateless API servers (can add more instances)
- Load balancer distributes traffic (round-robin, least-connections)
- Shared database (PostgreSQL with connection pooling)
- Shared object storage (S3, GCS, Azure Blob)

**Vertical Scaling:**
- Diff computation can be offloaded to background workers
- Database read replicas for analytics queries
- CDN for static content (diffs, binaries)

**Database Schema Design:**

**Principle:** Normalization for consistency, denormalization for performance

**Core Tables:**
- `apps` - Application metadata
- `versions` - Version information per app
- `diffs` - Diff metadata (from_version, to_version, size, storage_url)
- `update_events` - Analytics (device_id, version, timestamp, success)
- `rollout_rules` - Gradual rollout configuration

**Indexes:**
- `apps(name)` - Fast lookup by app name
- `versions(app_id, version)` - Unique constraint
- `diffs(from_version, to_version)` - Fast diff lookup
- `update_events(timestamp)` - Time-series queries

### Client Patcher Architecture

**Design Pattern:** State Machine

**States:**
- Idle
- CheckingForUpdates
- DownloadingDiff
- VerifyingSignature
- ApplyingPatch
- VerifyingUpdate
- Completed / Failed

**State Transitions:**
```
Idle → CheckingForUpdates → DownloadingDiff → VerifyingSignature → ApplyingPatch → VerifyingUpdate → Completed
                    ↓              ↓                  ↓                  ↓                ↓
                    └──────────────┴──────────────────┴──────────────────┴───────────→ Failed
                                                                                         ↓
                                                                                    [Rollback]
                                                                                         ↓
                                                                                       Idle
```

**Core Modules:**

**Scheduler:**
- Timer-based update checking (configurable interval)
- Jitter to prevent thundering herd
- Respect system state (battery, network type)

**Downloader:**
- HTTP client with resume capability (Range requests)
- Streaming checksum verification
- Bandwidth throttling

**Crypto Verifier:**
- Public key embedded at compile time
- Signature verification using libsodium or equivalent
- Constant-time comparison

**Patch Applicator:**
- bspatch implementation (or Courgette)
- Atomic file replacement (rename system call)
- Backup and rollback mechanism

**Platform Abstraction Layer:**

**Interface:**
- `ServiceManager`: start, stop, restart service
- `FileSystem`: atomic_replace, backup, restore
- `ProcessManager`: is_running, terminate, restart
- `ConfigProvider`: get_config, set_config

**Implementations:**
- `LinuxServiceManager`: systemd integration
- `WindowsServiceManager`: Windows Service API
- `MacOSServiceManager`: launchd integration

---

## Data Flow

### Update Flow (Normal Case)

**Step 1: Publisher Builds New Version**
```
Build system → new binary (v2.0.0)
Publisher runs: vbdp-register --version 2.0.0 --binary ./app
Publisher runs: vbdp-sign --version 2.0.0
Publisher runs: vbdp-publish --version 2.0.0
```

**Step 2: Server Receives Publication**
```
API receives: POST /api/publish
Validates: signature, metadata
Stores: binary to object storage
Computes: diffs from recent versions (v1.9.0 → v2.0.0, v1.8.0 → v2.0.0)
Updates: database (new version record, diff records)
Triggers: VersionPublished event
```

**Step 3: Client Checks for Update**
```
Client timer triggers
Client sends: GET /api/check-update?app=myapp&current_version=1.9.0&device_id=abc123
Server checks: rollout rules (is device in rollout group?)
Server responds: {update_available: true, target_version: "2.0.0", diff_url: "...", diff_size: 1048576}
```

**Step 4: Client Downloads and Applies**
```
Client downloads: diff from diff_url (1 MB instead of 100 MB)
Client verifies: signature matches publisher's public key
Client applies: bspatch old_binary diff → new_binary
Client verifies: checksum of new_binary matches expected
Client replaces: atomic rename of new_binary
Client reports: POST /api/report-status {success: true, version: "2.0.0", bandwidth_saved: 99000000}
```

### Rollback Flow (Error Case)

**Trigger: High Error Rate Detected**
```
Analytics Service detects: >5% of updates failing
Alerts: operations team
Admin triggers: vbdp-rollback --version 2.0.0 --rollback-to 1.9.0
```

**Server Response:**
```
Updates rollout_rules: pause v2.0.0 rollout
Updates check-update API: return v1.9.0 as target for devices on v2.0.0
Triggers: RollbackTriggered event
```

**Client Response:**
```
Client on v2.0.0 checks for update
Server responds: {update_available: true, target_version: "1.9.0" (downgrade)}
Client downloads: diff v2.0.0 → v1.9.0 (reverse diff)
Client applies: patch to return to v1.9.0
```

---

## Design Decisions

### Decision 1: Version-Aware vs Content-Addressed

**Chosen:** Version-Aware

**Alternatives Considered:**
- Content-addressed (like casync, IPFS): chunks identified by hash, not version

**Rationale:**
- Version-awareness enables explicit control (gradual rollout, A/B testing)
- Better analytics (know exactly which version each user has)
- Simpler for publishers (just register versions sequentially)
- Optimal diffs (compute exact diff between two known versions)

**Trade-offs:**
- Version-aware requires server-side computation
- Content-addressed can deduplicate across different files
- Decision: Version-aware better fits software update use case

### Decision 2: Pre-computed vs On-Demand Diffs

**Chosen:** Hybrid (pre-compute recent, on-demand for old)

**Alternatives Considered:**
- Pre-compute all diffs: N² storage (every version to every other version)
- On-demand only: CPU-intensive, slow first request

**Rationale:**
- Pre-compute: v(n) → v(n-1), v(n) → v(n-2), v(n) → v(n-3) (most common upgrades)
- On-demand: v(n) → v(old) for long-dormant users (rare)
- Cache on-demand diffs after first computation

**Implementation:**
- Publisher publishes v2.0.0
- Server immediately computes: v2.0.0 ← v1.9.0, v2.0.0 ← v1.8.0, v2.0.0 ← v1.7.0
- Server lazily computes: v2.0.0 ← v1.0.0 (if user with v1.0.0 requests update)
- Server caches: lazy diffs in object storage for reuse

### Decision 3: Diff Algorithm Selection

**Chosen:** bsdiff (default), Courgette (executables), configurable

**Alternatives Considered:**
- bsdiff only: simple, works for all binaries
- Courgette only: excellent for executables, complex
- xdelta3: faster, larger diffs

**Rationale:**
- bsdiff: proven (Firefox uses it since 2003), excellent compression, slow
- Courgette: Google Chrome uses it, exe-aware (relocates pointers), 10x smaller diffs for executables
- xdelta3: fast but 20-30% larger diffs than bsdiff

**Implementation:**
- Default: bsdiff (works for everything)
- Executables: Courgette (if binary is ELF/PE/Mach-O and Courgette available)
- Override: publisher can specify algorithm in metadata
- Fallback: if patch fails, download full binary

### Decision 4: Signature Verification Placement

**Chosen:** Client-side only (server does NOT verify)

**Alternatives Considered:**
- Server-side verification: server verifies publisher's signature before storing
- Both: server and client verify

**Rationale:**
- Client MUST verify (cannot trust server)
- Server verification is redundant for security (client verifies anyway)
- Server verification adds latency and CPU cost
- Publisher's private key should never touch server (key management risk)

**Implementation:**
- Publisher signs locally (private key stays with publisher)
- Server stores signature as metadata (does not verify)
- Client verifies before applying (public key embedded in client)

### Decision 5: Database Choice

**Chosen:** PostgreSQL (production), SQLite (publisher toolkit, client)

**Alternatives Considered:**
- MySQL: similar to PostgreSQL
- MongoDB: NoSQL, flexible schema
- DynamoDB: serverless, auto-scaling

**Rationale:**
- PostgreSQL: mature, strong consistency, excellent performance, JSONB for flexible fields
- SQLite: perfect for local tools (publisher toolkit, client database), zero-config, single file
- MongoDB: overkill for structured data, eventual consistency problematic
- DynamoDB: vendor lock-in, cost unpredictable

**Implementation:**
- Server: PostgreSQL with read replicas for analytics
- Publisher Toolkit: SQLite (versions.db)
- Client Patcher: SQLite (installed_apps.db)

---

## Non-Functional Requirements

### Performance Requirements

**Update Check:**
- Latency: <500ms (p99)
- Throughput: 10,000 requests/second (per server instance)

**Diff Download:**
- Bandwidth: Saturate client connection (no artificial throttling by default)
- Resume: Support HTTP Range requests (resume interrupted downloads)

**Patch Application:**
- Time: <10 seconds for 100MB binary (on modern hardware)
- Memory: <100MB peak RAM usage
- CPU: <50% of one core (leave resources for user apps)

### Reliability Requirements

**Availability:**
- Target: 99.9% uptime (43 minutes downtime per month)
- Mechanism: Load-balanced multi-instance deployment
- Fallback: CDN serves cached diffs even if API server down

**Data Durability:**
- Target: 99.999999999% (11 nines) - no data loss
- Mechanism: Object storage (S3, Azure Blob) provides this by default
- Backup: Database backups daily, retained 30 days

**Update Success Rate:**
- Target: >99% of update attempts succeed
- Mechanism: Atomic updates with rollback, fallback to full download

### Security Requirements

**Authentication:**
- Server API: API keys (HMAC-signed requests)
- Client verification: Ed25519 signature verification

**Authorization:**
- Rollout rules: server decides which clients receive which updates
- Admin API: separate auth for publisher operations

**Integrity:**
- All diffs: Blake3 checksums
- All binaries: SHA-256 checksums
- Transport: TLS 1.2+ mandatory

**Confidentiality:**
- Diffs: not encrypted (binaries are public anyway)
- Analytics: anonymized device IDs (hash of hardware info)
- Admin data: encrypted at rest

### Scalability Requirements

**Horizontal Scaling:**
- API servers: stateless, scale to 100+ instances
- Database: connection pooling, read replicas
- Object storage: unlimited (S3, GCS, Azure Blob scale automatically)

**Vertical Scaling:**
- Single API server: 4 CPU cores, 8GB RAM
- Database: 8 CPU cores, 32GB RAM (can scale up to 128 cores)

**Traffic Patterns:**
- Spiky: new release → sudden traffic surge
- Solution: CDN absorbs spikes, auto-scaling for API servers

---

## Deployment Architectures

### Small Deployment (< 10,000 users)

**Configuration:**
- 1 server (API + database on same machine)
- Filesystem storage (local disk)
- No CDN (direct download from server)

**Cost:** ~$50/month (single VPS)

**Limitations:**
- No high availability
- Limited to single region
- Bandwidth costs can spike

### Medium Deployment (10,000 - 100,000 users)

**Configuration:**
- 2-3 API servers (load-balanced)
- 1 PostgreSQL database (with streaming replication standby)
- S3/Azure Blob for storage
- CDN in front (CloudFlare, CloudFront)

**Cost:** ~$500/month

**Benefits:**
- High availability (one server can fail)
- Global distribution via CDN
- Bandwidth costs predictable

### Large Deployment (100,000+ users)

**Configuration:**
- 10+ API servers (auto-scaling group)
- PostgreSQL cluster (primary + 2 read replicas)
- Multi-region object storage
- CDN with edge caching
- Redis for metadata caching
- Dedicated analytics database (ClickHouse, TimescaleDB)

**Cost:** ~$5,000+/month

**Benefits:**
- Global low-latency (multi-region)
- Auto-scales with traffic
- Advanced analytics
- 99.99% uptime

---

## Monitoring and Observability

### Metrics (Prometheus)

**System Metrics:**
- `api_request_duration_seconds` - API latency histogram
- `api_request_total` - Request count by endpoint and status
- `diff_generation_duration_seconds` - Diff computation time
- `active_connections` - Current API connections

**Business Metrics:**
- `updates_total` - Count by version and success/failure
- `bandwidth_saved_bytes` - Total bandwidth savings
- `current_version_distribution` - Histogram of user versions
- `rollout_progress_percent` - Percentage of users on target version

### Logging (Structured JSON)

**Log Levels:**
- ERROR: Failures requiring attention
- WARN: Degraded performance, retries
- INFO: Normal operations (version published, update applied)
- DEBUG: Detailed troubleshooting info

**Log Fields:**
- `timestamp`: ISO 8601
- `level`: ERROR, WARN, INFO, DEBUG
- `component`: api, diff-generator, analytics, etc.
- `event`: version_published, update_requested, etc.
- `metadata`: event-specific fields (version, app_name, device_id, etc.)

### Tracing (OpenTelemetry)

**Distributed Traces:**
- Trace update flow from client request → API → database → storage
- Identify bottlenecks (slow database query, slow diff generation)
- Correlate errors across services

**Spans:**
- `check_update`: Total time for check-update API call
- `query_database`: Time to query version info
- `check_rollout_rules`: Time to evaluate rollout logic
- `fetch_diff_metadata`: Time to get diff info from storage

---

## Disaster Recovery

### Backup Strategy

**Database Backups:**
- Full backup: daily at 2 AM UTC
- Incremental: every 4 hours
- Retention: 30 days
- Storage: separate region from primary database

**Object Storage:**
- S3 versioning enabled (recover from accidental deletion)
- Cross-region replication (disaster recovery)
- Glacier archival after 90 days (cost optimization)

### Recovery Procedures

**Database Failure:**
1. Promote read replica to primary (< 1 minute)
2. Update DNS/load balancer to new primary
3. Restore replication to new standby

**Object Storage Failure:**
- S3/Azure Blob have built-in redundancy (automatic recovery)
- Cross-region replication provides backup

**Complete Region Failure:**
1. Failover to secondary region (manual or automatic)
2. Update DNS to secondary region load balancer
3. Secondary region serves traffic while primary recovers

**Recovery Time Objective (RTO):** < 15 minutes
**Recovery Point Objective (RPO):** < 15 minutes (data loss window)

---

## Security Architecture

### Threat Model

**Threats Protected Against:**
- Malicious update injection (signature verification)
- Man-in-the-middle (TLS + signature)
- Compromised update server (client verifies signature)
- Replay attacks (timestamp in signed metadata)
- Rollback attacks (version monotonicity check)

**Threats NOT Protected Against (Out of Scope for v1.0):**
- Compromised publisher signing key (key rotation helps)
- Zero-day exploits in patch algorithm (use trusted libraries)
- Physical access to client device (OS-level security)

### Defense in Depth

**Layer 1: Network**
- TLS 1.2+ for all traffic
- Certificate pinning (optional, for high-security deployments)
- Rate limiting (prevent DoS)

**Layer 2: Application**
- Input validation (reject malformed requests)
- Authentication (API keys, JWT)
- Authorization (rollout rules, admin permissions)

**Layer 3: Data**
- Signature verification (Ed25519)
- Checksum validation (Blake3, SHA-256)
- Atomic operations (prevent partial updates)

**Layer 4: Infrastructure**
- Firewall rules (only necessary ports open)
- Security groups (isolate database from public internet)
- Secrets management (KMS, Vault)

---

## Future Architecture Evolution

### Phase 2 Enhancements (Month 13-18)

**Peer-to-Peer Distribution:**
- Clients can share diffs with nearby clients (LAN)
- Reduces server bandwidth costs
- Protocol: BitTorrent or custom P2P
- Verification: same signature mechanism

**Edge Computation:**
- Compute diffs at CDN edge (Cloudflare Workers, Lambda@Edge)
- Reduces latency (compute near user)
- Challenge: CPU limits on edge functions

**Advanced Rollout:**
- Machine learning for rollout decisions (predict failure risk)
- Geographic rollout (roll out to specific regions first)
- User cohort targeting (beta users, enterprise customers)

### Phase 3 Enhancements (Month 19+)

**Blockchain Audit Trail:**
- Immutable log of all version publications
- Transparency for users (verify publisher behavior)
- Optional (not required for core functionality)

**Post-Quantum Cryptography:**
- Transition to quantum-resistant signatures (e.g., SPHINCS+)
- Phased migration (support both classical and PQ signatures)

**Live Patching:**
- Apply updates without restarting application
- OS-specific mechanisms (Linux: kpatch, Windows: hot patching)
- Limited applicability (some updates require restart)

---

## Conclusion

VBDP's architecture is designed for:
- **Simplicity:** Clear component boundaries, easy to understand
- **Scalability:** Horizontal scaling at every layer
- **Reliability:** Fault-tolerant, automatic recovery
- **Security:** Defense in depth, cryptographic verification
- **Performance:** Optimized for low latency and high throughput

The architecture follows SOLID principles:
- **Single Responsibility:** Each component has one job
- **Open/Closed:** Extensible (new algorithms, storage backends) without modifying core
- **Liskov Substitution:** Platform abstraction allows swapping implementations
- **Interface Segregation:** Clean APIs between components
- **Dependency Inversion:** Depend on abstractions (storage interface, not S3-specific code)

**Next Steps:**
- **For deployment:** Read [Deployment Guides](../deployment/)
- **For operations:** Read [Maintenance](../operations/MAINTENANCE.md)
- **For security:** Read [Security Model](../security/SECURITY_MODEL.md)
- **For API details:** Read [API Specification](../api/API_SPECIFICATION.md)

---

**End of System Design Document**
