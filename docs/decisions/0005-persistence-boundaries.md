# ADR 0005 — Persistence boundaries and derivation rules

- **Status:** Accepted
- **Date:** 2026-09-22
- **Issue:** #33 (Data Model — `DATA_MODEL.md`)
- **Supersedes:** —
- **Superseded by:** —

*Revised during the PR #113 review: rebuildable data may own no identity that
persistent data references, so the persistent core/runtime version anchors were
separated from the rebuildable installed-state index and the minted
`ManagedComponentId` was removed (§1a, §4); a core assignment no longer references
installed state (§3, §4); the 90-day run retention and the run foreign keys were
reconciled (§5); and the local firmware digest was separated from the trusted
reference hashes owned by #89 (Consequences).*

## Context

`DATA_MODEL.md` is the first document that fixes BitArchive's persisted shape.
Deriving it from `ARCHITECTURE.md` §54, `PRODUCT.md`, `docs/UI_UX_CONCEPT.md` and
ADRs 0001–0004 forced four groups of decisions that are **not** local to a single
table and that a later schema, port or subsystem would otherwise re-decide
differently:

1. How a domain identity and a fingerprint are represented in SQLite, and what
   happens when the persisted bytes are not what BitArchive wrote.
2. Which values the database owns and which are derived from other rows, because
   every wrongly stored derived value becomes a second truth that nothing can
   reconcile.
3. Where the boundary runs between the curated catalogue, the online
   distribution manifest and the database.
4. Whether installed runtime and core artifacts are database state, given that
   ADR 0001 and ADR 0002 deliberately made the component store
   file-system-authoritative and refused a database registry.

Without a record, each of these would be settled again by whichever Issue
introduces persistence, and the second answer would be the one that ships.

## Decision

### 1. Identity is a 16-byte UUIDv7 `BLOB`, validated on every read

A domain identity is stored as an exactly 16-byte `BLOB` in RFC 4122 network byte
order. On read it MUST validate, in this order:

```text
length is exactly 16 bytes
RFC 4122 variant bits are set
version nibble is exactly 7
```

A failure is a **typed error that names the table, the column and the offending
bytes**, and the read fails. There is no fallback to a freshly minted identity, to
`NULL`, or to a best-effort value.

**Why.** `bitarchive-domain::id` makes `new()` the only constructor, so a domain
identity can never wrap an arbitrary UUID. The read path must hold the same line:
a silently regenerated identity detaches every piece of user metadata from its
game, and a silently dropped row hides data loss behind an empty screen. The
version check is not decorative — UUIDv7 is the property that makes identities
time-ordered and index-friendly, and accepting a v4 byte string would accept
something BitArchive never wrote.

The ten identity types are fixed by `ARCHITECTURE.md` §7 plus the stable store
`SystemId` and `CoreId` needed (ADR 0004). No further UUID identity is introduced
without a fachliche need; a database row number stays internal and never reaches
a UI contract, a cover-rail anchor or a user preference.

A **hash is never a primary key and never a foreign key target**, and a **path is
never a domain identity** (invariants 2 and 3). A digest column is `NOT NULL`
where required and indexed where looked up; that is the only role it has.

### 1a. Rebuildable data owns no identity that persistent data references

> **Rule (binding).** Rebuildable data may be dropped in full at any time without
> any user-visible loss.
>
> **Corollary (binding).** No persistent data may reference an identity that is
> owned by rebuildable data.

Both halves are needed. The rule is what makes a cache a cache; the corollary is
what makes dropping it safe. Without the corollary, "rebuildable" would be a claim
the schema does not honour, because a rebuild could re-mint an identity and
silently detach a save state, a session, a core-option schema or an override from
the build it describes.

The model therefore separates **persistent fachliche anchors** from the
**rebuildable installed-state index**:

| Table | Ownership | Lifecycle | Offers an identity? |
|---|---|---|---|
| `core_versions`, `runtime_versions` | the core / runtime build or version a fachliche fact refers to | Persistent | yes — this is their purpose |
| `managed_components`, `component_index_state` | what the component store currently contains | Rebuildable | **no** — keyed by the artifact itself |
| `firmware_entries`, `firmware_index_state` | what the firmware folder currently contains | Rebuildable | **no** — keyed by a path no entity references |
| `search_index` | the effective title projection | Rebuildable | **no** — keyed by the `GameId` it indexes |

Save states, sessions, core-option schemas and core-option overrides reference
`core_versions`; sessions additionally reference `runtime_versions`. Dropping and
rebuilding the whole installed-state index therefore changes no persistent
reference.

