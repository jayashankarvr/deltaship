# Complete System Flow

**Document:** End-to-end flow from software build to user update
**Audience:** All stakeholders
**Last Updated:** 2026-01-07

---

## Overview

This document traces a complete update from the moment a developer builds a new version through publication, distribution, and application on end-user devices.

---

## Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│  PHASE 1: BUILD & PUBLISH (Developer/Publisher)             │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│  PHASE 2: STORAGE & PREPARATION (Update Server)             │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│  PHASE 3: DISCOVERY & DOWNLOAD (Client Patcher)             │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│  PHASE 4: VERIFICATION & APPLICATION (Client Patcher)       │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│  PHASE 5: VALIDATION & REPORTING (Client & Server)          │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Build & Publish

### 1.1 Developer Actions

**Scenario:** Developer fixes a bug in version 1.0.0, releases version 1.0.1

**Steps:**

1. **Code changes made**
   - Developer commits bug fix to repository
   - CI/CD pipeline triggered

2. **Build process**
   - Automated build compiles new binary
   - Binary produced: `my-app-1.0.1` (5.2 MB)

3. **Version registration**
   - Publisher toolkit invoked (automatically or manually)
   - Binary registered in version database
   - Metadata captured:
     - Version number: 1.0.1
     - Build timestamp
     - Git commit hash
     - Platform/architecture
     - Binary checksum (Blake3)

### 1.2 Diff Generation

**Automatic Process:**

1. **Previous version retrieved**
   - System loads: `my-app-1.0.0` (5.2 MB)
   - Multiple previous versions checked (1.0.0, 0.9.5, 0.9.4, etc.)

2. **Binary diff computed**
   - Algorithm selected based on file type:
     - Executable → Courgette (executable-aware)
     - Archive → bsdiff (general purpose)
     - Custom → algorithm specified in config

3. **Diff size analyzed**
   - 1.0.0 → 1.0.1: 45 KB (0.8% of file) ✅
   - 0.9.5 → 1.0.1: 234 KB (4.5% of file) ✅
   - 0.9.4 → 1.0.1: 456 KB (8.8% of file) ✅

   Decision: All diffs < 50% threshold, worth storing

4. **Diff optimization**
   - Compression applied (zstd)
   - Final sizes:
     - 1.0.0 → 1.0.1: 38 KB (compressed)
     - 0.9.5 → 1.0.1: 198 KB (compressed)

### 1.3 Cryptographic Signing

**Security Step:**

1. **Load publisher's private key**
   - Key retrieved from secure storage
   - Password/HSM required for access

2. **Sign each diff**
   - Hash diff content (Blake3)
   - Hash target binary (Blake3)
   - Create signature payload:
     - Source version: 1.0.0
     - Target version: 1.0.1
     - Diff checksum
     - Target binary checksum
     - Timestamp
   - Sign with Ed25519 private key

3. **Sign full binary**
   - In case diff fails, full binary available
   - Signature covers entire binary + metadata

### 1.4 Testing

**Pre-publish validation:**

1. **Diff application test**
   - Load 1.0.0 binary
   - Apply diff
   - Verify result matches 1.0.1
   - Checksum validation
   - Repeat for all version pairs

2. **Signature verification test**
   - Verify all signatures with public key
   - Ensure no tampering detectable

3. **Smoke tests**
   - Execute patched binary
   - Run basic functionality tests
   - Verify version string

4. **Rollback test**
   - Simulate failed patch
   - Verify original binary restored
   - No corruption detected

### 1.5 Publication

**Upload to server:**

1. **Full binary uploaded**
   - Destination: `/binaries/my-app/1.0.1/x86_64-linux`
   - Size: 5.2 MB
   - Checksum verified after upload

2. **Diffs uploaded**
   - `/diffs/my-app/1.0.0-to-1.0.1.diff`
   - `/diffs/my-app/0.9.5-to-1.0.1.diff`
   - All compressed

3. **Signatures uploaded**
   - `/signatures/my-app/1.0.1/` directory
   - Contains signatures for all diffs and binary

4. **Metadata uploaded**
   - JSON manifest with:
     - Version info
     - Checksums
     - Available diff paths
     - Release notes
     - Rollout configuration

