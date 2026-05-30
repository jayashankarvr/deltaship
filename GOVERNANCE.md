# Project Governance

**Document:** VBDP Project Governance
**Last Updated:** 2026-01-14

> **NOTE:** Email addresses on this page use the RFC 2606 `example.com` domain and are placeholders. Replace with real contacts before publishing.

---

## Overview

This document describes the governance model for VBDP (Version-Aware Binary Differential Update System). Our goal is to maintain a transparent, inclusive, and efficient decision-making process that ensures the project's long-term sustainability while encouraging community participation.

## Project Principles

1. **Transparency**: All decisions and discussions happen in public
2. **Meritocracy**: Contributions and expertise are valued over titles
3. **Consensus-seeking**: We aim for agreement, but have clear escalation paths
4. **Inclusivity**: Everyone can contribute regardless of experience level
5. **Quality**: We maintain high standards for security, performance, and reliability

---

## Roles and Responsibilities

### Users

Anyone using VBDP in their projects.

**Rights:**
- Report bugs and request features
- Ask questions in discussions
- Provide feedback on proposals

**No special permissions required.**

### Contributors

Anyone who has submitted accepted contributions (code, documentation, bug reports, etc.).

**Rights:**
- All user rights
- Recognition in AUTHORS.md
- Participate in RFC discussions
- Propose features and improvements

**How to become a contributor:**
Submit a pull request that gets merged, or provide significant non-code contributions (documentation, bug triage, community support).

### Committers

Contributors who have demonstrated sustained commitment and technical expertise.

**Rights:**
- All contributor rights
- Review and merge pull requests
- Triage issues
- Participate in security discussions
- Vote on RFCs

**Responsibilities:**
- Review pull requests promptly
- Maintain code quality standards
- Help new contributors
- Participate in technical discussions
- Follow the Code of Conduct

**How to become a committer:**
1. Sustained high-quality contributions over 3+ months
2. Demonstrated understanding of project architecture and goals
3. Active participation in reviews and discussions
4. Nomination by existing maintainer or committer
5. Approval by 2/3 majority vote of current maintainers

**Expectations:**
- Review at least 2-3 PRs per month
- Participate in security issue triage
- Be available for technical discussions
- Commit to 6-month minimum active participation

### Maintainers

Committers who are responsible for project direction and releases.

**Rights:**
- All committer rights
- Merge breaking changes
- Create releases
- Manage project infrastructure
- Access to security vulnerability reports
- Final say in technical disputes
- Modify governance (with 2/3 vote)

**Responsibilities:**
- Ensure project health and sustainability
- Make final decisions on technical direction
- Coordinate releases
- Mentor committers and contributors
- Handle security vulnerabilities
- Enforce Code of Conduct

**How to become a maintainer:**
1. Active committer for 6+ months
2. Significant contributions across multiple areas
3. Demonstrated leadership in technical discussions
4. Shown good judgment in reviews and decisions
5. Nomination by existing maintainer
6. Approval by 2/3 majority vote of current maintainers

**Current Maintainers:**
- Listed in AUTHORS.md with email contacts
- At least 3 maintainers required for healthy governance
- Maintainers represent different organizations/perspectives where possible

### Emeritus Status

Former committers or maintainers who are no longer active but retain recognition.

**How it works:**
- Voluntary stepping down with recognition of past contributions
- Automatic after 6 months of inactivity (can be extended by request)
- Can return to previous role with simple majority vote
- Retains access to private alumni channels
- Listed in AUTHORS.md with emeritus designation

---

## Decision-Making Process

### Standard Decisions

For routine matters (bug fixes, minor features, documentation):

1. **Propose**: Open a pull request or issue
2. **Discuss**: Committers and community provide feedback
3. **Revise**: Address feedback
4. **Approve**: At least 1 committer approval, no blocking objections
5. **Merge**: After 24-48 hour waiting period for review

### Major Decisions

For significant changes requiring RFC (Request for Comments):

**Requires RFC for:**
- Breaking API changes
- New major features
- Architecture changes
- Changes to security model
- Changes to supported platforms
- Addition of significant new dependencies

**RFC Process:**

1. **Draft**: Create RFC document in `rfcs/` directory
   ```
   rfcs/NNNN-feature-name.md
   ```
   Include:
   - Motivation and use case
   - Detailed design
   - Drawbacks and alternatives
   - Implementation plan
   - Migration strategy (if breaking change)