**An earlier revision of this ADR's subject document violated the corollary** by
minting a `ManagedComponentId` inside the rebuildable component index and
referencing it from persistent tables. It was found in review and corrected by the
split above. `DATA_MODEL.md` §20.3 test 22 is the regression test.

### 2. Store what cannot be derived; derive what can

For every value the model asks: is it a pure function of other stored rows?

- **Stored:** fachliche identity, relationships, manual overrides, settings and
  overrides, scan and scrape history, session history, user flags, provider
  provenance.
- **Derived:** playtime and every statistic, save-state compatibility, setting and
  core-option validity, the search projection, and the whole firmware and
  component **installed-state** index.
- **Stored even though it looks derived:** the local firmware SHA-256, because it
  is a measurement of the user's file that the index is required to hold, not a
  judgement about it (see Consequences); and the `core_versions`/`runtime_versions`
  anchors, because persistent history must be able to name the build it refers to
  (decision 1a).

A derived value MUST have a deterministic definition and a **total tie-breaker
over stable identities**, so reading the same data twice produces the same answer
(`ARCHITECTURE.md` §36.3).

**Why, with the two cases that matter most.** Statistics are derived from session
rows rather than materialized, because a materialized playtime would have to be
kept in step by every session write, crash recovery, session deletion and library
reset, and its most likely failure mode is silent: a number that is slightly wrong
forever with no way to tell which of the two values is right. Save-state
compatibility is derived from core identity and core version for the same reason
in a stronger form: a stored verdict would be unverifiable the moment the active
core version changed.

Derivation is only safe because the inputs are retained. Because lifelong playtime
is derived, **sessions are not pruned**; bounding the table would require
lifetime totals written before any pruning, which is a future Issue and not a
silent retention rule.

### 3. Three separate owners: curated catalogue, distribution manifest, database

> The curated catalogue is **shipped with the app as reviewed data and
> constants**. It is not stored in the database, and the database never overrides
> a curated definition.

Consequences for the model:

- There are no `catalog_systems`, `catalog_cores`, `capabilities`,
  `setting_definitions`, `detection_rules` or firmware-requirement tables.
- `systems` and `cores` exist **only to bind a stable identity to a curated
  key**, because a persisted row needs a `SystemId`/`CoreId` that outlives a
  session while the curated slug is not a domain identity (ADR 0004 §7).
- A user's preference is a **scope assignment**, never an edited catalogue entry.
- **A curated core is identifiable with no build installed.** The existence of a
  `cores` row and of an installed artifact are independent facts owned by different
  authorities, so no fachliche core assignment can depend on installed state.
- The curated catalogue, the online distribution manifest and the database stay
  three separate things — the `ARCHITECTURE.md` §15 separation, extended to
  persistence.

**Why.** `ARCHITECTURE.md` §15 makes the catalogue versioned, data-driven, CI
validated and delivered with the app. Mirroring it into SQLite would create a
second copy that a catalogue update could not reliably refresh, and the curation
would stop being reviewable in one place.

The same boundary answers the settings subsystem: `ARCHITECTURE.md` §26.2's
`SettingDefinition` describes type, default and supported scopes, and it is
curated code, not database content. Stored overrides are therefore validated
against the current definitions **at load time**, and validity is derived rather
than frozen into a status column — otherwise a later definition that *fixes* a key
could never make an existing override usable again without a data migration, and
one that *breaks* a key would leave a stale "valid" flag behind.

### 4. The component store stays authoritative; the database index is rebuildable

Installed runtime and core artifacts remain file-system-authoritative. The
database gets a **`Rebuildable` index** over them, for the two questions the file
system cannot answer: the core-update impact report of `ARCHITECTURE.md` §23.2
(which save states would become a version mismatch) and orphan detection across
the session and save-state history.

- Reconciliation flows **store → index**, never index → store.
- An empty index falls back to the store, never to an error.
- There is **no `is_active` column** for a runtime and **no active-core concept at
  all**: the runtime's activation record is the file
  `components/runtime/<id>/active`, and core selection is answered by
  `System → Game → optional Release` (invariant 21). A database flag would be a
  second, contradictory answer to both.
- Runtime and cores stay separate component classes with separate store layouts
  (ADR 0001 §4, ADR 0002 §6) even though one index table carries both, because the
  *observations* are identical and the *fachliche* types are not. Their version
  anchors are two separate persistent tables for the same reason.
- The index is keyed by the **artifact** `(component_class, component_key,
  platform, version)`, not by a minted id, so it offers persistent data nothing to
  reference (decision 1a).

**Why this does not contradict ADR 0001/0002.** Those ADRs rejected introducing
SQLite **as the registry** and rejected a database core registry because the
installed set is derivable from the store layout, and that remains true: the store
is authoritative and the installed set is derived by listing directories. This
decision adds a disposable cache whose loss changes no fachliche answer.

