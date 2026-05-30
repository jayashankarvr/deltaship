# Security Model

**Document:** Security architecture and threat mitigation for Deltaship
**Audience:** Security engineers, auditors, architects
**Last Updated:** 2026-01-07

---

## Overview

This document describes the security model of Deltaship (Version-Aware Binary Differential Update System), including threat models, security mechanisms, cryptographic algorithms, and best practices.

**Security Principles:**
- **Defense in Depth:** Multiple layers of security
- **Fail Secure:** Default to secure state on error
- **Principle of Least Privilege:** Minimal permissions required
- **Cryptographic Verification:** All updates signed and verified
- **Transparency:** Open protocol, auditable security

---

## Threat Model

### Assets to Protect

**Critical Assets:**
1. **User Binaries:** Applications on end-user devices
2. **Publisher Private Keys:** Used to sign updates
3. **Update Server:** Infrastructure serving updates
4. **Database:** Version metadata and analytics
5. **Update Distribution:** Diffs and binaries in transit

**Impact of Compromise:**
- **User Binaries:** Malicious code execution on user devices (CRITICAL)
- **Publisher Private Keys:** Ability to sign malicious updates (CRITICAL)
- **Update Server:** Service disruption, data loss (HIGH)
- **Database:** Privacy breach, service disruption (MEDIUM)
- **Distribution:** Man-in-the-middle attacks (MITIGATED by signatures)

### Threat Actors

**Adversaries:**

**1. Network Attacker (Man-in-the-Middle):**
- **Capabilities:** Intercept, modify, replay network traffic
- **Goals:** Inject malicious updates, downgrade attacks
- **Likelihood:** MEDIUM (public Wi-Fi, compromised routers)

**2. Malicious Publisher:**
- **Capabilities:** Valid signing key, publishes intentionally malicious update
- **Goals:** Distribute malware through legitimate channel
- **Likelihood:** LOW (requires key compromise or insider threat)

**3. Update Server Compromise:**
- **Capabilities:** Control over update server, database access
- **Goals:** Distribute malicious updates, steal data
- **Likelihood:** MEDIUM (server vulnerabilities, misconfigurations)

**4. Client-Side Attacker:**
- **Capabilities:** Local access to end-user device
- **Goals:** Tamper with client patcher, bypass updates
- **Likelihood:** HIGH (malware on user device)

**5. Storage Provider Compromise:**
- **Capabilities:** Access to S3/object storage
- **Goals:** Modify stored binaries/diffs
- **Likelihood:** LOW (cloud provider security is strong)

### Threat Scenarios

**Scenario 1: Malicious Update Injection**

**Attack:** Attacker modifies diff in transit to inject malware

**MITIGATED BY:**
- **Cryptographic Signatures:** Client verifies Ed25519 signature before applying
- **HTTPS/TLS:** Transport encryption prevents tampering
- **Hash Verification:** Client verifies hash of patched binary matches expected

**Result:** Attack detected and rejected

**Scenario 2: Downgrade Attack**

**Attack:** Attacker serves old version with known vulnerability

**MITIGATED BY:**
- **Version Monotonicity:** Client rejects downgrades (version must increase)
- **Timestamp in Signature:** Prevents replay of old signed updates
- **Rollback Authorization:** Only authorized rollbacks allowed (signed by publisher)

**Result:** Attack detected and rejected

**Scenario 3: Compromised Update Server**

**Attack:** Attacker gains control of update server, tries to distribute malicious update

