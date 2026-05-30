# Platform Requirements

**Document:** System requirements for running VBDP on different platforms
**Audience:** System administrators, users, deployment engineers
**Last Updated:** 2026-01-07

---

## Overview

This document specifies the minimum and recommended system requirements for running VBDP components (Publisher Toolkit, Update Server, Client Patcher) on various platforms.

---

## Platform Support Tiers

### Tier 1: Full Support
- Active development and testing
- Commercial support available
- Performance optimized
- Full feature set

**Platforms:**
- Linux (x86_64, aarch64)
- Windows (x86_64)
- macOS (x86_64, Apple Silicon)

### Tier 2: Community Support
- Community-maintained
- Basic testing
- May have limitations

**Platforms:**
- FreeBSD, OpenBSD
- Linux (armv7, riscv64)
- Windows (aarch64)

### Tier 3: Experimental
- No official support
- May work but untested
- Community contributions welcome

**Platforms:**
- Android (via app integration)
- iOS (via app integration)
- Embedded Linux (custom builds)

---

## Client Patcher Requirements

### Linux

**Tier 1 Distributions:**
- Ubuntu 20.04 LTS, 22.04 LTS, 24.04 LTS
- Debian 11 (Bullseye), 12 (Bookworm)
- RHEL 8, 9
- Fedora 38, 39, 40
- CentOS Stream 8, 9
- Rocky Linux 8, 9
- openSUSE Leap 15.5+

**Minimum Requirements:**
- **CPU:** 1 GHz single-core (x86_64 or aarch64)
- **RAM:** 256MB available
- **Disk:** 50MB for client patcher + 2x binary size for updates
- **Kernel:** Linux 3.10+ (for systemd)
- **Init System:** systemd (preferred) or sysvinit

**Recommended Requirements:**
- **CPU:** 2 GHz dual-core
- **RAM:** 512MB available
- **Disk:** 1GB free space
- **Network:** 1 Mbps or faster
- **Storage:** SSD (for faster patch application)

**Dependencies:**
- **Runtime:**
  - glibc 2.17+ or musl libc 1.2+
  - OpenSSL 1.1.1+ or 3.0+ (for TLS)
  - libsodium 1.0.18+ (for Ed25519, or use built-in)
- **Optional:**
  - systemd 219+ (for service management)
  - dbus (for desktop notifications)

**Installation Methods:**
- `.deb` package (Debian, Ubuntu)
- `.rpm` package (RHEL, Fedora, openSUSE)
- Flatpak (distribution-agnostic)
- Binary tarball (manual installation)

---

### Windows

**Supported Versions:**
- Windows 10 (version 1809 or later)
- Windows 11 (all versions)
- Windows Server 2019, 2022

**Minimum Requirements:**
- **CPU:** 1 GHz single-core (x86_64)
- **RAM:** 512MB available
- **Disk:** 100MB for client patcher + 2x binary size for updates
- **OS:** Windows 10 1809+ or Windows Server 2019+

**Recommended Requirements:**
- **CPU:** 2 GHz dual-core
- **RAM:** 1GB available
- **Disk:** 2GB free space
- **Network:** 1 Mbps or faster
- **Storage:** SSD

**Dependencies:**
- **Runtime:**
  - .NET Framework 4.8 (included in Windows 10 1903+)
  - Or: .NET 6.0+ Runtime (for .NET Core version)
