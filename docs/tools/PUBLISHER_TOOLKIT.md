# Publisher Toolkit

**Component:** Tools for software distributors to create, sign, test, and publish binary diffs
**Audience:** Software publishers, release engineers, CI/CD administrators
**Last Updated:** 2026-01-07

---

## Overview

The Publisher Toolkit is a suite of command-line tools that enable software distributors to:
- Register new software versions
- Generate binary diffs between versions
- Sign diffs and binaries cryptographically
- Test diffs before deployment
- Publish updates to distribution servers
- Monitor update adoption and rollback if needed

---

## Design Principles

### Single Responsibility
Each tool performs one well-defined task:
- Registration: Track versions
- Diffing: Compute binary differences
- Signing: Apply cryptographic signatures
- Testing: Validate before deployment
- Publishing: Upload to servers

### Automation-First
- Designed for CI/CD integration
- Minimal manual intervention required
- Comprehensive error reporting
- Scriptable operations

### Security-Focused
- Private keys never transmitted
- Signatures cover all critical data
- Test-before-publish mandatory option
- Audit trail for all operations

### Performance-Optimized
- Parallel diff computation
- Incremental processing
- Efficient storage (deduplication)
- Low resource usage

---

## Core Components

### 1. Version Registry

**Purpose:** Central database tracking all software versions and their relationships

**Features:**
- **Version tracking:** Store metadata for each version
- **Binary storage:** Keep binaries for diff computation
- **Relationship mapping:** Track which diffs exist between versions
- **Signature storage:** Maintain cryptographic signatures
- **Metadata management:** Release notes, timestamps, build info

**Storage Format:**
- **Database:** SQLite for metadata (portable, no server needed)
- **File system:** Binaries stored in organized directory structure
- **Format independence:** Works with any binary format (ELF, PE, Mach-O, etc.)

**Data Retention:**
- **Recent versions:** Last 10 releases always kept
- **LTS versions:** Long-term support versions retained indefinitely
- **Old versions:** Automatic cleanup after 6 months (configurable)
- **Diffs:** Cached based on usage patterns

### 2. Diff Generator

**Purpose:** Compute binary differences between software versions

**Algorithm Selection:**
- **Automatic detection:** Analyzes file type and selects optimal algorithm
- **bsdiff:** General-purpose binary diffing (default)
- **Courgette:** Executable-aware diffing (for compiled binaries)
- **xdelta3:** Alternative algorithm (faster, larger diffs)
- **Custom:** Pluggable architecture for custom algorithms

**Diff Strategy:**
- **Direct diffs:** Most common version transitions (e.g., N-1 to N)
- **LTS diffs:** Long-term support version to latest
- **Multi-hop:** Chains of diffs for rare transitions
- **Full binary:** Fallback when diff exceeds threshold

**Optimization:**
- **Parallel processing:** Multiple diffs computed concurrently
- **Incremental mode:** Only compute new diffs, reuse existing
- **Size threshold:** Don't store diffs larger than 50% of file (configurable)
- **Compression:** Apply zstd compression to diffs

**Quality Metrics:**
- **Diff size:** Target < 5% of file size for minor updates
- **Compression ratio:** Target 20-30% additional reduction with compression
- **Computation time:** Target < 5 seconds per 100MB binary
- **Accuracy:** 100% - verified by checksum after patching

### 3. Signature Manager

**Purpose:** Cryptographically sign diffs and binaries

**Signature Algorithm:**
- **Primary:** Ed25519 (fast, small signatures, secure)
- **Alternative:** ECDSA P-256 (broader compatibility)
- **Future:** Post-quantum algorithms (Dilithium, Falcon)

**Signature Coverage:**
- **Diff content:** Hash of diff file
- **Target binary:** Hash of resulting binary after patch
- **Version metadata:** Source version, target version, timestamp
- **Algorithm info:** Diff algorithm used
- **Publisher identity:** Embedded in key pair

**Key Management:**
- **Private key protection:**
  - Password-protected on disk (recommended)
  - HSM integration (optional, for enterprises)
  - Never transmitted over network
  - Separate keys per product (isolation)

- **Public key distribution:**
  - Embedded in client patcher at build time
  - Available from well-known URL (with HTTPS)
  - Certificate pinning option
  - Key rotation mechanism

**Signature Verification:**
- **Bi-level verification:**
  1. Diff signature verifies diff hasn't been tampered
  2. Target checksum verifies patch produces correct result
- **Fail-fast:** Any verification failure aborts update
- **Audit logging:** All verification attempts logged

