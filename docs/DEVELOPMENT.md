# BitArchive Development Guide

This document defines the development workflow for BitArchive.

The goal is to keep development traceable, reproducible, reviewable, and safe while using AI-assisted implementation.

The central rule is:

> **No issue, no code. No pull request, no main.**

And for AI-assisted development:

> **No handover, no review. No approval, no merge.**

---

## 1. Development Principles

GitHub is the central remote repository and source of truth for BitArchive development.

The `main` branch represents the stable state of the project.

The following rules apply:

- `main` must remain stable and buildable.
- Development never happens directly on `main`.
- Every change starts with a GitHub Issue.
- Every Issue is implemented on its own branch.
- Every change reaches `main` through a Pull Request.
- Relevant automated tests must pass before merge.
- Every AI implementation ends with a structured handover.
- Every Pull Request must be reviewed before merge.
- Implementation and review are logically separated.
- Temporary development information must not unnecessarily become part of the repository history.

---

## 2. Development Workflow


The standard BitArchive development workflow is:

```text
Issue / Task
    ↓
Branch
    ↓
AI implements
    ↓
Tests
    ↓
Commit
    ↓
Push
    ↓
Pull Request
    ↓
Handover Protocol
    ↓
Review by ChatGPT
    ↓
Changes if required
    ↓
Tests
    ↓
Commit + Push
    ↓
Updated Handover
    ↓
Re-review
    ↓
Approval
    ↓
Squash Merge into main
```

No step should normally be skipped.

---

## 3. GitHub Repository

GitHub is used for:

- source control
- Issues
- Pull Requests
- code review
- CI checks
- project history
- release history
- development discussions related to Issues and Pull Requests

The GitHub CLI (`gh`) is the preferred command-line interface for GitHub operations.

The repository is public.

---

## 4. Main Branch

`main` is the only permanent development branch.

`main` represents the current stable project state.

Direct development on `main` is prohibited.

Direct pushes to `main` should be prevented through GitHub branch protection rules.

Changes enter `main` exclusively through reviewed Pull Requests.

The project does not use a permanent `develop` branch.

Release branches should only be introduced later if the release process requires them.

---

## 5. GitHub Issues

GitHub Issues are the project's ticket system.

Every development task requires an Issue before implementation begins.

This includes:

- features
- bug fixes
- refactoring
- documentation changes
- maintenance work
- dependency changes
- build system changes
- technical debt

The general rule is:

```text
No Issue → No Implementation
```

### Issue Content

An Issue should contain enough information that another developer or AI agent can understand the expected result without relying on undocumented context.

At minimum, an implementation Issue should contain:

- title
- context
- goal
- requirements
- acceptance criteria

Where useful, it should additionally contain:

- technical notes
- affected components
- screenshots or mockups
- dependencies on other Issues
- known constraints
- out-of-scope items

### Acceptance Criteria

Acceptance criteria should describe observable results.

Example:

```markdown
## Acceptance Criteria

- Configured ROM directories can be scanned recursively.
- Unsupported file extensions are ignored.
- Duplicate ROM entries are not created.
- An inaccessible directory does not abort the entire scan.
- Automated tests cover the new behavior.
```

Acceptance criteria are the primary reference during implementation and review.

---

## 6. Issue Scope

Issues should be small enough to be implemented and reviewed independently.

Large features should be split into multiple Issues.

An implementation should remain within the scope of its Issue.

Unrelated refactoring or cleanup should not be added simply because the implementation touches the same area.

If additional work is discovered during implementation, a new Issue should normally be created.

This keeps Pull Requests focused and reviewable.

---

## 7. Issue References

Branches, commits, and Pull Requests should reference the associated Issue whenever practical.

Pull Requests must close or reference their Issue.

Preferred syntax:

```text
Closes #42
```

This allows GitHub to automatically close the Issue when the Pull Request is merged.

---

## 8. Branch Strategy

