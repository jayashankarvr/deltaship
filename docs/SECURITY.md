# Security Policy

> **NOTE:** Email addresses on this page use the RFC 2606 `example.com` domain and are placeholders. Replace with real contacts before publishing.

## Supported Versions

Currently, VBDP is in the design and documentation phase. Once implementation begins, this section will list which versions receive security updates.

| Version | Status | Security Support |
|---------|--------|------------------|
| 1.0.x   | Future | Not yet released |
| 0.x.x   | Future | Development versions |

**Note:** Security support policy will be established before v1.0 release. Expected policy:
- Latest major version: Full security support
- Previous major version: Security fixes for 1 year
- Older versions: No support (upgrade recommended)

---

## Reporting a Vulnerability

**⚠️ DO NOT report security vulnerabilities through public GitHub issues.**

Security vulnerabilities should be reported privately to allow time for fixes before public disclosure.

### How to Report

<!-- TODO: Before public release, update to actual security contact email -->
**Email:** security@vbdp.example.com (PGP key available below)

**Include:**
1. **Description:** Detailed description of the vulnerability
2. **Impact:** What an attacker could do with this vulnerability
3. **Steps to Reproduce:** Detailed steps or proof-of-concept code
4. **Affected Versions:** Which versions are affected (if known)
5. **Your Contact Info:** How we can reach you for follow-up