### 4. Test Framework

**Purpose:** Validate diffs before publication

**Test Categories:**

**1. Integrity Tests (Critical)**
- Diff applies successfully
- Checksum matches expected result
- No corruption introduced
- All bytes accounted for

**2. Functional Tests (Essential)**
- Patched binary is executable
- Correct platform/architecture
- Version string correct
- File permissions preserved

**3. Smoke Tests (Recommended)**
- Binary executes `--version` flag
- Exit code is 0 (success)
- Output contains expected version number
- No segfaults or crashes

**4. Regression Tests (Optional)**
- Run application test suite
- Performance benchmarks
- Integration tests
- User-defined validation scripts

**Test Execution:**
- **Isolated environment:** Tests run in sandboxed directories
- **Multiple versions:** Test all source→target pairs
- **Parallel execution:** Run independent tests concurrently
- **Comprehensive reporting:** Detailed logs for failures

**Test Gates:**
- **Mandatory tests:** Must pass before publication allowed
- **Warning tests:** Failures logged but don't block
- **Optional tests:** Run if time permits
- **Custom gates:** User-defined pass/fail criteria

### 5. Publisher

**Purpose:** Upload diffs, binaries, and metadata to update server

**Upload Strategy:**
- **Parallel uploads:** Diffs and binaries uploaded concurrently
- **Resumable uploads:** Large files support resume on interruption
- **Bandwidth throttling:** Configurable upload speed limits
- **Compression:** On-the-fly compression during upload (optional)

**Integrity Verification:**
- **Upload checksum:** Server verifies uploaded content matches expected
- **Round-trip test:** Download and verify after upload
- **Atomic activation:** Version only activated after all uploads complete

**Rollout Control:**
- **Gradual rollout:** Start with small percentage of users
- **Canary deployment:** Specific user groups first
- **Geographic rollout:** Region by region
- **Emergency stop:** Ability to halt rollout immediately

**Metadata Publishing:**
- **Version manifest:** JSON describing version, diffs, signatures
- **Release notes:** Markdown-formatted user-facing notes
- **Compatibility info:** Minimum OS version, architecture
- **Update policy:** Force update, optional, recommended

---

## Tool Specifications

### Tool 1: `vbdp-init`

**Purpose:** Initialize publisher toolkit for a project

**Inputs:**
- Project name
- Binary path (or pattern)
- Update server URL
- Key generation options

**Outputs:**
- Configuration file
- Signing key pair (private + public)
- Version registry (empty database)
- Directory structure

**Features:**
- Interactive setup wizard
- Non-interactive mode (for scripts)
- Validation of inputs
- Secure key generation

**Key Generation:**
- Uses operating system's secure random number generator
- Ed25519 key pair generated
- Private key encrypted with password (optional)
- Public key saved separately for distribution

**Configuration:**
- Stored in `.vbdp/config.toml`
- Human-readable format
- Version controlled (except private key)
- Environment variable overrides supported

### Tool 2: `vbdp-register`

**Purpose:** Register a new version in the version registry

**Inputs:**
- Binary file path
- Version number (semantic versioning: X.Y.Z)
- Release notes (optional)
- Platform/architecture (auto-detected or manual)
- Build metadata (git commit, timestamp, etc.)

**Outputs:**
- Version registered in database
- Binary stored in repository
- Diffs computed from previous versions
- Metadata JSON created

**Process:**
1. **Binary analysis:**
   - Compute Blake3 checksum
   - Detect file format (ELF, PE, Mach-O, other)
   - Extract embedded version string (if present)
   - Measure file size

2. **Diff computation:**
   - Load recent previous versions (configurable, default 10)
   - Compute diff for each pair
   - Measure diff sizes
   - Decide which diffs to keep (threshold-based)
   - Compress diffs

3. **Storage:**
   - Store binary in organized directory
   - Store diffs in separate directory
   - Update database with metadata
   - Create version manifest JSON

**Performance:**
- Target: < 30 seconds for 100MB binary with 10 previous versions
- Parallel diff computation
- Progress reporting
- Resource limits (CPU, memory)

### Tool 3: `vbdp-sign`

**Purpose:** Sign diffs and binaries with publisher's private key

**Inputs:**
- Version to sign
- Private key path (or prompt for password)
- Algorithm selection (default: Ed25519)

**Outputs:**
- Signature files created
- Metadata updated with signature info
- Audit log entry

**Process:**
1. **Load private key:**
   - Prompt for password if encrypted
   - Validate key format
   - Check key not expired (if applicable)

