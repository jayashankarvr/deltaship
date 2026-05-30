# Publisher Setup Guide

**Document:** Setup procedures for software publishers using Deltaship
**Audience:** Software developers, release engineers, DevOps teams
**Last Updated:** 2026-01-07

---

## Overview

This guide describes how software publishers (application developers, vendors) set up the Deltaship Publisher Toolkit to distribute efficient updates for their applications.

**Setup Time:** 15-30 minutes
**Prerequisites:** Development machine, signing keys, build pipeline access
**Platforms:** Linux, macOS, Windows (developer workstation)

---

## What is the Publisher Toolkit?

The Publisher Toolkit is a set of command-line tools that enables publishers to:
- Register versions of their software
- Generate binary diffs between versions
- Cryptographically sign updates
- Test update packages
- Publish to update server
- Analyze update statistics
- Manage rollbacks

**Tools Included:**
- `deltaship-init` - Initialize publisher project
- `deltaship-register` - Register new version
- `deltaship-sign` - Sign diffs and binaries
- `deltaship-test` - Test update packages locally
- `deltaship-publish` - Publish to update server
- `deltaship-analyze` - Analyze bandwidth savings
- `deltaship-rollback` - Rollback problematic releases
- `deltaship-stats` - View statistics

---

## Prerequisites

### System Requirements

**Operating System:**
- Linux (Ubuntu 20.04+, Debian 11+, RHEL 8+)
- macOS 11+ (Big Sur or later)
- Windows 10+ (with WSL2 recommended)

**Resources:**
- 4 GB RAM (8 GB recommended for large binaries)
- 10 GB disk space (plus space for your binaries)
- CPU: Modern multi-core processor (diff generation is CPU-intensive)

**Network:**
- Internet access (to upload to update server)
- Firewall allows outbound HTTPS (port 443)

### Software Requirements

**Required:**
- Rust 1.70+ (or use pre-built binaries)
- OpenSSL development libraries
- Git (for version control)

**Optional:**
- Docker (for CI/CD integration)
- CI/CD platform (GitHub Actions, GitLab CI, Jenkins)

---

## Installation

### Method 1: Package Manager (Recommended)

**Linux (Debian/Ubuntu):**

```
wget https://releases.deltaship.io/publisher/deltaship-publisher-toolkit_1.0.0_amd64.deb
sudo dpkg -i deltaship-publisher-toolkit_1.0.0_amd64.deb
```

**Linux (RHEL/Fedora):**

```
wget https://releases.deltaship.io/publisher/deltaship-publisher-toolkit-1.0.0-1.x86_64.rpm
sudo dnf install deltaship-publisher-toolkit-1.0.0-1.x86_64.rpm
```

**macOS (Homebrew):**

```
brew tap deltaship/publisher
brew install deltaship-publisher-toolkit
```

**Windows (Chocolatey):**

```
choco install deltaship-publisher-toolkit
```

**Verify Installation:**

```
deltaship-init --version
```

Expected output: `deltaship-init 1.0.0`

### Method 2: Pre-Built Binaries

**Download:**

```
# Linux x86_64
wget https://releases.deltaship.io/publisher/deltaship-publisher-toolkit-1.0.0-linux-x86_64.tar.gz

# macOS (Intel)
wget https://releases.deltaship.io/publisher/deltaship-publisher-toolkit-1.0.0-darwin-x86_64.tar.gz

# macOS (Apple Silicon)
wget https://releases.deltaship.io/publisher/deltaship-publisher-toolkit-1.0.0-darwin-arm64.tar.gz

# Windows
wget https://releases.deltaship.io/publisher/deltaship-publisher-toolkit-1.0.0-windows-x86_64.zip
```

**Extract and Install:**

```
# Linux/macOS
tar -xzf deltaship-publisher-toolkit-*.tar.gz
cd deltaship-publisher-toolkit-*
sudo ./install.sh

# Windows
unzip deltaship-publisher-toolkit-*.zip
cd deltaship-publisher-toolkit-*
install.bat
```

**Add to PATH:**

```
# Linux/macOS (add to ~/.bashrc or ~/.zshrc)
export PATH="$HOME/.deltaship/bin:$PATH"

# Windows (add to system PATH via System Properties)
C:\Users\YourName\.deltaship\bin
```

### Method 3: Build from Source

**Clone Repository:**

```
git clone https://github.com/deltaship/publisher-toolkit.git
cd publisher-toolkit
```

**Build:**

```
cargo build --release
```

**Install:**

```
cargo install --path .
```

**Verify:**

```
deltaship-init --version
```

