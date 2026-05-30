# Frequently Asked Questions (FAQ)

**Document:** Common questions about Deltaship
**Audience:** Developers, decision makers, new users
**Last Updated:** 2026-01-13

> **NOTE:** Email addresses on this page use the RFC 2606 `example.com` domain and are placeholders. Replace with real contacts before publishing.

---

## General Questions

### What is Deltaship?

Deltaship (Version-Aware Binary Differential Update System) is a high-performance software update system that transmits only the differences between binary versions instead of entire files. It consists of three main components:

1. **Publisher Toolkit** - For creating and signing patches
2. **Update Server** - For hosting and serving updates
3. **Client Patcher** - For downloading and applying updates on end-user devices

The key innovation is that the server knows both source and target versions, enabling optimal binary differencing that can reduce update sizes by 90-99%.

---

### How much bandwidth can I save?

Typical bandwidth savings with Deltaship:

| Update Type | Savings |
|-------------|---------|
| Minor updates (bug fixes) | 95-99% |
| Feature releases | 80-95% |
| Major version upgrades | 60-80% |

**Real-world example:**
- Traditional update: 100MB download
- Deltaship update: 1-5MB download

For a million users updating monthly with a 100MB application:
- Traditional: ~100TB bandwidth/month
- Deltaship: ~1-5TB bandwidth/month

The system automatically falls back to full download if the diff exceeds 50% of the original file size.

---

### What platforms are supported?

**Tier 1 (Full Support):**
- Linux (x86_64, aarch64) - systemd integration
- Windows (x86_64) - Windows Service
- macOS (x86_64, Apple Silicon) - launchd integration

**Tier 2 (Basic Support):**
- FreeBSD, OpenBSD
- Android (via app integration)
- iOS (via app integration)

**Tier 3 (Planned):**
- Embedded Linux (minimal resource version)
- RTOS platforms

---

### Is it secure?

Yes. Deltaship implements multiple security layers:

**Cryptographic Verification:**
- **Ed25519 signatures** - Publisher signs all updates; clients verify before applying
- **Blake3 hashing** - Fast cryptographic checksums verify every byte
- **ChaCha20-Poly1305 encryption** - Optional encryption for private updates

**Security Features:**
- End-to-end verification (publisher signs, client verifies)
- Atomic updates with automatic rollback on failure
- Version monotonicity prevents downgrade attacks
- TLS 1.2/1.3 for transport security

**Threat Model Coverage:**
- Man-in-the-middle attacks (signature verification)
- Corrupted downloads (checksum validation)
- Compromised servers (publisher private key not on server)
- Partial/interrupted updates (atomic operations)

For complete details, see [Security Model](security/SECURITY_MODEL.md).

---

## Getting Started

### How do I get started?

**For Software Publishers:**
1. Install the publisher toolkit: `cargo install deltaship-publisher`
2. Generate a signing key pair
3. Integrate diff generation into your build pipeline
4. Upload diffs to your update server

**For System Administrators:**
1. Deploy the update server: `cargo install deltaship-server`
2. Configure storage backend (local, S3, or database)
3. Set up monitoring and alerting
4. Configure rollout policies

**For End Users/Developers:**
1. Install the client patcher: `cargo install deltaship-client`
2. Configure the update server URL
3. Run the client as a background service

Quick start guides:
- [Publisher Setup](deployment/PUBLISHER_SETUP.md)
- [Server Deployment](deployment/SERVER_DEPLOYMENT.md)
- [Client Installation](deployment/CLIENT_INSTALLATION.md)

---

### What's the difference between Deltaship and other update systems?

| Feature | Deltaship | rsync | Traditional |
|---------|------|-------|-------------|
| **Protocol** | HTTP/HTTPS | Custom | HTTP |
| **CDN-friendly** | Yes | No | Yes |
| **Version-aware** | Yes | No | No |
| **Diff computation** | Server-side | Client-side | None |
| **Compressed files** | Works | Limited | N/A |
| **Signature verification** | Built-in | External | External |
| **Gradual rollout** | Built-in | No | No |
| **Analytics** | Built-in | No | No |

