# ADR 0002 — Managed core acquisition

- **Status:** Accepted
- **Date:** 2026-09-18
- **Issue:** #21 (B6 — Managed Core Acquisition)
- **Supersedes:** —
- **Superseded by:** —

## Context

`ARCHITECTURE.md` §23 defines runtime and core components as versioned and
immutable, and §23.4 states that they are separate component classes with
different identities, versioning, and licensing. ADR 0001 established the runtime
half of that: one pinned RetroArch version, downloaded from the official host,
verified against a pinned SHA-256, installed immutably, and activated.

The core half was missing entirely. Nothing in the workspace described what a
libretro core is, where it may come from, which license it carries, or how a
versioned core build would be installed. `PRODUCT.md` §16.1 requires BitArchive to
manage its own components, and the following launch slice needs a core whose
origin, revision, and license are recorded in the repository.

Two properties of the official libretro build host shape this decision:

- it publishes macOS core binaries **separately per architecture** below a
  **rolling** `latest` path, with no per-build (dated) URL:
  `nightly/apple/osx/<arch>/latest/<core>_libretro.dylib.zip`;
- it publishes **no cryptographic digest** for those files — only a date and a
  CRC-32 of the library in its `.index-extended` listing.

## Decision

This step establishes managed, curated, license-aware core acquisition for
**exactly one libretro core on macOS: mGBA**, with these choices.

### 1. One curated core, and the allowlist is code

The curated definition lives in `bitarchive-domain::managed_core` as reviewed
constants and as `mgba_bootstrap(platform)`; `curated_cores()` is the complete
allowlist and `curated_core(component_id, platform)` is the only way to obtain a
definition:

```text
CoreComponentId:  mgba                    (distribution identity)
Display name:     mGBA
Core name:        mgba_libretro           (as RetroArch addresses it)
Build id:         mgba-0.11-212-7a12d6d
Platform:         macos-arm64 | macos-x86_64
Artifact:         https://buildbot.libretro.com/nightly/apple/osx/<arch>/latest/mgba_libretro.dylib.zip
SHA-256:          1aa000e5… (arm64) · 15308808… (x86_64)
Archive kind:     libretro-core-archive { member: "mgba_libretro.dylib" }
Library:          mgba_libretro.dylib
Upstream:         mGBA · mgba-emu/mgba · MPL-2.0
Provenance:       revision 7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6 (2026-09-17)
```

There is no core catalog, no enumeration of the build host, no `--core` argument
that names an uncurated component, and no constructor in the workspace that builds
a definition from a user-supplied URL, core name, or library path. Adding a core
means adding a reviewed function like `mgba_bootstrap` and listing it in
`curated_cores()`.

mGBA was chosen because it is an established, actively developed core with one
clear upstream project, because upstream mGBA is `MPL-2.0` (file-level copyleft,
redistribution-friendly, easy to record honestly), because it covers GBA as well as
GB/GBC, and because the external BIOS is **optional** for GBA — so the first real
launch slice does not have to solve firmware before it can start content. The
choice, the license, and the artifacts were re-verified against the official
sources while this step was implemented.

### 2. The rolling URL is transport; the pin is the identity

The build host replaces the file below `latest/` whenever it rebuilds the core, and
it offers no immutable per-build URL. Conflating that transport path with the
identity of what is installed would be the mistake, so the two are separated:

- the **identity** of an installed core is the `CoreBuildId` plus the `CorePlatform`
  plus the pinned `Sha256Digest` — that is what names an installation directory and
  what a resolution asks for;
- the **URL** is a moving pointer, used only to *fetch* bytes that are then checked
  against the pinned digest.

`latest` is therefore deliberately not a build id and not a version. A rebuild
upstream does not silently become an update: the bytes no longer match the pin, the
installation is refused with `DigestMismatch`, and raising the pin is a reviewed
source change. This was observed during implementation, not hypothesised: the pin
taken on 2026-09-16 (`0.11-219-e31759b`, CRC-32 `9b009f3e`/`df82f4a1`) no longer
matched the artifacts published on 2026-09-17 (`0.11-212-7a12d6d`, CRC-32
`69c0d054`/`c543643a`), and the step re-pinned to the verified current build.

The observed version string is not monotonic (`219` → `212` after a rebuild), which
is exactly why the commit *count* inside it is not identity: the revision is.

**How a pin is raised** (the procedure this step verified):

