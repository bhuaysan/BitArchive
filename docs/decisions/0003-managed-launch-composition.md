# ADR 0003 — Managed launch composition

- **Status:** Accepted
- **Date:** 2026-09-21
- **Issue:** #23 (B7 — First real managed RetroArch launch)
- **Supersedes:** —
- **Superseded by:** —

## Context

By B4 the technical launch path existed as separate, reviewed pieces:

```text
RetroArchLaunchInput        (bitarchive-emulation,  Issue #13)
        ↓
RetroArchBackend::prepare_launch
        ↓
PreparedLaunch              (bitarchive-application, Issue #11)
        ↓
ProcessController::spawn    (bitarchive-platform,   Issue #15)
        ↓
SpawnedProcess
```

By B6 both components it needs were managed, versioned, immutable, and resolvable:

```text
RuntimeDefinition → ComponentStore → ManagedRuntime::executable_path()   (Issue #19)
CoreDefinition    → CoreStore      → ManagedCore::library_path()          (Issue #21)
```

Nothing had ever joined the two paths. The composition root
(`bitarchive-desktop`, `ARCHITECTURE.md` §5.7) started the UI, the runtime
acquisition command, or the core acquisition command — never a real RetroArch
process.

Issue #17 and PR #18 proposed the first launch as a developer smoke launcher that
took three concrete paths:

```text
bitarchive-desktop <retroarch-executable> <core-library> <content>
```

That Issue was closed as *not planned*. Its design predates B5/B6 and contradicts
them: after B5/B6 the runtime and the core are BitArchive-managed components with a
pinned identity, an immutable installation, and a deterministic resolution step
each. A developer-supplied path would make "which RetroArch ran?" and "which core
ran?" unanswerable and would reintroduce exactly the `PATH`, `/Applications`, and
local-file fallbacks the managed component design removed.

This decision records how the launch path resolves a runtime and a core, what a
developer may still supply, and what a launch may not do.

## Decision

### 1. A launch resolves both components from the managed component stores, and from nowhere else

```text
pinned_retroarch_runtime()  → retroarch_runtime::resolve(store) → executable_path()
curated_core(mgba, host)    → CoreStore::resolve(definition)    → library_path()
```

There is no fallback of any kind: no developer argument, no user setting, no
`PATH` lookup, no `/Applications`, no `/usr/local/bin`, no system RetroArch, no
arbitrary local core file, and no scanning of the store for "some" build. The core
is asked for by the pinned `CoreDefinition` of the host architecture, so another
build, another architecture, and another core are never substituted. This is the
launch-path half of the rule B5/B6 already apply to installation.

### 2. The composition root composes; the layers keep their responsibilities

`bitarchive-desktop` is the composition root, so the two resolutions, the content,
and the launch path are joined there — once:

```text
resolve managed runtime ─┐
resolve managed core ────┼→ RetroArchLaunchInput
developer content ───────┘        ↓
                        RetroArchBackend::prepare_launch()   ← only place that spells the CLI
                                  ↓
                            PreparedLaunch
                                  ↓
                        ProcessController::spawn()           ← only place that starts a process
                                  ↓
                            SpawnedProcess::wait()
```

The RetroArch command-line shape (`-L <core> <content>`, optional `--config`) stays
the sole responsibility of `RetroArchBackend`; the composition root assembles the
input and never builds arguments. Executable and arguments stay separate values and
no shell takes part (`ARCHITECTURE.md` §20.3). There is no second launch pipeline,
no `LaunchService`, and no `EmulationBackend` port: the product orchestration that
would consume one — release resolution, readiness, configuration, sessions — does
not exist yet, and a port defined before its consumer would only guess at the
boundary.

### 3. A launch never acquires a component

Acquisition is an explicit, deliberate step (ADR 0001, ADR 0002): it is the only
network activity in BitArchive, it is opt-in, and CI never runs it. A launch that
downloaded a runtime or a core would make what is running depend on network timing,
would create a second acquisition flow beside the reviewed one, and would blend two
lifecycle steps that have different failure modes. A component that is **not
installed** is therefore reported together with the existing command that installs
it, resolved against the same store the launch used:

```text
launch <content>                    → cargo run -p bitarchive-desktop -- acquire-retroarch-runtime
                                      cargo run -p bitarchive-desktop -- acquire-core
launch <content> --root /tmp/store  → the same commands with --root /tmp/store
```

The advice is deliberately narrow: only "not installed" has an installing answer. A
build directory that exists without its library is a broken installation, and the
store refuses to install a build that is already installed — so that failure is
reported for diagnosis instead of being answered with a command that cannot repair
it. No repair or force-reinstall operation exists.

### 4. The developer slice supplies content only

