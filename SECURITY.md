# Security Policy

> **NOTE:** Email addresses on this page use the RFC 2606 `example.com` domain and are placeholders. Replace with real contacts before publishing.

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.x.x   | :white_check_mark: |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

<!-- TODO: Before public release, replace with actual security contact email -->
Instead, report vulnerabilities via email to: security@vbdp.example.com

Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### PGP Key

<!-- TODO: Before public release, add actual PGP public key for encrypted security reports -->
For encrypted reports, you can use our PGP key (to be published upon official release).

Instructions for adding your PGP key:
1. Generate a dedicated security key: `gpg --full-generate-key`
2. Export the public key: `gpg --armor --export security@vbdp.example.com`
3. Replace this section with the exported public key
4. Publish the key to public keyservers
5. Include the key ID and fingerprint

## Response Timeline

- **Acknowledgment**: Within 48 hours
- **Initial assessment**: Within 7 days
- **Resolution target**: Within 90 days

## Disclosure Policy

We follow coordinated disclosure. We will:
1. Confirm receipt of your report
2. Investigate and validate the issue
3. Develop and test a fix
4. Release a patch and advisory
5. Credit you (unless you prefer anonymity)

## Security Model

VBDP implements a defense-in-depth security architecture with multiple cryptographic layers.

### Ed25519 Signatures

All updates are digitally signed using Ed25519, a modern elliptic curve signature scheme:
- **Key Size:** 256-bit (32 bytes)
- **Signature Size:** 512-bit (64 bytes)
- **Security Level:** 128-bit (equivalent to RSA-3072)
- **Performance:** Very fast (~15 microseconds signing, ~30 microseconds verification)

Publishers sign all version metadata and diffs. Clients verify signatures before applying any update, ensuring authenticity even if the update server is compromised.

### Blake3 Hashing

VBDP uses Blake3 for all content integrity verification:
- **Output Size:** 256-bit (32 bytes)
- **Performance:** Extremely fast (>2 GB/s on modern CPUs)
- **Security:** Collision-resistant and pre-image resistant

Every binary, diff, and metadata file includes Blake3 checksums. Clients verify hashes after download and after patch application.

### ChaCha20-Poly1305 Encryption

For private or enterprise updates, VBDP supports optional encryption:
- **Algorithm:** ChaCha20-Poly1305 (AEAD cipher)
- **Key Size:** 256-bit
- **Use Cases:** Beta versions, enterprise deployments, sensitive updates

Encryption is end-to-end: publishers encrypt, clients decrypt. The update server never sees plaintext content.

### Argon2id Key Derivation

When password-based encryption is used (e.g., for private key storage):
- **Algorithm:** Argon2id (winner of Password Hashing Competition)
- **Memory Cost:** Configurable (default 64MB)
- **Time Cost:** Configurable (default 3 iterations)
- **Resistance:** Side-channel resistant, GPU/ASIC resistant

Private signing keys are stored encrypted with Argon2id-derived keys.

## Threat Model and Trust Assumptions

### Trust Boundaries

VBDP's security model explicitly defines trust boundaries to help users understand security guarantees:

#### Trusted Components (Must Not Be Compromised)

1. **Publisher's Signing Environment**
   - Private signing keys must remain secret
   - Build/signing process must be secure
   - Compromise = attacker can sign malicious updates
   - Mitigation: HSM storage, key rotation, isolated build environments

2. **Client Installation Integrity**
   - Initial VBDP client binary must be authentic
   - Compromise = attacker controls update mechanism
   - Mitigation: Verify installation checksum, install from official sources only

3. **Publisher's Database (for metadata integrity)**
   - Stores version history, checksums, signatures
   - Compromise = attacker can modify stored hashes/signatures
   - Mitigation: Regular backups, integrity monitoring, file locking

#### Untrusted Components (Assumed Hostile)

1. **Network Infrastructure**
   - ALL network traffic assumed intercepted/modified
   - Protection: End-to-end cryptographic verification (signatures + checksums)
   - TLS provides defense-in-depth but is NOT relied upon

2. **Update Server**
   - Server may be fully compromised by attacker
   - Protection: Server only transports files, never signs them
   - Private keys NEVER stored on update server

3. **CDN and Storage Backends**
   - S3, CloudFront, or other storage may be compromised
   - Protection: Client verifies signatures regardless of source
   - Object versioning for tamper detection

4. **Client's Network Environment**
   - DNS may be poisoned
   - Proxies may modify traffic
   - Protection: Signature verification works regardless of server identity

### Mitigated Threats

#### 1. Man-in-the-Middle (MitM) Attacks
- **Attack:** Attacker intercepts traffic and serves malicious updates
- **Defense:** Client verifies Ed25519 signatures before applying updates
- **Result:** MitM cannot forge signatures without private key