2. **For each diff:**
   - Compute Blake3 hash of diff
   - Compute Blake3 hash of target binary
   - Create signature payload (hashes + metadata)
   - Sign with private key
   - Save signature file

3. **For full binary:**
   - Compute Blake3 hash
   - Create signature payload
   - Sign with private key
   - Save signature file

**Security Features:**
- Private key never logged
- Memory wiped after use
- Audit trail (who signed, when)
- Signature includes timestamp (freshness)

### Tool 4: `vbdp-test`

**Purpose:** Test diffs before publication

**Inputs:**
- Version to test
- Test suite selection (all, quick, custom)
- Test configuration file (optional)

**Outputs:**
- Test report (pass/fail for each test)
- Detailed logs for failures
- Summary statistics
- Exit code (0 = all passed, non-zero = failures)

**Test Execution:**
1. **Setup:**
   - Create temporary test directory
   - Copy necessary files

2. **Integrity tests:**
   - Apply each diff
   - Verify checksums
   - Check for corruption

3. **Functional tests:**
   - Check binary format
   - Verify executable
   - Test version string

4. **Smoke tests:**
   - Execute binary with `--version`
   - Check exit code
   - Verify output

5. **Custom tests:**
   - Run user-defined scripts
   - Configurable timeout
   - Parse output for pass/fail

6. **Cleanup:**
   - Remove temporary files
   - Restore environment

**Reporting:**
- Console output (color-coded)
- JUnit XML format (for CI integration)
- JSON format (for programmatic parsing)
- HTML report (human-readable)

### Tool 5: `vbdp-publish`

**Purpose:** Publish version to update server

**Inputs:**
- Version to publish
- Server URL (from config or override)
- API credentials
- Rollout configuration

**Outputs:**
- Upload confirmation
- Activation status
- Public URLs for downloads
- Dashboard link

**Process:**
1. **Pre-publish validation:**
   - Version registered: ✓
   - Diffs signed: ✓
   - Tests passed: ✓
   - Server credentials valid: ✓

2. **Upload files:**
   - Full binary (with progress bar)
   - All diffs (parallel uploads)
   - Signatures
   - Metadata manifest

3. **Server-side validation:**
   - Checksums verified
   - Signatures verified
   - Storage confirmed

4. **Activation:**
   - Version marked as available
   - Rollout policy applied
   - Analytics tracking started

**Rollout Options:**
- **Immediate (100%):** All users get update
- **Gradual (percentage-based):** Start at X%, increase daily
- **Canary (group-based):** Specific user groups first
- **Scheduled:** Activate at specific time

### Tool 6: `vbdp-analyze`

**Purpose:** Analyze version history and diff efficiency

**Inputs:**
- Version range (optional, default: all)
- Analysis type (size trends, diff efficiency, etc.)

**Outputs:**
- Statistical report
- Visualizations (ASCII graphs)
- Recommendations

**Metrics Analyzed:**
- **Binary size growth over time**
- **Average diff size by version transition**
- **Diff efficiency (% of file size)**
- **Most common update paths**
- **Storage overhead**
- **Bandwidth savings estimates**

**Use Cases:**
- Understand version bloat
- Optimize diff strategy
- Plan storage capacity
- Estimate bandwidth costs

### Tool 7: `vbdp-rollback`

**Purpose:** Rollback a published version

**Inputs:**
- Version to rollback
- Rollback reason
- Target version (optional, default: previous)

**Outputs:**
- Deactivation confirmation
- Downgrade diff generated (if needed)
- Notification to users (if configured)

**Process:**
1. **Validation:**
   - Confirm version is active
   - Require reason (audit trail)
   - Check target version available

2. **Deactivation:**
   - Mark version as rolled back
   - Update latest version pointer
   - Generate reverse diff (new → old)

3. **Client updates:**
   - Clients receive downgrade update
   - Applied same as normal update
   - Verified with signatures

4. **Reporting:**
   - Log rollback event
   - Notify stakeholders
   - Update analytics

**Safety Features:**
- Confirmation prompt (prevent accidents)
- Audit log entry
- Rollback can be reversed
- Automatic testing of reverse diff

### Tool 8: `vbdp-stats`

**Purpose:** View update statistics and analytics

**Inputs:**
- Time range (last 24h, 7d, 30d, all)
- Version filter (optional)
- Platform filter (optional)

**Outputs:**
- Statistics report
- Graphs and charts
- CSV export (optional)

