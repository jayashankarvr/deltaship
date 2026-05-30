# Client Patcher

**Component:** Background service that downloads and applies binary patches on end-user devices
**Audience:** System administrators, enterprise IT, end users
**Last Updated:** 2026-01-07

---

## Overview

The Client Patcher is a lightweight background daemon running on end-user devices that:
- Automatically checks for software updates
- Downloads minimal differential updates (diffs)
- Verifies authenticity and integrity
- Applies patches atomically with automatic rollback
- Reports success/failure metrics

**Key Characteristics:**
- **Transparent:** Runs in background, minimal user interaction
- **Efficient:** Downloads only changed bytes (95-99% bandwidth savings)
- **Secure:** Verifies cryptographic signatures before applying
- **Reliable:** Atomic updates with automatic rollback on failure
- **Lightweight:** < 10MB disk, <100MB RAM peak, <5% CPU

---

## Design Principles

### User Experience First
- Silent background operation (no interruptions)
- Configurable update schedule (or manual)
- Bandwidth-aware (respect metered connections)
- Battery-aware (defer on low battery for mobile)
- Minimal resource usage

### Security by Default
- All updates cryptographically verified
- No execution of untrusted code
- Sandboxed patch application
- Rollback on any failure
- Audit logging

### Reliability
- Atomic operations (all-or-nothing)
- Automatic retry with exponential backoff
- Fallback to full download if diff fails
- Crash recovery (resume interrupted downloads)
- Self-healing (detect and fix corrupted cache)

### Simplicity
- Zero configuration for most users
- Self-updating capability
- Clear error messages
- Single executable (no dependencies)

---

## Core Components

### 1. Update Checker

**Purpose:** Periodically check for available updates

**Features:**

**Schedule Management:**
- Default: Check every 4 hours
- Configurable interval (1 hour to 30 days)
- Random jitter (prevent thundering herd)
- Manual trigger (user or admin initiated)
- Respect system sleep/wake cycles

**Inventory Scanning:**
- Detect installed software
- Read version information from:
  - Binary metadata (embedded version string)
  - Manifest files (JSON, XML, INI)
  - Registry (Windows)
  - Package manager database (Linux: dpkg, rpm)
  - Application directory structure
- Compute checksums to verify version

**Server Communication:**
- HTTPS only (TLS 1.2+)
- Request includes:
  - App name
  - Current version
  - Platform/architecture
  - Device ID (anonymized hash)
  - Patcher version
- Parse response (JSON)
- Handle errors gracefully (network timeout, server unavailable)

**Decision Logic:**
```
IF update available
  AND (not forced OR user preference allows)
  AND rollout group includes this device
  AND (not metered connection OR user allows metered)
  AND (not low battery OR charging)
THEN schedule download
ELSE wait for next check
```

### 2. Download Manager

**Purpose:** Download diffs, signatures, and metadata

**Features:**

**Smart Downloading:**
- Prefer diff over full binary (if available and smaller)
- Parallel downloads (diff + signature concurrently)
- Resume interrupted downloads (HTTP range requests)
- Verify content-length before downloading
- Checksum validation during download (streaming hash)

**Network Management:**
- Respect bandwidth limits (configurable)
- Pause on metered connections (optional)
- Prioritize by update urgency (forced > recommended > optional)
- Retry with exponential backoff (3 attempts)
- Fallback to mirror servers (if configured)

**Storage Management:**
- Download to temporary directory
- Isolated from application data
- Cleanup on completion or failure
- Pre-check available disk space
- Compress during transfer (if server supports)

**Progress Tracking:**
- Bytes downloaded / total
- Download speed (bytes/second)
- ETA (estimated time remaining)
- Status (queued, downloading, verifying, applying)
- Can pause/resume (user control)

### 3. Signature Verifier

**Purpose:** Verify cryptographic signatures before applying patches

**Features:**

**Public Key Management:**
- Built-in public keys (embedded at compile time)
- Additional keys from trusted sources
- Key rotation support (multiple valid keys)
- Revocation checking (optional, via CRL or OCSP)

**Verification Process:**
1. Load publisher's public key
2. Load signature file
3. Compute hash of diff file
4. Verify signature covers:
   - Diff content hash
   - Target binary hash
   - Version metadata (from→to)
   - Timestamp (freshness check)