Every Issue receives its own branch.

Branch names use the following format:

```text
<type>/<issue-number>-<short-description>
```

Supported branch types are:

```text
feat/
fix/
refactor/
docs/
test/
chore/
```

Examples:

```text
feat/42-library-scanner
fix/51-scraper-timeout
refactor/63-config-loader
docs/70-development-workflow
test/82-library-scan-tests
chore/91-update-dependencies
```

Branch names should:

- contain the Issue number
- be short
- use lowercase characters
- use hyphens as separators
- describe the task clearly

Branches should be created from the latest `main`.

Example:

```bash
git switch main
git pull --ff-only
git switch -c feat/42-library-scanner
```

---

## 9. Branch Lifetime

Issue branches are temporary.

After a Pull Request has been merged:

- the remote branch should be deleted
- the local branch may be deleted
- further changes require a new Issue and branch unless they belong to an active follow-up review

GitHub should be configured to automatically delete merged branches.

---

## 10. AI-Assisted Implementation

BitArchive uses AI agents for implementation work.

The implementation AI is responsible for more than writing code.

Before implementation, the AI should:

1. read the complete Issue
2. inspect relevant existing code
3. understand surrounding architecture
4. identify existing conventions
5. inspect related tests
6. ensure the branch is based on the intended state of `main`

During implementation, the AI should:

- stay within Issue scope
- follow existing architecture
- avoid unnecessary abstractions
- avoid unrelated refactoring
- preserve compatibility unless the Issue explicitly changes behavior
- add or update tests
- update relevant documentation
- handle realistic error cases

The repository and Issue are the source of truth.

---

## 11. Implementation Scope Discipline

AI implementation must avoid opportunistic changes.

For example, while implementing a ROM scanner, the AI should not also rewrite the configuration system unless the Issue requires it.

If unrelated problems are discovered:

```text
Discover problem
    ↓
Document it
    ↓
Create separate Issue
    ↓
Continue original task
```

This keeps Pull Requests focused and reduces review risk.

---

## 12. Testing

Testing is part of implementation and not a separate optional phase.

A task is not considered implemented until the relevant tests and validation checks have been executed successfully.

Depending on the affected component, validation can include:

- unit tests
- integration tests
- regression tests
- build checks
- linting
- formatting checks
- static analysis
- manual functional verification

### New Features

New behavior should normally receive automated tests.

### Bug Fixes

Bug fixes should normally include a regression test reproducing the original defect.

The preferred sequence is:

```text
Reproduce bug
    ↓
Add failing test when practical
    ↓
Implement fix
    ↓
Verify test passes
```

### Existing Tests

Existing tests must continue to pass.

A change must not disable, delete, or weaken existing tests merely to make the Pull Request pass unless the Issue explicitly changes the expected behavior.

---

## 13. Test Exceptions

Not every behavior can always be tested automatically.

When automated testing is impractical, the implementation must document:

- why automated testing was not practical
- how the behavior was verified manually
- what scenarios were tested

This information belongs in the Pull Request handover.

---

## 14. Commits

Commits should represent logical units of work.

Commits should be understandable without requiring knowledge of the entire implementation session.

BitArchive uses Conventional Commit style.

Preferred commit types include:

```text
feat:
fix:
refactor:
test:
docs:
chore:
```

Examples:

```text
feat: add ROM library scanner
fix: handle scraper timeout
refactor: simplify config loading
test: add scanner regression tests
docs: document development workflow
chore: update build dependencies
```

The related Issue number should preferably be included:

```text
feat: add ROM library scanner (#42)
```

---

## 15. Commit Quality

Commits should not contain:

- debugging artifacts
- unrelated generated files
- temporary notes
- secrets
- credentials
- API keys
- local environment configuration
- editor-specific files unless intentionally tracked

Before committing, the implementation AI should inspect the working tree.

Example:

```bash
git status
git diff
```

---

## 16. Secrets