**Metrics:**
- **Update count:** Total updates in period
- **Success rate:** Percentage of successful updates
- **Error breakdown:** By error type
- **Bandwidth savings:** Total bytes saved vs full downloads
- **Version distribution:** Pie chart of installed versions
- **Update latency:** Time from publication to user update
- **Platform breakdown:** Updates by OS/architecture

**Visualizations:**
- ASCII bar charts (terminal-friendly)
- Sparklines for trends
- Tables with alignment
- Color coding (green=good, red=bad)

---

## Configuration

### Configuration File Format

**Location:** `.vbdp/config.toml`

**Structure:**

```toml
[application]
name = "my-app"
binary_path = "target/release/my-app"
version_pattern = "semver"  # or "date", "custom"

[server]
url = "https://updates.example.com"
api_key_env = "VBDP_API_KEY"  # Read from environment
upload_timeout = 300  # seconds
retry_attempts = 3

[signing]
algorithm = "Ed25519"
private_key = ".vbdp/keys/private.key"
public_key = ".vbdp/keys/public.key"
key_encrypted = true

[diffing]
default_algorithm = "auto"  # or "bsdiff", "courgette", "xdelta3"
max_diff_ratio = 0.5  # Don't store diffs > 50% of file
compression = "zstd"
compression_level = 3
parallel_jobs = 4  # CPU cores for diff computation

[testing]
required_tests = ["integrity", "functional"]
optional_tests = ["smoke", "regression"]
timeout_per_test = 60  # seconds
fail_on_warnings = false

[storage]
binary_retention_days = 180
diff_retention_days = 180
lts_versions = ["1.0.0", "2.0.0"]  # Never delete

[publishing]
default_rollout = "gradual"
gradual_percentage_start = 10
gradual_percentage_increment = 20
gradual_increment_interval = "24h"

[analytics]
enable_telemetry = true
anonymous_only = true
```

---

## CLI Design Principles

### Usability
- **Consistent interface:** All tools follow same patterns
- **Helpful errors:** Clear error messages with suggested fixes
- **Progress feedback:** Show what's happening for long operations
- **Defaults:** Sensible defaults, minimal required flags

### Composability
- **Unix philosophy:** Do one thing well
- **Pipeable:** Output parseable by other tools
- **Scriptable:** Exit codes, JSON output, no interactive prompts (when flag set)

### Safety
- **Confirmations:** Destructive operations require confirmation
- **Dry-run mode:** Preview changes without executing
- **Rollback:** Most operations can be undone
- **Audit trail:** All operations logged

### Performance
- **Lazy loading:** Only load data when needed
- **Incremental:** Avoid recomputing unchanged data
- **Parallel:** Use all available CPU cores
- **Progress:** Show ETA for long operations

---

## Workflow Patterns

### Pattern 1: Manual Release

**Steps:**
1. Developer builds new version manually
2. Run `vbdp-register` to add to registry
3. Run `vbdp-sign` to create signatures
4. Run `vbdp-test` to validate
5. Run `vbdp-publish` to deploy

**Use case:** Small projects, infrequent releases

### Pattern 2: CI/CD Automated Release

**Steps:**
1. Git tag triggers CI pipeline
2. CI builds binary
3. CI runs `vbdp-register --version $TAG`
4. CI runs `vbdp-sign` (private key from secret store)
5. CI runs `vbdp-test --suite quick`
6. CI runs `vbdp-publish` (if tests pass)

**Use case:** Medium to large projects, frequent releases

### Pattern 3: Staged Release

**Steps:**
1. Automated registration and signing (CI)
2. Manual testing (QA team)
3. Manual publish with canary rollout
4. Monitor for 24 hours
5. Increase rollout percentage manually
6. Eventually reach 100%

**Use case:** Critical software, risk-averse deployments

### Pattern 4: Multi-Platform Release

**Steps:**
1. Build for all platforms (Linux, Windows, macOS, each arch)
2. Register each platform's binary separately
3. Sign all platforms
4. Test each platform
5. Publish all platforms together (atomic)

**Use case:** Cross-platform applications

---

## Integration Points

### CI/CD Systems

**GitHub Actions:**
- Action available: `vbdp-actions/publish@v1`
- Secrets: `VBDP_SIGNING_KEY`, `VBDP_API_KEY`
- Workflow templates provided

**GitLab CI:**
- Docker image: `vbdp/publisher:latest`
- Variables: `VBDP_SIGNING_KEY`, `VBDP_API_KEY`
- Example `.gitlab-ci.yml` provided

**Jenkins:**
- Plugin available: `vbdp-publisher`
- Credentials integration
- Pipeline script examples