**Key differentiators:**

1. **vs rsync:** Deltaship is HTTP-native (works through firewalls/proxies), includes cryptographic signing, and is designed for end-user updates rather than server synchronization.

2. **vs Traditional downloads:** Deltaship reduces bandwidth by 90-99% by sending only changed bytes.

3. **vs Browser updaters (Chrome/Firefox):** Deltaship is a generic framework any application can use, not tied to a specific vendor.

4. **vs BDP/hsynz:** Deltaship handles compressed files correctly and provides version-aware diffing with rollout control.

For detailed comparisons, see [Comparison](COMPARISON.md).

---

## Mobile and Special Use Cases

### Can I use Deltaship with mobile apps?

Yes, with some considerations:

**Android:**
- Integrate the Deltaship client library into your app
- Updates apply to app assets, not the APK itself (Google Play handles APK updates)
- Ideal for games with large content updates or apps with downloadable resources

**iOS:**
- Similar to Android: use for in-app content updates
- App Store handles the main binary; Deltaship handles supplementary content
- Useful for: game assets, ML models, configuration bundles

**Mobile Considerations:**
- Battery impact: Schedule updates during charging
- Network: Respect metered connections; prefer Wi-Fi
- Storage: Ensure sufficient space before patching
- Background limits: iOS and Android restrict background activity

**Best Use Cases for Mobile:**
- Games with large asset bundles (hundreds of MB)
- Apps with frequently updated content (news, catalogs)
- Enterprise apps with frequent releases
- Apps with downloadable ML models or databases

---

### How does gradual rollout work?

Deltaship supports staged deployments to minimize risk:

**Rollout Stages:**

1. **Canary (1-5%)** - Initial deployment to a small percentage of users
2. **Early Adopters (10-25%)** - Expand if canary succeeds
3. **General Availability (50-100%)** - Full rollout

**Configuration Example:**
```json
{
  "rollout": {
    "strategy": "percentage",
    "stages": [
      {"name": "canary", "percentage": 5, "duration": "24h"},
      {"name": "early", "percentage": 25, "duration": "48h"},
      {"name": "general", "percentage": 100}
    ],
    "auto_pause_on_error_rate": 0.01
  }
}
```

**Features:**
- **Automatic progression:** Move to next stage after duration/success criteria
- **Automatic pause:** Stop rollout if error rate exceeds threshold
- **Manual control:** Pause, resume, or rollback at any time
- **Targeting:** Roll out to specific regions, OS versions, or user groups first

**Monitoring During Rollout:**
- Success/failure rates per stage
- Error types and frequencies
- User feedback signals
- Performance metrics (patch time, bandwidth)

**Emergency Rollback:**
- One-click rollback to previous version
- Affects all users or specific groups
- Preserves user data and settings

---

## Additional Questions

### What file types work best with Deltaship?

**Excellent (95-99% savings):**
- Uncompressed executables
- Native binaries (ELF, PE, Mach-O)
- Database files (SQLite)
- Large JSON/XML configurations

**Good (80-95% savings):**
- Disk images
- Uncompressed archives (tar)
- Game assets (textures, models)

**Moderate (50-80% savings):**
- Pre-compressed archives (.zip, .tar.gz) - Deltaship decompresses, diffs, then recompresses
- Container images (requires special handling)

**Not Recommended:**
- Highly compressed files with minor changes (encrypted data)
- Streaming media (use streaming protocols instead)
- Live databases (use replication instead)

---

### How do I report security issues?

Please do NOT report security vulnerabilities through public GitHub issues.

**Report via email:** security@deltaship.example.com

Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

**Response Timeline:**
- Acknowledgment: Within 48 hours
- Initial assessment: Within 7 days
- Resolution target: Within 90 days

See [SECURITY.md](../SECURITY.md) for complete security policy and PGP key.

---

### Where can I get help?

- **Documentation:** [docs/README.md](README.md)
- **GitHub Issues:** For bugs and feature requests
- **Discussions:** For questions and community support
- **Security Issues:** security@deltaship.example.com (private disclosure)

---

**End of FAQ Document**
