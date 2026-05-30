# Release Process

This document describes the release process for VBDP (Versioned Binary Delta Patcher).

## Overview

VBDP uses automated GitHub Actions workflows to build, test, and release binaries for multiple platforms. Releases are triggered by pushing version tags to the repository.

## Release Types

### Semantic Versioning

VBDP follows [Semantic Versioning 2.0.0](https://semver.org/):

- **MAJOR** version (X.0.0): Incompatible API changes
- **MINOR** version (0.X.0): New functionality, backwards compatible
- **PATCH** version (0.0.X): Bug fixes, backwards compatible

### Pre-release Versions

Pre-release versions use suffixes:

- **Alpha**: `v0.2.0-alpha.1` - Early testing, unstable
- **Beta**: `v0.2.0-beta.1` - Feature complete, needs testing
- **Release Candidate**: `v0.2.0-rc.1` - Final testing before release

## Release Checklist

### Pre-Release (1-2 weeks before)

- [ ] **Review milestone** - Ensure all planned features/fixes are complete
- [ ] **Update dependencies** - `cargo update` and review changes
- [ ] **Run security audit** - `cargo audit --deny warnings`
- [ ] **Update CHANGELOG.md** - Document all changes since last release
- [ ] **Update version numbers** in:
  - [ ] `Cargo.toml` (workspace version)
  - [ ] All crate `Cargo.toml` files
  - [ ] Documentation if needed
- [ ] **Review documentation** - Ensure docs match current functionality
- [ ] **Test on all platforms**:
  - [ ] Linux x86_64
  - [ ] macOS (Intel and Apple Silicon)
  - [ ] Windows x86_64
- [ ] **Run full test suite** - `cargo test --all-features --workspace`
- [ ] **Check code coverage** - Ensure no regressions
- [ ] **Performance benchmarks** - Compare with previous version
- [ ] **Review open issues** - Close fixed issues, postpone others

### Release Day

1. **Final Verification**
   ```bash
   # Clean build
   cargo clean

   # Build all targets
   cargo build --all-features --workspace --release

   # Run all tests
   cargo test --all-features --workspace --verbose

   # Check formatting
   cargo fmt --all -- --check

   # Run clippy
   cargo clippy --all-features --workspace -- -D warnings

   # Dry-run publish
   cargo publish --dry-run -p vbdp-core
   # (repeat for all crates)
   ```

2. **Create and Push Tag**
   ```bash
   # Ensure you're on main branch
   git checkout main
   git pull origin main

   # Create annotated tag
   git tag -a v0.1.0 -m "Release v0.1.0"

   # Push tag to trigger release workflow
   git push origin v0.1.0
   ```

3. **Monitor Release Workflow**
   <!-- TODO: Update URL when repository is published -->
   - Go to GitHub Actions: `https://github.com/jayashankarvr/vbdp/actions`
   - Watch the "Release" workflow
   - Ensure all jobs pass:
     - ✅ Test Suite
     - ✅ Build (all platforms)
     - ✅ Create Release
     - ✅ Publish to crates.io (dry-run)

4. **Verify Release Artifacts**
   <!-- TODO: Update URL when repository is published -->
   - Go to GitHub Releases: `https://github.com/jayashankarvr/vbdp/releases`
   - Verify all platform binaries are present:
     - `vbdp-client-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`
     - `vbdp-client-v0.1.0-x86_64-unknown-linux-musl.tar.gz`
     - `vbdp-client-v0.1.0-x86_64-apple-darwin.tar.gz`
     - `vbdp-client-v0.1.0-aarch64-apple-darwin.tar.gz`
     - `vbdp-client-v0.1.0-x86_64-pc-windows-msvc.zip`
     - (Same for `vbdp-publisher` and `vbdp-server`)
   - Verify `SHA256SUMS.txt` is present
   - Download and verify checksums:
     ```bash
     # TODO: Update URL when repository is published
     wget https://github.com/jayashankarvr/vbdp/releases/download/v0.1.0/SHA256SUMS.txt
     shasum -a 256 -c SHA256SUMS.txt
     ```

### Post-Release

1. **Test Release Binaries**
   ```bash
   # Download and test on each platform
   # TODO: Update URL when repository is published
   wget https://github.com/jayashankarvr/vbdp/releases/download/v0.1.0/vbdp-client-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
   tar xzf vbdp-client-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
   ./vbdp-client --version
   ./vbdp-client --help
   ```

2. **Update Documentation Sites** (if applicable)
   - Update installation instructions with new version
   - Update any version-specific documentation
   - Regenerate API docs if published

3. **Publish to crates.io** (when ready)

   **Note**: Currently set to dry-run. To publish for real:

   a. Obtain crates.io API token:
      - Go to https://crates.io/settings/tokens
      - Create new token with "publish-update" scope
      - Add to GitHub secrets as `CRATES_IO_TOKEN`

   b. Update `.github/workflows/release.yml`:
      - Uncomment the actual publish steps
      - Ensure dependency order is correct:
        1. `vbdp-core`
        2. `vbdp-crypto`
        3. `vbdp-diff`
        4. `vbdp-db`
        5. `vbdp-publisher`
        6. `vbdp-server`
        7. `vbdp-client`

   c. Add delays between publishes:
      ```yaml
      - name: Publish vbdp-core
        run: cargo publish -p vbdp-core --token ${{ secrets.CRATES_IO_TOKEN }}

      - name: Wait for crates.io
        run: sleep 30

      - name: Publish vbdp-crypto
        run: cargo publish -p vbdp-crypto --token ${{ secrets.CRATES_IO_TOKEN }}
      ```

4. **Announcement**
   - [ ] Update README.md with latest version
   - [ ] Post release announcement:
     - [ ] GitHub Discussions
     - [ ] Twitter/X
     - [ ] Reddit (r/rust)
     - [ ] Discord/Slack communities
     - [ ] Blog post (if applicable)
   - [ ] Send notification to users/stakeholders

5. **Create Next Milestone**
   - Create GitHub milestone for next version
   - Move postponed issues to new milestone
   - Plan features for next release

## Release Workflow Details

The release workflow (`.github/workflows/release.yml`) performs:

### 1. Test Suite Job
- Runs full test suite on Linux
- Runs clippy with deny warnings
- Checks code formatting
- Blocks release if any checks fail

### 2. Build Job (Matrix)
Builds binaries for all supported platforms:

| Platform | Target | Static | Notes |
|----------|--------|--------|-------|
| Linux x86_64 | `x86_64-unknown-linux-gnu` | No | Requires glibc |
| Linux x86_64 | `x86_64-unknown-linux-musl` | Yes | Standalone binary |
| macOS x86_64 | `x86_64-apple-darwin` | No | Intel Macs |
| macOS ARM64 | `aarch64-apple-darwin` | No | Apple Silicon |
| Windows x86_64 | `x86_64-pc-windows-msvc` | No | MSVC toolchain |

For each platform:
- Builds release binaries with optimizations
- Creates archives (`.tar.gz` or `.zip`)
- Generates SHA256 checksums
- Uploads artifacts

### 3. Release Job
- Downloads all platform artifacts
- Combines checksums into `SHA256SUMS.txt`
- Generates release notes from `CHANGELOG.md`
- Creates GitHub release with all artifacts
- Marks as pre-release if version contains `alpha`, `beta`, or `rc`

### 4. Publish Job (Dry-Run)
- Validates all crates can be published
- Currently runs `cargo publish --dry-run`
- Does not actually publish to crates.io

### 5. Notify Job
- Creates summary of release
- Lists post-release tasks

## Manual Release (Emergency)

If automated release fails, you can create a manual release:

### Build Binaries Locally

```bash
# Install cross-compilation tools
rustup target add x86_64-unknown-linux-musl
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
rustup target add x86_64-pc-windows-msvc

# Build for each target
cargo build --release --target x86_64-unknown-linux-gnu
cargo build --release --target x86_64-unknown-linux-musl
# (continue for other targets)

# Create archives
cd target/x86_64-unknown-linux-gnu/release
tar czf vbdp-client-v0.1.0-x86_64-unknown-linux-gnu.tar.gz vbdp-client

# Generate checksums
shasum -a 256 *.tar.gz *.zip > SHA256SUMS.txt
```

### Create GitHub Release Manually

<!-- TODO: Update URL when repository is published -->
1. Go to: `https://github.com/jayashankarvr/vbdp/releases/new`
2. Choose tag: Select or create `v0.1.0`
3. Release title: `Release v0.1.0`
4. Description: Copy from `CHANGELOG.md`
5. Upload artifacts:
   - All platform archives
   - `SHA256SUMS.txt`
6. Click "Publish release"

## Hotfix Release Process

For critical bugs in production:

1. **Create Hotfix Branch**
   ```bash
   git checkout v0.1.0
   git checkout -b hotfix/0.1.1
   ```

2. **Fix the Bug**
   ```bash
   # Make minimal changes to fix the issue
   git commit -m "Fix critical bug in XYZ"
   ```

3. **Test Thoroughly**
   ```bash
   cargo test --all-features --workspace
   ```

4. **Update Version**
   - Bump PATCH version: `0.1.0` → `0.1.1`
   - Update `CHANGELOG.md`

5. **Merge and Release**
   ```bash
   git checkout main
   git merge hotfix/0.1.1
   git tag -a v0.1.1 -m "Hotfix release v0.1.1"
   git push origin main v0.1.1
   ```

## Rollback a Release

If a release has critical issues:

### 1. Mark Release as Pre-release
- Edit the GitHub release
- Check "This is a pre-release"
- Add warning to release notes

### 2. Create Hotfix or Revert
- Create hotfix release (preferred)
- OR revert commits and create new release

### 3. Do NOT Delete Release
- Deleting releases breaks existing installations
- Users may have already downloaded binaries
- Keep for historical record

### 4. Communicate
- Update release notes with warning
- Post announcement about the issue
- Provide migration path to fixed version

## Version Branching Strategy

### main Branch
- Always contains latest stable code
- All development merges here
- Protected: requires PR and reviews

### Release Tags
- Tags mark specific releases: `v0.1.0`, `v0.2.0`
- Never delete or move tags
- Use annotated tags: `git tag -a v0.1.0 -m "Release v0.1.0"`

### Hotfix Branches
- Created from release tags
- Merged back to main
- Named: `hotfix/X.Y.Z`

## Troubleshooting Releases

### Release Workflow Fails at Test Step
- **Cause**: Tests failing on CI
- **Fix**: Fix tests locally, push fix, delete and recreate tag

### Build Fails for Specific Platform
- **Cause**: Platform-specific compilation error
- **Fix**:
  - Check matrix configuration
  - Ensure all targets are installable
  - Test cross-compilation locally

### Checksums Don't Match
- **Cause**: Binary modified after checksum generation
- **Fix**: Re-run workflow, ensure no local modifications

### GitHub Release Not Created
- **Cause**: Missing `GITHUB_TOKEN` permission
- **Fix**: Ensure workflow has `contents: write` permission

### Artifacts Missing from Release
- **Cause**: Upload failed or wrong path
- **Fix**: Check `upload-artifact` and `download-artifact` steps

## Security Considerations

### Signing Releases

**Future Enhancement**: Add GPG signing of releases

```bash
# Generate GPG key
gpg --full-generate-key

# Sign tag
git tag -s v0.1.0 -m "Release v0.1.0"

# Verify signature
git tag -v v0.1.0

# Sign binaries
gpg --detach-sign --armor vbdp-client-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

### Supply Chain Security

- All dependencies are vendored and audited
- `cargo-audit` runs in CI
- Checksums provided for all artifacts
- Reproducible builds (future goal)

## Related Documentation

- [CHANGELOG.md](../CHANGELOG.md) - Release history
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Development workflow
- [SECURITY.md](../SECURITY.md) - Security policy
- [DATABASE_MIGRATIONS.md](./DATABASE_MIGRATIONS.md) - Database migrations

## Release History

| Version | Date | Type | Notes |
|---------|------|------|-------|
| v0.1.0 | TBD | Initial | First public release |

## Support

For release-related questions:

1. Check this document
2. Review [FAQ.md](./FAQ.md)
3. Search GitHub Issues
4. File new issue with `release` label