- **Optional:**
  - Windows Service (for background operation)
  - Task Scheduler (fallback if service can't be installed)

**Installation Methods:**
- MSI installer (Windows Installer)
- Chocolatey package
- Winget package
- Portable ZIP (no installation)

**Permissions:**
- **System-wide installation:** Requires Administrator privileges
- **User-level installation:** No admin privileges needed (limited to user applications)

---

### macOS

**Supported Versions:**
- macOS 12 (Monterey) and later
- macOS 13 (Ventura)
- macOS 14 (Sonoma)

**Architectures:**
- Intel (x86_64)
- Apple Silicon (arm64)
- Universal Binary (both architectures in one package)

**Minimum Requirements:**
- **CPU:** Apple M1 or Intel Core i3
- **RAM:** 512MB available
- **Disk:** 100MB for client patcher + 2x binary size for updates
- **OS:** macOS 12.0+

**Recommended Requirements:**
- **CPU:** Apple M2 or Intel Core i5
- **RAM:** 1GB available
- **Disk:** 2GB free space
- **Network:** 1 Mbps or faster

**Dependencies:**
- **Runtime:**
  - macOS system libraries (bundled with OS)
  - No additional dependencies required
- **Optional:**
  - launchd (for automatic startup)
  - Notification Center (for update notifications)

**Installation Methods:**
- `.pkg` installer (macOS Installer)
- Homebrew formula
- DMG with app bundle (drag-and-drop)

**Code Signing:**
- **Required:** Developer ID certificate (for Gatekeeper)
- **Notarization:** Required for macOS 10.15+ (automated in CI/CD)

---

## Publisher Toolkit Requirements

### All Platforms

**Supported:**
- Linux (x86_64, aarch64)
- macOS (x86_64, Apple Silicon)
- Windows (x86_64)
- Any platform with Rust 1.70+ support

**Minimum Requirements:**
- **CPU:** 2 GHz dual-core (4 cores for large binaries)
- **RAM:** 2GB available (for diffing 100MB binaries)
  - Formula: ~2x binary size for in-memory diffing
- **Disk:** 500MB for toolkit + 10GB for version storage
- **Network:** Broadband (for uploading to server)

**Recommended Requirements:**
- **CPU:** 3 GHz quad-core or better
- **RAM:** 8GB available (for diffing large binaries)
- **Disk:** 100GB for version history
- **Storage:** SSD (for faster diff generation)
- **Network:** 10 Mbps upload or faster

**Software Dependencies:**
- **Runtime:**
  - Rust 1.70+ (if building from source)
  - Or: Pre-built binary (no dependencies)
- **Build Dependencies (source build only):**
  - cargo (Rust package manager)
  - cmake 3.15+ (for native dependencies)
  - OpenSSL development headers
  - libsodium development headers (optional)

**Installation Methods:**
- Pre-built binary (recommended)
- Cargo install (from crates.io)
- Package manager (apt, dnf, homebrew)
- Build from source

---

## Update Server Requirements

### Small Deployment (<10,000 users)

**Minimum Server Specs:**
- **CPU:** 2 cores (2.5 GHz)
- **RAM:** 4GB
- **Disk:** 100GB SSD
- **Network:** 100 Mbps
- **OS:** Ubuntu 22.04 LTS or Docker

**Software Requirements:**
- **Database:** SQLite 3.35+ (bundled, file-based)
- **Storage:** Local filesystem
- **Web Server:** Built-in (no external web server needed)

**Estimated Capacity:**
- Concurrent updates: 100
- Check-update requests/second: 500
- Total users: 10,000

---

### Medium Deployment (10,000 - 100,000 users)

**Recommended Server Specs (per API instance):**
- **CPU:** 4 cores (3.0 GHz)
- **RAM:** 8GB
- **Disk:** 50GB SSD (API server, no storage)
- **Network:** 1 Gbps
- **OS:** Ubuntu 22.04 LTS or Docker

**Infrastructure:**
- **API Servers:** 2-3 instances (load-balanced)
- **Database:** PostgreSQL 14+ (8 cores, 16GB RAM, 100GB SSD)
- **Storage:** S3-compatible object storage (AWS S3, MinIO, etc.)
- **CDN:** CloudFlare, CloudFront, or Fastly
- **Load Balancer:** HAProxy, Nginx, or cloud LB

**Software Requirements:**
- **Database:** PostgreSQL 14+, MySQL 8.0+, or MariaDB 10.6+
- **Object Storage:** S3 API compatible
- **Cache (optional):** Redis 6.0+ (for metadata caching)
- **Reverse Proxy:** Nginx 1.20+ or Caddy 2.0+

**Estimated Capacity:**
- Concurrent updates: 1,000
- Check-update requests/second: 5,000
- Total users: 100,000

---

### Large Deployment (100,000 - 1,000,000 users)

**Recommended Server Specs (per API instance):**
- **CPU:** 8 cores (3.5 GHz)
- **RAM:** 16GB
- **Disk:** 100GB SSD
- **Network:** 10 Gbps
- **OS:** Ubuntu 22.04 LTS or Kubernetes

**Infrastructure:**
- **API Servers:** 10+ instances (auto-scaling)
- **Database:** PostgreSQL cluster (primary + 2 read replicas, 16 cores, 32GB RAM each)
- **Storage:** Multi-region S3 (AWS S3, Google Cloud Storage)
- **CDN:** Global CDN with edge caching
- **Cache:** Redis cluster (3 nodes, 16GB RAM each)
- **Monitoring:** Prometheus, Grafana, ELK stack

**Software Requirements:**
- **Database:** PostgreSQL 14+ with replication
- **Object Storage:** AWS S3, GCS, Azure Blob Storage
- **Cache:** Redis 6.0+ cluster
- **Orchestration:** Kubernetes 1.24+
- **Service Mesh (optional):** Istio, Linkerd

**Estimated Capacity:**
- Concurrent updates: 10,000
- Check-update requests/second: 50,000
- Total users: 1,000,000

---

## Network Requirements

### Client Patcher

**Outbound Connections:**
- Port 443 (HTTPS) to update server
- Optional: Port 80 (HTTP) for redirect to HTTPS

**Firewall Rules:**
- Allow outbound to update server domain
- No inbound connections required

**Bandwidth:**
- Minimal (1-5 KB per update check)
- Variable for diff download (typically 1-10 MB)
- Burst usage during updates, idle otherwise

**Proxy Support:**
- HTTP proxy: Supported via environment variables
- HTTPS proxy: Supported
- Authenticated proxy: Supported (Basic Auth)
- PAC (Proxy Auto-Config): Supported via OS settings

---

### Publisher Toolkit

**Outbound Connections:**
- Port 443 (HTTPS) to update server (for publish)
- Port 443 (HTTPS) to S3 (for direct upload)

**Bandwidth:**
- Upload: Proportional to binary size
- For 100MB binary: ~10 seconds on 100 Mbps upload
- For 1GB binary: ~80 seconds on 100 Mbps upload

---

### Update Server

**Inbound Connections:**
- Port 443 (HTTPS) from clients and publishers
- Optional: Port 80 (HTTP) for redirect

**Outbound Connections:**
- Database server (PostgreSQL: port 5432)
- Object storage (S3: port 443)
- CDN (for cache purging: port 443)

**Bandwidth:**
- Check-update: ~5KB per request
- Download (if not CDN): Proportional to diff size
- With CDN: Minimal (most traffic served from CDN)

---

## Security Requirements

### TLS/SSL Certificates

**Client Patcher:**
- Trusts system certificate store
- Validates server certificates
- Optional: Certificate pinning for high-security

**Update Server:**
- Valid TLS certificate (Let's Encrypt, commercial CA)
- TLS 1.2+ required (TLS 1.3 recommended)
- Strong cipher suites (no RC4, DES, MD5)

### Cryptographic Libraries

**Required:**
- Ed25519 signature verification
- Blake3 or SHA-256 hashing
- HMAC-SHA256 (for API authentication)

**Recommended Libraries:**
- libsodium 1.0.18+ (provides all crypto primitives)
- Or: OpenSSL 3.0+ (but Ed25519 support check required)
- Or: ring (Rust crypto library, fast and audited)

---

## Storage Requirements

### Client Patcher

**Persistent Storage:**
- Client binary: ~10MB
- Version database: <1MB (SQLite)
- Configuration: <1KB

**Temporary Storage:**
- Downloaded diff: Variable (deleted after application)
- Binary backup: 1x binary size (for rollback)
- Maximum during update: 2x binary size + diff size

**Example (100MB application):**
- Base: 10MB (client) + 1MB (database) = 11MB
- During update: 100MB (backup) + 100MB (new) + 1MB (diff) = 201MB
- After update: 11MB (diff and backup deleted)

---

### Publisher Toolkit

**Persistent Storage:**
- Toolkit binary: ~20MB
- Version database: Variable (~100KB per version)
- Binaries: 1x size per version
- Diffs: ~1-5% size per version pair

**Example (100MB binary, 10 versions):**
- Toolkit: 20MB
- Database: 1MB
- Binaries: 10 × 100MB = 1GB
- Diffs: 10 × 5 × 1MB = 50MB
- **Total: ~1.1GB**

---

### Update Server

**Database Storage:**
- Metadata: ~1KB per version
- Analytics: ~100 bytes per update event
- For 100 apps, 100 versions each, 1M updates:
  - Metadata: 10MB
  - Analytics: 100MB
  - **Total: ~110MB**

**Object Storage:**
- Binaries: 1x size per version
- Diffs: ~1-5% size per version transition
- See PERFORMANCE_TARGETS.md for growth model

---

## Browser Requirements (for Admin UI)

**Supported Browsers:**
- Chrome 90+
- Firefox 88+
- Safari 14+
- Edge 90+

**Requirements:**
- JavaScript enabled
- Cookies enabled (for authentication)
- WebSocket support (for real-time updates)

---

## Container Requirements

### Docker

**Minimum Docker Version:** 20.10+

**Images:**
- `vbdp/server:latest` (API server)
- `vbdp/publisher:latest` (Publisher toolkit)

**Resource Limits:**
- CPU: 2 cores minimum
- Memory: 4GB minimum
- Storage: 10GB minimum

---

### Kubernetes

**Minimum Kubernetes Version:** 1.24+

**Required Features:**
- StatefulSets (for database)
- Services (LoadBalancer or Ingress)
- PersistentVolumes (for storage)
- ConfigMaps and Secrets

<!-- TODO: Update chart name when published -->
**Helm Chart:** `jayashankarvr/vbdp-server` (version 1.0+)

---

## Development Requirements

### Building from Source

**Required Tools:**
- Rust 1.70+ and cargo
- Git 2.20+
- CMake 3.15+ (for native dependencies)
- C/C++ compiler (gcc 7+, clang 10+, or MSVC 2019+)

**Optional Tools:**
- Docker 20.10+ (for containerized builds)
- cross (for cross-compilation)

**Build Time:**
- Publisher Toolkit: ~5 minutes (release build)
- Update Server: ~8 minutes
- Client Patcher: ~6 minutes

---

## Compatibility Matrix

### Operating Systems

| OS | Client Patcher | Publisher Toolkit | Update Server |
|----|----------------|-------------------|---------------|
| Ubuntu 20.04+ | ✅ Tier 1 | ✅ Tier 1 | ✅ Tier 1 |
| Debian 11+ | ✅ Tier 1 | ✅ Tier 1 | ✅ Tier 1 |
| RHEL 8+ | ✅ Tier 1 | ✅ Tier 1 | ✅ Tier 1 |
| Fedora 38+ | ✅ Tier 1 | ✅ Tier 1 | ✅ Tier 1 |
| Windows 10+ | ✅ Tier 1 | ✅ Tier 1 | ⚠️ Tier 2 |
| Windows Server 2019+ | ✅ Tier 1 | ✅ Tier 1 | ✅ Tier 1 |
| macOS 12+ (Intel) | ✅ Tier 1 | ✅ Tier 1 | ✅ Tier 1 |
| macOS 12+ (Apple Silicon) | ✅ Tier 1 | ✅ Tier 1 | ✅ Tier 1 |
| FreeBSD 13+ | ⚠️ Tier 2 | ⚠️ Tier 2 | ⚠️ Tier 2 |
| OpenBSD 7+ | ⚠️ Tier 2 | ⚠️ Tier 2 | ❌ Not supported |
| Android | 🧪 Tier 3 | ❌ Not supported | ❌ Not supported |
| iOS | 🧪 Tier 3 | ❌ Not supported | ❌ Not supported |

---

## Database Compatibility

### Supported Databases

**Client and Publisher (embedded):**
- SQLite 3.35+ ✅

**Update Server:**
- PostgreSQL 14, 15, 16 ✅ (Recommended)
- MySQL 8.0+ ⚠️ (Community support)
- MariaDB 10.6+ ⚠️ (Community support)
- CockroachDB 22.1+ 🧪 (Experimental, for massive scale)

---

## Cloud Platform Compatibility

### Supported Cloud Providers

**Infrastructure as a Service (IaaS):**
- AWS (EC2, RDS, S3) ✅
- Google Cloud (Compute Engine, Cloud SQL, Cloud Storage) ✅
- Microsoft Azure (VMs, Database, Blob Storage) ✅
- DigitalOcean (Droplets, Managed Database, Spaces) ✅
- Linode, Vultr, Hetzner ✅

**Platform as a Service (PaaS):**
- Heroku ⚠️ (with limitations)
- Google Cloud Run ✅
- AWS App Runner ✅
- Azure Container Instances ✅

**Container Orchestration:**
- Kubernetes (GKE, EKS, AKS, self-hosted) ✅
- Docker Swarm ⚠️
- Nomad 🧪

---

## Next Steps

**For System Administrators:**
- Review requirements for your deployment size
- Provision infrastructure accordingly
- Verify all dependencies are met

**For Developers:**
- Ensure development environment meets requirements
- Install required tools for building from source

**Related Documents:**
- [Server Deployment](../deployment/SERVER_DEPLOYMENT.md) - Deployment guide
- [Client Installation](../deployment/CLIENT_INSTALLATION.md) - Installation guide
- [Feasibility Analysis](FEASIBILITY_ANALYSIS.md) - Technical feasibility

---

**End of Platform Requirements**