2. **Discussion**: Open PR with RFC
   - Minimum 2-week comment period (4 weeks for breaking changes)
   - Active discussion in PR comments
   - Maintainers may request extensions for complex RFCs

3. **Consensus**: Aim for agreement
   - Address all concerns raised
   - Update RFC with discussion outcomes
   - Seek "lazy consensus" - silence = agreement after waiting period

4. **Decision**: Maintainers vote
   - Simple majority (>50%) for new features
   - 2/3 majority (≥67%) for breaking changes
   - All maintainers notified of vote
   - 1-week voting period
   - Votes recorded in RFC PR

5. **Implementation**: After approval
   - RFC merged to main branch
   - Implementation tracked in linked issues/PRs
   - May be delegated to proposer or volunteers

### Security Decisions

For security vulnerabilities:

<!-- TODO: Before public release, update to actual security contact email -->
1. **Report**: Via security@vbdp.example.com (private)
2. **Triage**: Maintainers assess severity within 48 hours
3. **Fix**: Develop patch in private repository fork
4. **Coordinate**: Contact affected parties if needed
5. **Release**: Security release with advisory
6. **Disclose**: Public disclosure after patch available

See [SECURITY.md](SECURITY.md) for full security policy.

### Governance Changes

Changes to this document require:
- RFC following major decision process
- 2/3 majority vote of maintainers
- 4-week minimum comment period
- Public announcement before vote

---

## Commit and Release Rights

### Commit Access

**Who can merge:**
- Committers and maintainers can merge PRs
- Author cannot merge their own PR
- At least 1 approval required
- CI must pass
- No unresolved review comments

**Protected branches:**
- `main`: Requires PR and review, no direct commits
- `release/*`: Requires maintainer approval
- `security/*`: Requires 2 maintainer approvals

**Merge criteria:**
```
✓ At least 1 committer/maintainer approval
✓ No requested changes outstanding
✓ All CI checks pass
✓ No merge conflicts
✓ 24-48 hour review window elapsed (can be waived for urgent security fixes)
✓ DCO sign-off on all commits
```

### Release Rights

**Who can release:**
- Only maintainers
- Requires 2 maintainer approvals for major/minor releases
- 1 maintainer approval for patch releases
- Emergency security releases: 1 maintainer with post-release notification

**Release process:**
1. Create release PR updating CHANGELOG.md and version numbers
2. Get required approvals
3. Merge release PR
4. Tag release: `git tag -s vX.Y.Z`
5. Create GitHub release with notes
6. Publish to crates.io
7. Announce in discussions and relevant channels

See [RELEASE_PROCESS.md](docs/RELEASE_PROCESS.md) for details.

---

## Conflict Resolution

### Level 1: Discussion

Most disagreements resolved through respectful discussion.

**Process:**
1. Discuss in PR/issue comments
2. Seek to understand different perspectives
3. Find common ground or compromise
4. Document decision rationale

### Level 2: Maintainer Review

If no consensus after discussion:

**Process:**
1. Either party requests maintainer review
2. Maintainer(s) review the discussion
3. Provide guidance or decision
4. Decision documented with reasoning

### Level 3: Maintainer Vote

For significant disputes or technical deadlocks:

**Process:**
1. Request formal vote in maintainer channel
2. Present both sides with equal opportunity
3. 1-week voting period
4. Simple majority decides (ties go to status quo)
5. Decision is final unless new information emerges

### Level 4: Code of Conduct Issues

For interpersonal or conduct issues:

**Process:**
See [CODE_OF_CONDUCT.md](docs/CODE_OF_CONDUCT.md) for full process.

<!-- TODO: Before public release, update to actual conduct contact email -->
1. Report to conduct@vbdp.example.com
2. Confidential investigation
3. Action taken as appropriate
4. Appeals process available

---

## Communication Channels

### Public Channels

**GitHub Issues**: Bug reports, feature requests
- Expected response: 48-72 hours for triage

**GitHub Discussions**: General questions, ideas, announcements
- Expected response: 1 week

**GitHub Pull Requests**: Code review, technical discussion
- Expected response: 48 hours for initial review

### Private Channels

