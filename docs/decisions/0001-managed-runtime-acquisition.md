# ADR 0001 — Managed RetroArch runtime acquisition

- **Status:** Accepted
- **Date:** 2026-09-16
- **Issue:** #19 (B5 — Managed RetroArch Runtime Acquisition)
- **Supersedes:** —
- **Superseded by:** —

## Context

BitArchive manages RetroArch as a runtime component. `PRODUCT.md` §16.1 states
that BitArchive manages its own RetroArch installation and that an existing
RetroArch installation of the user is not required for normal operation;
`ARCHITECTURE.md` §23 defines runtime and core components as versioned and
immutable, with the flow `Download → Verify → Stage → Install → Activate →
Rollback`.

Before this decision the launch path could already prepare and start a launch
(`RetroArchLaunchInput → prepare_launch → PreparedLaunch → ProcessController::spawn`),
but nothing supplied the runtime it starts. There was no runtime definition, no
HTTP layer, no component store, no path service, and no `bitarchive-infrastructure`
crate.

`ARCHITECTURE.md` §24 also requires signed distribution manifests, and that
infrastructure does not exist. The format, key distribution, and key management
are still listed as open in `PRODUCT.md` §45.

## Decision

This step establishes managed runtime acquisition for **one pinned RetroArch
version on macOS**, with these choices:

### 1. The pinned definition is the trust anchor, and it is code

The definition lives in `bitarchive-emulation::runtime` as reviewed constants and
not as data fetched at run time:

```text
RuntimeId:      retroarch
Version:        1.22.2
Platform:       macos-universal
Source:         https://buildbot.libretro.com/stable/1.22.2/apple/osx/universal/RetroArch_Metal.dmg
SHA-256:        81b79121ba26d539064ae13b4d0419a120c3d165afbe656cf5f5412b15fdb434
Artifact kind:  apple-disk-image { bundle: "RetroArch.app" }
Executable:     RetroArch.app/Contents/MacOS/RetroArch
License:        RetroArch · libretro/RetroArch · GPL-3.0-only
```

There is no `latest` resolution, no version discovery, and no digest taken from a
response. A version bump means editing the version, the URL, and the digest
together, which is what makes the change reviewable.

The definition is not placed in `resources/catalog/`: `ARCHITECTURE.md` §15
requires the curated catalog to stay strictly separate from online distribution
manifests.

### 2. Signature verification is deferred, and nothing is weakened by deferring it

`ARCHITECTURE.md` §24 and `PRODUCT.md` §18.2 require signed manifests. That
infrastructure — key generation, distribution, storage, and rotation — is not
part of this step, and inventing it here would produce a key that a later,
properly designed scheme would have to replace.

Until it exists, SHA-256 pinning is the integrity guarantee:

- the expected digest is part of the BitArchive-side definition;
- the downloader hashes the bytes **as they are written** and discards them when
  they do not match (`PRODUCT.md` §18.3: installation blocked, artifact discarded);
- the download comes only from the official host over https, and redirects are
  refused.

No verification is weakened by this decision, because no signature verification
existed to weaken. The seam is the `ArtifactDownloader` boundary: a signed
manifest check belongs before any bytes are trusted, which is exactly there.

**Follow-up:** the Issue that introduces signed distribution manifests replaces
this trust anchor and must not silently keep the pinned digest as the only check.

### 3. A synchronous, narrowly scoped HTTP client for this step

`ARCHITECTURE.md` §31 describes a central network layer built on `reqwest`, and
§48 states that network access is asynchronous. That layer is the target.

This step uses the synchronous `ureq` client instead, because:

- the only caller is an opt-in developer command with no UI, no `JobManager`, and
  no progress reporting, so there is nothing to keep responsive and nothing to
  cancel;
- it downloads one artifact from one pinned host with bounded timeouts;
- introducing Tokio and an async client stack before anything consumes them adds
  a runtime, a scheduler, and a dependency tree for no behavioural gain — the
  case Issue #19 explicitly excludes.

The consequence is accepted deliberately: this downloader cannot report progress
and is not integrated into the `JobManager`. When downloads move into the
`JobManager`, the `ArtifactDownloader` port stays as it is and only the adapter
behind it is replaced by the shared layer. The module documentation of
`http_download` records the same reasoning next to the code.

### 4. The store layout puts the platform below the component class

```text
<component root>/
├── runtime/<platform>/<version>/…
├── staging/install-<id>-<version>-<platform>/
└── artifacts/<id>-<version>-<platform>
```

`ARCHITECTURE.md` §23.1 shows `runtime/<version>/`. Inserting the platform keeps a
later platform from silently reusing another platform's installation, and keeps
the `cores/<core-id>/<version>/` tree that §23.1 also defines independent of it.

Installation is one rename of a fully staged payload; activation replaces a small
record file with one rename. Both are single operations on one filesystem, so a
crash leaves either the previous state or the new state. A version directory is
never written again: re-installing an installed version is refused.