---

## Initial Setup

### Step 1: Initialize Project

Navigate to your application's build directory and initialize Deltaship:

```
cd /path/to/your/app
deltaship-init --app-name "MyApp" --update-server "https://updates.example.com"
```

**Interactive prompts:**
```
Application name: MyApp
Update server URL: https://updates.example.com
Generate signing key pair? (y/n): y
Binary location pattern (glob): ./bin/myapp
```

**What this creates:**
- `.deltaship/` directory in your project root
- `config.toml` - Publisher configuration
- `versions.db` - SQLite database tracking versions
- `keys/` directory with Ed25519 key pair
  - `private.key` - KEEP SECRET! Used to sign updates
  - `public.key` - Distribute with clients for verification

**IMPORTANT:** Add to `.gitignore`:
```
.deltaship/keys/private.key
```

Public key should be committed (needed by clients).

### Step 2: Review Configuration

Edit `.deltaship/config.toml`:

```toml
[app]
name = "MyApp"
binary_pattern = "./bin/myapp"
platform = "linux"  # linux, windows, macos
architecture = "x86_64"  # x86_64, arm64

[server]
url = "https://updates.example.com"
api_key = ""  # Will be set later

[diff]
algorithm = "bsdiff"  # bsdiff, courgette, auto
compression = "zstd"  # zstd, gzip, none
max_size_mb = 500  # Fallback to full binary if diff exceeds this

[signing]
private_key_path = ".deltaship/keys/private.key"
public_key_path = ".deltaship/keys/public.key"

[publish]
auto_publish = false  # Set true for CI/CD
dry_run = false  # Set true to test without uploading

[versioning]
version_file = "VERSION"  # File containing current version
version_format = "semver"  # semver, date, custom
```

### Step 3: Obtain API Key

**From Update Server Administrator:**

Contact your update server administrator to obtain an API key for your application.

**Add to configuration:**

```toml
[server]
api_key = "your-api-key-here"
```

**Or use environment variable (recommended for CI/CD):**

```
export DELTASHIP_API_KEY="your-api-key-here"
```

### Step 4: Distribute Public Key

**Embed public key in client application:**

**Option A: Compile-time embedding (recommended)**

Copy public key to your application source:

```
cp .deltaship/keys/public.key src/assets/deltaship-public.key
```

Include in your application binary (compile-time constant).

**Option B: Distribute with installer**

Include `public.key` in your installer package. Client patcher reads it during installation.

**Option C: Hard-code key bytes**

Read public key bytes and hard-code in your application:

```
cat .deltaship/keys/public.key | xxd -i
```

Copy output to your code as a constant array.

---

## Basic Workflow

### Step 1: Build Your Application

Build your application as usual:

```
./build.sh
# Or: cargo build --release
# Or: make
# Or: npm run build
```

Binary produced: `./bin/myapp` (or your configured path)

### Step 2: Register New Version

After building a new version:

```
deltaship-register --version 1.1.0 --binary ./bin/myapp
```

**What this does:**
- Reads binary file
- Computes Blake3 hash
- Records version in `versions.db`
- Generates diffs from recent versions (if any)
- Stores diffs in `.deltaship/diffs/`

**Output:**
```
Registering version 1.1.0...
Binary: ./bin/myapp (85.4 MB)
Hash: blake3:a1b2c3d4...

Generating diffs from recent versions:
  1.0.0 → 1.1.0: 1.2 MB (98.6% reduction)
  1.0.1 → 1.1.0: 800 KB (99.1% reduction)

Version 1.1.0 registered successfully.
```

### Step 3: Sign the Version

Cryptographically sign the version:

```
deltaship-sign --version 1.1.0
```

**What this does:**
- Computes signature over:
  - Binary hash
  - All diff hashes
  - Version metadata
  - Timestamp
- Uses private key (Ed25519)
- Stores signatures in `.deltaship/signatures/`

**Output:**
```
Signing version 1.1.0...
Using private key: .deltaship/keys/private.key

Signed:
  - Binary (myapp-1.1.0): signature created
  - Diff 1.0.0→1.1.0: signature created
  - Diff 1.0.1→1.1.0: signature created

Version 1.1.0 signed successfully.
Signature file: .deltaship/signatures/1.1.0.sig
```

### Step 4: Test Locally

Test the update package before publishing:

```
deltaship-test --from 1.0.0 --to 1.1.0
```

**What this does:**
- Simulates update process locally
- Applies diff to old binary (1.0.0)
- Verifies signature
- Checks result matches expected hash
- Reports success/failure