**Why the persistent version anchors are not "a second authoritative component
registry".** `core_versions` and `runtime_versions` record **no installed state and
no store path**, so they cannot contradict the store: they say *which build a
fachliche fact refers to*, never *whether that build is on disk*. Installed state
lives exclusively in the rebuildable index, and the store stays its authority.
ADR 0002's rejected "database core registry" answered a different question — which
core is installed or active — and that question is still answered only by the store
and by invariant 21's resolver.

**Why a core assignment does not reference installed state.** `cores` binds a
`CoreId` to a curated `component_key` and deliberately has **no foreign key** into
the component index. Two reasons, and the first alone settles it: several builds of
one core can be installed for one platform, so `(component_key, platform)` is not a
unique target and the constraint would be invalid; and a user may configure a core
that is not installed yet, which must be remembered so that installing it later
resolves. The curated catalogue is the authority for "this core exists"; the store
is the authority for "this build is installed".

### 5. Deletion is explicit, and the database never deletes an external file

- Every foreign key declares its delete behavior. A cascade is used only where the
  child is an **intrinsic part** of the parent — currently only a release's region
  and language tags.
- **No scan, scrape, provider refresh, migration or cleanup routine deletes a
  `manual_overrides` row, or an override that merely became invalid** (invariant
  19). A provider refresh may rewrite `provider_values`; it may not touch
  overrides.
- **Missing content is a state, not a deletion.** An offline, permission-denied or
  unplugged source marks nothing missing at all, and a content with no present
  location keeps its identity, fingerprints and every referencing row.
- **Destructive reconciliation requires a scan that completed its whole target**
  (invariant 18). Eligibility is a stored fact on the scan run, not an inference
  from counters, because a run that skipped an unreadable subtree looks successful
  by counter alone.
- Save-state deletion is **file-system operation first**; the row survives a
  failed file deletion (`ARCHITECTURE.md` §29.6).
- **The MVP has no user-facing per-game delete action.** `PRODUCT.md` §41 offers
  hide, ignore and reset index entry, all reversible and none destructive. A
  `games` row is removed only by the whole-library operations of `PRODUCT.md` §40,
  and even a full reset never touches a user file.
- Startup cleanup touches only unambiguously BitArchive-owned temporary or expired
  artifacts (invariant 20).
- **A retention rule and a foreign key must be able to hold at the same time.**
  Where long-lived data referenced a prunable `scan_runs`/`scrape_runs` row purely
  as provenance, the reference is nullable with `ON DELETE SET NULL`; where the row
  is an intrinsic part of its run, it cascades. Ownership relations keep
  `RESTRICT`. A `RESTRICT` on a provenance link would have made the 90-day run
  retention rule unexecutable, which is why the two were reconciled together.

## Consequences

- Schema v1 derives its types, keys and constraints from `DATA_MODEL.md` without
  re-modelling the product. The document is binding, and a later Issue that
  deviates updates it in the same change.
- The concrete delivery order is #34 (SQLite foundation and migration runner, no
  domain tables), then #109 (schema v1), then the per-aggregate persistence Issues
  beginning with #35, which is also where the first repository ports and domain
  types appear. The architectural step "Domain Types / Repository Ports" of
  `ARCHITECTURE.md` §54 is not one Issue.
- Repository ports must expose derivation rather than a stored aggregate for
  statistics and compatibility, and must validate identities on read.
- Some queries become aggregates over indexed history instead of lookups. This is
  accepted deliberately; the alternative is a second truth.
- `DATA_MODEL.md` §20.5 records the scope-key choices (`SystemId` for persisted
  scope anchors, `CoreId` for core selection, distribution slug for the
  distributable component) so schema v1 does not re-decide them.
- Where `ARCHITECTURE.md` wording is stale relative to an accepted ADR, the ADR
  wins and `DATA_MODEL.md` follows it: a core's store version segment is its
  **build id**, not a version string (ADR 0002 §6).