```text
bitarchive-desktop launch <content> [--root <dir>]
```

`<content>` is the one launch input the developer supplies, and the only thing
BitArchive does not own: content is a user-owned external file (`AGENTS.md` §4). It
is checked for existence and passed to the launch path unchanged — never acquired,
copied, renamed, hashed, imported, or scanned. Contents of a ROM library, imports,
and content hashing are later workflows.

`--root <dir>` redirects the component store root, exactly as the two acquisition
commands already allow, so the launch path can be verified against a throwaway
store. It names a store, not a component: it cannot express a runtime or a core.
The store it selects is also the store the recovery advice names, so a `--root` run
is told to repair the same root (§3).

An argument that names a component (`--retroarch`, `--runtime`, `--core`,
`--core-library`, `--core-path`, `--core-url`, `--core-version`, `--runtime-url`,
`--runtime-version`, …) is rejected.

### 5. Waiting is the whole process lifecycle in this step

The developer command reports the PID, waits for the process through the existing
`SpawnedProcess` handle, and reports the exit code the process ended with as its own
exit code, so a script or a developer sees the number the launched process reported.
A code `std::process::ExitCode` cannot carry (one outside the `0–255` range of its
`u8`), and a process that reported no code at all — a signal termination, for
example — are answered with the generic failure code instead of a truncated or
invented number. The command does not signal, terminate, or kill the process, keeps
no registry, persists nothing, and measures no playtime. Exclusivity, recovery,
persistence, and ordered shutdown belong to the session lifecycle
(`ARCHITECTURE.md` §22), which is not implemented yet.

## Consequences

**Positive**

- The first real launch goes through the architecture that the product will use, so
  it validates the real dependency direction instead of a parallel test path.
- "Which runtime and which core started?" has exactly one answer, derived from the
  pinned definitions and the installed components.
- The launch path performs no network I/O, so a launch is reproducible, offline,
  and cannot be redirected to an unverified binary by an argument or an
  environment change.
- The whole composition is covered by offline tests: the resolved executable, the
  resolved library, and the content produce an exact `PreparedLaunch`, and the
  wait path is exercised against a controlled child process.

**Negative / accepted**

- The developer verification needs both components installed first, which means two
  deliberate acquisition runs before the first launch. This is the price of not
  having a second acquisition flow inside the launch path.
- A developer cannot point BitArchive at a RetroArch or a core they already have,
  not even for debugging. That is the same rule the product needs, and a debugging
  override would be the first step back to #17.
- Still no product play flow: no UI trigger, no readiness check, no configuration
  generation, no core options, no session, no save states. Those remain later
  Issues, and this decision deliberately does not pre-empt them.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Implement #17 again: accept `<retroarch-executable> <core-library> <content>` | Contradicts the managed component architecture of B5/B6 and makes the origin of the started binary unverifiable. The Issue was closed as *not planned* for that reason. |
| Fall back to a system RetroArch or a local core when nothing is installed | `PRODUCT.md` §16.1/§16.3 make BitArchive own the runtime's version, install state, and integrity; a fallback silently undoes that. |
| Acquire a missing component from inside the launch command | Creates a second acquisition flow, makes a launch depend on the network, and hides a lifecycle step the user should trigger deliberately. |
| Resolve whichever core build the store happens to contain | Answers a different question than the pinned definition, and could start a build the user never installed deliberately. |
| Introduce an `EmulationBackend` port plus a `LaunchService` now | The consumer — the product play flow with readiness, configuration, and sessions — does not exist; a port invented before it would guess at the boundary (`AGENTS.md` §3). |
| A Cargo example target instead of a subcommand | The composition root is the binary; the two existing developer commands already live there, and an example would be a second entry point with its own argument handling. |
| A shell command line (`sh -c "retroarch -L …"`) | `ARCHITECTURE.md` §20.3 and `AGENTS.md` §10 forbid shell interpolation; separate arguments keep paths with spaces exact. |
| Let the developer command continue in the background and return immediately | The slice must report how the started process ended; a detached process would also leave an unobserved child behind. |

## References

- `ARCHITECTURE.md` §5.7 (composition root), §20 (launch flow, `PreparedLaunch`, no
  shell), §21 (readiness), §22 (session management), §23 (runtime and core
  management), §53 (invariants 10, 11, 21)
- `PRODUCT.md` §16 (runtime management), §27 (session behaviour)
- `AGENTS.md` §4 (user-owned files), §10 (network, downloads, no shell launch), §11
  (runtime, core, config, and save-state rules)
- ADR 0001 — Managed RetroArch runtime acquisition
- ADR 0002 — Managed core acquisition
- Issue #23 (B7), Issue #17 and PR #18 (discarded approach)