**Other:**
- Standalone Docker image works with any CI
- Command-line tools scriptable

### Build Systems

**Compatible with:**
- GNU Make
- CMake
- Cargo (Rust)
- npm/yarn (JavaScript)
- Maven/Gradle (Java)
- Any system that produces binaries

**Integration:**
- Post-build hook to run `vbdp-register`
- Pre-release hook to run `vbdp-test`
- Release hook to run `vbdp-publish`

### Secret Management

**Supported:**
- HashiCorp Vault
- AWS Secrets Manager
- Azure Key Vault
- Google Secret Manager
- Environment variables (basic)

**Private key storage:**
- Encrypted on disk (password-protected)
- HSM integration (PKCS#11)
- Cloud KMS (AWS KMS, Azure Key Vault, GCP KMS)

---

## Security Considerations

### Threat Model

**Protected Against:**
- ✅ Unauthorized publication (API keys required)
- ✅ Tampered diffs (signature verification)
- ✅ Malicious updates (must be signed by publisher)
- ✅ Corrupted downloads (checksum validation)
- ✅ Replay attacks (timestamps in signatures)

**Potential Vulnerabilities:**
- ⚠️ Compromised publisher machine (protect private key)
- ⚠️ Stolen private key (key rotation needed)
- ⚠️ Insider threat (audit logging helps detect)
- ⚠️ Supply chain attack (verify build environment)

### Best Practices

**Key Management:**
- Generate keys on secure machine
- Never commit private keys to version control
- Use password protection or HSM
- Rotate keys annually (or after suspected compromise)
- Revocation mechanism for old keys

**Access Control:**
- Limit who can publish updates
- Separate keys per product
- Audit all publish operations
- Two-person rule for critical updates (optional)

**Testing:**
- Always test before publishing
- Use isolated test environments
- Automate as many tests as possible
- Manual QA for major releases

**Monitoring:**
- Monitor update success rates
- Alert on anomalies
- Track who published what and when
- Regular security audits

---

## Performance Benchmarks

### Expected Performance

**Registration (per version):**
- Binary analysis: < 1 second
- Diff computation (10 previous versions): 15-30 seconds for 100MB binary
- Metadata creation: < 1 second
- **Total:** ~30 seconds

**Signing:**
- Ed25519 signing: < 100ms per signature
- 10 diffs + 1 binary = 11 signatures: < 2 seconds
- **Total:** ~2 seconds

**Testing:**
- Integrity tests: 10-15 seconds (depends on binary size)
- Functional tests: 2-5 seconds
- Smoke tests: 2-5 seconds
- **Total (quick suite):** ~20 seconds

**Publishing:**
- Upload speed limited by network
- 100MB binary on 10Mbps: ~90 seconds
- All diffs (500KB total) on 10Mbps: ~1 second
- Server-side processing: 5-10 seconds
- **Total:** ~100 seconds (network-bound)

**End-to-End:**
- From build to published: 2-5 minutes (typical)
- Automated pipeline: No human intervention needed

### Resource Requirements

**Publisher Machine:**
- CPU: Modern multi-core (4+ cores recommended)
- RAM: 2GB minimum, 8GB recommended
- Disk: 10x binary size for storage (diffs, signatures, temp files)
- Network: Stable internet for publishing

**Optimization Tips:**
- Use SSD for storage (faster I/O)
- More CPU cores = faster parallel diffing
- Place binary storage on fast disk
- Cache previous versions locally

---

## Troubleshooting

### Common Issues

**Issue:** "Diff larger than threshold"
- **Cause:** Changes too extensive for diffing
- **Solution:** Increase threshold or accept full download fallback

**Issue:** "Private key decryption failed"
- **Cause:** Wrong password or corrupted key file
- **Solution:** Verify password, restore key from backup

**Issue:** "Upload failed: timeout"
- **Cause:** Slow network or large binary
- **Solution:** Increase timeout, use resumable upload, compress binary

**Issue:** "Test failed: checksum mismatch"
- **Cause:** Bug in diff algorithm or corrupted binary
- **Solution:** Re-register version, check binary integrity

---

## Next Steps

- **For implementation:** Read [System Design](../architecture/SYSTEM_DESIGN.md)
- **For deployment:** Read [Publisher Setup](../deployment/PUBLISHER_SETUP.md)
- **For CI integration:** Read [CI/CD Integration](../integration/CI_CD_INTEGRATION.md)
- **For security:** Read [Security Model](../security/SECURITY_MODEL.md)

---

**End of Publisher Toolkit Specification**