Secrets must never be committed to the repository.

This includes:

- API keys
- access tokens
- credentials
- private keys
- passwords

Developer API keys must be supplied through appropriate local or environment configuration.

Example configuration may be committed only with placeholder values.

---

## 17. Pull Requests

Every implementation reaches `main` through a Pull Request.

A Pull Request should contain:

- concise summary
- associated Issue
- important implementation details
- relevant test information
- known limitations where applicable

The Pull Request should include:

```markdown
Closes #42
```

when it fully resolves the related Issue.

---

## 18. Pull Request Size

Pull Requests should remain focused.

A reviewer should be able to understand:

- what changed
- why it changed
- how it works
- how it was tested

without having to reconstruct multiple unrelated changes.

Large Pull Requests should be avoided where reasonable.

If an Issue naturally requires extensive changes, the implementation should still be structured into understandable components and commits.

---

## 19. Handover Protocol

Every AI implementation must end with a structured handover protocol.

The handover is mandatory.

It is not stored as a file in the repository.

Instead, it is posted as a Pull Request comment.

The handover must be the final implementation comment before review begins.

The rule is:

> **No handover, no review.**

The reviewing ChatGPT session should not have to reconstruct the implementation process from scratch.

However, the handover is not considered authoritative evidence by itself.

The reviewer must independently inspect the repository and Pull Request.

---

## 20. Required Handover Content

Every handover must contain at least:

- Issue number and title
- branch name
- Pull Request number or URL
- implementation summary
- major files or components changed
- tests added or modified
- validation commands executed
- validation results
- relevant technical decisions
- deviations from the Issue
- known limitations
- unresolved questions
- areas that deserve special review attention
- confirmation that all intended changes have been committed
- confirmation that the branch has been pushed

Example:

````markdown
# Handover

## Issue

#42 - Add ROM library scanner

## Branch

feat/42-library-scanner

## Pull Request

#57

## Implementation

Implemented recursive ROM discovery for configured system directories.

The scanner:

- reads configured ROM paths
- recursively discovers supported files
- ignores unsupported extensions
- prevents duplicate entries
- handles inaccessible directories without aborting the complete scan

## Changed Components

- `src/library/scanner.*`
- `src/library/library_service.*`
- `tests/library/scanner_tests.*`

## Tests

Added tests covering:

- empty directories
- nested directories
- unsupported extensions
- duplicates
- inaccessible directories

## Validation

Executed:

```bash
<test command>
<lint command>
<build command>
```

Result:

All checks passed.

## Decisions

Duplicate detection uses normalized absolute ROM paths.

## Deviations

None.

## Known Limitations

Symlink handling remains unchanged.

## Review Focus

Please pay particular attention to:

- path normalization
- duplicate detection
- inaccessible-directory handling

## Git Status

All intended changes are committed and pushed to:

`feat/42-library-scanner`
````

---

## 21. Publishing the Handover

The handover should be posted directly to the Pull Request.

Using GitHub CLI:

```bash
gh pr comment <PR-NUMBER> --body-file handover.md
```

A temporary local handover file may be used to submit the comment but must not be committed to the repository.

Alternatively:

```bash
gh pr comment <PR-NUMBER> --body "<handover>"
```

The Pull Request comment itself is the persistent record.

---

## 22. Review Responsibility

The implementation is reviewed by a separate ChatGPT conversation.

Because the BitArchive repository is public, the reviewer can independently inspect:

- GitHub Issue
- Pull Request
- branch
- commits
- changed files
- surrounding source code
- automated tests
- CI results

The reviewer must not rely solely on the handover.

The repository state is the source of truth.

---

## 23. Review Checklist

The reviewer should verify at least:

### Issue compliance

- Does the implementation satisfy the Issue?
- Are all acceptance criteria fulfilled?
- Is anything missing?
- Was unnecessary scope added?

### Correctness

- Does the implementation behave as expected?
- Are realistic edge cases handled?
- Are error paths sensible?