1. download both architecture artifacts from the official host;
2. compute their SHA-256 locally and record it;
3. read the version string out of each library and resolve the short revision to a
   full commit in `mgba-emu/mgba`;
4. record the build host's channel date and CRC-32 for each architecture;
5. update `MGBA_*` together — version, build id, revision, digests, dates, CRC-32 —
   and re-run the opt-in live command.

### 3. The trust anchor is the pinned SHA-256, and a download never supplies a digest

As in ADR 0001, and for the same reason: `ARCHITECTURE.md` §24 requires signed
distribution manifests and that infrastructure does not exist yet. Until it does,
the expected digest is part of the BitArchive-side definition, the downloader
hashes the bytes as they are written, and a mismatch discards the artifact. There is
no code path — for the runtime or for a core — that adopts a digest because a
download succeeded.

The build host's CRC-32 is recorded as a *change hint* from upstream's own listing,
never as an integrity check: CRC-32 is not collision-resistant, and upstream
publishes it for the library, not for the archive BitArchive pins.

### 4. One download contract for both component classes

The downloader was generalised from `RuntimeDefinition` to `ArtifactRequest` — the
component id, the pinned official source, the pinned digest, and the published file
name, and nothing else. A second `HttpCoreDownloader` would be a second copy of host
checking, redirect refusal, streaming hashing, partial-file handling, and timeouts,
and a second place for those rules to drift.

What is *not* shared is anything that would erase the difference between the
classes: there is no `Component` trait, no generic installation pipeline, and no
shared activation. A runtime has an activation record; a core must never have one.

### 5. Archive extraction is member-exact, and the archive never decides a path

Only the pinned member is ever written, and the destination is derived from the
definition's `RelativePath` — never from the archive. An archive that carries an
entry with an absolute name, a `..` component, a NUL byte, a directory entry, or a
symbolic link is refused before anything is written. Unrelated members are ignored
entirely, so an archive member can never add or overwrite a file.

The `zip` dependency is built with `default-features = false` and only the pure-Rust
`zlib-rs` DEFLATE backend, because the official host publishes one DEFLATE member:
no C toolchain, no system zlib, and no bzip2, LZMA, zstd, PPMd, AES, or ZIP64
machinery to review or to keep patched. An archive that needs any of those is
reported as unreadable rather than met with another decoder.

A bounded copy caps the written library, so a member that declares or expands to
something that is not a core library is refused instead of filling the disk.

### 6. The core store is platform- and build-specific, immutable, and has no activation

```text
<component root>/
├── cores/<component-id>/<platform>/<build-id>/<library>
├── staging/install-<component>-<build>-<platform>/     ← shared with the runtime
└── artifacts/<component>-<build>-<platform>            ← shared with the runtime
```

The platform sits below the component, as in the runtime tree, so an Apple Silicon
build and an Intel build can never share a path, and a host is never served the
other architecture's library. Installation is a single `rename` of a fully staged
payload with no copy fallback: a copy would be multi-step, and a failure halfway
through it would leave a partial directory at a path that means "this build is
installed completely". Re-installing an installed build is refused.

There is **no** activation record, no "current core", and no default core.
`ARCHITECTURE.md` §23.1 defines activation for runtimes; core selection is defined
by §20.4 and invariant 21 as `System → Game → optional Release` with no global
default. A stored "active core" would be a second, contradictory answer to a
question the resolution policy already answers, so no type in the core path can
express one, and several builds of one core sit side by side.

### 7. Core component identity is not the domain core identity

`CoreId` (UUIDv7) answers "which core did the user configure for this game";
`CoreComponentId` (`mgba`) answers "which distributable thing does BitArchive
install"; `mgba_libretro` is how RetroArch addresses the library. They are different
types, there is no conversion between them yet, and the file name is derived from
the definition rather than from a URL or an archive entry (`ARCHITECTURE.md` §7,
§11).

### 8. No emulation-layer core type in this step

The emulation boundary is untouched: installing a core is component management, not
a RetroArch-specific concern, and the launch path does not consume a core yet. B7
(the first real launch) introduces the `-L` argument and decides whether anything
RetroArch-specific is needed above `CoreInstaller::resolve`.

## Consequences

**Positive**

- A core BitArchive installs has a recorded origin, revision, architecture, digest,
  and license, and cannot be substituted by another build, another architecture, or
  another core.
- The runtime and the core path share exactly one HTTP implementation, one staging
  area, and one artifact cache, so the security-relevant download rules cannot
  drift between them.
