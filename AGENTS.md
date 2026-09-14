# AGENTS.md
# BitArchive Agent Contract
## Purpose
This file defines how coding agents should work on BitArchive.
It is intentionally compact. `AGENTS.md` is not a third product or architecture specification.
Agents should make reasonable technical decisions independently and keep moving.
When something is underspecified, prefer a sensible, reversible implementation that fits the existing product and architecture over stopping for clarification.
## 1. Sources of Truth

Use the project documents by **responsibility**, not by blindly treating one file as globally higher priority than every other file:

1. `PRODUCT.md` — product capabilities, MVP scope, user-facing behavior, non-goals, and product/domain constraints.
2. `docs/UI_UX_CONCEPT.md` — information architecture, interaction behavior, navigation, focus behavior, and presentation of product capabilities. Only sections marked `DECIDED` are binding; `FAVORED`, `OPEN`, and `DEFERRED` must be treated according to their status.
3. `ARCHITECTURE.md` — technical architecture, subsystem boundaries, implementation constraints, and architecture invariants.
4. More specific documents such as `DATA_MODEL.md`, accepted ADRs in `docs/decisions/`, schema notes, or subsystem docs when they exist.
5. `AGENTS.md` — how agents should work within those constraints.
6. `docs/DEVELOPMENT.md` — repository workflow, Issues, branches, tests, Pull Requests, handovers, reviews, and merge policy.

### Document ownership

Use this ownership model when documents overlap:

- **PRODUCT owns what the product must do.**
- **UI/UX owns how users navigate and interact with those capabilities.**
- **ARCHITECTURE owns how the product is implemented safely and which technical invariants must hold.**
- **Accepted ADRs own explicit cross-cutting decisions within their stated scope.**

A cosmetic or implementation detail in one document must not silently override a binding decision owned by another document.

If two authoritative documents genuinely contradict each other within overlapping ownership:

1. inspect relevant ADRs and surrounding context;
2. prefer an explicit later decision over stale wording;
3. update the stale document as part of the same change when the intended decision is clear;
4. ask only when the conflict represents a genuine product-level or irreversible choice that cannot responsibly be inferred.

Follow `docs/DEVELOPMENT.md`.

Do not duplicate large parts of `PRODUCT.md`, `docs/UI_UX_CONCEPT.md`, or `ARCHITECTURE.md` here.
If documents are incomplete or slightly ambiguous, infer the most consistent design and continue.
If a task intentionally changes an existing decision, update every affected source-of-truth document with the code.

### Architecture Decision Records
Before implementation, check `docs/decisions/` for relevant ADRs.

Create an ADR when a new decision has lasting architectural, technical, or cross-cutting implementation impact that should not live only in an Issue or chat history.

Do not silently bypass accepted ADRs. If a decision changes:

- update or supersede the ADR;
- update affected source-of-truth documents;
- mention the decision in the Issue, Pull Request, and handover where relevant.

An ADR does not replace `PRODUCT.md`, `docs/UI_UX_CONCEPT.md`, or `ARCHITECTURE.md`; those documents must remain consistent with accepted decisions in their area of ownership.