- **The local firmware SHA-256 and the trusted reference hashes are separate
  concerns.** `firmware_entries.digest` is the SHA-256 of the user's own file and
  is computed for every readable file, because `ARCHITECTURE.md` §25.1 asks the
  index to hold it; it does not depend on #89. What #89 owns is the *trusted*
  reference set in the curated catalogue and the core requirements, without which
  no content verdict (`WrongContent`, `CorrectContentWrongFilename`) may be
  claimed. A `NULL` local digest means "the file could not be read", never "wrong
  content".

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Store identities as `TEXT` UUIDs | Doubles the index size of every identity column and invites two spellings of one identity (case, hyphens). The 16-byte `BLOB` is what `ARCHITECTURE.md` §7 fixes. |
| Accept a non-UUIDv7 `BLOB` and coerce it | Silently reinterprets rows BitArchive never wrote, and turns a detectable corruption into a wrong screen. |
| Materialize playtime and play counts in a statistics table | A second truth that every session write, recovery, deletion and reset must maintain; a wrong value is silent and unreconcilable. |
| Store a save-state compatibility verdict | Unverifiable once the active core version changes, and it would have to be rewritten on every core update. |
| Persist the curated catalogue into SQLite and let users edit it | Duplicates reviewed data, breaks CI validation of the catalogue, and blurs curated catalogue with online distribution manifest (`ARCHITECTURE.md` §15). |
| Store a `scope`/`is_valid` flag on stored overrides | Freezes the verdict of one definition version; a later fix could never heal the override. |
| Keep the component installed set only in the database | Contradicts ADR 0001 §7 and ADR 0002 ("the installed set is derivable from the store layout"), and would make the database a rival authority to the store. |
| Add an `is_active` runtime column and an "active core" | Invariant 21 has no global core default, and ADR 0002 states a core must never have an activation record. |
| `ON DELETE CASCADE` from a library source down to games | The fachliche meaning of removing a source is "these games are no longer in the active library", not "forget them" (`PRODUCT.md` §7.3); it would also destroy metadata, favorites and statistics that must survive. |
| Delete a content row when its file disappears | Makes a temporarily unmounted volume indistinguishable from a deletion, and destroys the fingerprint that lets the content be recognized when it returns. |
| Store generated session `.cfg` files and logs as data | `ARCHITECTURE.md` §28 and invariant 7 make them session artifacts, not truth. |
| Put scan or scrape jobs in one generic job table now | `ARCHITECTURE.md` §30.4 keeps job persistence with the Issue that implements job recovery; only the fachliche run histories reconciliation depends on are modelled. |
| Mint an identity for a rebuildable index row and reference it from persistent data | Violates the corollary of decision 1a: a rebuild could re-mint the identity and detach save states, sessions and overrides. The installed-state index is keyed by the artifact itself instead. |
| Let persistent data hold the core build id as loose text instead of an anchor table | Four columns repeated across four tables, no place to record the pinned digest, and no way to express "same build" without comparing text. The anchor is one row and one 16-byte foreign key. |
| Point a core assignment at the installed-state index | The target is not unique (several builds per core and platform), and it would make a fachliche assignment depend on an artifact being installed, so a not-yet-installed core could not be configured. |
| Keep `RESTRICT` on provenance links from long-lived data to prunable runs | Makes the documented 90-day run retention unexecutable: a rule that says "prune old runs" cannot coexist with a constraint that refuses to delete a run a surviving row mentions. |
| Allow `Crc32`/`Md5`/`Sha1` in the fingerprint table while fixing the digest at 32 bytes | Inconsistent on its face (4 and 16 byte digests), would need an algorithm-dependent length rule, and would admit a reference-ROM system the MVP excludes. SHA-256 alone is what `ARCHITECTURE.md` §11 makes canonical. |
| Materialise the resolved preferred release into `games` | `PRODUCT.md` §8.6 puts an *explicit* default first precisely so that the automatic choice keeps being recomputed from the language and region preferences; a stored copy would silently stop honouring a later preference change. |
| Define a user-facing "Delete Game" action | `PRODUCT.md` §41 enumerates the MVP's per-game operations as hide, ignore and reset index entry, all reversible. A delete action would be new product scope, and the model must not imply one. |

## References

- `ARCHITECTURE.md` §7, §8, §9, §15, §16, §18, §22, §23, §25, §26, §27, §28,
  §29, §30.4, §34, §35, §36, §53, §54
- `PRODUCT.md` §7, §8, §9, §10, §15, §20, §21, §23, §27, §39, §40, §41
- `docs/UI_UX_CONCEPT.md` (Maintenance — Hidden Games, Ignored Content)
- ADR 0001 (managed runtime acquisition), ADR 0002 (managed core acquisition),
  ADR 0003 (managed launch composition), ADR 0004 (launch readiness and
  preparation)
- `DATA_MODEL.md` (this ADR records the cross-cutting decisions it applies)
- Issue #33 (Data Model), Issue #34 (SQLite foundation and migration runner),
  Issue #35 (first persistence Issue, with the repository ports),
  Issue #41 (firmware index and managed component metadata),
  Issue #89 (trusted source for curated firmware hashes),
  Issue #109 (schema v1)
