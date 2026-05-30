# Pull Request

## Description

**Summary of changes:**
[Provide a clear, concise description of what this PR does]

**Type of change:**
- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update
- [ ] Performance improvement
- [ ] Refactoring (no functional changes)
- [ ] Dependency update
- [ ] Other: [specify]

---

## Related Issues

**Fixes:** #[issue number]
**Related to:** #[issue number]
**Blocked by:** #[issue number]

---

## Motivation and Context

**Why is this change needed?**
[Explain the problem this PR solves or the feature it adds]

**What problem does it solve?**
[Describe the issue or use case]

**How does it solve the problem?**
[Explain your approach]

---

## Changes Made

**Files changed:**
- `path/to/file.rs`: [Brief description of changes]
- `path/to/other.rs`: [Brief description of changes]
- `docs/file.md`: [Brief description of changes]

**Key changes:**
1. [Major change 1]
2. [Major change 2]
3. [Major change 3]

**Code example (if applicable):**
```rust
// Before
let result = old_function();

// After
let result = new_function()
    .with_improved_api()
    .and_better_error_handling()?;
```

---

## Testing

**How has this been tested?**

**Test environment:**
- OS: [e.g., Ubuntu 22.04]
- Rust version: [e.g., 1.70.0]
- Component: [e.g., Server, Client, Publisher]

**Test cases:**
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manual testing performed
- [ ] Performance testing done
- [ ] Security testing done (if applicable)

**Test commands run:**
```bash
cargo test
cargo clippy
cargo fmt -- --check
```

**Test results:**
```
All tests passed: X/X
Code coverage: X%
```

**Manual testing steps:**
1. [Step 1]
2. [Step 2]
3. [Step 3]
4. [Expected result]

---

## Performance Impact

**Does this change affect performance?**
- [ ] No performance impact
- [ ] Performance improvement
- [ ] Performance regression (justified below)
- [ ] Unknown/not measured

**Benchmark results (if applicable):**
```
Before: X ops/sec
After:  Y ops/sec
Change: +Z% improvement
```

**Memory usage:**
- [ ] No change
- [ ] Reduced
- [ ] Increased (justified below)

---

## Breaking Changes

**Does this PR introduce breaking changes?**
- [ ] No
- [ ] Yes (describe below)

**If yes, describe the breaking changes:**
[Explain what breaks and why it's necessary]

**Migration path:**
[Explain how users can migrate from the old behavior to the new behavior]

**Backward compatibility:**
- [ ] Fully backward compatible
- [ ] Requires migration
- [ ] Breaking change (major version bump required)

---

## Security Implications

**Does this change have security implications?**
- [ ] No security implications
- [ ] Enhances security
- [ ] Requires security review
- [ ] Fixes security vulnerability (coordinate with security team)

**If security-related, describe:**
[Explain the security impact]

**Security checklist (if applicable):**
- [ ] No SQL injection vulnerabilities
- [ ] No command injection vulnerabilities
- [ ] No XSS vulnerabilities
- [ ] Cryptographic operations use secure libraries
- [ ] Secrets are not logged or exposed
- [ ] Input validation implemented
- [ ] Authentication/authorization checked

---

## Documentation

**Documentation updates:**
- [ ] No documentation needed
- [ ] Documentation included in this PR
- [ ] Documentation will be added in follow-up PR #[number]

**Documentation changes:**
- [ ] API documentation updated
- [ ] User guide updated
- [ ] README updated
- [ ] CHANGELOG updated
- [ ] Code comments added/updated
- [ ] Examples added/updated

---

## Checklist

**Before submitting, verify:**

**Code quality:**
- [ ] My code follows the project's style guidelines
- [ ] I have performed a self-review of my code
- [ ] I have commented my code, particularly in hard-to-understand areas
- [ ] I have made corresponding changes to the documentation
- [ ] My changes generate no new warnings
- [ ] I have run `cargo fmt`
- [ ] I have run `cargo clippy` and addressed warnings

**Testing:**
- [ ] I have added tests that prove my fix is effective or that my feature works
- [ ] New and existing unit tests pass locally with my changes
- [ ] I have tested on all affected platforms (if applicable)

**Documentation:**
- [ ] I have updated the documentation
- [ ] I have updated CHANGELOG.md (if not a trivial change)

**Git:**
- [ ] My commits are atomic and well-described
- [ ] My commit messages follow the project's convention
- [ ] I have rebased on the latest main branch
- [ ] My branch has no merge conflicts

**Legal:**
- [ ] I have the right to submit this contribution
- [ ] I agree to the project's [license](../LICENSE.md)

---

## Screenshots (if applicable)

**Before:**
[Screenshot of old behavior]

**After:**
[Screenshot of new behavior]

---

## Additional Notes

**Future work:**
[Any follow-up work that should be done]

**Known limitations:**
[Any known issues or limitations with this PR]

**Alternatives considered:**
[Other approaches you considered and why you chose this one]

**Dependencies:**
[Any new dependencies added or updated]

**Deployment notes:**
[Anything special needed for deployment]

---

## For Reviewers

**Areas to focus on:**
- [ ] [Specific area 1]
- [ ] [Specific area 2]
- [ ] [Specific area 3]

**Questions for reviewers:**
1. [Question 1]
2. [Question 2]

---

## Commit Message Convention

**Format:**
```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Formatting, missing semicolons, etc.
- `refactor`: Code change that neither fixes a bug nor adds a feature
- `perf`: Performance improvement
- `test`: Adding missing tests
- `chore`: Maintenance tasks

**Example:**
```
feat(client): add retry logic for failed downloads

Implement exponential backoff retry mechanism for download failures.
Retries up to 3 times with delays of 1s, 2s, 4s.

Fixes #42
```

---

## Reviewer Checklist

**For maintainers reviewing this PR:**

**Code review:**
- [ ] Code is well-structured and readable
- [ ] No obvious bugs or logic errors
- [ ] Error handling is appropriate
- [ ] Edge cases are handled
- [ ] Code follows Rust best practices

**Testing:**
- [ ] Tests are comprehensive
- [ ] Tests are well-written
- [ ] CI passes

**Documentation:**
- [ ] Documentation is clear and accurate
- [ ] Examples are provided (if needed)
- [ ] CHANGELOG is updated

**Performance:**
- [ ] No obvious performance issues
- [ ] Benchmarks provided (if performance-critical)

**Security:**
- [ ] No security vulnerabilities
- [ ] Security best practices followed

**Approval:**
- [ ] Approved
- [ ] Request changes
- [ ] Comment

---

Thank you for contributing to Deltaship!