**Output:**
```
Testing update: 1.0.0 → 1.1.0...

1. Loading old binary (1.0.0): ✓
2. Loading diff: ✓ (1.2 MB)
3. Applying diff: ✓ (8.3 seconds)
4. Verifying signature: ✓
5. Checking hash: ✓

Test PASSED
Resulting binary matches expected hash for version 1.1.0
```

### Step 5: Publish to Update Server

Publish the version to update server:

```
deltaship-publish --version 1.1.0
```

**What this does:**
- Uploads binary to update server
- Uploads all diffs
- Uploads signatures
- Uploads metadata (version info, changelog)
- Registers version in server database

**Output:**
```
Publishing version 1.1.0 to https://updates.example.com...

Uploading:
  [1/5] Binary (myapp-1.1.0.bin): 85.4 MB... ✓ (15s)
  [2/5] Diff (1.0.0→1.1.0): 1.2 MB... ✓ (1s)
  [3/5] Diff (1.0.1→1.1.0): 800 KB... ✓ (1s)
  [4/5] Signatures: 15 KB... ✓ (0.5s)
  [5/5] Metadata: 2 KB... ✓ (0.3s)

Version 1.1.0 published successfully!

Users on versions 1.0.0, 1.0.1 will receive differential updates.
Users on other versions will download full binary.
```

**Dry-run option (test without uploading):**

```
deltaship-publish --version 1.1.0 --dry-run
```

---

## Advanced Usage

### Multi-Platform Builds

If you build for multiple platforms:

**Directory structure:**
```
./builds/
  linux/myapp
  windows/myapp.exe
  macos/myapp
```

**Register each platform separately:**

```
deltaship-register --version 1.1.0 --binary ./builds/linux/myapp --platform linux
deltaship-register --version 1.1.0 --binary ./builds/windows/myapp.exe --platform windows
deltaship-register --version 1.1.0 --binary ./builds/macos/myapp --platform macos
```

**Sign all:**

```
deltaship-sign --version 1.1.0 --all-platforms
```

**Publish all:**

```
deltaship-publish --version 1.1.0 --all-platforms
```

### Changelog Integration

**Include changelog in metadata:**

Create `CHANGELOG-1.1.0.md`:
```markdown
# Version 1.1.0

## New Features
- Added dark mode
- Improved performance by 25%

## Bug Fixes
- Fixed crash on startup (#123)
- Fixed memory leak (#456)
```

**Publish with changelog:**

```
deltaship-publish --version 1.1.0 --changelog CHANGELOG-1.1.0.md
```

Clients will display changelog when notifying users of available update.

### Gradual Rollout

**Publish with rollout configuration:**

```
deltaship-publish --version 1.1.0 --rollout-percentage 10
```

Only 10% of users will initially receive update. Increase over time:

```
deltaship-publish --version 1.1.0 --rollout-percentage 25
deltaship-publish --version 1.1.0 --rollout-percentage 50
deltaship-publish --version 1.1.0 --rollout-percentage 100  # Full rollout
```

**Automatic gradual rollout (recommended):**

Configure in `config.toml`:
```toml
[rollout]
enabled = true
initial_percentage = 10
increase_interval_hours = 24
increase_amount = 10
```

Server will automatically increase rollout percentage every 24 hours by 10%.

### Rollback

If critical bug discovered in 1.1.0:

**Option 1: Pause rollout**

```
deltaship-rollback --version 1.1.0 --action pause
```

Stops new users from receiving 1.1.0, but doesn't downgrade existing users.

**Option 2: Full rollback**

```
deltaship-rollback --version 1.1.0 --action rollback --rollback-to 1.0.1
```

- Pauses 1.1.0 rollout
- Tells users on 1.1.0 to downgrade to 1.0.1
- Generates reverse diff (1.1.0 → 1.0.1) if not already available

**Option 3: Publish fixed version**

Build and publish 1.1.1 with fix:
```
deltaship-register --version 1.1.1 --binary ./bin/myapp
deltaship-sign --version 1.1.1
deltaship-publish --version 1.1.1
```

Users on 1.1.0 will receive 1.1.0 → 1.1.1 diff update.

---

## CI/CD Integration

### GitHub Actions

Create `.github/workflows/publish-release.yml`:

```yaml
name: Publish Release

on:
  release:
    types: [published]

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install Deltaship Publisher Toolkit
        run: |
          wget https://releases.deltaship.io/publisher/deltaship-publisher-toolkit_1.0.0_amd64.deb
          sudo dpkg -i deltaship-publisher-toolkit_1.0.0_amd64.deb

      - name: Build Application
        run: |
          ./build.sh

      - name: Register Version
        run: |
          deltaship-register --version ${{ github.ref_name }} --binary ./bin/myapp

      - name: Sign Version
        env:
          DELTASHIP_PRIVATE_KEY: ${{ secrets.DELTASHIP_PRIVATE_KEY }}
        run: |
          echo "$DELTASHIP_PRIVATE_KEY" > .deltaship/keys/private.key
          deltaship-sign --version ${{ github.ref_name }}

      - name: Publish to Update Server
        env:
          DELTASHIP_API_KEY: ${{ secrets.DELTASHIP_API_KEY }}
        run: |
          deltaship-publish --version ${{ github.ref_name }}
```

**Set GitHub Secrets:**
- `DELTASHIP_PRIVATE_KEY` - Your signing private key
- `DELTASHIP_API_KEY` - Update server API key

### GitLab CI

Create `.gitlab-ci.yml`:

```yaml
stages:
  - build
  - publish

build:
  stage: build
  script:
    - ./build.sh
  artifacts:
    paths:
      - bin/myapp

publish:
  stage: publish
  only:
    - tags
  before_script:
    - apt-get update && apt-get install -y wget
    - wget https://releases.deltaship.io/publisher/deltaship-publisher-toolkit_1.0.0_amd64.deb
    - dpkg -i deltaship-publisher-toolkit_1.0.0_amd64.deb
  script:
    - deltaship-register --version $CI_COMMIT_TAG --binary ./bin/myapp
    - echo "$DELTASHIP_PRIVATE_KEY" > .deltaship/keys/private.key
    - deltaship-sign --version $CI_COMMIT_TAG
    - deltaship-publish --version $CI_COMMIT_TAG
  variables:
    DELTASHIP_API_KEY: $DELTASHIP_API_KEY
    DELTASHIP_PRIVATE_KEY: $DELTASHIP_PRIVATE_KEY
```

**Set GitLab CI/CD Variables:**
- `DELTASHIP_PRIVATE_KEY` (masked)
- `DELTASHIP_API_KEY` (masked)

### Jenkins

Create `Jenkinsfile`:

```groovy
pipeline {
    agent any

    environment {
        DELTASHIP_API_KEY = credentials('deltaship-api-key')
        DELTASHIP_PRIVATE_KEY = credentials('deltaship-private-key')
    }

    stages {
        stage('Build') {
            steps {
                sh './build.sh'
            }
        }

        stage('Publish Release') {
            when {
                tag "v*"
            }
            steps {
                sh '''
                    deltaship-register --version ${TAG_NAME} --binary ./bin/myapp
                    echo "$DELTASHIP_PRIVATE_KEY" > .deltaship/keys/private.key
                    deltaship-sign --version ${TAG_NAME}
                    deltaship-publish --version ${TAG_NAME}
                '''
            }
        }
    }
}
```

---

## Statistics and Analytics

### View Update Statistics

```
deltaship-stats --version 1.1.0
```

**Output:**
```
Version 1.1.0 Statistics
========================

Published: 2026-01-07 10:30:00 UTC
Rollout Status: 100% (full rollout)

Installations:
  Total users: 10,543
  - Updated to 1.1.0: 9,852 (93.4%)
  - Still on older versions: 691 (6.6%)

Update Methods:
  - Differential updates: 9,234 (93.7%)
  - Full downloads: 618 (6.3%)

Bandwidth Savings:
  - Total: 823.7 GB saved
  - Average per user: 84.2 MB saved (98.6% reduction)

Update Success Rate: 99.2%
  - Successful: 9,775
  - Failed: 77
  - Common errors:
    - Signature verification failed: 35
    - Network timeout: 28
    - Disk space insufficient: 14

Rollout Timeline:
  Day 1 (10%):  1,054 users → 99.1% success
  Day 2 (25%):  2,636 users → 99.3% success
  Day 3 (50%):  5,271 users → 99.4% success
  Day 4 (100%): 10,543 users → 99.2% success
```

### Analyze Bandwidth Savings

```
deltaship-analyze --from 1.0.0 --to 1.1.0
```