5. **Activation**
   - Version marked as "active"
   - Gradual rollout starts (if configured)
   - Analytics tracking begins

**Timeline:** Entire process takes 2-5 minutes for typical application.

---

## Phase 2: Storage & Preparation

### 2.1 Server-Side Organization

**Storage structure:**

```
/storage/
├── binaries/
│   └── my-app/
│       ├── 1.0.1/
│       │   ├── x86_64-linux (full binary: 5.2 MB)
│       │   ├── aarch64-linux (full binary: 4.8 MB)
│       │   └── x86_64-windows.exe (full binary: 5.5 MB)
│       └── 1.0.0/
│           └── ... (previous versions)
├── diffs/
│   └── my-app/
│       ├── 1.0.0-to-1.0.1.diff (38 KB)
│       ├── 0.9.5-to-1.0.1.diff (198 KB)
│       └── ... (other version pairs)
├── signatures/
│   └── my-app/
│       └── 1.0.1/
│           ├── binary.sig
│           ├── diff-from-1.0.0.sig
│           └── ...
└── metadata/
    └── my-app/
        ├── 1.0.1.json (version manifest)
        └── catalog.json (all versions index)
```

### 2.2 Cache Strategy

**Pre-computed diffs:**

- Common version transitions cached
- Most popular: current → latest
- Recent versions: last 10 releases
- Long-term support: LTS → LTS

**On-demand computation:**

- Rare version pairs computed when requested
- Result cached for future requests
- Fallback to full download if computation too expensive

**Cache eviction:**

- Least recently used (LRU)
- Age-based (diffs older than 6 months)
- Space-based (when storage limit reached)

### 2.3 Rollout Management

**Gradual deployment:**

1. **Initial rollout (10%)**
   - Version 1.0.1 visible to 10% of users
   - Selection deterministic (hash of device ID)
   - Same user always in same group

2. **Monitoring window (24 hours)**
   - Track error rates
   - Monitor performance metrics
   - Gather user feedback

3. **Incremental rollout**
   - Day 2: 25%
   - Day 3: 50%
   - Day 4: 75%
   - Day 5: 100%

4. **Emergency rollback**
   - If error rate > threshold (e.g., 1%)
   - Automatic deactivation
   - Rollback to previous version
   - Notification to publishers

---

## Phase 3: Discovery & Download

### 3.1 Client Patcher Daemon

**Background service running on user's device:**

**Startup:**

- Service starts with OS
- Loads configuration
- Reads local software inventory

**Periodic checks:**

- Default: Every 4 hours
- Configurable by admin/user
- Can be triggered manually

### 3.2 Update Discovery

**Client initiates check:**

1. **Inventory scan**
   - Patcher scans installed software
   - Detects: `my-app` version 1.0.0 installed
   - Reads binary checksum to confirm version

2. **Query update server**

   ```
   Request:
   GET /api/v1/check-update
   Headers:
     App-Name: my-app
     Current-Version: 1.0.0
     Architecture: x86_64-linux
     Device-ID: unique-hash-of-device
     Client-Patcher-Version: 1.0
   ```

3. **Server response**

   ```
   Response:
   Status: 200 OK
   Headers:
     Latest-Version: 1.0.1
     Update-Available: true
     Update-Type: differential
     Rollout-Group: included (user is in 10% group)

   Body (JSON):
   {
     "target_version": "1.0.1",
     "diff_available": true,
     "diff_url": "/diffs/my-app/1.0.0-to-1.0.1.diff",
     "diff_size": 38912,
     "diff_checksum": "blake3:abc123...",
     "signature_url": "/signatures/my-app/1.0.1/diff-from-1.0.0.sig",
     "full_binary_url": "/binaries/my-app/1.0.1/x86_64-linux",
     "full_binary_size": 5242880,
     "target_checksum": "blake3:def456...",
     "release_notes": "Bug fix release...",
     "force_update": false,
     "download_priority": "normal"
   }
   ```

4. **Decision logic**
   - Update available: Yes
   - Diff available: Yes (38 KB vs 5.2 MB full download)
   - Rollout group: Included
   - Force update: No (can be deferred)
   - Decision: **Download diff**

### 3.3 Download Process

**Parallel downloads:**