**MITIGATED BY:**
- **Signature Verification on Client:** Server cannot sign updates (doesn't have private key)
- **End-to-End Security:** Publisher signs, client verifies, server just transports
- **Fallback Full Download:** If suspicious activity, clients can download from alternative source

**Result:** Malicious update rejected by clients (signature mismatch)

**Scenario 4: Publisher Key Compromise**

**Attack:** Attacker steals publisher's private signing key

**NOT FULLY MITIGATED:** Attacker can sign malicious updates

**Partial Mitigations:**
- **Key Rotation:** Regular key changes limit time window
- **Monitoring:** Anomaly detection on publisher behavior
- **Gradual Rollout:** Limits blast radius before detection
- **Emergency Revocation:** Publisher can revoke compromised key

**Result:** Requires manual intervention, potential user impact

**Scenario 5: Timing Attacks**

**Attack:** Observe timing of update checks to infer user behavior

**MITIGATED BY:**
- **Random Jitter:** Update checks at randomized intervals
- **Anonymized Device IDs:** Hash of hardware info, not personally identifiable
- **Batched Requests:** Server aggregates metrics (no individual tracking)

**Result:** Limited privacy impact

---

## Cryptographic Security

### Algorithms Used

**Digital Signatures:**

**Primary: Ed25519**
- **Algorithm:** Edwards-curve Digital Signature Algorithm
- **Key Size:** 256-bit (32 bytes)
- **Signature Size:** 512-bit (64 bytes)
- **Security Level:** 128-bit (equivalent to RSA-3072)
- **Performance:** Very fast (signing: ~15 μs, verification: ~30 μs)
- **Resistance:** Quantum-resistant? NO (post-quantum alternatives in Phase 3)

**Why Ed25519:**
- Fast verification (important for resource-constrained devices)
- Small signatures (minimal overhead)
- No timing attack vulnerabilities
- Widely supported (libsodium, OpenSSL 1.1+)

**Fallback: ECDSA P-256**
- **Algorithm:** Elliptic Curve Digital Signature Algorithm
- **Curve:** NIST P-256 (secp256r1)
- **Security Level:** 128-bit
- **Use Case:** Legacy systems without Ed25519 support

**Hash Functions:**

**Primary: Blake3**
- **Output Size:** 256-bit (32 bytes)
- **Performance:** Extremely fast (>2 GB/s on modern CPU)
- **Security:** Collision-resistant, pre-image resistant
- **Use Case:** Content hashing (diffs, binaries)

**Secondary: SHA-256**
- **Output Size:** 256-bit
- **Security:** NIST-approved, widely trusted
- **Use Case:** Backward compatibility, compliance requirements

**Symmetric Encryption (Optional):**

**For encrypted diffs (future feature):**
- **Algorithm:** ChaCha20-Poly1305
- **Key Size:** 256-bit
- **Use Case:** Private updates (beta versions, enterprise)

### Key Management

**Publisher Private Key:**

**Generation:**
```
Generate Ed25519 keypair using cryptographically secure RNG
Private key: 32 bytes (256 bits)
Public key: 32 bytes (256 bits)
```

**Storage:**
- **Encrypted at rest:** AES-256-GCM with passphrase-derived key
- **Stored securely:** Hardware Security Module (HSM) recommended for production
- **Access control:** Limited to authorized build systems/personnel
- **Backup:** Encrypted backup in separate secure location

**Never:**
- Commit to version control
- Store in plaintext
- Share via email/messaging
- Reuse across applications

**Public Key:**

**Distribution:**
- Embedded in client application (compile-time constant)
- Stored in client patcher configuration
- Publicly available (no secrecy required)

**Verification:**
```
SHA-256 fingerprint of public key published on publisher website
Users can verify: sha256sum public.key
```

**Key Rotation:**

**Procedure:**
1. Generate new keypair
2. Sign updates with both old and new keys (transition period: 90 days)
3. Distribute new public key in next update
4. After transition, sign only with new key
5. Revoke old key

**Frequency:** Annually (or immediately if compromised)

### Signature Format

**Signed Data Structure:**

```json
{
  "version": "1.1.0",
  "app": "myapp",
  "platform": "linux",
  "architecture": "x86_64",
  "binary_hash": "blake3:a1b2c3d4...",
  "binary_size": 85400000,
  "diffs": [
    {
      "from_version": "1.0.0",
      "diff_hash": "blake3:e5f6g7h8...",
      "diff_size": 1200000
    }
  ],
  "timestamp": "2026-01-07T12:34:56Z",
  "publisher": "Example Inc."
}
```

**Signature Computation:**

```
canonical_json = canonicalize(signed_data)  # RFC 8785
signature = ed25519_sign(private_key, canonical_json)
```

**Signature File (.sig):**

```json
{
  "signed_data": { ... },  # As above
  "signature": "base64(signature_bytes)",
  "public_key_fingerprint": "sha256(public_key)",
  "algorithm": "ed25519"
}
```

**Verification:**

```
1. Load signature file
2. Load publisher public key
3. Verify public key fingerprint matches expected
4. Canonicalize signed_data
5. Verify signature: ed25519_verify(public_key, canonical_json, signature)
6. Check timestamp (must be recent, not in future)
7. Verify version monotonicity (new_version > current_version)
8. Verify hashes match downloaded content
```

---

## Transport Security

### TLS/HTTPS

**Requirements:**

**TLS Version:**
- Minimum: TLS 1.2
- Recommended: TLS 1.3

**Cipher Suites (TLS 1.3):**
- TLS_AES_256_GCM_SHA384
- TLS_CHACHA20_POLY1305_SHA256
- TLS_AES_128_GCM_SHA256

**Cipher Suites (TLS 1.2):**
- ECDHE-RSA-AES256-GCM-SHA384
- ECDHE-RSA-AES128-GCM-SHA256
- ECDHE-RSA-CHACHA20-POLY1305

**Certificate Requirements:**
- Issued by trusted CA (Let's Encrypt, DigiCert, etc.)
- Subject Alternative Name (SAN) includes all server domains
- Valid for <90 days (automated renewal)
- 2048-bit RSA or 256-bit ECDSA

**Certificate Pinning (Optional):**

**For high-security deployments:**
- Pin certificate public key hash in client
- Prevents MITM even with compromised CA

**Implementation:**
```
Expected certificate hash: sha256(server_cert_public_key)
On connection:
  actual_hash = sha256(received_cert_public_key)
  if actual_hash != expected_hash:
    REJECT connection (possible MITM attack)
```

**Trade-off:** Requires updating pins when certificate rotates (complex)

### Content Integrity

**HTTP Headers:**

**Content-Disposition:**
```
Content-Disposition: attachment; filename="myapp-1.0.0-to-1.1.0.diff"
```
Prevents browser from executing diff as script.

**Content-Type:**
```
Content-Type: application/octet-stream
```

**X-Content-Type-Options:**
```
X-Content-Type-Options: nosniff
```
Prevents MIME type sniffing attacks.

**Subresource Integrity (SRI):**

For web-based clients:
```html
<script src="https://cdn.example.com/deltaship-client.js"
  integrity="sha384-ABC123..."
  crossorigin="anonymous"></script>
```

### Rate Limiting

**Prevent Abuse:**

**API Rate Limits:**
- Per IP: 1000 requests/minute (check-update)
- Per API Key: 100 requests/minute (publish)
- Global: 100,000 requests/second (auto-scale if exceeded)

**Implementation:**
- Token bucket algorithm
- Redis for distributed rate limiting
- Graceful degradation (queue excess requests)

**DDoS Protection:**
- CDN-level (CloudFlare, AWS Shield)
- SYN flood protection (iptables, hardware firewall)
- Application-level (rate limiting, CAPTCHA for suspicious IPs)

---

## Access Control

### Authentication

**Client (End-User Device):**

**No authentication required for download:**
- Updates are public (anyone can download)
- Signature verification ensures integrity

**Optional: Device Registration:**
- Device ID (anonymized hash) for analytics
- No personally identifiable information

**Publisher:**

**API Key Authentication:**
- Each publisher receives unique API key
- Format: `deltaship_publisher_a1b2c3d4e5f6...` (64 char hex)
- HMAC-SHA256 signed requests

**Request Signature:**
```
timestamp = current_unix_timestamp()
payload = request_body
signature = hmac_sha256(api_secret, f"{timestamp}:{payload}")

Headers:
  X-Deltaship-API-Key: deltaship_publisher_a1b2c3d4...
  X-Deltaship-Timestamp: {timestamp}
  X-Deltaship-Signature: {signature}
```

**Server Verification:**
```
1. Check timestamp (reject if >5 minutes old, prevents replay)
2. Compute expected signature
3. Compare with provided signature (constant-time comparison)
4. Allow if match, reject otherwise
```

**Administrator:**

**Web Admin Panel (Optional):**
- Username/password with 2FA (TOTP)
- Or: SSO (SAML, OAuth) for enterprise

**CLI Administration:**
- SSH with key-based authentication
- sudo required for sensitive operations

### Authorization

**Role-Based Access Control (RBAC):**

**Roles:**

**1. Publisher:**
- Can: Publish versions, sign updates, view own app statistics
- Cannot: View other publishers' apps, modify server config, delete published versions (immutable)

**2. Administrator:**
- Can: Manage publishers, configure server, view all statistics, emergency rollback
- Cannot: Sign updates (don't have publisher private keys)

**3. Auditor (Read-Only):**
- Can: View all data, download logs, run reports
- Cannot: Modify anything

**Permissions Matrix:**

| Action | Publisher | Admin | Auditor |
|--------|-----------|-------|---------|
| Publish version | ✅ (own apps) | ❌ | ❌ |
| View statistics | ✅ (own apps) | ✅ (all) | ✅ (all) |
| Modify rollout | ✅ (own apps) | ✅ (all) | ❌ |
| Emergency rollback | ❌ | ✅ | ❌ |
| Server configuration | ❌ | ✅ | ❌ |
| View logs | ❌ | ✅ | ✅ |

---

## Privacy

### Data Collection

**What is Collected:**

**Update Events (Required for service):**
- Device ID: Anonymized hash (SHA-256 of hardware identifiers)
- App name and version
- Platform and architecture
- Update success/failure status
- Timestamp
- Error message (if failed)

**What is NOT Collected:**
- User identity (name, email, IP address logged but not linked to device ID)
- Geographic location (approximate region from IP, but not stored)
- Personal data
- Application usage data (only update data)

**Data Retention:**
- Update events: 90 days (configurable)
- Aggregated statistics: Indefinitely (anonymized)
- IP addresses in logs: 7 days

**GDPR Compliance:**

**User Rights:**
- **Right to Access:** Device ID available to user, can request associated data
- **Right to Erasure:** Can request deletion of device-specific data
- **Right to Portability:** Can export update history

**Implementation:**
```
GET /api/privacy/my-data?device_id={device_id_hash}
Response: JSON with all data associated with device

DELETE /api/privacy/my-data?device_id={device_id_hash}
Deletes all data for device (keeps aggregated stats)
```

**Consent:**
- Analytics opt-in (default: enabled, user can disable)
- Minimal data collection for core functionality

### Anonymization

**Device ID Generation:**

```
hardware_info = {
  "mac_address": get_primary_mac(),
  "disk_serial": get_disk_serial(),
  "motherboard_uuid": get_motherboard_uuid()
}

device_id_raw = f"{hardware_info}:{random_salt}"
device_id_hash = sha256(device_id_raw)  # 64 hex characters

Store only: device_id_hash (cannot reverse to identity)
```

**IP Address Handling:**
- Logged for rate limiting and abuse prevention
- Not linked to device ID in database
- Deleted after 7 days

**Differential Privacy (Future):**
- Add noise to aggregate statistics
- Prevents inferring individual behavior from aggregates

---

## Audit and Compliance

### Audit Logging

**What is Logged:**

**Security-Relevant Events:**
- Version published (publisher, app, version, timestamp)
- Signature verification (device ID, result, timestamp)
- API authentication (IP, key used, result)
- Access to admin panel (user, action, timestamp)
- Configuration changes (admin, setting changed, old/new value)
- Rollbacks (admin, version, reason)

**Log Format:**

```json
{
  "timestamp": "2026-01-07T12:34:56Z",
  "event_type": "version_published",
  "actor": "publisher:acme",
  "action": "publish",
  "resource": "app:myapp version:1.1.0",
  "result": "success",
  "metadata": {
    "binary_size": 85400000,
    "diff_count": 3
  }
}
```

**Storage:**
- Write-only append log (immutable)
- Separate from operational database
- Retained for 1 year minimum (compliance requirement)

**Tamper Protection:**
- Logs signed with HMAC (server secret)
- Or: Write to immutable storage (S3 with object lock)

### Compliance

**Standards:**

**NIST Cybersecurity Framework:**
- Identify: Threat model documented
- Protect: Encryption, signatures, access control
- Detect: Monitoring, alerting, anomaly detection
- Respond: Incident response procedures
- Recover: Backup and disaster recovery

**OWASP Top 10 Mitigation:**

1. **Injection:** Parameterized queries, input validation
2. **Broken Authentication:** Strong API keys, rate limiting
3. **Sensitive Data Exposure:** TLS, encryption at rest, minimal collection
4. **XML External Entities (XXE):** N/A (no XML processing)
5. **Broken Access Control:** RBAC, least privilege
6. **Security Misconfiguration:** Secure defaults, hardening guides
7. **Cross-Site Scripting (XSS):** N/A (no web UI in core server)
8. **Insecure Deserialization:** Safe serialization (JSON), validation
9. **Using Components with Known Vulnerabilities:** Dependency scanning, updates
10. **Insufficient Logging & Monitoring:** Comprehensive audit logging

**ISO 27001 Alignment:**

**Information Security Controls:**
- Access control (A.9): RBAC, authentication
- Cryptography (A.10): Ed25519, TLS
- Operations security (A.12): Monitoring, backups
- Network security (A.13): Firewalls, TLS
- Incident management (A.16): Procedures documented

---

## Security Testing

### Vulnerability Assessment

**Regular Scanning:**

**Automated:**
- Dependency scanning (e.g., `cargo audit` for Rust)
- Container image scanning (e.g., Trivy, Clair)
- Static analysis (e.g., Clippy, Semgrep)
- Weekly schedule

**Manual:**
- Code review (all critical changes)
- Penetration testing (annually, external firm)
- Threat model review (quarterly)

**Bug Bounty (Recommended for production):**
- Reward security researchers for responsibly disclosed vulnerabilities
- Scope: Server, client, protocol
- Rewards: $100 - $10,000 depending on severity

### Penetration Testing

**Recommended Tests:**

**Network Security:**
- TLS configuration (downgrade attacks, weak ciphers)
- Man-in-the-middle (certificate validation)

**Application Security:**
- API fuzzing (malformed requests)
- Injection attacks (SQL, command injection)
- Authentication bypass
- Authorization bypass (access other publishers' data)

**Cryptographic Security:**
- Signature verification bypass attempts
- Hash collision attacks
- Timing attacks on signature verification

**Client Security:**
- Patch application vulnerabilities
- Rollback prevention bypass
- Local privilege escalation

**Report:**
- Findings with severity ratings (Critical, High, Medium, Low)
- Remediation recommendations
- Verification testing

---

## Incident Response

### Security Incident Types

**1. Compromised Publisher Key**

**Indicators:**
- Unexpected version published
- User reports malicious behavior

**Response:**
1. **Immediate:** Pause all updates for affected app
2. **Notify:** Alert users via all channels
3. **Revoke:** Mark compromised key as revoked in server
4. **Investigate:** Determine how key was compromised
5. **Rollback:** Roll back users to last known-good version
6. **Re-key:** Generate new keypair, distribute new public key
7. **Post-Mortem:** Document lessons learned

**2. Server Compromise**

**Indicators:**
- Unauthorized access in logs
- Unexpected configuration changes
- Malware detected

**Response:**
1. **Isolate:** Disconnect server from network
2. **Investigate:** Forensic analysis
3. **Restore:** Rebuild from clean backup
4. **Rotate:** All credentials (DB passwords, API keys)
5. **Audit:** Review all data for tampering
6. **Monitor:** Enhanced monitoring for repeat attack

**3. Vulnerability Disclosure**

**Public Vulnerability Report:**

**Procedure:**
1. **Acknowledge:** Within 24 hours
2. **Assess:** Severity and impact
3. **Develop:** Patch or mitigation
4. **Test:** Verify fix
5. **Deploy:** Roll out fix (coordinated disclosure)
6. **Disclose:** Public announcement after fix deployed

**Timeline:**
- Critical: 7 days
- High: 30 days
- Medium: 60 days
- Low: 90 days

---

## Best Practices

### For Publishers

**Key Management:**
- Use HSM for production signing keys
- Never store private key unencrypted
- Rotate keys annually
- Test key recovery procedure

**Signing:**
- Sign on isolated, air-gapped machine (highest security)
- Or: Sign in CI/CD with short-lived credentials
- Verify signature after signing

**Version Control:**
- Never commit private keys
- Use `.gitignore` for key directories
- Review all commits before publishing

### For Server Operators

**Hardening:**
- Minimal installed packages (reduce attack surface)
- Regular security updates
- Firewall (allow only necessary ports)
- SELinux/AppArmor enabled

**Monitoring:**
- Alert on security events (failed auth, unusual access patterns)
- Automated incident response (block abusive IPs)
- Regular security audits

**Backup:**
- Encrypted backups
- Stored in separate location (3-2-1 rule)
- Test restore regularly

### For Client Patcher

**Sandboxing:**
- Run with minimal privileges (dedicated user account)
- Restrict file system access (only update target and temp dir)
- Limit network access (only update server)

**Verification:**
- Always verify signatures before applying
- Use constant-time comparison (prevent timing attacks)
- Fail closed (reject on any doubt)

**Updates:**
- Keep client patcher updated (self-update capability)
- Security patches prioritized

---

## Future Security Enhancements

**Phase 2 (Month 7-12):**
- Certificate Transparency monitoring
- Automated key rotation
- Enhanced anomaly detection (ML-based)

**Phase 3 (Month 13-18):**
- Post-quantum cryptography (SPHINCS+, Kyber)
- Blockchain audit trail (immutable, public log)
- Hardware security module (HSM) integration for server

**Phase 4 (Month 19+):**
- Homomorphic encryption (private updates)
- Zero-knowledge proofs (verify without revealing)
- Secure multi-party computation (distributed signing)

---

## Conclusion

Deltaship's security model provides strong protection against common threats through:
- **End-to-end cryptographic verification:** Publisher signs, client verifies
- **Defense in depth:** Multiple security layers
- **Transparency:** Open protocol, auditable
- **Best practices:** Following industry standards (NIST, OWASP, ISO 27001)

**Security is never complete:** Continuous monitoring, testing, and improvement are essential.

---

## References

- Ed25519: https://ed25519.cr.yp.to/
- Blake3: https://github.com/BLAKE3-team/BLAKE3-specs
- TLS 1.3: RFC 8446
- NIST Cryptographic Standards: https://csrc.nist.gov/
- OWASP Top 10: https://owasp.org/www-project-top-ten/

---

**End of Security Model Document**