### Architecture

- Does the implementation fit the existing architecture?
- Are responsibilities placed in appropriate components?
- Was unnecessary complexity introduced?

### Code quality

- Is the code understandable?
- Are names meaningful?
- Is duplication reasonable?
- Are abstractions justified?

### Tests

- Are relevant tests present?
- Do tests verify behavior rather than implementation details?
- Are bug fixes protected by regression tests?
- Do existing tests still pass?

### Security and safety

- Are secrets excluded?
- Is external input handled safely?
- Are filesystem operations safe?
- Are potentially destructive operations sufficiently guarded?

### Documentation

- Were relevant developer or user-facing documents updated?
- Do comments explain why rather than merely restating code?

### Pull Request quality

- Does the PR match its Issue?
- Is the handover complete?
- Are CI checks successful?

---

## 24. Review Result

A review should end with one of the following outcomes:

```text
APPROVED
```

or

```text
CHANGES REQUESTED
```

An approval means that the reviewer considers the Pull Request ready for merge.

Requested changes must clearly describe what needs to be corrected.

---

## 25. Review Changes

When the reviewer requests changes, the implementation cycle resumes.

```text
Review feedback
    ↓
Implementation
    ↓
Tests
    ↓
Commit
    ↓
Push
    ↓
New Handover
    ↓
Re-review
```

The previous handover must not be edited.

Instead, a new handover comment is added.

This preserves the complete review history.

---

## 26. Updated Handovers

Each review iteration receives a new handover.

The newest handover represents the current implementation state.

A follow-up handover should additionally summarize the changes made since the previous review.

Example:

```markdown
## Changes Since Previous Review

Addressed review feedback:

- added canonical path handling
- added symlink regression test
- removed unrelated config refactoring
- improved error propagation

All validation checks pass.
```

The rule remains:

> **No updated handover, no re-review.**

---

## 27. Merge Policy

Pull Requests are merged using **Squash Merge**.

This keeps `main` history concise and Issue-oriented.

For example, an implementation branch may contain:

```text
feat: add initial scanner
test: add scanner tests
fix: handle inaccessible directories
fix: address review feedback
```

After Squash Merge, `main` receives one commit:

```text
feat: add ROM library scanner (#42)
```

---

## 28. Merge Requirements

A Pull Request may only be merged when:

- its Issue exists
- the Pull Request references the Issue
- implementation is complete
- the latest implementation has a handover
- relevant tests pass
- required CI checks pass
- review is complete
- reviewer approval has been given
- no unresolved blocking review comments remain

The rule is:

> **No approval, no merge.**

---

## 29. Branch Protection

GitHub branch protection should be enabled for `main`.

Recommended settings:

- require Pull Request before merging
- prohibit direct pushes
- require successful status checks
- require conversations to be resolved
- prevent branch deletion
- prevent force pushes
- automatically delete merged source branches

Where GitHub plan and repository settings allow it, protections should also apply to repository administrators.

---

## 30. Continuous Integration

CI should automatically validate Pull Requests.

The exact commands depend on the BitArchive technology stack, but CI should eventually cover:

```text
Formatting
    ↓
Linting
    ↓
Static Analysis
    ↓
Unit Tests
    ↓
Integration Tests
    ↓
Build
```

Only relevant checks need to run where appropriate, but the final required CI pipeline must pass before merge.

---

## 31. Failed CI

Failed required CI checks block merge.

The implementation AI should investigate failures rather than bypassing them.

Tests or checks must not be disabled solely to obtain a green Pull Request.

If a CI failure is unrelated to the Pull Request, this should be documented and handled explicitly.

---

## 32. GitHub CLI Workflow

The preferred development workflow uses `gh`.

Example:

```bash
gh issue view 42
```

Update local `main`:

```bash
git switch main
git pull --ff-only
```

Create the Issue branch:

```bash
git switch -c feat/42-library-scanner
```

