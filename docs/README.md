# Version-Aware Binary Differential Update System (VBDP)

## Documentation Overview

**Last Updated:** 2026-01-07
**Version:** 1.0
**Status:** Design Phase

---

## What is VBDP?

A **server-side computed, client-side applied** binary differential update system that enables software updates by downloading only changed bits/bytes instead of entire files.

**Key Innovation:** The server knows both the current and target versions, computes the optimal binary difference, and sends only the delta. The client applies this patch in-place to transform the existing binary to the new version.

---

## Core Principles

### 1. Simplicity

- Single responsibility for each component
- Clear interfaces between systems
- Minimal dependencies

### 2. Performance

- Optimal bandwidth usage (download only what changed)
- Low computational overhead on clients
- Efficient server-side caching

### 3. Security

- Cryptographic signature verification
- Checksum validation at every step
- Atomic updates with rollback capability

### 4. Reliability

- Graceful degradation (fall back to full download)
- Comprehensive error handling
- Self-healing mechanisms

### 5. Adaptability

- Works with existing software (no recompilation needed)
- Integrates with current CI/CD pipelines
- Platform-agnostic design

---

## Documentation Structure

### 📁 Tools Documentation

Detailed specifications for each system component:

- **[Publisher Toolkit](tools/PUBLISHER_TOOLKIT.md)** - Tools for software distributors to create and publish diffs
- **[Update Server](tools/UPDATE_SERVER.md)** - Server that stores versions and computes/serves diffs
- **[Client Patcher](tools/CLIENT_PATCHER.md)** - Client-side daemon that downloads and applies patches

### 📁 Architecture Documentation

System design and specifications:

- **[System Design](architecture/SYSTEM_DESIGN.md)** - Overall architecture and component interactions
- **[Protocol Specification](architecture/PROTOCOL_SPECIFICATION.md)** - Communication protocol between components
- **[Security Model](security/SECURITY_MODEL.md)** - Security architecture and threat model

### 📁 Technical Documentation

Feasibility and performance analysis:

- **[Feasibility Analysis](technical/FEASIBILITY_ANALYSIS.md)** - Technical feasibility for different platforms
- **[Performance Targets](technical/PERFORMANCE_TARGETS.md)** - Performance goals and benchmarks
- **[Platform Requirements](technical/PLATFORM_REQUIREMENTS.md)** - System requirements per platform

Core technical specifications:

- **[Database Schema](technical/DATABASE_SCHEMA.md)** - PostgreSQL and SQLite database schemas
- **[Version Specification](technical/VERSION_SPECIFICATION.md)** - Version comparison and ordering logic
- **[Compression Handling](technical/COMPRESSION_HANDLING.md)** - Handling compressed archives (npm, Docker)
- **[Rollback Specification](technical/ROLLBACK_SPECIFICATION.md)** - Rollback mechanisms and policies
- **[Storage Cost Model](technical/STORAGE_COST_MODEL.md)** - Storage cost analysis and retention policies

### 📁 Deployment Documentation

Installation and setup guides:

- **[Publisher Setup](deployment/PUBLISHER_SETUP.md)** - Setting up publisher tools
- **[Server Deployment](deployment/SERVER_DEPLOYMENT.md)** - Deploying update servers
- **[Client Installation](deployment/CLIENT_INSTALLATION.md)** - Installing client patcher on end-user systems

### 📁 Integration Documentation

Integrating with existing systems:

- **[Integration & Adoption Guide](integration/INTEGRATION_GUIDE.md)** ⭐ - The canonical end-to-end adoption guide: deploy → publish → integrate, supported platforms, the updater sidecar contract, HTTP API, and security model
- **[Language Examples](integration/LANGUAGE_EXAMPLES.md)** - Copy-paste updater integration for C, C++, C#, Go, Python, Node/Electron, Java, Rust, and shell
- **[Existing Systems Integration](integration/EXISTING_SYSTEMS.md)** - Adapting current software update mechanisms
- **[CI/CD Integration](integration/CI_CD_INTEGRATION.md)** - Integrating into build pipelines

### 📁 Operations Documentation

Running and maintaining the system:

- **[Monitoring](operations/MONITORING.md)** - Metrics, logging, and observability
- **[Maintenance](operations/MAINTENANCE.md)** - Ongoing maintenance tasks

