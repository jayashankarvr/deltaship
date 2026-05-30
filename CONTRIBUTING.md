# Contributing to VBDP

Thank you for your interest in contributing to VBDP!

## Table of Contents

- [Reporting Bugs](#reporting-bugs)
- [Suggesting Enhancements](#suggesting-enhancements)
- [Development Setup](#development-setup)
- [Building the Project](#building-the-project)
- [Running Tests](#running-tests)
- [Submitting Pull Requests](#submitting-pull-requests)
- [Code Standards](#code-standards)

## Reporting Bugs

1. Check existing issues to avoid duplicates
2. Use the bug report template
3. Include:
   - VBDP version (run `vbdp-client version`, `vbdp-publisher version`, or `vbdp-server --version`)
   - Operating system and version
   - Rust version (`rustc --version`)
   - Steps to reproduce
   - Expected vs actual behavior
   - Relevant logs or error messages

## Suggesting Enhancements

1. Check existing issues and discussions
2. Open an issue with the enhancement label
3. Clearly describe the use case and expected benefits
4. Include examples if possible

## Development Setup

### Prerequisites

- Rust 1.70 or later
- Git
- A C compiler (for SQLite and some cryptographic dependencies)
- Optional: Docker (for integration testing)

### Clone the Repository

```bash
# TODO: Before public release, update to actual GitHub organization
git clone https://github.com/jayashankarvr/vbdp.git
cd vbdp
```

### Install Dependencies

All dependencies are managed by Cargo and will be downloaded automatically during the first build.

## Building the Project

### Build All Components

```bash
cargo build
```

### Build in Release Mode

```bash
cargo build --release
```

The compiled binaries will be in:
- `target/debug/` (debug build)
- `target/release/` (release build)

### Build Individual Components

```bash
# Publisher toolkit
cargo build -p vbdp-publisher

# Update server
cargo build -p vbdp-server

# Client patcher
cargo build -p vbdp-client

# Core library
cargo build -p vbdp-core
```

## Running Tests

### Run All Tests

```bash
cargo test
```

### Run Tests for a Specific Package

```bash
cargo test -p vbdp-core
cargo test -p vbdp-crypto
cargo test -p vbdp-diff
```

### Run Tests with Output

```bash
cargo test -- --nocapture
```

### Run a Specific Test

```bash
cargo test test_name
```

## Submitting Pull Requests

1. Fork the repository
2. Create a feature branch from `main`
   ```bash
   git checkout -b feature/my-feature
   ```
3. Make your changes
4. Ensure all tests pass: `cargo test`
5. Format your code: `cargo fmt`
6. Check for lints: `cargo clippy -- -D warnings`
7. Commit your changes with a descriptive message
8. Push to your fork
9. Submit a PR against `main`

## Code Standards

### Code Style

- **Formatting**: All code must be formatted with `cargo fmt`
  ```bash
  cargo fmt --all
  ```

- **Linting**: Code must pass `cargo clippy` with no warnings
  ```bash
  cargo clippy --all-targets --all-features -- -D warnings
  ```

- **Documentation**: Public APIs must have doc comments
  - Use `///` for item documentation
  - Include examples where helpful
  - Document error conditions
  - Explain non-obvious behavior

- **Error Handling**:
  - Use `anyhow::Context` to add context to errors
  - Include relevant information (file paths, URLs, IDs)
  - Example: `.with_context(|| format!("Failed to read file at {:?}", path))?`

- **Logging**:
  - Use structured logging fields with `tracing`
  - Example: `tracing::info!(binary = %name, version = %ver, "Publishing version")`
  - Log at appropriate levels: `error`, `warn`, `info`, `debug`, `trace`

### Testing

- Add tests for new functionality
- Ensure tests are meaningful and test actual behavior
- Use descriptive test names: `test_version_comparison_ordering`
- Add both positive and negative test cases
- Mock external dependencies where appropriate

### Documentation

- Update README.md if adding user-facing features
- Update CHANGELOG.md following Keep a Changelog format
- Add inline comments for complex logic
- Update examples if APIs change

### Pull Request Guidelines

- **Title**: Use a clear, descriptive title
- **Description**:
  - Explain what changes were made and why
  - Reference related issues
  - Include screenshots for UI changes
  - List breaking changes prominently

- **Size**: Keep PRs focused and reasonably sized
  - Large changes should be split into multiple PRs when possible
  - Discuss large architectural changes in an issue first

- **Review**: Be responsive to feedback
  - Address all review comments
  - Ask questions if something is unclear
  - Update the PR description if scope changes

### Commit Messages

Use conventional commit format:

```
type(scope): short description

Longer description if needed.
- Bullet points for details
- Multiple paragraphs if necessary

Fixes #123

Signed-off-by: Your Name <your.email@example.com>
```

**Types**:
- `feat` - New feature
- `fix` - Bug fix
- `docs` - Documentation changes
- `style` - Code style changes (formatting, etc.)
- `refactor` - Code refactoring
- `test` - Adding or updating tests
- `chore` - Build process, dependencies, etc.
- `perf` - Performance improvements

**Scope**: The component affected (e.g., `client`, `server`, `publisher`, `crypto`)

### DCO Sign-Off

All commits must include a sign-off line certifying you wrote or have the right to submit the code:

```
Signed-off-by: Your Name <your.email@example.com>
```

Add `-s` flag to `git commit` to add this automatically.

## License

By contributing, you agree that your contributions will be licensed under the Apache-2.0 license.