**Output:**
```
Diff Analysis: 1.0.0 → 1.1.0
=============================

Binary Sizes:
  Old version (1.0.0): 83.2 MB
  New version (1.1.0): 85.4 MB
  Size increase: 2.2 MB (+2.6%)

Diff Size: 1.2 MB
Compression Ratio: 98.6%
Bandwidth Saving: 84.2 MB per user

Estimated Savings (for 10,000 users):
  Traditional (full download): 854 GB
  Deltaship (differential): 12 GB
  Bandwidth saved: 842 GB (98.6%)
  Cost saved: $84.20 (at $0.10/GB)

Diff Generation:
  Algorithm: bsdiff
  Generation time: 45.2 seconds
  Compression: zstd (level 19)

Patch Application:
  Average time: 8.3 seconds
  Memory usage: 95 MB peak
  CPU usage: 42% (single core)
```

---

## Best Practices

### Version Numbering

**Use Semantic Versioning (recommended):**
- Major.Minor.Patch format (e.g., 1.2.3)
- Increment major for breaking changes
- Increment minor for new features
- Increment patch for bug fixes

**Benefits:**
- Clear communication of change severity
- Users understand update importance
- Easier to manage backward compatibility

### Signing Key Management

**Security:**
- NEVER commit private key to version control
- Store private key encrypted (e.g., with `gpg`)
- Use different keys for development vs production
- Rotate keys annually

**Backup:**
- Keep encrypted backup of private key in secure location
- Document key recovery procedure
- Test key recovery process

**Key Rotation:**

When rotating keys:

1. Generate new key pair:
   ```
   deltaship-init --generate-keys-only --key-path ./new-keys/
   ```

2. Sign upcoming releases with both old and new keys (transition period)

3. Distribute new public key to clients (in next update)

4. After transition period, sign only with new key

### Testing Before Publishing

**Always test locally:**

```
deltaship-test --from previous-version --to new-version
```

**Test on staging server first:**

Configure staging server in `.deltaship/config.toml`:
```toml
[server.staging]
url = "https://updates-staging.example.com"
api_key = "staging-api-key"
```

Publish to staging:
```
deltaship-publish --version 1.1.0 --environment staging
```

**Test with real clients:**
- Configure test clients to use staging server
- Verify updates apply correctly
- Monitor for errors

### Changelog Best Practices

**Include:**
- New features (user-facing)
- Bug fixes (critical ones)
- Breaking changes (if any)
- Security fixes (always mention)

**Avoid:**
- Internal refactoring (users don't care)
- Developer-only changes
- Too much technical detail

**Example:**

Good:
```
## Version 1.1.0

- Added dark mode (Settings → Appearance)
- Fixed crash when importing large files
- Security: Fixed XSS vulnerability in settings page
```

Bad:
```
## Version 1.1.0

- Refactored database layer
- Updated dependency X from 1.2 to 1.3
- Fixed typo in comments
```

---

## Troubleshooting

### Common Issues

**Issue: "Failed to generate diff"**

**Possible causes:**
- Insufficient disk space
- Binary too large (>2 GB)
- Corrupted old binary

**Solutions:**
- Free up disk space
- Split binary into multiple parts (advanced)
- Re-register old version

**Issue: "Signature verification failed" during test**

**Possible causes:**
- Private key mismatch (wrong key used)
- Corrupted signature file
- Binary modified after signing

**Solutions:**
- Verify using correct private key
- Re-sign version
- Ensure binary not modified between register and sign

**Issue: "Upload failed" during publish**

**Possible causes:**
- Network connectivity
- Invalid API key
- Server not reachable
- Insufficient permissions

**Solutions:**
- Check internet connection
- Verify API key in configuration
- Test server URL: `curl https://updates.example.com/health`
- Contact server administrator

---

## Migration from Traditional Updates

### Phased Migration

**Phase 1: Parallel Distribution**
- Continue traditional downloads
- Add Deltaship support (test with subset of users)
- Distribute client patcher
- Monitor adoption

**Phase 2: Gradual Shift**
- Make Deltaship default for new installations
- Encourage existing users to install client patcher
- Maintain traditional downloads as fallback

**Phase 3: Full Migration**
- Deltaship becomes primary distribution method
- Traditional downloads deprecated (or removed)
- 90%+ users on Deltaship

**Timeline:** 6-12 months for complete migration

---

## Next Steps

**For First-Time Publishers:**
1. Complete initial setup (above)
2. Build and register first version
3. Sign and test version
4. Publish to staging (if available)
5. Publish to production

**For Production Use:**
- Set up CI/CD integration
- Configure gradual rollout
- Monitor statistics dashboard
- Plan key rotation schedule

**For More Information:**
- Read: [Complete Flow](../COMPLETE_FLOW.md)
- Read: [Publisher Toolkit Details](../tools/PUBLISHER_TOOLKIT.md)
- Read: [Security Model](../security/SECURITY_MODEL.md)

---

**End of Publisher Setup Guide**
