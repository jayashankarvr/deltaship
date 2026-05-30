---
name: Bug Report
about: Report a bug or unexpected behavior in Deltaship
title: '[BUG] '
labels: 'bug, needs-triage'
assignees: ''
---

## Bug Description

**Clear, concise description of the bug:**
[Describe what happened]

---

## To Reproduce

**Steps to reproduce the behavior:**

1. [First step]
2. [Second step]
3. [Third step]
4. [See error]

**Example command or code (if applicable):**
```bash
deltaship-client update --binary myapp
```

---

## Expected Behavior

**What you expected to happen:**
[Clear description of expected behavior]

---

## Actual Behavior

**What actually happened:**
[Clear description of actual behavior]

**Error Message (if any):**
```
[Paste full error message here]
```

**Logs (if available):**
```
[Paste relevant log excerpts]
```

---

## Environment

**Deltaship Version:**
- Component: [Server / Client / Publisher]
- Version: [e.g., 1.0.0]
- Installation method: [e.g., cargo install, deb package, from source]

**Operating System:**
- OS: [e.g., Ubuntu 22.04, Windows 11, macOS 13.0]
- Architecture: [e.g., x86_64, aarch64]
- Kernel version: [e.g., 5.15.0] (Linux only)

**Rust Version (if building from source):**
- Rust version: [e.g., 1.70.0]
- Cargo version: [e.g., 1.70.0]

**Database (if server-related):**
- Database: [e.g., PostgreSQL 14.5, SQLite 3.40]
- Database version: [version]

**Storage (if server-related):**
- Storage backend: [e.g., S3, MinIO, local filesystem]
- Storage version/provider: [e.g., AWS S3, MinIO 2023.11]

---

## Additional Context

**Configuration files (redact secrets!):**
```toml
# Paste relevant config sections (remove API keys, passwords, etc.)
```

**Binary information (if relevant):**
- Binary size: [e.g., 50MB]
- Binary type: [e.g., ELF executable, PE executable, dynamic library]
- From version: [e.g., 1.0.0]
- To version: [e.g., 1.0.1]

**Screenshots (if applicable):**
[Attach screenshots showing the issue]

**Related issues:**
- Related to #[issue number]
- Possibly related to #[issue number]

---

## Possible Solution

**If you have ideas about what might be causing this or how to fix it:**
[Your analysis or suggestions]

---

## Impact

**How does this bug affect you?**

- [ ] Blocks critical functionality (cannot use Deltaship)
- [ ] Blocks non-critical functionality (workaround available)
- [ ] Inconvenience (minor issue)
- [ ] Cosmetic (doesn't affect functionality)

**How many users are affected?**
- [ ] Just me
- [ ] Small number of users (specific configuration)
- [ ] Many users (common scenario)
- [ ] All users

---

## Checklist

Before submitting, please verify:

- [ ] I have searched existing issues to avoid duplicates
- [ ] I have included version information
- [ ] I have included steps to reproduce
- [ ] I have redacted any sensitive information (API keys, passwords, private data)
- [ ] I have included relevant logs or error messages
- [ ] This is not a security vulnerability (if security-related, email security@deltaship.example.com instead)

---

## For Maintainers

**Do not fill this section - for maintainer use only**

**Triage:**
- [ ] Bug confirmed
- [ ] Cannot reproduce
- [ ] Need more information
- [ ] Duplicate of #

**Priority:**
- [ ] P0 - Critical (security, data loss, complete breakage)
- [ ] P1 - High (major functionality broken)
- [ ] P2 - Medium (minor functionality broken)
- [ ] P3 - Low (cosmetic, enhancement)

**Component:**
- [ ] Server
- [ ] Client
- [ ] Publisher
- [ ] Documentation
- [ ] Other:

**Assigned to:** @
**Target version:**
**Fixed in:** #PR