### 📄 Additional Documents

- **[Complete Flow](COMPLETE_FLOW.md)** - End-to-end flow from build to user update
- **[Comparison](COMPARISON.md)** - Comparison with existing solutions (rsync, BDP, Docker layers)
- **[Roadmap](ROADMAP.md)** - Development phases and timeline

---

## Quick Start Paths

### For Application Developers (embedding the updater)

1. Read the [Integration & Adoption Guide](integration/INTEGRATION_GUIDE.md)
2. Pick your language in [Language Examples](integration/LANGUAGE_EXAMPLES.md)
3. Review the [updater sidecar contract](integration/INTEGRATION_GUIDE.md#5-the-vbdp-updater-sidecar-contract) and [per-platform notes](integration/INTEGRATION_GUIDE.md#7-per-platform-integration-notes)

### For Software Publishers

1. Read [Publisher Toolkit](tools/PUBLISHER_TOOLKIT.md)
2. Review [Publisher Setup](deployment/PUBLISHER_SETUP.md)
3. Check [CI/CD Integration](integration/CI_CD_INTEGRATION.md)

### For System Administrators

1. Read [Update Server](tools/UPDATE_SERVER.md)
2. Review [Server Deployment](deployment/SERVER_DEPLOYMENT.md)
3. Check [Monitoring](operations/MONITORING.md)

### For Enterprise IT

1. Read [System Design](architecture/SYSTEM_DESIGN.md)
2. Review [Existing Systems Integration](integration/EXISTING_SYSTEMS.md)
3. Check [Client Installation](deployment/CLIENT_INSTALLATION.md)

### For Decision Makers

1. Read [Feasibility Analysis](technical/FEASIBILITY_ANALYSIS.md)
2. Review [Comparison](COMPARISON.md)
3. Check [Roadmap](ROADMAP.md)

---

## System Components Overview

```bash
┌─────────────────────────────────────────────────────┐
│  PUBLISHER TOOLKIT                                   │
│  (Software Distributor Side)                         │
│  ─────────────────────────                           │
│  • Version registration                              │
│  • Diff generation                                   │
│  • Cryptographic signing                             │
│  • Testing & validation                              │
│  • Publishing to server                              │
└─────────────────┬───────────────────────────────────┘
                  │
                  ▼ Upload diffs & metadata
┌─────────────────────────────────────────────────────┐
│  UPDATE SERVER                                       │
│  (Centralized or CDN)                                │
│  ──────────────────────                              │
│  • Version storage                                   │
│  • Diff caching                                      │
│  • On-demand diff computation                        │
│  • Signature verification                            │
│  • Analytics & monitoring                            │
└─────────────────┬───────────────────────────────────┘
                  │
                  ▼ Download diffs (4KB instead of 50MB)
┌─────────────────────────────────────────────────────┐
│  CLIENT PATCHER                                      │
│  (End-User Device)                                   │
│  ───────────────────                                 │
│  • Version detection                                 │
│  • Update checking                                   │
│  • Diff downloading                                  │
│  • Signature verification                            │
│  • Atomic patch application                          │
│  • Rollback on failure                               │
└─────────────────────────────────────────────────────┘
```

---

## Key Benefits

### For End Users

- **Faster updates:** Download only 1-5% of file size
- **Lower data usage:** Critical for metered connections
- **Transparent:** Updates happen in background
- **Reliable:** Automatic rollback on failure

### For Software Distributors

- **Bandwidth savings:** 95-99% reduction in CDN costs
- **Faster rollouts:** Users update quickly (small downloads)
- **Better analytics:** Track version transitions
- **Gradual rollouts:** Canary deployments built-in

### For System Administrators

- **Centralized management:** One patcher for all software
- **OS-level integration:** No per-app configuration
- **Audit trail:** Complete update history
- **Security:** Cryptographic verification

---

## Design Philosophy

### Single Responsibility Principle

Each component has one clear purpose:

- Publisher: Create and sign diffs
- Server: Store and serve diffs
- Client: Apply patches

### Open/Closed Principle

- Open for extension: Custom diff algorithms, storage backends, protocols
- Closed for modification: Core logic remains stable

### Liskov Substitution Principle

- Any diff algorithm implementation is interchangeable
- Any storage backend (S3, local, database) works
- Any signature scheme (Ed25519, RSA) is supported

### Interface Segregation Principle

- Small, focused interfaces
- Publishers don't need client code
- Clients don't need server code
- Each tool standalone

### Dependency Inversion Principle

- Depend on abstractions (protocols, interfaces)
- Not concrete implementations
- Swap components without breaking system

---

## Target Use Cases

### Primary

1. **Desktop Applications** (Electron, native apps)
2. **Mobile Applications** (APK, IPA updates)
3. **IoT/Embedded Firmware** (constrained bandwidth)
4. **Enterprise Software** (internal tools, frequent updates)

### Secondary

1. **Game Updates** (large files, frequent patches)
2. **Operating System Components** (system libraries, kernel modules)
3. **Container Images** (Docker, but uncompressed layers)

### Excluded (v1.0)

- Pre-compressed archives (.tar.gz, .zip) - see technical limitations
- Streaming media files (different use case)
- Database files (use database replication)

---

## Performance Targets

### Bandwidth Savings

- **Target:** 95-99% reduction for typical updates
- **Minimum:** 80% reduction for major updates
- **Fallback:** Automatic full download if diff > 50% of file

### Update Speed

- **Diff computation:** < 5 seconds for 100MB binary
- **Patch application:** < 10 seconds for 100MB binary
- **Total update time:** < 2 minutes including download (on 10Mbps connection)

### Resource Usage

- **Client CPU:** < 5% average during patching
- **Client Memory:** < 100MB peak
- **Server CPU:** < 1 core per 100 concurrent updates
- **Storage overhead:** 2-3x (original + diffs + signatures)

---

## Security Model

### Three Pillars

1. **Integrity:** Cryptographic checksums (Blake3) verify every byte
2. **Authenticity:** Digital signatures (Ed25519) prove publisher identity
3. **Atomicity:** All-or-nothing updates with automatic rollback

### Threat Model Coverage

- ✅ Man-in-the-middle attacks (signature verification)
- ✅ Corrupted downloads (checksum validation)
- ✅ Partial updates (atomic operation)
- ✅ Malicious servers (signature from publisher's private key)
- ⚠️ Compromised publisher key (key rotation mechanism required)
- ⚠️ Side-channel attacks (out of scope for v1.0)

---

## Platform Support

### Tier 1 (Full Support)

- Linux (x86_64, aarch64) - systemd integration
- Windows (x86_64) - Windows Service
- macOS (x86_64, Apple Silicon) - launchd integration

### Tier 2 (Basic Support)

- FreeBSD, OpenBSD
- Android (via app integration)
- iOS (via app integration)

### Tier 3 (Future)

- Embedded Linux (minimal resource version)
- RTOS platforms

---

## Getting Started

1. **Understand the system:** Read [Complete Flow](COMPLETE_FLOW.md)
2. **Check feasibility:** Review [Feasibility Analysis](technical/FEASIBILITY_ANALYSIS.md)
3. **Choose deployment model:** See [System Design](architecture/SYSTEM_DESIGN.md)
4. **Follow setup guide:** Based on your role (publisher/admin/developer)

---

## Contributing

This is a design document for a new system. Feedback welcome on:

- Architecture decisions
- Security model
- Performance targets
- Platform requirements
- Use case coverage

---

## References

### Similar Technologies

- **bsdiff/bspatch:** Binary diff algorithm (Colin Percival, 2003)
- **Google Courgette:** Executable-aware diffing for Chrome updates
- **Windows Update:** Delta compression in OS updates
- **zsync/rsync:** File synchronization protocols
- **casync:** Content-addressable data synchronizer

### Academic Papers

- "Fast CDC for Data Deduplication" (USENIX ATC 2016)
- "Towards Web-based Delta Synchronization" (USENIX FAST 2018)
- "Binary Diffing Algorithms Survey" (ACM Computing Surveys 2020)

### Standards

- RFC 3229: Delta encoding in HTTP
- IETF SUIT: Software Updates for IoT
- COSE: CBOR Object Signing and Encryption

---

## License

Documentation: CC BY 4.0
Future Implementation: To be determined

---

**Next:** Read [COMPLETE_FLOW.md](COMPLETE_FLOW.md) to understand the end-to-end update process.