Implement and validate the change.

Inspect changes:

```bash
git status
git diff
```

Commit:

```bash
git add .
git commit -m "feat: add ROM library scanner (#42)"
```

Push:

```bash
git push -u origin feat/42-library-scanner
```

Create the Pull Request:

```bash
gh pr create
```

Inspect the Pull Request:

```bash
gh pr view
```

Inspect CI:

```bash
gh pr checks
```

Post the handover:

```bash
gh pr comment <PR-NUMBER> --body-file handover.md
```

Review then takes place in the designated ChatGPT review conversation.

---

## 33. Suggested GitHub Labels

The repository should use a small and predictable label set.

Recommended type labels:

```text
type: feature
type: bug
type: refactor
type: documentation
type: test
type: chore
```

Recommended status or workflow labels:

```text
status: blocked
status: needs-review
status: changes-requested
```

Recommended priority labels:

```text
priority: critical
priority: high
priority: normal
priority: low
```

Additional labels should only be introduced when they provide meaningful workflow value.

---

## 34. Issue Templates

GitHub Issue templates should eventually be provided for at least:

- Feature
- Bug
- Technical Task

Templates should encourage clear acceptance criteria and reproducible bug descriptions.

A bug report should ideally contain:

```text
Problem
Expected behavior
Actual behavior
Steps to reproduce
Environment
Acceptance criteria
```

---

## 35. Pull Request Template

The repository should include a Pull Request template.

Recommended structure:

```markdown
## Summary

<!-- What was changed? -->

## Issue

Closes #

## Changes

<!-- Main implementation changes -->

## Testing

<!-- Automated and manual validation -->

## Notes

<!-- Relevant limitations or implementation details -->

## Handover

A full handover will be posted as the final implementation comment before review.
```

The full handover remains a separate Pull Request comment.

---

## 36. Documentation Changes

Changes that affect architecture, configuration, development workflow, or user-visible behavior should update the appropriate documentation as part of the same Issue whenever reasonable.

Documentation should not knowingly be left inconsistent with the implementation.

---

## 37. Generated and Temporary Files

Temporary implementation artifacts should not be committed unless they are an intentional part of the project.

Examples include:

- temporary handover files
- debug logs
- local test output
- local databases
- IDE caches
- OS metadata
- generated scratch files

Relevant patterns should be added to `.gitignore`.

---

## 38. Dependency Changes

Dependency changes require the same Issue and Pull Request process as other changes.

Dependency updates should avoid unrelated version changes.

The handover should mention:

- packages added
- packages removed
- important version changes
- reason for the change
- relevant compatibility implications

---

## 39. Architecture Decisions

Important architectural decisions discovered during implementation should not exist only inside AI conversation history.

Depending on their significance, they should be documented in:

- source documentation
- architecture documentation
- the related Issue
- the Pull Request
- a future Architecture Decision Record system if introduced

The handover should mention relevant decisions made during implementation.

---

## 40. Definition of Done

An Issue is considered done only when:

- acceptance criteria are fulfilled
- implementation is complete
- relevant tests exist
- tests pass
- relevant documentation is updated
- code has been committed
- branch has been pushed
- Pull Request exists
- handover has been posted
- independent review has approved the implementation
- Pull Request has been squash merged into `main`
- GitHub Issue has been closed

In short:

```text
Implemented ≠ Done

Implemented
+ Tested
+ Pushed
+ Handover
+ Reviewed
+ Approved
+ Merged
= Done
```

---

## 41. Core Development Rules

The BitArchive workflow can be summarized by the following rules:

> **No issue, no code.**

> **No direct development on main.**

> **One Issue, one branch.**

> **Keep changes within Issue scope.**

> **Tests are part of implementation.**

> **No handover, no review.**

> **Review the repository, not just the handover.**

> **No approval, no merge.**

> **Main stays stable.**

> **GitHub is the development source of truth.**