### 5. Staging lives inside the store, not in the cache directory

`ARCHITECTURE.md` §35 puts `staging/` and `downloads/` under
`~/Library/Caches/BitArchive/` and the component store under
`~/Library/Application Support/BitArchive/`.

The installer stages **inside the store root** so that the install step is a
same-filesystem rename and therefore genuinely atomic. The document's cache
staging directory remains the place for work that does not have to be atomic with
the store. Staging is transient on every path: it is removed after a successful
install and after a failure.

### 6. Reading the disk image goes through a port, and through no shell

The official macOS artifact is a `.dmg`. It is mounted read-only by the
platform's `/usr/bin/hdiutil`, started through `std::process::Command` with an
absolute program path and one argument per value — never through a shell
(`ARCHITECTURE.md` §20.3).

A pure-Rust disk-image reader was rejected: it would mean reimplementing the UDIF
container, its compression, and a file system for one artifact whose integrity the
pinned digest covers. The bundle is copied with `std::fs`, and symbolic links are
reproduced as links so the `Applications` link inside the image is never followed.

The copy is deliberately not done with `ditto` or `unzip`: one external tool that
has no in-process alternative is the minimum, and the copy has one.

### 7. The activation record is a small text file

`ARCHITECTURE.md` §6.1 names a "Component Registry" and §54 lists `Components` as
a future data-model deliverable, but no schema exists and this step may not
introduce SQLite (`Issue #19`).

The registry is therefore derived from the filesystem plus one record per runtime:

```text
components/runtime/retroarch/active
    version=1.22.2
    platform=macos-universal
    executable=RetroArch.app/Contents/MacOS/RetroArch
```

The installed set is derived by listing version directories. There is no user
setting and no editable runtime path, so the active runtime can only ever point at
an installed managed version. A record that names a missing version is reported as
an error instead of falling back to something else.

### 8. The trust anchor covers the runtime only, never cores

Cores are a separate component class with their own identity, versions, and
licenses (`ARCHITECTURE.md` §23.4, §53.10). Nothing in this step downloads,
bundles, lists, or installs a core, and nothing assumes a RetroArch artifact
contains one. Core distribution gets its own Issue with an explicit allowlist and
a license review.

## Consequences

**Positive**

- The launch path has a runtime whose origin, version, digest, and license are
  recorded in the repository.
- Verification happens before installation, and a failure leaves the previous
  active runtime untouched.
- Runtime versions are immutable and side-by-side installable, so a later
  one-step rollback (§23.3) does not need a layout change.
- No user RetroArch installation is read, used, or modified.

**Negative / accepted**

- No signature verification yet; the pinned digest is the only integrity anchor.
- No progress reporting, cancellation, or retry for the download.
- The artifact is kept in the store's `artifacts/` directory after a successful
  install. It is rebuildable and never authoritative; which artifacts are pruned
  is an update-policy decision for a later Issue.
- A runtime update is a new pin plus a re-run, not a user-confirmed update flow.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Resolve the newest stable version from the build host at run time | A moving target cannot be verified against a reviewed digest, and `Issue #19` requires an explicit pin. |
| Use `reqwest` with its blocking client | Pulls Tokio, hyper, and a large tree for one synchronous call, without providing the async behaviours the architecture wants from the shared layer either. |
| Reuse a user-installed RetroArch or search `/Applications` | Contradicts `PRODUCT.md` §16.1 and §16.3; makes "which RetroArch ran?" unanswerable. |
| Extract the `.dmg` with `ditto` or `unzip` through a shell | `ARCHITECTURE.md` §20.3 and `AGENTS.md` §10 forbid shell interpolation, and `unzip` cannot read a disk image. |
| Pure-Rust UDIF/HFS+ reader | Reimplements a container, a compression format, and a file system for one artifact already covered by the pinned digest. |
| Install into `/Applications/RetroArch.app` | `ARCHITECTURE.md` §43 keeps mutable data out of the app bundle, and §23.1 requires a versioned store; it would also overwrite a user installation. |
| Introduce SQLite for the component registry now | `Issue #19` excludes it, and no data model exists yet. |
| Bundle all libretro cores with the runtime | `Issue #19` §19 excludes it; cores need their own license review. |

## References

- `ARCHITECTURE.md` §6.1 (startup order), §20.3 (no shell), §23 (runtime and core
  management), §24 (signed manifests), §31 (network layer), §35 (app data and
  lifecycle), §43 (macOS packaging), §44 (bootstrap runtime), §45.4 (network
  tests), §53 (invariants)
- `PRODUCT.md` §16 (runtime management), §18 (manifests and verification), §19
  (update channels), §42 (network behaviour), §45 (open questions)
- `AGENTS.md` §9 (paths and cleanup), §10 (network and downloads), §11 (runtime
  and core rules), §14 (dependencies)
- Issue #19, Pull Request (see the Issue for the link)