1. **Download diff**
   - URL: `/diffs/my-app/1.0.0-to-1.0.1.diff`
   - Size: 38 KB
   - Progress tracking enabled
   - Retry logic: 3 attempts with exponential backoff
   - Timeout: 30 seconds
   - Download time: ~0.5 seconds (on 10 Mbps connection)

2. **Download signature**
   - URL: `/signatures/my-app/1.0.1/diff-from-1.0.0.sig`
   - Size: ~200 bytes
   - Critical for verification

3. **Download public key (if not cached)**
   - Publisher's public key
   - Cached locally after first download
   - Validated against trusted key store

**Bandwidth management:**

- Respect user's bandwidth limits (if configured)
- Pause on metered connections (if configured)
- Schedule downloads during off-peak hours (if configured)

**Storage:**

- Downloads stored in temporary directory
- Isolated from main system
- Cleaned up after successful application

---

## Phase 4: Verification & Application

### 4.1 Pre-Application Verification

**Critical security steps (fail-fast):**

1. **Signature verification**
   - Load publisher's public key
   - Verify signature over:
     - Diff content
     - Target binary checksum
     - Version metadata
   - Result: ✅ Signature valid
   - **If failed:** Delete diff, report error, exit

2. **Diff integrity check**
   - Compute Blake3 hash of downloaded diff
   - Compare with expected checksum from manifest
   - Result: ✅ Checksum matches
   - **If failed:** Re-download (up to 3 times), then exit

3. **Source binary verification**
   - Compute checksum of current `my-app` binary
   - Verify it matches expected source (1.0.0)
   - Result: ✅ Correct source version
   - **If failed:** Version mismatch, cannot patch, download full binary

### 4.2 Atomic Patch Application

**All-or-nothing operation:**

1. **Create backup**
   - Copy current binary to backup location
   - Path: `/var/cache/patcher/backups/my-app-1.0.0-backup`
   - Backup marked with timestamp
   - Old backups cleaned up (keep last 3)

2. **Prepare temporary space**
   - Allocate space for patched binary
   - Path: `/tmp/patcher/my-app-1.0.1-temp`
   - Space check: Ensure sufficient disk space

3. **Apply diff**
   - Algorithm: bspatch (or Courgette for executables)
   - Input: Current binary (5.2 MB)
   - Patch: Diff file (38 KB)
   - Output: New binary (5.2 MB)
   - Process:
     - Read current binary into memory (or stream)
     - Apply diff instructions:
       - Copy blocks from old binary
       - Insert new blocks from diff
       - Skip deleted blocks
     - Write to temporary location
   - Duration: ~3 seconds

4. **Verify patched binary**
   - Compute Blake3 checksum of result
   - Expected: `blake3:def456...` (from manifest)
   - Actual: `blake3:def456...`
   - Result: ✅ Match
   - **If failed:** Delete temp file, restore backup, report error

5. **Platform-specific validation**
   - **Linux/Unix:** Check ELF header, verify executable bit
   - **Windows:** Check PE header, verify signature (Authenticode)
   - **macOS:** Check Mach-O header, verify code signature
   - Result: ✅ Valid executable

6. **Atomic replacement**
   - Method depends on platform:
     - **POSIX:** `rename()` system call (atomic)
     - **Windows:** `ReplaceFile()` API (atomic with backup)
   - Operation:
     - Rename `/tmp/patcher/my-app-1.0.1-temp` to `/usr/bin/my-app`
     - Old file atomically replaced
     - No intermediate state visible to system
   - Duration: < 1 millisecond

7. **Post-replacement verification**
   - Verify new binary in place
   - Check permissions/ownership preserved
   - Update local version database
   - Result: ✅ Update successful

8. **Cleanup**
   - Delete downloaded diff
   - Delete temporary files
   - Backup retained for rollback

**Total duration:** ~5 seconds (3s patch + 2s verification)

### 4.3 Rollback Mechanism

**If any step fails:**

1. **Detect failure**
   - Checksum mismatch
   - Patch application error
   - Verification failure

2. **Restore backup**
   - Copy `/var/cache/patcher/backups/my-app-1.0.0-backup` to `/usr/bin/my-app`
   - Atomic operation
   - Verify restoration successful

3. **Clean up failed update**
   - Delete partial files
   - Mark update as failed in local database