5. Signature valid = proceed, invalid = abort

**Security:**
- Constant-time comparison (prevent timing attacks)
- Fail-closed (any doubt = reject)
- Log all verification attempts
- Alert on repeated failures (potential attack)

**Supported Algorithms:**
- Ed25519 (primary)
- ECDSA P-256 (fallback)
- RSA 2048+ (legacy, deprecated)

### 4. Patch Applicator

**Purpose:** Apply binary diffs to transform old version to new

**Features:**

**Atomic Application:**
1. **Backup current binary**
   - Copy to backup directory
   - Verify backup successful
   - Mark with timestamp

2. **Create temporary workspace**
   - Isolated directory
   - Sufficient space verified
   - Secure permissions

3. **Apply patch algorithm**
   - bsdiff/bspatch (general binaries)
   - Courgette (executables)
   - Stream-based (low memory usage)
   - Progress tracking

4. **Verify patched binary**
   - Compute checksum
   - Compare with expected
   - Validate file format (ELF, PE, Mach-O)
   - Check executable permissions

5. **Atomic replacement**
   - POSIX: rename() system call
   - Windows: ReplaceFile() API
   - Preserves permissions/ownership
   - No intermediate states visible

6. **Cleanup**
   - Delete temporary files
   - Update version database
   - Log success

**Rollback Mechanism:**
- On any failure:
  - Restore backup
  - Delete partial files
  - Log error details
  - Report to server (anonymous)
  - Schedule retry or fallback

**Performance:**
- Stream processing (memory efficient)
- Multi-threaded (if beneficial)
- Target: <10 seconds for 100MB binary
- Resource limits (CPU, memory)

### 5. Version Database

**Purpose:** Track installed software and update history

**Storage:**
- SQLite database (local, no server)
- Location: `/var/lib/deltaship/versions.db` (Linux)
- Lightweight schema

**Schema:**
```
apps table:
  - app_id (primary key)
  - name
  - current_version
  - binary_path
  - last_updated
  - install_date

updates table:
  - update_id (primary key)
  - app_id (foreign key)
  - from_version
  - to_version
  - timestamp
  - method (diff, full, rollback)
  - bytes_downloaded
  - success (boolean)
  - error_message (if failed)

config table:
  - key
  - value
```

**Features:**
- Update history (audit trail)
- Version tracking (current and previous)
- Statistics (bandwidth saved, update count)
- Crash recovery (detect incomplete updates)

### 6. Configuration Manager

**Purpose:** Manage patcher settings

**Configuration Sources (priority order):**
1. Command-line arguments (highest priority)
2. Environment variables
3. Configuration file (user-editable)
4. System-wide policy (enterprise)
5. Built-in defaults (lowest priority)

**Configuration File:**
- Location: `/etc/deltaship/config.toml` (system) or `~/.config/deltaship/config.toml` (user)
- Format: TOML (human-readable)

**Configurable Options:**
```toml
[updates]
auto_check = true
check_interval_hours = 4
auto_download = true
auto_apply = true  # or require user approval
allow_metered = false
allow_on_battery = true

[network]
max_download_speed_kbps = 0  # 0 = unlimited
concurrent_downloads = 2
retry_attempts = 3
timeout_seconds = 300

[storage]
cache_dir = "/var/cache/deltaship"
max_cache_size_mb = 1000
keep_backups = 3

[server]
update_server_url = "https://updates.example.com"
fallback_servers = [
  "https://updates2.example.com",
  "https://updates3.example.com"
]

[security]
verify_signatures = true  # cannot be disabled
log_verification_failures = true

[ui]
show_notifications = true
notification_level = "errors_only"  # or "all", "none"
```

**Policy Enforcement (Enterprise):**
- System-wide policy cannot be overridden by user
- Examples:
  - Force automatic updates
  - Restrict to corporate servers only
  - Mandate specific update schedules
  - Disable user-visible notifications

---

## Platform-Specific Implementation

### Linux

**Service Integration:**
- **systemd service:** `/lib/systemd/system/deltaship.service`
- Starts on boot, runs as non-root user
- Manages lifecycle, restarts on crash
- Journal logging integration

**Package Management:**
- .deb package (Debian, Ubuntu)
- .rpm package (RHEL, Fedora, openSUSE)
- Flatpak (distro-agnostic)
- Snap (alternative)