<!-- TODO: Before public release, update all placeholder emails to actual addresses -->
**security@vbdp.example.com**: Security vulnerability reports
- Expected response: 48 hours

**conduct@vbdp.example.com**: Code of Conduct issues
- Expected response: 48 hours

**maintainers@vbdp.example.com**: Maintainer coordination
- Not for general questions

---

## Maintainer Responsibilities

### Technical

- **Code Review**: Review PRs promptly and thoroughly
- **Architecture**: Ensure consistency with project design
- **Security**: Respond to security reports within SLA
- **Releases**: Coordinate and execute releases
- **Quality**: Maintain high standards for code, tests, docs

### Community

- **Mentorship**: Help new contributors succeed
- **Communication**: Keep community informed of decisions
- **Inclusivity**: Welcome diverse perspectives
- **Recognition**: Acknowledge contributions publicly

### Process

- **Governance**: Participate in votes and decisions
- **Planning**: Contribute to roadmap discussions
- **Conflict Resolution**: Help resolve disputes fairly
- **Evolution**: Improve processes based on learnings

### Time Commitment

**Expected availability:**
- Minimum 4 hours per week on project activities
- Respond to urgent security issues within 48 hours
- Participate in votes within voting period
- Attend monthly maintainer sync (1 hour)

**Leave of absence:**
- Can request temporary reduction in responsibilities
- Should notify other maintainers
- Emeritus status after 6 months inactive (unless extended)

---

## Adding and Removing Maintainers

### Adding Maintainers

**Process:**
1. **Nomination**: Any maintainer nominates a committer
2. **Discussion**: Private maintainer discussion (1 week)
3. **Vote**: 2/3 majority required
4. **Invitation**: Private invitation to nominee
5. **Acceptance**: Nominee accepts responsibilities
6. **Announcement**: Public announcement in Discussions
7. **Onboarding**: Access to systems, introduction to processes

**Nomination criteria:**
- Active committer for 6+ months
- Significant technical contributions
- Good judgment in reviews and discussions
- Leadership in community
- Commitment to project values
- Available time for responsibilities

### Removing Maintainers

**Voluntary:**
1. Maintainer notifies others
2. Transition period (2-4 weeks) to hand off responsibilities
3. Move to emeritus status
4. Public thank you announcement

**Involuntary (rare):**
Only for serious situations:
- Sustained inactivity (6+ months) without communication
- Repeated Code of Conduct violations
- Serious security breach or negligence
- Loss of trust from other maintainers

**Process:**
1. Private discussion among other maintainers
2. Attempt to resolve concerns directly
3. If unresolved, vote (2/3 majority required)
4. Private notification to affected maintainer
5. 2-week appeal period
6. If upheld, access removed and announcement made
7. May return through normal maintainer process after 1 year

---

## Project Assets and Infrastructure

**Ownership:**
- GitHub organization: Owned by maintainers collectively
- Domain names: Registered to project, not individuals
- Social media: Controlled by maintainers
- Trademark: Project name and logo (if registered)
- Finances: Transparent accounting, maintainer approval required

**Access control:**
- Maintainers: Full access to all systems
- Committers: Access to necessary systems for their role
- Automated systems: Infrastructure as code, auditable

**Succession planning:**
- At least 3 maintainers with full access
- Documented procedures for all critical operations
- Regular access audits
- Emergency contact information

---

## Amendments

This governance document may be amended through:

1. **Proposal**: RFC describing changes and rationale
2. **Discussion**: Minimum 4-week comment period
3. **Vote**: 2/3 majority of maintainers
4. **Announcement**: Public notification before and after
5. **Effective date**: Minimum 2 weeks after approval

**History of amendments:**
- 2026-01-14: Initial governance document created

---

## Inspiration and References

This governance model is inspired by successful open-source projects:
- Rust Language (RFC process, clear roles)
- Kubernetes (SIG structure, contributor ladder)
- Apache Software Foundation (meritocracy, consensus)
- Node.js (Technical Steering Committee)

---

## Questions?

For questions about governance:
- **General questions**: GitHub Discussions
- **Governance clarifications**: Open an issue
<!-- TODO: Before public release, update to actual maintainers contact email -->
- **Private concerns**: maintainers@vbdp.example.com

---

**End of Governance Document**