4. **Report failure**
   - Send error report to server (anonymous)
   - Log detailed error locally
   - Notify user (if configured)

5. **Retry strategy**
   - Wait before retry (exponential backoff)
   - Maximum 3 retry attempts
   - After 3 failures: Fall back to full download
   - After full download fails: Give up, log error

---

## Phase 5: Validation & Reporting

### 5.1 Post-Update Validation

**Client-side checks:**

1. **Smoke test**
   - Execute updated binary with `--version` flag
   - Expected output: "my-app 1.0.1"
   - Actual output: "my-app 1.0.1"
   - Result: ✅ Binary functional

2. **Functional test (optional)**
   - If configured, run basic functionality tests
   - Example: Check if app can connect to license server
   - Result: ✅ App working

3. **Update local database**
   - Record successful update:
     - From version: 1.0.0
     - To version: 1.0.1
     - Update timestamp
     - Update method: differential
     - Bytes downloaded: 38 KB
     - Total duration: 5 seconds

### 5.2 Telemetry & Reporting

**Anonymous analytics sent to server:**

```
POST /api/v1/update-report
Body:
{
  "app_name": "my-app",
  "device_id_hash": "anon-hash-123",
  "from_version": "1.0.0",
  "to_version": "1.0.1",
  "update_method": "differential",
  "diff_size": 38912,
  "success": true,
  "duration_seconds": 5,
  "platform": "x86_64-linux",
  "patcher_version": "1.0",
  "timestamp": "2026-01-07T10:30:00Z"
}
```

**Server-side aggregation:**

- Total updates: Counter incremented
- Success rate: 99.98% (2 failures out of 10,000 updates)
- Average download size: 42 KB
- Average duration: 6 seconds
- Platform breakdown tracked
- Version transition matrix updated

### 5.3 Publisher Dashboard

**Real-time metrics:**

- Version 1.0.1 adoption: 12.3% (10% rollout + early adopters)
- Total updates in last 24h: 10,234
- Success rate: 99.98%
- Bandwidth saved: 52 GB (vs full downloads: 53 GB)
- Error breakdown:
  - Signature failures: 0
  - Checksum mismatches: 2 (network corruption, retried successfully)
  - Patch application errors: 0

**Alerting:**

- If error rate > 1%: Email alert to publisher
- If specific error pattern detected: Automatic investigation
- If rollback needed: Manual approval required

---

## Alternative Flows

### Flow A: Full Download Fallback

**When diff unavailable or too large:**

1. **Decision point**
   - Diff size: 2.8 MB (54% of file)
   - Threshold: 50%
   - Decision: Download full binary instead

2. **Download full binary**
   - URL: `/binaries/my-app/1.0.1/x86_64-linux`
   - Size: 5.2 MB
   - Duration: ~5 seconds (on 10 Mbps)

3. **Verification**
   - Same signature verification
   - Same checksum validation

4. **Atomic replacement**
   - Same process as diff application
   - Direct replacement, no patching step

**Use cases:**

- Very old source version (diff larger than beneficial)
- First-time install (no previous version)
- Diff computation failed
- Corrupted source binary

### Flow B: First-Time Installation

**New installation (no previous version):**

1. **Discovery**
   - User installs app from app store / website
   - Patcher detects new software
   - No current version exists

2. **Download**
   - Full binary downloaded
   - Signature verified
   - Installed to system

3. **Registration**
   - App registered in local version database
   - Future updates will use diffs

### Flow C: Downgrade/Rollback

**User or admin initiates rollback:**

1. **Request downgrade**
   - Current: 1.0.1 (buggy)
   - Target: 1.0.0 (stable)

2. **Reverse diff**
   - Server provides: 1.0.1 → 1.0.0 diff
   - Or full 1.0.0 binary

3. **Application**
   - Same process as forward update
   - Signature verified
   - Atomic replacement

**Use cases:**

- Bug discovered in new version
- Incompatibility with user's setup
- Manual rollback by admin

### Flow D: Multi-Hop Update

**User on very old version:**

1. **Current version:** 0.9.0
2. **Latest version:** 1.0.5
3. **Direct diff:** Not available or too large (15 MB)

