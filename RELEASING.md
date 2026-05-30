# Releasing

See [`docs/RELEASE_PROCESS.md`](docs/RELEASE_PROCESS.md) for the full release guide.

## Quick reference

Releases are triggered by pushing a version tag. The CI pipeline builds signed binaries for Linux (x86_64, musl), macOS (Intel, ARM), and Windows automatically.

```bash
# Update version in Cargo.toml workspaces, then:
git tag v0.2.0
git push origin v0.2.0
```

GitHub Actions (`release.yml`) then:
1. Runs the full test suite
2. Builds multi-platform binaries
3. Signs artifacts
4. Creates a GitHub Release with all assets

For pre-releases use `v0.2.0-beta.1`, `v0.2.0-rc.1` etc. — the workflow marks them as pre-release automatically.