## 2. Default Working Style
Agents have broad autonomy.
You may independently:
- choose implementation details;
- define internal APIs;
- add small abstractions;
- refactor when useful;
- choose appropriate Rust patterns;
- design tests;
- make reasonable schema decisions;
- improve naming and module structure;
- fix small nearby issues when clearly correct;
- update documentation when a decision has lasting significance.
Do not ask for permission for ordinary engineering decisions.
When uncertain:
1. inspect relevant documentation and nearby code;
2. infer the most consistent design;
3. choose the simplest reasonable solution;
4. keep it understandable and reversible;
5. continue.
Prefer progress over ceremony.
## 3. Decision Principle
Use this default rule:
> Choose the simplest design that satisfies the product requirement, fits the architecture, remains testable, and does not unnecessarily constrain known future extensions.
Prefer existing patterns when they are good enough.
Do not over-engineer hypothetical requirements.
Future extensibility matters especially around platform integration, emulation backends, metadata providers, persistence boundaries, and runtime/core management.
Extensibility does not require generic frameworks everywhere.
A concrete implementation behind a clear boundary is usually preferable to speculative abstraction.
## 4. Core Product Boundaries
### User-owned files
ROMs, ISOs, BIOS files, and firmware are external user-owned resources.
BitArchive must not silently move, rename, reorganize, convert, or delete them.
Only explicit product-defined actions may modify external files.
### Domain identity
Keep these concepts distinct:
```text
Game != Release != Content
```
Paths are locations, not domain identities.
Hashes are fingerprints, not domain identities or primary keys.
### Local-first
Do not add cloud dependencies, user accounts, telemetry, automatic crash reporting, or hidden background networking unless the product specification intentionally changes.
### Emulation and saves
RetroArch is the MVP backend, not the architectural identity of BitArchive.
BitArchive manages RetroArch save states in the MVP.
Normal SRAM, memory cards, and other regular RetroArch save files remain outside BitArchive scope.
### Scraping
Library scanning and metadata scraping are separate workflows.
## 5. Architectural Direction
BitArchive broadly follows:
```text
UI
 ↓
Application
 ↓
Domain
```
Infrastructure, emulation, and platform implementations sit outside the inner layers and implement required boundaries.
Preserve the intent even if modules evolve.
### Domain
Domain code should not depend directly on SQLite, Slint, OS APIs, HTTP clients, or RetroArch process details.
### Application
Application code owns use cases, orchestration, ports, workflows, and application services.
### Infrastructure
Infrastructure contains concrete persistence, HTTP, provider, download, media, backup, and similar adapters.
### Emulation
RetroArch-specific implementation belongs behind the emulation boundary.
### Platform
OS-specific functionality should remain behind platform abstractions where practical.
### UI
Slint is presentation only.
Slint components must not directly perform SQL, HTTP, filesystem scanning, RetroArch process management, or platform integration.
## 6. Architecture Invariants
Preserve these unless the architecture is intentionally changed:
1. `Game`, `Release`, and `Content` remain distinct.
2. Paths are not domain IDs.
3. Hashes are not primary keys.
4. External ROM, ISO, BIOS, and firmware files remain outside BitArchive ownership.
5. Slint does not directly call infrastructure.
6. Domain code does not depend on SQLite, Slint, or OS-specific types.
7. Generated RetroArch configuration is output, not source of truth.
8. Emulation remains backend-neutral at the architectural boundary.
9. Firmware requirements belong to cores.
10. Runtime and cores are independently versioned components.
11. App updates do not implicitly update runtime or cores.
12. Save states belong to `Release + Core + Core Version`.
13. Normal RetroArch saves remain outside BitArchive scope.
14. Scraping remains separate from library scanning.
15. Network activity belongs to explicit subsystems.
16. No telemetry or automatic external crash reporting in the MVP.
17. Background work must not block the UI thread.
18. Destructive scan reconciliation only happens after a successful full scan.
19. Invalid user overrides are not silently deleted.
20. Internal cleanup must never delete external user files.
21. Core resolution follows `System → Game → optional Release`; there is no global core default.
22. Frontend focus and restoration use stable domain IDs rather than list indices.
23. BitArchive must not consume app-navigation controller input in parallel with a foreground RetroArch session.
If an implementation conflicts with one of these rules, redesign the implementation rather than quietly weakening the rule.
## 7. Rust and Code Quality
Write idiomatic, readable Rust.
Prefer clarity over cleverness.
Use strong types where they improve correctness, especially for IDs and state.
Prefer explicit ownership and clear boundaries over broad shared mutable state.
`Arc<Mutex<T>>` is a tool, not the default architecture.
Avoid unnecessary `unsafe`.
If `unsafe` is required, keep it narrow and document the invariant that makes it sound.
Avoid panics for expected runtime conditions.
Use typed errors where callers need to react to failure categories.
Do not expose secrets in logs, `Debug`, or error messages.
## 8. Async, Concurrency, and I/O
Tokio is the main async runtime.
Keep the Slint UI thread responsive.
Do not perform expensive filesystem work, hashing, database work, or similar blocking operations directly on the UI thread.
Use bounded parallelism for large scans and other per-file work.
Do not spawn unbounded tasks.
Support cancellation for long-running workflows when meaningful.
Use clear transaction boundaries for database work and batch related writes where appropriate.
## 9. Persistence and Files
SQLite is the local primary database.
Treat persisted data as long-lived user state.
Use versioned forward migrations for schema changes.
Do not rewrite already-released migrations merely to make them prettier.
Use constraints where they protect real invariants.
Use transactions around logically atomic changes.
Generated files, caches, and launch artifacts must not become authoritative substitutes for persistent domain state.
Use central path abstractions rather than scattering platform-specific application paths through the codebase.
Cleanup must be conservative and limited to files BitArchive clearly owns.
## 10. Network, Secrets, and Downloads
Network access must have an explicit purpose.
Prefer the shared network layer over ad-hoc HTTP clients.
Respect timeouts, cancellation, retries, rate limits, and credential redaction where relevant.
Store user secrets through the intended secret-storage boundary.
Never log passwords, tokens, secret headers, or equivalent credentials.
Do not weaken runtime/core signature or integrity verification for convenience.
Launch external processes through process APIs rather than shell interpolation.
## 11. Runtime, Core, Config, and Save-State Rules
Runtime and core versions are immutable managed components.
Keep runtime and core management separate even when they share infrastructure.
Launch preparation should be deterministic and explicit.
Firmware readiness must be based on indexed firmware content and core requirements.
Do not silently copy, rename, or repair user firmware.
RetroArch configuration is structured BitArchive state.
Generated `.cfg` files are output artifacts.
Preserve configuration inheritance:
```text
Global → System → Game
```
Core options use:
```text
Core Defaults → System → Game
```
Invalid stored overrides should be preserved for diagnosis rather than silently deleted.
RetroArch remains the technical source of save-state files.
Save-state compatibility should remain conservative.
Preserve the single-active-session MVP rule unless the product changes.
## 12. Testing
Add tests where they provide meaningful confidence.
Prefer:
- unit tests for deterministic domain rules;
- integration tests for SQLite, filesystem behavior, orchestration, and adapter interaction;
- fixtures or golden tests for parsers and generated output;
- property tests for invariant-heavy logic.
Normal tests should not depend on live ScreenScraper or other unreliable external services.
Process integration should normally use fake process adapters.
When fixing a bug, add a regression test when practical.
Do not write tests only to satisfy a quota.
## 13. Quality Checks
Use the checks available in the repository.
Typically:
```text
cargo fmt
cargo clippy
cargo test
```
Run narrower checks while iterating when useful.
Run broader workspace checks before finishing substantial changes when practical.
Do not leave known compiler errors, failing tests, or newly introduced warnings without explanation.
The repository's real CI configuration takes precedence over examples here.
## 14. Refactoring and Dependencies
Refactoring is allowed when it improves the current work.
Avoid unrelated large rewrites unless necessary.
Small nearby improvements are welcome when they clearly improve correctness, clarity, ownership, or testability.
Avoid speculative framework-building.
New dependencies are allowed when they provide clear value.
Before adding one, consider whether an existing dependency already solves the problem, maintenance quality, platform support, licensing, and runtime or binary impact.
Do not introduce JavaScript, Node, npm, WebViews, or another frontend/runtime stack unless the architecture is intentionally changed.
## 15. Documentation and Scope
Update documentation when a change materially alters product behavior, architecture, persistence semantics, major subsystem behavior, public project conventions, or MVP scope.
Do not document every internal implementation detail.
Use `PRODUCT.md` to distinguish MVP scope from later ideas.
Use `docs/UI_UX_CONCEPT.md` for interaction, navigation, focus, and presentation rules; do not implement `FAVORED`, `OPEN`, or `DEFERRED` items as binding requirements unless the Issue intentionally resolves them.
Do not accidentally turn a future feature into an MVP dependency.
Avoid unnecessary design choices that block known future extensions when a simple boundary prevents that problem.
## 16. Missing Design Details
Not every implementation detail must be specified before coding.
When a required detail is missing:
- inspect the product intent;
- inspect architecture and nearby code;
- choose a reasonable design;
- keep it simple;
- preserve important invariants;
- document the decision only if it has lasting significance.
A missing dedicated design document is not automatically a blocker.
If a task requires part of the data model before a complete `DATA_MODEL.md` exists, derive the necessary model consistently and capture important decisions as part of the work.
## 17. When to Ask Instead of Decide
The default is to decide independently.
Ask only when proceeding requires a genuinely product-level choice that cannot be responsibly inferred, especially when it is:
- irreversible or destructive to user data;
- security-sensitive;
- legally or licensing-sensitive;
- a clear contradiction between authoritative requirements;
- a major expansion or removal of user-facing scope;
- a deliberate break from a core architecture invariant.
Do not escalate routine choices about naming, APIs, schemas, modules, tests, implementation strategy, or internal abstractions.
When a reasonable reversible assumption exists, make it and continue.
## 18. Definition of Done
A change is done when:
- the requested behavior works;
- relevant existing behavior still works;
- important invariants are preserved;
- appropriate tests pass;
- formatting and static checks are clean where applicable;
- user secrets and external files are handled safely;
- documentation is updated when materially affected;
- the implementation is understandable to the next contributor.
Perfection is not required.
Prefer a small, correct, maintainable solution over unnecessary complexity.
## Final Principle
BitArchive agents should behave like trusted engineers, not constrained code generators.
Understand the intent.
Use the architecture.
Protect user data.
Make decisions.
Test what matters.
Keep the design simple.
Leave the codebase better than you found it.