4. **Multi-step path**
   - Server suggests: 0.9.0 → 0.9.5 → 1.0.0 → 1.0.5
   - Total diff size: 2 MB (vs 15 MB direct or 5.2 MB full)

5. **Execution**
   - Apply diffs sequentially
   - Verify after each step
   - Rollback to original if any step fails

**Optimization:**

- Server computes optimal path (Dijkstra's algorithm)
- Minimize total bytes downloaded

---

## Timeline Summary

### From Build to User Update

**Typical case (user on previous version):**

| Phase | Duration | Bandwidth | Notes |
|-------|----------|-----------|-------|
| Build & publish | 3 minutes | 5 MB upload | One-time per version |
| Server preparation | Instant | Cached | Pre-computed diffs |
| Update discovery | 0.5 seconds | 2 KB | Periodic check |
| Download | 0.5 seconds | 38 KB | Diff only |
| Verification | 1 second | 0 | Local computation |
| Patch application | 3 seconds | 0 | Local computation |
| Validation | 1 second | 0 | Smoke test |
| **Total (user perspective)** | **~6 seconds** | **38 KB** | 99.2% bandwidth saved |

### Comparison with Traditional Update

**Traditional (full download):**

- Download: 5.2 MB
- Duration: 50 seconds (on 10 Mbps)
- Replacement: 1 second
- **Total: 51 seconds**

**Deltaship (differential update):**

- Download: 38 KB
- Duration: 0.5 seconds
- Patch: 3 seconds
- Verification: 2 seconds
- **Total: 6 seconds**

**Improvement:**

- **Speed:** 8.5x faster
- **Bandwidth:** 99.2% reduction
- **User experience:** Seamless background update

---

## Error Scenarios & Recovery

### Scenario 1: Network Interruption During Download

1. **Failure detected:** Incomplete download
2. **Retry logic:** Wait 5 seconds, retry (exponential backoff)
3. **Maximum retries:** 3 attempts
4. **Final fallback:** User notified, retry scheduled for later
5. **User impact:** Minimal, update deferred

### Scenario 2: Signature Verification Failure

1. **Failure detected:** Invalid signature
2. **Immediate action:** Delete downloaded files
3. **Security log:** Record potential attack attempt
4. **No retry:** Signature failures not retried
5. **User impact:** Protected from malicious update

### Scenario 3: Checksum Mismatch After Patching

1. **Failure detected:** Patched binary checksum wrong
2. **Rollback:** Restore backup immediately
3. **Report:** Send error report to server
4. **Retry:** Try re-downloading diff
5. **Fallback:** After 3 failures, download full binary
6. **User impact:** No corruption, update delayed

### Scenario 4: Insufficient Disk Space

1. **Detection:** Before download starts
2. **Action:** Cleanup old backups, temp files
3. **Retry:** Attempt again after cleanup
4. **User notification:** If still insufficient space
5. **User impact:** Update deferred until space available

### Scenario 5: Server Unavailable

1. **Detection:** HTTP error 5xx
2. **Retry:** Exponential backoff (5s, 10s, 20s, 40s)
3. **Maximum wait:** 2 hours
4. **Fallback:** Schedule retry for next periodic check
5. **User impact:** Update delayed, no user action needed

---

## Monitoring Points

### Key Metrics Tracked

1. **Update success rate:** Target > 99%
2. **Average update duration:** Target < 10 seconds
3. **Bandwidth savings:** Target > 95%
4. **Error rate by type:** Each type < 0.1%
5. **Rollout coverage:** Track adoption curve
6. **Platform distribution:** Usage by OS/architecture
7. **Version fragmentation:** Users on old versions

### Alerts Configured

1. **Critical:** Error rate > 1% (immediate)
2. **Warning:** Error rate > 0.5% (within 1 hour)
3. **Info:** New version adoption < expected (daily)
4. **Security:** Signature verification failures (immediate)

---

## Next Steps

- **For Publishers:** Read [Publisher Toolkit](tools/PUBLISHER_TOOLKIT.md)
- **For Administrators:** Read [Server Deployment](deployment/SERVER_DEPLOYMENT.md)
- **For Developers:** Read [System Design](architecture/SYSTEM_DESIGN.md)
- **For Security Teams:** Read [Security Model](security/SECURITY_MODEL.md)

---

**End of Complete Flow Documentation**
