# ADR 0004 — Launch readiness and game launch preparation

- **Status:** Accepted
- **Date:** 2026-09-22
- **Issue:** #29 (B8 — Launch Readiness / Game Launch Preparation)
- **Supersedes:** —
- **Superseded by:** —

*Revised during the PR #30 review: the blocked state is now structurally non-empty
(§1), runtime reasons separate "not installed" from "unusable" (§3), an unusable stored
configuration key has its own readiness category instead of being reported as invalid
content (§2, §6), the configuration precedence is defined by the scope rather than by
the caller's order (§6), and the configuration seam is documented as not yet rendered
into a RetroArch configuration file (§4).*

## Context

B7 (Issue #23, ADR 0003) proved the real managed launch path end to end: a
developer-supplied content path, the active managed RetroArch runtime, and the
curated mGBA build are composed into a `PreparedLaunch` and started as a real
process.

What B7 deliberately left open is the *product decision in front of that path*:

```text
Can this concrete game be started with the currently configured
and installed BitArchive state?
```

B7 answers "how is a launch started". Nothing answered "may it be started, and
with which inputs" for a *game*. `LaunchReadiness` and `ReadinessIssue` existed as
vocabulary since Issue #11, but no step produced them, and the categories they name
(`SourceOffline`, `SourcePermissionDenied`, `InvalidContent`, `SessionActive`, …)
describe a library, a filesystem source model, and a session registry that do not
exist yet.

At the same time, three facts had become true and had never been joined:

- the curated mGBA core serves Game Boy Advance, Game Boy Color, and Game Boy, so a
  content format can select a system and a system can select a core;
- a core can have firmware expectations, and mGBA's Game Boy Advance external BIOS
  is *optional* upstream, so firmware must not block a GBA launch;
- `Global → System → Game` is the product's configuration hierarchy, and no launch
  step had ever applied it.

This decision records how a launch is decided and prepared, where that step ends,
what a readiness result may claim, and what firmware readiness means when BitArchive
has no verified digest source for a commercial BIOS.

## Decision

### 1. Readiness is a structured result of a negotiation, not an error

A launch is decided by one call:

```text
GameLaunchRequest + observed state + FirmwareChecker
        ↓
GameLaunchPlan
   ├── prepared: PreparedGameLaunch        → the launch may proceed
   └── blocked:  Vec<LaunchBlocker>        → at least one reason
```

The negotiation never fails. Every condition it reports is a *state* — something is
not installed, not available, or not curated — and a caller needs all of them at
once, not the first one an error type would report.

**`Blocked` is structurally non-empty.** The preparation's state is private and a
blocked one can only be built from a non-empty blocker collection, so

```text
LaunchPreparation::Blocked(vec![])        ← not expressible
Blocked(no reasons) + readiness() == Ready ← not expressible
```

have no representation, not even for external code:

```rust
pub struct LaunchPreparation(Preparation);       // private state

enum Preparation {
    Blocked(BlockingReasons),                    // private wrapper, non-empty
    Prepared(Box<PreparedGameLaunch>),
}
```

`from_blockers` answers `Option<Self>` and returns `None` for an empty list, so a
caller that collected no reason cannot obtain a blocked preparation by accident
either. `LaunchReadiness` is *derived* from the preparation rather than stored beside
it, so a blocked preparation is never ready and a ready one never names a blocker.
No runtime assertion and no `unsafe` is involved: the invariant is carried by the
type, as it already is for `LaunchReadiness` itself.

A `LaunchBlocker` carries structure and no presentation: an identity, a platform, a
list of file names. It has no severity, no ordering, no user-facing text, and no
recovery action. Rendering it is the composition root's or the UI's job
(`ARCHITECTURE.md` §21).

### 2. A readiness check reports only what it can observe

The blocker set is a strict subset of the readiness categories `ARCHITECTURE.md`
§21 names:

```text
RuntimeMissing              → RuntimeUnavailable
UnsupportedSystemOrCore     → UnsupportedFormat
CoreUnusable                → CoreMissing
ContentMissing              → ContentUnavailable
FirmwareMissing             → FirmwareMissing
InvalidConfiguration        → InvalidConfiguration
```

`InvalidConfiguration` is the category this decision adds to §21 and a launch decision
genuinely needs: the configuration hierarchy is part of the launch path, and a stored
override can be unusable. It is deliberately **not** mapped to `InvalidContent`: an
unusable stored key says nothing about the bytes to launch, and a UI that reported
invalid content would offer the wrong action.

`SourceOffline`, `SourcePermissionDenied`, `CoreIntegrityFailure`,
`FirmwareWrongContent`, `FirmwareWrongFilename`, `InvalidContent`, and
`SessionActive` are **not** produced. Each needs a library index, a filesystem
source model, a hash-verified firmware index, content validation, or a session
registry, and none of those exists. A readiness check that reported them would be
claiming to have checked something it did not check.

Categories are not interchangeable: a condition is reported as the category whose
*repair* matches, never as a neighbouring one.

A capability that is not implemented must not appear as a satisfied check either.
The reason a source cannot be offline is that there is no source yet, not that the
source was reached.

### 3. The decision lives in the application layer, the state arrives through ports

```text
bitarchive-domain        curated systems, core selection, firmware requirements,
                         configuration precedence        ← rules, no I/O
        ↓
bitarchive-application   GameLaunchRequest, negotiation, LaunchBlocker,
                         ManagedEmulationState + FirmwareChecker ports
        ↓
bitarchive-emulation     ManagedEmulation: the component stores, translated
bitarchive-infrastructure FilesystemFirmwareChecker: the one firmware folder
        ↓
bitarchive-desktop       composition root: developer command `prepare`
```

Two ports carry everything the decision reads, and nothing else does:

- `ManagedEmulationState::runtime()` and `::core_for_system()` — the managed runtime
  and the curated core build, resolved from the component stores;
- `FirmwareChecker::available()` — which of the file names a core asked for are
  present.

The ports answer in the **application layer's own vocabulary**, not in store error
types: an unusable runtime is `LaunchRuntimeResolution::Unavailable` with a
`RuntimeUnavailableReason`, not a `RetroArchRuntimeError`; a core is an identity plus
a directory and a library, not a `ManagedCore`. The adapter that knows how to read a
store translates its failures, and that translation is the only place the two
vocabularies meet.

### "Not installed" is not "unusable"

Both component classes keep that distinction, in their own vocabularies:

```text
runtime   RuntimeUnavailableReason::{NotInstalled, ExecutableMissing, StoreUnreadable}
core      CoreUnusableReason::{NotInstalled, Unusable}
```

They are different repairs: `NotInstalled` is what the existing acquisition command
installs, while a broken installation or an unreadable store needs diagnosis — and
the stores refuse to install a component they already have, so re-installing is not
the answer. `RuntimeUnavailableReason::is_installable()` is the single place that
decision is made, and the presentation layer reads it instead of guessing from a
string.

### Reasons are typed, never rendered

No application or domain type carries a user-facing sentence. A `LaunchBlocker` holds
identities, platforms, paths, versions, file names, and typed reasons:

```rust
enum UnsupportedSystemOrCoreReason {
    UnsupportedContentFormat,
    SystemNotCurated,
    CoreNotCuratedForPlatform { component_id: String, platform: CorePlatform },
}

enum RuntimeUnavailableReason {
    NotInstalled,
    ExecutableMissing { expected: PathBuf, version: String },
    StoreUnreadable,
}
```

Granularity stays deliberately small: a reason names the condition a caller must act
on, not a copy of every infrastructure error. Phrasing, localization, and the choice
of a recovery command belong to the presentation layer
(`apps/bitarchive-desktop/src/prepare.rs` today, the UI later).

### 4. The step ends at `ready + prepared`, and never start a process

```text
GameLaunchRequest
        ↓
system of the content              (curated list, from the content extension)
        ↓
curated core + installation        (managed core store)
        ↓
managed runtime                    (managed RetroArch runtime)
        ↓
firmware readiness                 (core requirements vs. available files)
        ↓
effective launch configuration     (Global → System → Game)
        ↓
PreparedGameLaunch                 ← B8 ends here
        ↓
RetroArchLaunchInput → RetroArchBackend::prepare_launch → PreparedLaunch
        ↓
ProcessController::spawn           ← B7, not B8
```

`PreparedGameLaunch` is backend-neutral and holds no process: the runtime
(identity, version, managed executable), the curated `CoreDefinition` and the
installed build (directory, library), the exact content path, the effective
configuration, and the firmware outcomes. A value exists only for a launch that is
ready — it is built after every question was answered — so holding one is evidence
that the decision was made.

`RetroArchBackend` remains the sole owner of the RetroArch command line. B8
composes its input and spells no argument; the composition root calls
`prepare_launch` only to show what the later play flow would start.

**What the seam carries today, and what it does not:**

```text
PreparedGameLaunch
  ├── runtime executable ─┐
  ├── core library ───────┼→ RetroArchLaunchInput → RetroArchBackend::prepare_launch
  ├── content ────────────┘                               ↓
  │                                                  PreparedLaunch
  │                                                       ↓
  │                                 ProcessController::spawn  ← B7, not B8
  └── effective configuration
           ↓ carried as product preparation state — NOT part of RetroArchLaunchInput
```

The effective configuration is resolved and carried, and it is **not yet handed to
RetroArch**. Reaching RetroArch with a configuration means rendering a `.cfg` file
whose path the launch input names, and no `.cfg` is generated by B8: `ARCHITECTURE.md`
§28 makes a generated launch configuration a session artifact, and no session exists.
The prepared RetroArch input therefore names no configuration file, and RetroArch keeps
its own lookup rules until the launch-artifact step exists. Documentation and output
state this rather than implying that the configuration already reaches RetroArch.

### 5. Firmware readiness is file-name presence, and required firmware is the exception

A `FirmwareRequirement` belongs to a `CoreDefinition` (invariant 9) and records:

```text
level              Required | Optional
expected_filenames the names the core looks for
systems            the systems it applies to (empty = all the core serves)
```

It carries **no accepted hash list**. `ARCHITECTURE.md` §25.2 shows
`accepted_hashes`, but BitArchive has no reviewed source for the SHA-256 of a
commercial BIOS dump, and inventing one would be a false statement about bytes
BitArchive has never verified. So readiness answers exactly one question — is a file
with an expected name present? — and:

- a missing **optional** firmware file never blocks a launch; the prepared launch
  records it in a `FirmwareOutcome` so a caller can show it;
- a missing **required** firmware file blocks the launch;
- an **unreadable** firmware location counts as "not available", so a required
  requirement blocks and a launch never claims to be ready on the strength of a
  check that did not happen;
- nothing is ever downloaded, copied, renamed, repaired, or hashed, and no ROM or
  BIOS file is added to the repository.

The curated mGBA pin records exactly one firmware entry — `gba_bios.bin`,
`Optional`, for Game Boy Advance — which is upstream's own behaviour and the reason
a GBA launch works without a BIOS. Required-firmware blocking is implemented and
tested against a synthetic definition, because no curated core requires external
firmware today; no artificial required case is added to the product to make the code
path reachable.

Hash-based firmware identification is deferred until a reviewable source of truth
for a digest exists.

### 6. The effective configuration follows `Global → System → Game`, and an invalid override is preserved

```text
Global → System → Game        game > system > global
```

The rule is domain code (`resolve_launch_config`). **The scope defines the
precedence, never the caller's iteration order:** the resolution groups the sources by
their `ConfigScope` and applies them in `ConfigScope::precedence()` order — `Global`,
then `System`, then `Game` — whatever order they were supplied in. A caller cannot
invert the hierarchy by iterating differently, because its order is not what the rule
reads. The order *within* one scope stays the caller's, so several sources of the same
scope (a global settings record and a global default, for instance) still resolve
deterministically: the last one wins.

The result reports for every effective key which scope supplied it and which less
specific scopes it replaced, least specific first. A conflict is not an error: an
intentional game override *is* a conflict with a system value, and the hierarchy exists
to resolve it.

RetroArch settings deliberately have **no release scope**, unlike core selection
(`ARCHITECTURE.md` §20.4, `PRODUCT.md` §17.2).

A stored key that is not a usable technical key neither disappears nor silently
takes part: the resolution reports it and still applies every usable override, and the
launch reports `InvalidConfiguration` while the stored value stays preserved
(invariant 19).

What a launch configuration *is* remains deliberately small: typed key/value
overrides, no `SettingDefinition` registry, no defaults, no knowledge of which scopes
a concrete RetroArch key allows. That belongs to the settings subsystem, which does
not exist. Generated `.cfg` files remain output artifacts and are not produced by
this step (`ARCHITECTURE.md` §2.6, §28).

### 7. A system is selected by the content format, from a curated list

`EmulatedSystemKey` is a stable curated slug (`gba`, `gbc`, `gb`), not a `SystemId`
UUID, and the curated list contains exactly the systems the one curated core serves.
Listing NES, SNES, or PlayStation would promise a system that no curated core can
start. The extension decides, case-insensitively; a directory name, a file stem, or a
missing extension selects nothing, and no system is ever guessed.

Core resolution stays deterministic: `System → one curated definition for the host
platform`, through the existing allowlist. There is no catalogue search, no scoring,
no "start with…", and no plugin registry. A system whose core is not curated is a
structured `UnsupportedSystemOrCore` blocker.

## Consequences

**Positive**

- The product question is answered by one testable, offline call whose whole input
  arrives through two ports, so every state is exercised with fakes and no
  filesystem, network, ROM, BIOS, or GUI.
- A blocked launch reports *everything* that is wrong at once, and each reason names
  the component, the platform, or the files that need attention — which is what the
  later UI needs to offer the right action.
- Readiness cannot overstate itself: the blocker set is exactly what the code can
  observe, and the mapping to the readiness categories stays total.
- A blocked preparation without a reason and a readiness that contradicts its own
  preparation are both unrepresentable, so no caller needs a fallback for either.
- The decision a caller makes — install the component, or diagnose it — reads a typed
  reason, so it cannot be wrong because of how a state was phrased.
- "Which RetroArch and which core would run, with which content and which
  configuration?" has one derived answer, taken from the same managed components
  B7 starts and from no other source.
- Firmware readiness is honest: it answers a question BitArchive can actually
  answer, blocks real required cases, and never blocks for optional firmware.

**Negative / accepted**

- The content path is still supplied by the caller, and the release identity is
  supplied by the composition root, because content resolution and the library do
  not exist. When they do, they produce the request instead of a developer, and the
  release identity stops being an input.
- Required-firmware blocking is proven by a synthetic definition, not by a real
  curated core. The mechanism is real; the product case is not there yet.
- Firmware is matched by file name only. A file with the right name and wrong content
  is reported as present; `FirmwareWrongContent` and `FirmwareWrongFilename` remain
  unimplemented on purpose.
- The step is a developer command, not a product flow: no play button, no session, no
  generated configuration file, no content validation. Each of those is a later
  Issue, and this decision deliberately does not pre-empt them.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Return `Result<PreparedGameLaunch, LaunchError>` | A launch usually has several reasons at once (missing runtime *and* missing core), and an error type reports the first one. A blocked launch is a state, not an exception. |
| Report every category `ARCHITECTURE.md` §21 lists | BitArchive cannot observe source availability, permissions, content validity, core integrity, or an active session yet. Reporting them would claim checks that did not run. |
| Keep `ReadinessIssue` as the only readiness type and put the detail in log messages | The caller needs the detail — which component, which file — to offer an action; text is not a contract, and a readiness result has no presentation. |
| Add a `SystemId` for the curated systems | A `SystemId` is the UUIDv7 a persisted release carries. There is no store to make one stable across sessions, and paths or slugs are not domain identities (invariant 2/3). The curated slug is what the catalogue needs. |
| Model accepted firmware hashes from a public list | No reviewable source with clear provenance, and a wrong digest is a false statement about the user's file. Presence by name is what can be verified honestly. |
| Make a missing optional firmware file a *warning* blocker | A warning category would be a second severity in a model whose contract is "does this block the launch". The prepared launch already records the absent optional file as an outcome. |
| Generate the RetroArch `.cfg` in this step | `ARCHITECTURE.md` §28 makes launch artifacts session artifacts; generating one before a session exists would create an unbounded artifact with no owner. The effective configuration is the value; rendering it is a later step. |
| Reuse the process adapter for a "dry run" spawn | Starting anything contradicts the point of the step, and a dry run would need the very session machinery the product does not have yet. The prepared launch is printed instead. |
| Let the readiness check acquire a missing component | Creates a second acquisition flow beside the reviewed one and makes a decision depend on the network (ADR 0001, ADR 0002, ADR 0003). |
| Keep `Blocked { blockers: Vec<LaunchBlocker> }` public and document the non-empty rule | A documented invariant that the public API contradicts is not an invariant: external code could build a blocked preparation with no reason, and `readiness()` would then report ready for a blocked plan. |
| Report a configuration problem as `InvalidContent` | Claims the content bytes are invalid although they were never checked, and would make a UI offer a content action for a stored settings problem. |
| Let the caller define the configuration precedence by supplying scopes in order | Makes a product invariant (`game > system > global`) depend on every caller's discipline instead of on the scope, so one differently-ordered call would silently invert the hierarchy. |
| Render a `.cfg` path into the prepared RetroArch input now | No step generates that file, and `ARCHITECTURE.md` §28 makes a generated launch configuration a session artifact. A path that no step produced would be a false statement about what RetroArch loads. |
| Fall back to a system RetroArch or a developer-supplied core when the store has none | Undoes the managed-component invariant B5/B6/B7 established and makes "which binary ran?" unanswerable (invariant 10). |

## References

- `ARCHITECTURE.md` §19 (emulation backend), §20 (launch flow, `PreparedLaunch`,
  core resolution, no shell), §21 (readiness), §23 (runtime and core management),
  §25 (firmware, core requirements, readiness), §26 (settings inheritance), §28
  (generated launch configuration), §53 (invariants 9, 10, 19, 21)
- `PRODUCT.md` §13 (supported systems), §14 (system metadata), §16 (runtime
  management), §17 (core management, system defaults and overrides)
- `AGENTS.md` §4 (user-owned files), §7 (typed errors), §11 (runtime, core, config,
  and firmware rules)
- ADR 0001 — Managed RetroArch runtime acquisition
- ADR 0002 — Managed core acquisition
- ADR 0003 — Managed launch composition
- Issue #29 (B8), Issue #23 (B7, the process-launch proof)