- Platform separation and immutability are properties of the layout and the code,
  not conventions: `/cores/mgba/macos-arm64/<build>` is the only place an arm64
  build can be, and it is written once.
- The offline test suite covers the whole path — download over a loopback socket,
  digest verification, member-exact extraction, installation, resolution, and the
  refusals — so CI never needs the public network.

**Negative / accepted**

- The pin goes stale whenever upstream rebuilds a core, and the failure mode is a
  refused installation rather than an automatic update. That is deliberate: a
  moving upstream must not silently change what BitArchive installs, and raising the
  pin is a reviewed change.
- No signature verification yet; the pinned digest remains the only integrity
  anchor, exactly as in ADR 0001.
- No core catalog, no second core, no update flow, no rollback UI, and no core
  options. Each is a later Issue with its own license review.
- The artifact archive is kept in the store's `artifacts/` directory after a
  successful install. It is rebuildable and never authoritative.
- Duplicate member names are collapsed by the archive reader, so the extractor's
  refusal of an ambiguous archive cannot currently be triggered by input; the rule
  stays in place because it belongs to the extractor, not to the archive library.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Resolve the newest core build at run time, or adopt the digest of whatever was downloaded | A moving target cannot be verified against a reviewed digest; TOFU makes the pin decorative and would let a compromised host change what BitArchive installs. |
| Pin a dated, immutable upstream URL | The official host publishes no such URL for core binaries. Achieving it would mean BitArchive mirroring and redistributing cores, which is a different decision with its own hosting and license review. |
| Use the build host's `.index` CRC-32 as the integrity check | CRC-32 is not collision-resistant, and upstream computes it over the library while BitArchive installs the archive. It is recorded as a change hint only. |
| Unpack the archive wholesale into the staged payload | The member name is known in advance, so a wholesale unpack adds attack surface (traversal, symlinks, unexpected members) for no benefit. |
| Extract with `unzip` through a shell, or with a disk-imaging tool | `ARCHITECTURE.md` §20.3 and `AGENTS.md` §10 forbid shell interpolation, and a tool that writes the whole archive cannot express "only this member, only here". |
| Enable all `zip` features, or link system zlib | Pulls bzip2, LZMA, zstd, PPMd, AES, and ZIP64 — none of which a core archive uses — plus a C toolchain or a system library as a build requirement. |
| One generic `ComponentStore<…>` for runtimes and cores | A runtime has an activation record and a core must never have one; a shared pipeline would have to be parameterised exactly along that seam, which is where the two classes differ most. |
| A global "active core" or a stored default core | Contradicts the `System → Game → optional Release` policy and invariant 21, and answers a question the resolution policy already answers. |
| Model a `macos-universal` core | Upstream ships separate arm64 and x86_64 libraries; claiming a universal artifact would be a false statement about the bytes installed and would allow an Intel build on Apple Silicon. |
| Bundle cores with the runtime, or install into RetroArch's own core directory | Issue #21 excludes both; a core needs its own license review, and RetroArch's configuration and directories are not BitArchive-owned state. |
| Introduce SQLite for a core registry now | No schema exists, and the installed set is derivable from the store layout; the allowlist is the source of definitions. |

## License note

mGBA is recorded as `MPL-2.0`, taken from the upstream project
(`mgba-emu/mgba`, `LICENSE`), not from a third-party core list. The binaries are
built and published by the libretro build host, while the project, its source, and
its license are mGBA's — the provenance records the revision so the build can be
traced back to the upstream commit. MPL-2.0 is file-level copyleft, which makes
BitArchive's position as a downloader (not a redistributor) straightforward to
describe: BitArchive fetches the artifact from the official host for the user and
installs it locally.

## References

- `ARCHITECTURE.md` §7 and §11 (identity and keys), §20.3 (no shell), §20.4 (core
  resolution), §23 (runtime and core management), §24 (signed manifests), §31
  (network layer), §35 (app data and lifecycle), §45.4 (network tests), §53
  (invariants, especially 20 and 21)
- `PRODUCT.md` §16 (runtime management), §18 (manifests and verification), §25
  (firmware), §42 (network behaviour)
- `AGENTS.md` §4 (user-owned files), §9 (paths and cleanup), §10 (network and
  downloads), §11 (runtime, core, config, and save-state rules), §14 (dependencies)
- ADR 0001 — Managed RetroArch runtime acquisition (the runtime half of §23)
- Issue #21, Pull Request (see the Issue for the link)