**Optional:**
- Suggested fix or mitigation
- CVSS score (if you've calculated one)
- Whether you plan to publish independently

### Example Report Template

```
Subject: [SECURITY] Signature Verification Bypass in Client Patcher

Description:
The client patcher's signature verification can be bypassed by...

Impact:
An attacker could distribute malicious updates that would be accepted
by clients, leading to arbitrary code execution.

Steps to Reproduce:
1. Create a malicious binary
2. Generate invalid signature with...
3. Client accepts the update without verification

Affected Versions:
Likely affects all versions (theoretical, as implementation not started)

Contact:
researcher@example.com
```

### What to Expect

**Within 24 hours:**
- Acknowledgment of your report
- Initial assessment of severity

**Within 7 days:**
- Detailed response about the vulnerability
- Timeline for fix (if valid)
- Credit options (see below)

**Typical Timeline:**
- **Critical:** Fix within 7 days
- **High:** Fix within 30 days
- **Medium:** Fix within 60 days
- **Low:** Fix within 90 days

### Coordinated Disclosure

We follow **coordinated disclosure** (also known as responsible disclosure):

1. **You report** the vulnerability privately
2. **We investigate** and develop a fix
3. **We release** a patched version
4. **We publish** a security advisory
5. **You may publish** your findings after the advisory

**Embargo Period:** We request a 90-day embargo before public disclosure to give users time to update.

**Early Disclosure:** If you need to publish earlier, please discuss with us. We understand research timelines and conference deadlines.

---

## Security Advisories

Security advisories will be published at:
- **GitHub Security Advisories:** https://github.com/jayashankarvr/vbdp/security/advisories <!-- TODO: Update URL when repository is published -->
<!-- TODO: Before public release, update to actual mailing list address -->
- **Mailing List:** vbdp-security-announce@vbdp.example.com (subscribe to receive notifications)
- **Website:** https://vbdp.io/security

### Advisory Format

Each advisory will include:
- **CVE ID:** If applicable
- **Severity:** Critical, High, Medium, Low
- **Affected Versions:** Which versions are vulnerable
- **Fixed Versions:** Which versions contain the fix
- **Description:** What the vulnerability is
- **Impact:** What an attacker could do
- **Mitigation:** Temporary workarounds (if available)
- **Credit:** Attribution to reporter (if desired)

---

## Bug Bounty Program

**Status:** Not yet available (planned for post-v1.0)

When the bug bounty program launches:
- **Scope:** VBDP Server, Client Patcher, Publisher Toolkit, Official SDKs
- **Rewards:** TBD (based on severity)
- **Platform:** TBD (HackerOne, Bugcrowd, or self-hosted)

**Out of Scope:**
- Social engineering
- Physical attacks
- Denial of Service (DoS/DDoS)
- Spam
- Brute force attacks

---

## Security Best Practices

### For Publishers

**Signing Keys:**
- Generate strong Ed25519 keys (use `vbdp-init` or `ssh-keygen -t ed25519`)
- Store private keys encrypted (use passphrase)
- Never commit private keys to version control
- Rotate keys annually
- Use Hardware Security Module (HSM) for production (recommended)

**Key Storage:**
```bash
# Good: Encrypted with GPG
gpg --symmetric --cipher-algo AES256 private.key

# Bad: Plaintext in repository
git add private.key  # NEVER DO THIS
```

**Build Security:**
- Use isolated build environments
- Verify build reproducibility
- Sign immediately after building
- Test signature verification before publishing

### For Server Operators

**Infrastructure:**
- Keep software up-to-date (OS, dependencies, VBDP)
- Use strong TLS configuration (TLS 1.2+ only)
- Enable firewall (allow only ports 80, 443)
- Use security groups (cloud) or iptables (self-hosted)

**Database:**
- Use strong passwords (32+ characters, random)
- Enable SSL/TLS for database connections
- Restrict database access (not publicly accessible)
- Regular backups (test restore procedures)

**Monitoring:**
- Monitor for unusual API patterns
- Alert on failed signature verifications
- Track unauthorized access attempts
- Review logs regularly

**Example Alert:**
```yaml
# Alert on multiple signature verification failures
- alert: SuspiciousSignatureFailures
  expr: rate(vbdp_signature_verification_failures[5m]) > 10
  annotations:
    summary: "High rate of signature verification failures"
    description: "Possible attack or misconfiguration"
```

### For Client Users

**Installation:**
- Install from official sources only
- Verify download checksums (if provided)
- Use package manager when possible (automatic updates)

**Configuration:**
- Don't disable signature verification (even if update fails)
- Use HTTPS update server URLs only
- Trust official public keys only

**Monitoring:**
- Check logs for suspicious activity (`/var/log/vbdp/`)
- Verify updates completed successfully
- Report repeated update failures

---

## Known Security Considerations

### Design Phase Notes

These security considerations are documented during the design phase and will be addressed in implementation:

#### 0. TOCTOU Race Condition in Path Validation (Partially Mitigated)

**Status:** Partially mitigated with documented residual risk

**Issue:** Time-of-Check-Time-of-Use (TOCTOU) race condition exists in install path validation. Between validating that a path is not a symlink and actually writing to it, a local attacker could create a symlink to redirect the write.

**Mitigations Implemented:**

1. **Double validation**: Path is validated twice - once at update start and once immediately before write (microsecond window only)
2. **Atomic operations**: Final write uses atomic rename() operation
3. **Post-write verification**: Checksum of installed file is verified immediately after write
4. **Restrictive permissions**: Temporary files created with 0600 permissions
5. **Security logging**: All path validations and writes are logged for monitoring

**Residual Risk:**

A sophisticated local attacker with:
- Write access to the install directory or parent directories
- Ability to execute code during the microsecond window between final validation and write
- Precise timing capabilities

Could potentially redirect writes to unintended locations (e.g., replacing a symlink to /usr/bin/sudo).

**Impact Assessment:**

- **Likelihood:** Very Low - requires local access, write permissions, and microsecond-precision timing
- **Severity:** High if successful - could overwrite critical system files
- **Overall Risk:** Medium - mitigations significantly reduce attack surface

**User Recommendations:**

1. **Never run VBDP client as root** - run as unprivileged user
2. **Use mandatory access control** - AppArmor/SELinux profiles to restrict writes
3. **Install in user directories** - Use ~/.local/bin instead of /usr/bin
4. **Monitor file systems** - Alert on unexpected symlink creation
5. **Use immutable flags** - Protect critical directories with chattr +i (Linux)

**Why Not Completely Fixed:**

A complete fix requires platform-specific file descriptor-based validation and writes:
- Linux: open() with O_NOFOLLOW, validate fd, write to fd
- macOS: Similar but different flags
- Windows: Different APIs entirely

This is complex and error-prone. We chose to:
1. Be honest about the limitation
2. Implement multiple defense layers
3. Provide clear user guidance
4. Plan full fix for future version with platform-specific code

**Code References:**
- `crates/vbdp-client/src/patcher.rs` - validate_install_path() function documentation
- `crates/vbdp-client/src/commands/add.rs` - Binary registration validation

#### 1. Signature Verification Timing Attacks

**Risk:** Timing differences in signature verification could leak information

**Mitigation:** Use constant-time comparison functions
- Recommended library: libsodium (provides constant-time operations)
- Avoid: OpenSSL (some versions have timing vulnerabilities in Ed25519)

#### 2. Rollback Attacks

**Risk:** Attacker serves old signed version with known vulnerabilities

**Mitigation:**
- Version monotonicity check (reject if new_version < current_version)
- Timestamp in signature (reject if timestamp too old)
- Authorized rollback only (signed by publisher)

#### 3. Compromised Update Server

**Risk:** Attacker gains control of update server

**Impact:** Limited (cannot sign malicious updates without private key)

**Defense in Depth:**
- End-to-end security (publisher signs, client verifies)
- Server only transports, doesn't generate signatures
- Separate storage for private keys (never on server)

#### 4. Storage Provider Compromise

**Risk:** Attacker modifies files in S3/object storage

**Mitigation:**
- Client verifies signatures (even if storage compromised)
- Object versioning enabled (can recover from tampering)
- Monitor for unauthorized modifications

#### 5. Denial of Service

**Risk:** Attacker floods server with requests

**Mitigation:**
- Rate limiting (per IP, per API key)
- CDN (absorbs traffic, DDoS protection)
- Auto-scaling (handle legitimate spikes)

---

## Vulnerability Severity Classification

We use CVSS v3.1 for severity scoring, mapped to our categories:

### Critical (CVSS 9.0-10.0)
- Remote code execution without authentication
- Complete system compromise
- Cryptographic bypass allowing arbitrary updates

**Response:** Emergency patch within 7 days

### High (CVSS 7.0-8.9)
- Remote code execution with authentication
- Privilege escalation
- Information disclosure (sensitive data)

**Response:** Patch within 30 days

### Medium (CVSS 4.0-6.9)
- Denial of Service
- Authentication bypass (limited scope)
- Information disclosure (non-sensitive)

**Response:** Patch within 60 days

### Low (CVSS 0.1-3.9)
- Security feature bypass (low impact)
- Information leak (minimal sensitivity)

**Response:** Patch within 90 days or next release

---

## Security Update Process

When a security fix is released:

1. **Security Advisory Published:**
   - GitHub Security Advisory created
   - Mailing list notification sent
   - CVE assigned (if applicable)

2. **Patched Version Released:**
   - New version with fix published
   - Changelog updated with security note
   - Release notes highlight critical nature

3. **Notification:**
   <!-- TODO: Before public release, update to actual mailing list address -->
   - Email to vbdp-security-announce@vbdp.example.com
   - GitHub notification
   - Social media announcement (for critical issues)

4. **Documentation:**
   - Security advisory page updated
   - FAQ updated with upgrade instructions
   - Blog post (for critical/high severity)

---

## Researcher Recognition

We value security researchers and offer:

### Hall of Fame

Security researchers who report valid vulnerabilities will be listed in our Security Hall of Fame (with your permission):
- https://vbdp.io/security/hall-of-fame

### Credit

In security advisories, we will credit you as:
- Your name (or handle/alias if preferred)
- Your organization (optional)
- Link to your website/Twitter (optional)

### CVE Assignment

For qualifying vulnerabilities, we will:
- Request CVE ID from MITRE
- List you as the discoverer
- Reference your research (if public)

### Bug Bounty (Future)

When our bug bounty program launches, you may be eligible for monetary rewards based on:
- Severity (Critical, High, Medium, Low)
- Quality of report (clear, detailed, with PoC)
- Responsible disclosure (coordination, no early publication)

---

## Security Contact

<!-- TODO: Before public release, update to actual security contact email -->
**Primary Contact:** security@vbdp.example.com

**PGP Key:** (Will be provided when implementation starts)
```
Fingerprint: [TO BE ADDED]
Key ID: [TO BE ADDED]
Download: https://vbdp.io/security/pgp-key.asc
```

**Response Time:**
- Acknowledgment: Within 24 hours
- Initial assessment: Within 7 days
- Regular updates: Every 7-14 days

**Emergency Contact:** For critical issues requiring immediate attention:
<!-- TODO: Before public release, update to actual emergency security contact email -->
- Email: security-urgent@vbdp.example.com
- Subject line: [URGENT SECURITY]

---

## Legal

### Safe Harbor

We support security research conducted in good faith:

**We will not pursue legal action** against you for:
- Good faith security research
- Accidental violations during research
- Coordinated disclosure following this policy

**To qualify for safe harbor:**
- Report vulnerabilities privately
- Don't access data beyond minimum needed
- Don't exploit vulnerabilities for personal gain
- Don't harm VBDP or its users
- Follow coordinated disclosure timeline

**Out of scope:**
- Social engineering of staff
- Physical attacks on infrastructure
- Attacks on third parties

### Compliance

VBDP security practices are designed to comply with:
- **GDPR:** Data protection and privacy
- **CCPA:** California Consumer Privacy Act
- **SOC 2:** Security controls (future certification)
- **ISO 27001:** Information security management (future certification)

---

## Updates to This Policy

This security policy may be updated. Changes will be:
- Committed to Git (history preserved)
- Announced via mailing list (for significant changes)
- Effective immediately upon publication

**Last Updated:** 2026-01-07

**Version:** 1.0

---

## Questions?

If you have questions about this security policy:
<!-- TODO: Before public release, update to actual security contact email -->
- Email: security@vbdp.example.com
- Open a GitHub Discussion (for non-sensitive questions)
- See our [FAQ](https://vbdp.io/security/faq) (when available)

---

## Additional Resources

- [Security Model Documentation](security/SECURITY_MODEL.md) - Technical security architecture
- [Threat Model](security/SECURITY_MODEL.md#threat-model) - Detailed threat analysis
- [OWASP Top 10](https://owasp.org/www-project-top-ten/) - Common vulnerabilities reference
- [CWE Top 25](https://cwe.mitre.org/top25/) - Software weakness patterns

---

**Thank you for helping keep VBDP secure!** 🔒
