---
name: Feature Request
about: Suggest a new feature or enhancement for Deltaship
title: '[FEATURE] '
labels: 'enhancement, needs-triage'
assignees: ''
---

## Feature Summary

**One-paragraph description of the feature:**
[Clear, concise summary of what you want to add or change]

---

## Motivation

**Why is this feature needed?**

**Problem it solves:**
[What problem does this feature address?]

**Use case:**
[Describe your specific use case]

**Who benefits:**
- [ ] Publishers
- [ ] Server operators
- [ ] Client users
- [ ] SDK developers
- [ ] All users

**Frequency of use:**
- [ ] Every deployment
- [ ] Daily
- [ ] Weekly
- [ ] Occasionally
- [ ] Edge case

---

## Proposed Solution

**How would this feature work?**
[Detailed description of your proposed solution]

**Example usage:**
```bash
# Example command or code showing how the feature would be used
deltaship-publisher register --version 2.0.0 --with-new-feature
```

**Expected behavior:**
[What should happen when using this feature]

---

## Alternatives Considered

**Have you considered any alternative solutions or workarounds?**

**Alternative 1:**
- Description: [How else could this be solved?]
- Pros: [Benefits]
- Cons: [Drawbacks]
- Why not chosen: [Reason]

**Alternative 2:**
- Description:
- Pros:
- Cons:
- Why not chosen:

**Current workaround (if any):**
[How are you currently working around the lack of this feature?]

---

## Technical Details

**Component affected:**
- [ ] Publisher Toolkit
- [ ] Update Server
- [ ] Client Patcher
- [ ] API
- [ ] Database schema
- [ ] Storage
- [ ] Security/crypto
- [ ] Documentation
- [ ] Other: [specify]

**API changes required:**
- [ ] No API changes
- [ ] New API endpoints
- [ ] Modify existing endpoints
- [ ] Breaking changes

**Backward compatibility:**
- [ ] Fully backward compatible
- [ ] Requires migration
- [ ] Breaking change

**Implementation complexity:**
- [ ] Trivial (< 1 day)
- [ ] Simple (1-3 days)
- [ ] Moderate (1-2 weeks)
- [ ] Complex (> 2 weeks)
- [ ] Unknown

---

## Examples from Other Projects

**Similar features in other tools:**

**Example 1:**
- Project: [e.g., Docker, APT, npm]
- Feature: [How they implement something similar]
- Link: [URL to documentation or code]

**Example 2:**
- Project:
- Feature:
- Link:

---

## Design Considerations

**User interface:**
[How would users interact with this feature?]

**Configuration:**
```toml
# Example configuration if needed
[feature_name]
enabled = true
option = "value"
```

**Performance impact:**
- [ ] No performance impact
- [ ] Minor impact (< 5% overhead)
- [ ] Moderate impact (5-20% overhead)
- [ ] Significant impact (> 20% overhead)
- [ ] Performance improvement

**Security implications:**
- [ ] No security implications
- [ ] Enhances security
- [ ] Requires security review
- [ ] Potential security concerns: [describe]

**Storage impact:**
- [ ] No additional storage
- [ ] Minimal storage (< 1MB)
- [ ] Moderate storage (1-100MB)
- [ ] Significant storage (> 100MB)

---

## Success Criteria

**How will we know this feature is successful?**

**Acceptance criteria:**
- [ ] [Specific, testable requirement 1]
- [ ] [Specific, testable requirement 2]
- [ ] [Specific, testable requirement 3]

**Metrics:**
- [What metrics would improve? e.g., "Reduces bandwidth by 20%"]
- [What would be measurable? e.g., "User adoption > 50%"]

**Documentation requirements:**
- [ ] API documentation
- [ ] User guide
- [ ] Tutorial/examples
- [ ] Migration guide
- [ ] FAQ entry

---

## Additional Context

**Related issues:**
- Related to #[issue number]
- Blocks #[issue number]
- Blocked by #[issue number]

**Related discussions:**
- [Link to GitHub Discussion]
- [Link to Discord conversation]

**External references:**
- [Link to blog post, paper, or relevant article]

**Screenshots/Mockups:**
[If applicable, add visual examples or mockups]

**Code examples:**
```rust
// Example code if you have a proof-of-concept
```

---

## Priority and Impact

**Priority (your perspective):**
- [ ] Critical (blocks my deployment)
- [ ] High (strongly needed)
- [ ] Medium (would be nice to have)
- [ ] Low (minor improvement)

**Impact (number of users):**
- [ ] All users
- [ ] Many users (common use case)
- [ ] Some users (specific scenarios)
- [ ] Few users (niche use case)

**Urgency:**
- [ ] Needed immediately
- [ ] Needed for next release
- [ ] No specific timeline

---

## Willingness to Contribute

**Are you willing to help implement this feature?**

- [ ] Yes, I can implement this myself (will submit PR)
- [ ] Yes, I can help with implementation (need guidance)
- [ ] Yes, I can help with testing
- [ ] Yes, I can help with documentation
- [ ] No, but I can provide feedback on design
- [ ] No, I cannot contribute to implementation

**If yes, your experience level:**
- [ ] Expert in Rust
- [ ] Intermediate Rust developer
- [ ] Beginner in Rust
- [ ] Experienced in other languages
- [ ] Documentation/testing only

---

## Checklist

Before submitting, please verify:

- [ ] I have searched existing issues and discussions to avoid duplicates
- [ ] I have clearly described the problem this feature solves
- [ ] I have considered alternative solutions
- [ ] I have thought about backward compatibility
- [ ] This is not a bug report (if bug, use bug report template)
- [ ] This aligns with Deltaship's goals (efficient binary updates)

---

## For Maintainers

**Do not fill this section - for maintainer use only**

**Triage:**
- [ ] Accepted
- [ ] Needs discussion
- [ ] Need more information
- [ ] Declined (out of scope)
- [ ] Duplicate of #

**Alignment with roadmap:**
- [ ] On roadmap
- [ ] Under consideration
- [ ] Not planned
- [ ] Future (post-1.0)

**Priority:**
- [ ] P0 - Critical (must have for v1.0)
- [ ] P1 - High (should have)
- [ ] P2 - Medium (nice to have)
- [ ] P3 - Low (future consideration)

**Estimated effort:**
- [ ] Small (< 1 week)
- [ ] Medium (1-4 weeks)
- [ ] Large (1-3 months)
- [ ] Epic (> 3 months)

**Assigned to:** @
**Target version:**
**Design doc required:** [ ] Yes [ ] No