#### 2. Compromised Update Server
- **Attack:** Attacker gains full control of update server
- **Defense:** Server doesn't hold signing keys, can only serve existing signed files
- **Result:** Attacker cannot create new malicious updates

#### 3. Corrupted Downloads
- **Attack:** Network errors or storage corruption damages update files
- **Defense:** Blake3 checksum verification before and after patch application
- **Result:** Corrupted files rejected, rollback preserves previous version

#### 4. Downgrade Attacks
- **Attack:** Attacker serves old vulnerable version with valid signature
- **Defense:** Version monotonicity check (reject if new_version < current_version)
- **Result:** Cannot downgrade to vulnerable versions
- **Note:** While automatic downgrades are prevented, users can manually install older versions. Version revocation mechanism for compromised releases is planned for a future enhancement.

#### 5. Partial Update Failures
- **Attack:** Update fails mid-application, leaving system in broken state
- **Defense:** Atomic file operations, automatic rollback on failure
- **Result:** System always in working state (old or new version, never broken)

#### 6. Path Traversal Attacks
- **Attack:** Malicious binary name like "../../usr/bin/sudo" to overwrite system files
- **Defense:** Path validation, symlink detection, canonicalization checks
- **Result:** Cannot write outside intended directories

#### 7. Race Condition Exploits (TOCTOU) - MITIGATED
- **Attack:** Replace install path's parent directory with a symlink between validation and write
- **Mitigations Applied:**
  - Double validation (before update start and immediately before write)
  - Parent directory identity check (dev/inode verified at temp-file creation and again before rename, detecting directory replacement)
  - Temp file created inside the destination directory (same filesystem, so a replaced directory causes the rename to fail)
  - Atomic file operations (rename-based installation; rename does not dereference the final destination component)
  - Post-write checksum verification
  - Restrictive temp file permissions (0600 during write, 0755 before rename)
  - Security logging of path validation and write operations
- **Recommended User Protections:**
  - Run client as non-root user (never as root)
  - Use AppArmor/SELinux to restrict file writes
  - Install binaries in user-owned directories (~/.local/bin)
- **See:** Code comments in `crates/vbdp-client/src/patcher.rs` for detailed security discussion

#### 8. Denial of Service (Resource Exhaustion)
- **Attack:** Flood server with requests or send huge diffs
- **Defense:** Rate limiting, size limits, gradual rollout
- **Result:** Limited impact, legitimate users still served

### Partially Mitigated Threats

#### 1. Compromised Publisher Private Key
- **Risk:** If attacker obtains signing key, can sign malicious updates
- **Mitigations:**
  - Gradual rollout (limits blast radius)
  - Monitoring for unexpected updates
  - Key rotation (limits time window)
  - HSM storage (hardens key security)
- **Remaining Risk:** Short-term compromise possible before detection
- **Recovery:** Revoke compromised key, rotate to new key, issue corrective update

#### 2. Supply Chain Attacks on Build Process
- **Risk:** Attacker injects malicious code during build
- **Mitigations:**
  - Reproducible builds (verify consistency)
  - Isolated build environments
  - Dependency pinning and verification
- **Remaining Risk:** Sophisticated attacks may still succeed
- **Recovery:** Monitor for unexpected behavior, binary analysis

### Out of Scope (Version 1.0)

These threats are acknowledged but not addressed in v1.0:

1. **Side-Channel Attacks on Client**
   - Timing attacks, power analysis, etc.
   - Rationale: Requires local access, mitigated by OS-level protections

2. **Physical Device Compromise**
   - Attacker with physical access modifies files directly
   - Rationale: Physical security is user's responsibility

3. **Post-Quantum Cryptography**
   - Quantum computer breaks Ed25519
   - Rationale: Quantum computers not yet viable threat
   - Future: Planned for v2.0 (hybrid classical + post-quantum)

4. **Compromised OS or Runtime**
   - Malware running on client system
   - Rationale: Cannot protect against root-level compromise

5. **Social Engineering**
   - Tricking publisher into signing malicious code
   - Rationale: Human factors outside technical scope

### Critical Security Design Decisions

1. **Signature Verification is Mandatory**
   - No configuration option to disable
   - Prevents accidental or malicious bypass
   - Compiled in, cannot be removed at runtime

2. **End-to-End Security Model**
   - Publisher signs, client verifies
   - Server is just a transport layer
   - No trusted intermediaries

3. **Fail-Safe Defaults**
   - Missing signature = reject update
   - Checksum mismatch = abort and rollback
   - Any error = preserve previous working version

4. **Defense in Depth**
   - Multiple layers: TLS + signatures + checksums
   - Each layer provides independent security
   - Compromise of one layer doesn't break security

For complete technical security documentation, see [docs/SECURITY.md](docs/SECURITY.md).
