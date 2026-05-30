# Contributing to VBDP

> **NOTE:** Email addresses on this page use the RFC 2606 `example.com` domain and are placeholders. Replace with real contacts before publishing.

Thank you for your interest in contributing to the Version-Aware Binary Differential Update System (VBDP)!

This document provides guidelines for contributing to the project. Currently, VBDP is in the **design and documentation phase**. Contributions are welcome in the form of:

- Documentation improvements
- Design feedback and suggestions
- Identifying issues or inconsistencies
- Proposing features or enhancements
- Creating proof-of-concept implementations

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How Can I Contribute?](#how-can-i-contribute)
- [Documentation Contributions](#documentation-contributions)
- [Code Contributions (Future)](#code-contributions-future)
- [Reporting Issues](#reporting-issues)
- [Suggesting Enhancements](#suggesting-enhancements)
- [Development Setup](#development-setup)
- [Style Guides](#style-guides)
- [Community](#community)

---

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code. Please report unacceptable behavior to [conduct@vbdp.example.com](mailto:conduct@vbdp.example.com).

---

## How Can I Contribute?

### Current Phase: Documentation

Since VBDP is currently in design phase, the most valuable contributions are:

1. **Review Documentation:** Read through the specs and identify gaps, inconsistencies, or unclear sections
2. **Propose Improvements:** Suggest better designs, algorithms, or architectural choices
3. **Add Examples:** Create walkthroughs, diagrams, or use-case scenarios
4. **Fix Errors:** Correct typos, broken links, or factual errors
5. **Translate:** Help translate documentation to other languages (future)

---

## Documentation Contributions

### Making Changes

1. **Fork the Repository**

   ```bash
   # TODO: Update URL when repository is published
   git clone https://github.com/jayashankarvr/vbdp-docs.git
   cd vbdp-docs
   ```

2. **Create a Branch**

   ```bash
   git checkout -b docs/improve-client-patcher-spec
   ```

3. **Make Your Changes**
   - Edit markdown files in your preferred editor
   - Follow the [Documentation Style Guide](#documentation-style-guide)
   - Verify links are not broken
   - Preview changes locally (use a markdown viewer)

4. **Commit Your Changes**

   ```bash
   git add .
   git commit -m "docs: improve client patcher specification

   - Added missing rollback scenarios
   - Fixed broken links to API spec
   - Clarified signature verification process"
   ```

   **Commit Message Format:**

   ```
   <type>: <subject>

   <body>

   <footer>
   ```

   **Types:**
   - `docs:` Documentation changes
   - `fix:` Bug fixes in existing docs
   - `feat:` New documentation sections
   - `refactor:` Restructuring without changing meaning
   - `style:` Formatting, typos
   - `test:` Adding validation or checks

5. **Push and Create Pull Request**

   ```bash
   git push origin docs/improve-client-patcher-spec
   ```

   Then open a pull request on GitHub with:
   - **Title:** Clear, concise description
   - **Description:** What changed and why
   - **Related Issues:** Link any related issues

### Documentation Review Process

1. **Initial Review:** Maintainer reviews within 3-5 days
2. **Feedback:** Suggestions for improvements
3. **Revision:** Make requested changes
4. **Approval:** At least one maintainer approval required
5. **Merge:** Maintainer merges the PR

---

## Code Contributions (Future)

Once implementation begins, code contributions will be welcome for:

- Publisher Toolkit (Rust)
- Update Server (Rust)
- Client Patcher (Rust for Linux/macOS, C#/.NET for Windows)
- SDKs (JavaScript, Python, Go)

Guidelines for code contributions will be added when implementation starts. Expected:

- **Language:** Primarily Rust (stable channel)
- **Testing:** Unit tests required, integration tests encouraged
- **Code Style:** `rustfmt` with project configuration
- **Linting:** `clippy` with no warnings

---

## Reporting Issues

### Security Issues

**DO NOT** open public issues for security vulnerabilities. Instead:

1. Email <security@vbdp.example.com> with details
2. Include: Detailed description, steps to reproduce, potential impact
3. Allow 90 days for fix before public disclosure (coordinated disclosure)

See [SECURITY.md](SECURITY.md) for full policy.

### Bugs and Problems

Open an issue on GitHub with:

**Title:** Clear, specific description
**Example:** "Broken link in CLIENT_PATCHER.md section 5.3"

**Description Template:**

```markdown
## Description
[Clear description of the problem]

## Location
- File: `path/to/file.md`
- Section: "Rollback Logic"
- Line: 234

## Expected
[What you expected to see]

## Actual
[What you actually see]

## Screenshots (if applicable)
[Attach screenshots]

## Additional Context
[Any other relevant information]
```

---

## Suggesting Enhancements

We welcome suggestions for improving VBDP! Open an issue with:

**Title:** Start with "Enhancement:" or "Feature Request:"
**Example:** "Enhancement: Add support for multi-hop diff paths"

**Description Template:**

```markdown
## Summary
[One-paragraph summary of the enhancement]

## Motivation
[Why is this enhancement needed? What problem does it solve?]

## Proposed Solution
[Your suggested approach]

## Alternatives Considered
[Other approaches you've thought about]

## Additional Context
[Links to related issues, similar implementations, etc.]
```

---

## Development Setup

### Documentation Development

**Requirements:**

- Git
- Markdown editor (VS Code, Typora, or any text editor)
- Markdown linter (optional but recommended)

**Tools:**

- **Link Checker:** Verify no broken links

  ```bash
  npm install -g markdown-link-check
  markdown-link-check **/*.md
  ```

- **Spell Checker:** Catch typos

  ```bash
  npm install -g markdown-spellcheck
  mdspell **/*.md --report
  ```

- **Linter:** Enforce consistent style

  ```bash
  npm install -g markdownlint-cli
  markdownlint **/*.md
  ```

### Future: Code Development

**Requirements:**

- Rust 1.70+ and Cargo
- Git 2.20+
- Platform-specific: Build tools for your OS

**Setup:**

```bash
# TODO: Update URL when repository is published
git clone https://github.com/jayashankarvr/vbdp.git
cd vbdp
cargo build
cargo test
```

---

## Style Guides

### Documentation Style Guide

**General:**

- Use clear, concise language
- Write for international audience (avoid idioms)
- Use present tense ("is" not "will be")
- Use active voice ("the server processes" not "is processed by")

**Formatting:**

- Headers: Use sentence case ("Installing the client" not "Installing The Client")
- Code blocks: Always specify language for syntax highlighting
- Lists: Use bullet points for unordered, numbers for steps
- Links: Use descriptive text, not "click here"

**Structure:**

- **Overview:** Brief summary at top of document
- **Table of Contents:** For documents >3 pages
- **Sections:** Use headers (##, ###) hierarchically
- **Examples:** Include concrete examples where possible

**Technical Writing:**

- Define acronyms on first use
- Use consistent terminology (see GLOSSARY.md)
- Include "Audience:" header at top of technical docs
- Link to related documents at bottom

**Example Good Documentation:**

```markdown
## Installation

To install the VBDP client patcher on Ubuntu:

1. **Download the package:**
   \`\`\`bash
   wget https://releases.vbdp.io/client/vbdp-client_1.0.0_amd64.deb
   \`\`\`

2. **Install:**
   \`\`\`bash
   sudo dpkg -i vbdp-client_1.0.0_amd64.deb
   \`\`\`

3. **Verify:**
   \`\`\`bash
   systemctl status vbdp
   \`\`\`

   Expected output: "active (running)"
```

### Code Style Guide (Future)

**Rust:**

- Follow official Rust style guide
- Run `cargo fmt` before committing
- Zero clippy warnings
- Document all public APIs

**Testing:**

- Unit tests in same file as code (in `#[cfg(test)]` module)
- Integration tests in `tests/` directory
- Minimum 80% code coverage target

---

## Community

### Communication Channels

- **GitHub Discussions:** Q&A, general discussion
- **GitHub Issues:** Bug reports, feature requests
- **Discord:** Real-time chat (link TBD)
- **Mailing List:** Announcements (<vbdp-announce@vbdp.example.com>)

### Getting Help

**Before asking:**

1. Search existing issues and discussions
2. Read relevant documentation
3. Try to debug yourself

**When asking:**

- Provide context (what you're trying to do)
- Include relevant code/config
- Describe what you've already tried
- Be specific and concise

### Recognition

Contributors will be:

- Listed in AUTHORS.md
- Mentioned in release notes (for significant contributions)
- Given credit in published papers/presentations (if applicable)

### Maintainers

Current maintainers:

- [To be determined]

Maintainers review pull requests, triage issues, and guide project direction.

---

## Development Process

### Branches

- `main` - Stable, documentation-only currently
- `develop` - Active development (future, when code starts)
- `feature/*` - Feature branches
- `docs/*` - Documentation branches
- `fix/*` - Bug fix branches

### Pull Request Process

1. **Create PR** with clear title and description
2. **CI Checks:** Automated checks must pass (link checker, spell check, etc.)
3. **Review:** At least one maintainer approval
4. **Merge:** Maintainer merges (squash merge for clean history)

### Release Process (Future)

- Semantic Versioning (MAJOR.MINOR.PATCH)
- Changelog maintained in CHANGELOG.md
- Releases tagged in Git
- Release notes generated from changelog

---

## License

By contributing to VBDP, you agree that your contributions will be licensed under the [MIT License](LICENSE.md).

**Specifically:**

- You have the right to submit the contribution
- You grant us a perpetual, worldwide, non-exclusive, royalty-free license to use your contribution
- Your contribution is provided "as-is" without warranties

---

## Questions?

If you have questions about contributing, feel free to:

- Open a GitHub Discussion
- Email maintainers at [maintainers@vbdp.example.com](mailto:maintainers@vbdp.example.com)
- Join our Discord (link TBD)

---

## Thank You

Your contributions help make VBDP better for everyone. We appreciate your time and effort!

Happy contributing! 🎉