**File Locations:**
- Binary: `/usr/bin/deltaship`
- Config: `/etc/deltaship/config.toml`
- Data: `/var/lib/deltaship/`
- Cache: `/var/cache/deltaship/`
- Logs: `/var/log/deltaship/` or journald

**Permissions:**
- Runs as dedicated user (`deltaship`)
- Sudo/polkit for system binary updates
- User-level for user applications

### Windows

**Service Integration:**
- Windows Service (runs at startup)
- Background task (Task Scheduler fallback)
- System tray icon (optional UI)

**Installation:**
- MSI installer (enterprise-friendly)
- Chocolatey package (developer-friendly)
- winget package (Microsoft Store)

**File Locations:**
- Binary: `C:\Program Files\Deltaship\deltaship.exe`
- Config: `C:\ProgramData\Deltaship\config.toml`
- Data: `C:\ProgramData\Deltaship\data\`
- Cache: `C:\Users\{User}\AppData\Local\Deltaship\cache\`
- Logs: Windows Event Log + file logs

**Permissions:**
- Runs as SYSTEM (for system-wide updates)
- User context (for per-user applications)
- UAC prompts (if needed for elevation)

### macOS

**Service Integration:**
- launchd daemon (`/Library/LaunchDaemons/com.deltaship.patcher.plist`)
- Login item (user-level alternative)
- Menu bar app (optional UI)

**Installation:**
- .pkg installer (native)
- Homebrew formula (`brew install deltaship`)
- DMG with app bundle

**File Locations:**
- Binary: `/usr/local/bin/deltaship`
- Config: `/Library/Application Support/Deltaship/config.toml`
- Data: `/var/lib/deltaship/`
- Cache: `~/Library/Caches/Deltaship/`
- Logs: `/var/log/deltaship/` or Console.app

**Permissions:**
- Runs as root (for system apps)
- User context (for user apps)
- Notarization (required for macOS 10.15+)
- Code signing with Developer ID

---

## User Interface

### Minimal UI (Default)

**Philosophy:** Updates happen silently, users rarely interact

**Components:**

**System Tray/Menu Bar Icon:**
- Shows update status (idle, checking, downloading, applying)
- Right-click menu:
  - Check for updates now
  - Pause updates
  - View update history
  - Open settings
  - Quit

**Notifications:**
- **Success:** "AppName updated to version X.Y.Z" (optional, off by default)
- **Available:** "Update available for AppName" (if manual approval required)
- **Error:** "Update failed for AppName, will retry" (errors only)
- **Restart Required:** "Please restart AppName to complete update"

**Settings Dialog:**
- Simple checkboxes and dropdowns
- Apply button, cancel button
- Help text for each option
- Link to advanced settings

### Advanced UI (Optional)

**Full Application Window:**
- **Updates tab:** List of apps, versions, update status
- **History tab:** Update log with timestamps, sizes, results
- **Settings tab:** All configuration options
- **Statistics tab:** Bandwidth saved, update count, charts

**Command-Line Interface (CLI):**
- `deltaship check` - Check for updates now
- `deltaship status` - Show current status
- `deltaship history` - View update history
- `deltaship config` - View/edit configuration
- `deltaship pause` - Pause updates
- `deltaship resume` - Resume updates
- Useful for automation, scripting, remote management

---

## Enterprise Features

### Centralized Management

**Group Policy (Windows):**
- Deploy configuration via Active Directory
- Override user settings
- Force specific update servers
- Schedule update windows

**MDM Integration (Mobile, macOS):**
- Configuration profiles
- Remote policy enforcement
- Update monitoring dashboard
- Compliance reporting

**Puppet/Ansible/Chef:**
- Configuration management integration
- Deploy patcher to fleets
- Enforce policies
- Monitor status

### Reporting & Analytics

**Telemetry (Opt-in):**
- Anonymous device ID
- Update success/failure
- Bandwidth usage
- Platform/version info
- No personally identifiable information

**Enterprise Dashboard:**
- Fleet-wide update status
- Version compliance (% on latest version)
- Error trends
- Bandwidth savings
- Security posture (% with unpatched vulnerabilities)

**Integration:**
- Export to SIEM (Splunk, ELK)
- Metrics to monitoring (Prometheus, DataDog)
- Alerts to incident management (PagerDuty, Opsgenie)

---

## Security Considerations

### Threat Model

**Protected Against:**
- ✅ Malicious updates (signature verification)
- ✅ Man-in-the-middle (HTTPS + certificate pinning option)
- ✅ Corrupted downloads (checksum validation)
- ✅ Privilege escalation (least-privilege execution)
- ✅ Denial of service (resource limits, rate limiting)

**Risks:**
- ⚠️ Compromised update server (mitigated by signature verification)
- ⚠️ Local privilege escalation (mitigated by sandboxing, least privilege)
- ⚠️ Side-channel attacks (out of scope for v1.0)

### Sandboxing

**Process Isolation:**
- Download process: no file system access beyond cache
- Patch process: limited file system access (temp dir + target binary)
- Verification process: read-only access

**Platform Mechanisms:**
- Linux: seccomp-bpf, AppArmor/SELinux profiles
- Windows: Job Objects, restricted tokens
- macOS: Sandbox entitlements

**Resource Limits:**
- CPU: <5% average, <50% peak
- Memory: <100MB
- Disk I/O: rate-limited
- Network: configurable bandwidth limit

### Logging & Auditing

**What's Logged:**
- Update checks (timestamp, result)
- Downloads (URL, size, duration, result)
- Signature verifications (success/failure)
- Patch applications (success/failure)
- Configuration changes
- Errors and warnings

**What's NOT Logged:**
- User identity (anonymized device ID only)
- Personal data
- Full file paths (sanitized)

**Log Rotation:**
- Max size: 100MB
- Retention: 30 days
- Compress old logs

---

## Performance Characteristics

### Resource Usage

**Idle State:**
- CPU: <1%
- Memory: ~20MB
- Disk I/O: negligible
- Network: periodic checks (few KB every 4 hours)

**Checking for Updates:**
- CPU: <2%
- Memory: ~30MB
- Network: ~5KB per app (manifest download)
- Duration: <1 second

**Downloading Update:**
- CPU: <5% (hash computation)
- Memory: ~50MB (buffering)
- Network: variable (diff download, typically 10KB-1MB)
- Disk I/O: write to cache (~1-2 MB/s)

**Applying Patch:**
- CPU: 20-50% (patch algorithm, single core)
- Memory: ~100MB peak (binary in memory)
- Disk I/O: read old binary, write new binary
- Duration: 3-10 seconds for 100MB binary

### Benchmarks

**Target Performance:**
- Update check: <500ms
- Download 1MB diff: <10 seconds (on 10Mbps connection)
- Apply patch to 100MB binary: <10 seconds
- Total user-perceived time: <20 seconds (typical update)

**Compared to Traditional Update:**
- Traditional: Download 100MB + install = 100+ seconds
- Deltaship: Download 1MB + patch = 20 seconds
- **5x faster, 99% less bandwidth**

---

## Troubleshooting

### Common Issues

**Issue:** Updates not checking
- **Diagnosis:** Check service status, review logs
- **Solution:** Restart service, check network connectivity, verify configuration

**Issue:** Downloads failing
- **Diagnosis:** Check network, server status, disk space
- **Solution:** Clear cache, check firewall, retry manually

**Issue:** Signature verification failures
- **Diagnosis:** Check public key, server certificate, system time
- **Solution:** Update public key, sync system time, reinstall patcher

**Issue:** Patch application failing
- **Diagnosis:** Check disk space, file permissions, binary corruption
- **Solution:** Free disk space, fix permissions, re-download, fallback to full binary

### Diagnostic Tools

**Built-in Diagnostics:**
- `deltaship diagnose` - Run health checks
- `deltaship verify` - Verify installed software integrity
- `deltaship reset` - Reset to defaults (clears cache, resets config)

**Logs:**
- Check logs for errors
- Increase verbosity (`deltaship --verbose`)
- Export logs for support

---

## Next Steps

- **For installation:** Read [Client Installation](../deployment/CLIENT_INSTALLATION.md)
- **For architecture:** Read [System Design](../architecture/SYSTEM_DESIGN.md)
- **For enterprise:** Read [Existing Systems Integration](../integration/EXISTING_SYSTEMS.md)
- **For troubleshooting:** Read [Maintenance](../operations/MAINTENANCE.md)

---

**End of Client Patcher Specification**
