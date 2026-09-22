# BitArchive – Data Model

- **Status:** Defined (Issue #33)
- **Scope:** Conceptual data model for the local SQLite database (schema v1)
- **Derives from:** [`ARCHITECTURE.md`](./ARCHITECTURE.md) §54, [`PRODUCT.md`](./PRODUCT.md)
- **Derived into:** the SQLite foundation (#34), schema v1 (#109), and the persistence Issues that follow

---

## 1. Purpose and scope

### 1.1 What this document is

This document fixes BitArchive's concrete data model: which entities exist, what
identifies them, how they relate, what each constraint protects, who owns each
piece of data, and when it may be deleted.

It is the binding reference between the architecture and the schema.

`ARCHITECTURE.md` §54 states the **architectural** sequence — which artifact
follows which:

```text
ARCHITECTURE.md
      ↓
DATA_MODEL.md
      ↓
SQLite Schema v1
      ↓
Domain Types / Repository Ports
      ↓
MVP Implementation Plan
```

The **concrete backlog** that sequence now maps onto is finer-grained, because
the persistence work was split after this document was first drafted:

```text
#33   DATA_MODEL.md  (this document)
      ↓
#34   SQLite foundation + forward-only migration runner   (no domain tables)
      ↓
#109  SQLite schema v1                                    (tables, keys, FTS5)
      ↓
fachliche persistence Issues, beginning with #35
      (library sources, games, releases, contents — including the repository
       ports and domain types for those aggregates)
```

The two views do not conflict; the second only says *which* Issue delivers which
step. The architectural step "Domain Types / Repository Ports" is **not** a single
Issue: it is delivered per aggregate by the persistence Issues, starting with #35.
No Issue number is ever *the* domain-types Issue.

The model is **concrete enough that SQLite schema v1 can be derived from it
without re-modelling the product**, and it is deliberately **not SQL**. No
`CREATE TABLE` statement, no migration file, and no Rust type is defined here.

### 1.2 Binding force

The rules in this document are **binding rules, not examples**.

- `MUST` and `MUST NOT` state constraints that schema v1, the repository ports,
  and every later persistence Issue have to honour.
- A later Issue that needs to deviate updates this document in the same change
  (AGENTS.md, "Document ownership").
- Where the source documents leave a modeling choice open, this document makes
  one, following "choose the simplest design that satisfies the product
  requirement, fits the architecture, remains testable, and does not
  unnecessarily constrain known future extensions" (AGENTS.md §3), and records
  the choice and its reason in a **Decision** block.
- Genuine **product** questions are not decided here. They are listed in §20.4
  with their document status (`DECIDED` / `FAVORED` / `OPEN` / `DEFERRED`).

### 1.3 Terminology

The repository documents are written in German and the code in English. This
document uses the repository's existing mixture: prose is English, and domain
terms that the source documents establish (`Game`, `Release`, `Content`,
`Library Source`, `fachlich`) keep their established spelling.

### 1.4 What is out of scope

- SQL statements, index DDL, migration files, `rusqlite` usage.
- Rust types, traits, repository ports, bootstrap wiring.
- The curated catalogue's own data (systems, cores, formats, launch rules). It
  ships as reviewed Rust constants and catalog data, not as database content
  (§12.2).
- Product decisions (scope, MVP features, UX behavior).

---

## 2. Modeling principles

### 2.1 The four separations the model exists to protect

```text
Game  ≠  Release  ≠  Content  ≠  Content Location
```

`Game` is the logical work, `Release` a concrete publication or variant,
`Content` the concrete technical content unit, and `Content Location` merely
where that content was observed. `ARCHITECTURE.md` §53.1 and AGENTS.md §4 make
this separation an invariant, so the model keeps it in **four separate tables**
with four separate identities (see §6).

The same reasoning separates other pairs that are easy to conflate and expensive
to conflate later:

| Kept apart | Why |
|---|---|
| Provider metadata vs. manual overrides vs. effective value | A provider refresh must never destroy a user's edit (§9). |
| RetroArch settings vs. core options | Two hierarchies with different scopes and different owners (`ARCHITECTURE.md` §26, §27). |
| Runtime vs. cores | Independently versioned components (`ARCHITECTURE.md` §53.10). |
| Curated catalogue vs. online distribution manifest | `ARCHITECTURE.md` §15 and the backlog audit; never one table. |
| Save-state metadata vs. normal saved games | Invariant 13: SRAM, memory cards and other regular saves are outside BitArchive scope and MUST NOT be modelled. |
| BitArchive media store vs. media cache | One is persistent user-visible state, the other is a rebuildable cache. |

### 2.2 Storage vs. derivation

> **Store what cannot be derived. Derive what can.**

A value is **stored** when the database is its owner, when it records a fact the
file system or the curated catalogue cannot answer, or when losing it would lose
user data. A value is **derived** when it is a pure function of stored rows.

Consequences that recur through this document:

- Playtime is **derived** from sessions, not stored a second time (§16.3).
- Save-state compatibility is **derived** from core identity and core version,
  not stored (§15.3).
- Core-option and setting validity are **derived** by validating against the
  current catalogue at load time, not frozen into a status column (§13, §14.5).
- Statistics are **derived** from the session history (§16.3).
- The component index **can be rebuilt** from the component store (§12.6).

Derivation MUST be deterministic and MUST NOT depend on the order rows happen to
be read in. Every derived value needs a total tie-breaker over stable identities
(`ARCHITECTURE.md` §36.3).

#### The rebuildability rule

> **Rule (binding).** Rebuildable data may be dropped in full at any time without
> any user-visible loss.
>
> **Corollary (binding).** No persistent data may reference an identity that is
> owned by rebuildable data.

The rule and its corollary are two halves of one statement and both must hold
simultaneously. The rule alone is what makes a cache a cache; the corollary alone
is what makes dropping it safe. Without the corollary, "rebuildable" would be a
claim the schema does not honour: a rebuild could re-mint an identity and silently
detach a save state, a session, an override or a setting from the thing it
describes.

The model satisfies both, and the ownership of every identity is explicit:

| Identity | Owned by | Lifetime | Referenced by persistent data? |
|---|---|---|---|
| `GameId`, `ReleaseId`, `ContentId`, `LibrarySourceId` | `games`, `releases`, `contents`, `library_sources` | Persistent | yes — by design |
| `SystemId`, `CoreId` | `systems`, `cores` | Persistent | yes — by design |
| `CoreVersionId`, `RuntimeVersionId` | `core_versions`, `runtime_versions` | Persistent | yes — this is their purpose (§12.4, §12.5) |
| `SessionId`, `SaveStateId`, `ScanRunId`, `ScrapeRunId`, `MediaAssetId` | their own persistent tables | Persistent | yes — by design |
| *none* | `managed_components`, `component_index_state`, `firmware_entries`, `firmware_index_state`, `search_index` | Rebuildable | **no — these tables offer no identity to reference** |
| `(relative_path)` in `firmware_entries` | the firmware index | Rebuildable | no — it is an index key, not an identity |

The last two rows are the operative ones: **the rebuildable tables own no
identity at all.** `managed_components` is keyed by the artifact it describes
rather than by a minted id (§12.6), the firmware index is keyed by a path that no
fachliche entity references (§11.1), and the FTS projection is keyed by the
`GameId` it indexes rather than by an identity of its own (§17.2). That is why
there is nothing a persistent row could hold on to by mistake.

An earlier revision of this document violated the corollary by minting a
`ManagedComponentId` inside the rebuildable component index and referencing it
from `save_states`, `sessions`, `core_option_schemas` and
`core_option_overrides`. It was found in review and corrected by the split
described in §12.1; §20.3 test 22 is the regression test for it.

### 2.3 Authoritativeness

Each kind of data has exactly one owner. The database is the owner of the
fachliche model, never a shadow of another owner:

| Data | Authoritative owner |
|---|---|
| Fachliche identity, relationships, manual overrides, settings, session history | SQLite |
| ROM/ISO/ZIP/ZIP-entry bytes | The user's file system, outside BitArchive |
| Firmware bytes | The user's firmware folder, outside BitArchive |
| Managed runtime and core binaries, and which runtime version is active | The component store on disk |
| RetroArch save-state files and thumbnails | RetroArch and the user's file system |
| Media blobs (covers, screenshots) | The managed media store on disk |
| CURATED catalogue definitions | Reviewed Rust constants / catalog data shipped with the app |
| Session `.cfg` files, logs, generated playlists | Generated artifacts, rebuildable |

When two owners could disagree, the **non-database owner wins** and the database
is reconciled to it. §12.7 makes that rule concrete for the component store.

### 2.4 Numeric identity is never domain identity

The model uses database-generated row numbers only as an internal storage
convenience. Every entity a user, a resolver, or a saved focus can name carries a
UUIDv7 identity (§3). Row numbers MUST NOT appear in a UI contract, in a
persisted preference, or in a log that a user is asked to reason about
(`ARCHITECTURE.md` §53.22).

**Where a row number is the storage primary key.** Several persisted structures
have **no domain identity at all**, because they are observations, evidence or
scoped settings rather than entities: a content location, a content fingerprint,
and the override/mapping tables of §9.3, §9.4, §12.3, §13.1 and §14.1. Such a table
still needs exactly one implementable SQLite primary key (§3.5), and it declares

```text
row_id  INTEGER PRIMARY KEY     storage convenience only
```

This key is never a domain identity, never a UI identity, never a foreign-key
target for fachliche data, and never part of a business rule. The table's real
uniqueness is expressed **separately**, as the `UNIQUE` constraints named in its
section (commonly partial unique indexes). §3.9 states the rule and §3.8 audits
every table against it.

---

## 3. Identity and storage conventions

### 3.1 Domain identities

Domain entities use stable UUIDv7 identities (`ARCHITECTURE.md` §7).
The following identity types exist. The list is closed for schema v1: **no
additional UUID identity is introduced unless a fachliche need requires it.**

| Identity | Identifies | Introduced by |
|---|---|---|
| `GameId` | the logical work | §6.1 |
| `ReleaseId` | a concrete publication or variant | §6.2 |
| `ContentId` | a concrete technical content unit | §6.3 |
| `LibrarySourceId` | one configured library source | §5.1 |
| `SystemId` | one curated system, stably | §6.5 |
| `CoreId` | one core a scope can select | §12.3 |
| `CoreVersionId` | one concrete core build (a reviewed build identity) | §12.4 |
| `RuntimeVersionId` | one concrete runtime version | §12.5 |
| `ScanRunId` | one library scan run | §8.1 |
| `ScrapeRunId` | one metadata scraping run | §9.5 |
| `SaveStateId` | one indexed save state | §15.1 |
| `SessionId` | one emulation session | §16.1 |
| `MediaAssetId` | one media asset record | §10.1 |

`SystemId` and `CoreId` already exist in `bitarchive-domain`; this document
supplies the stable store they were missing (ADR 0004, "Alternatives
considered").

**There is no identity for a component-store index entry.** An earlier revision of
this document minted a `ManagedComponentId` for a row of the managed component
index. That was wrong and the type is removed, because that index is
`Rebuildable` (§12.6): an identity owned by data that may be dropped cannot be
referenced by persistent data (§2.2, §12.1). The persistent anchors are
`CoreVersionId` and `RuntimeVersionId`, which belong to tables SQLite owns.

The minted identities are deliberately **not** named after
`bitarchive-domain::ComponentId`: that name is taken by the **distribution slug**
of a managed component (`retroarch`, `mgba`), which is a string, is never minted,
and is never a primary key. This document calls the slug `component_key` when it
stores it (§12.6).

### 3.2 Encoding

| Logical type | SQLite storage | Notes |
|---|---|---|
| UUIDv7 identity | `BLOB`, exactly 16 bytes | RFC 4122 network byte order. `NOT NULL` on every identity column. |
| SHA-256 digest | `BLOB`, exactly 32 bytes | Raw bytes, never hex text, never base64. SHA-256 is the **only** digest algorithm schema v1 persists (§7.1). |
| Provider external identifier | `TEXT` | Opaque to BitArchive; never an identity, never a key. |
| Technical key (setting key, core option key, catalog key) | `TEXT` | ASCII, lower case, case-sensitive comparison. |
| Locale / BCP-47 language tag | `TEXT` | **RFC 5646 canonical case**: lower-case language subtag, Title-case script, upper-case region, lower-case variant — `de`, `en-US`, `zh-Hans`, `sr-Latn-RS`. `NULL` means "language-neutral" and is never spelled with a tag (§3.5). |
| Storage row number | `INTEGER` | `row_id INTEGER PRIMARY KEY` on tables that own no domain identity (§2.4). Internal only. |
| Enum | `TEXT` | Spelled exactly like the document's or the domain enum's variant, e.g. `ArchiveEntry`. Never a bare integer. |
| Boolean | `INTEGER`, `0` or `1` | `CHECK`-constrained to those two values. |
| Timestamp | `INTEGER`, milliseconds since the Unix epoch, UTC | Monotonic source is the system clock; the model stores instants, not local time. |
| Duration | `INTEGER`, milliseconds | Monotonic measurement, not wall-clock subtraction. |
| Counter / size | `INTEGER`, signed 64-bit | Byte sizes are `>= 0`. |
| Float setting value | `REAL` | Only ever alongside an explicit type tag (§13.2). |
| Free text | `TEXT` | UTF-8. |

**Decision — timestamps are integer milliseconds.** An ISO-8601 string sorts
lexicographically and is human-readable, but it needs a canonical form, a
canonical timezone, and a parser on every compare. Integer milliseconds give a
total order for `ORDER BY` and `lastPlayedAt` sorting with no text collation
rules, and they are unambiguous across locales and calendar systems. Formatting
belongs to presentation.

**Decision — enums are stored as their spelled names.** A numeric enum makes
every stored row depend on the declaration order of a Rust enum, so inserting a
variant silently reinterprets existing data. Names are readable in a database
dump during diagnosis and cannot drift silently.

### 3.3 Reading a persisted identity

> **Rule (binding).** A persisted identity MUST be validated back into its domain
> type. Invalid persisted UUID bytes MUST NOT be accepted silently.

The read path is:

```text
BLOB column
   ↓ length exactly 16?                        no → error, never a default
   ↓ RFC 4122 variant bits set?                 no → error
   ↓ version nibble exactly 7?                  no → error
   ↓ DomainTypeId
```

A failed validation produces a typed error that names the column, the table, and
the offending bytes, and the affected read fails. It MUST NOT fall back to a
freshly generated identity, to a `NULL`, or to a "best effort" value: a silently
regenerated identity detaches user metadata from its game, and a silently dropped
row hides data loss.

The same rule holds one level up. A referenced identity that does not exist is a
broken invariant, and it stays broken when foreign keys are enforced
(`ARCHITECTURE.md` §8.1), because the read failed loudly instead of inventing a
row. **Foreign keys MUST stay enabled on every connection**, and schema v1 MUST
be derivable without relying on SQLite's default of disabled enforcement.

The version check is not decorative. Only UUIDv7 identities are minted
(`bitarchive-domain::id`), and UUIDv7 is time-ordered, so it is the property that
makes an identity sortable and index-friendly. Accepting a v4 byte string would
silently accept something BitArchive never wrote.

### 3.4 What is never an identity or a key

> **Rules (binding).**
>
> - A **path is not a domain identity** (`ARCHITECTURE.md` §53.2). No table's
>   primary key is a path, and no fachliche relationship is expressed through a
>   path.
> - A **hash is not an identity and not a primary key** (`ARCHITECTURE.md` §53.3).
>   A digest column is `NOT NULL` where required and indexed where looked up, and
>   it is never a primary key, never a foreign key target, and never a substitute
>   for `ContentId`.
> - A **provider external ID is not a BitArchive identity**. It is provenance
>   (§9.4).
> - A **file name** is not an identity.
> - A **title** is not an identity. Titles change, are translated, and collide.

### 3.5 Uniqueness

A unique constraint expresses a fachliche rule:

- Two rows that mean "the same thing" MUST collide.
- A unique constraint MUST be justified by the rule it protects, not by
  convenience. Every table below names that rule.
- Where a uniqueness rule depends on a discriminator (a scope, an algorithm, a
  file-system case policy), the constraint is a **partial unique index over that
  discriminator** rather than a nullable column trick.

#### Key columns are NOT NULL, or pinned by a predicate (binding)

> **Rule (binding).** Every column that participates in a primary key or a unique
> constraint MUST be `NOT NULL`, **or** the constraint MUST be a partial unique
> index whose predicate proves the column is non-`NULL` for every row it covers.

This is not a style rule. SQLite treats `NULL` values as **distinct from each
other** in `UNIQUE` indexes, and it does not forbid `NULL` in the columns of a
composite `PRIMARY KEY` either. Measured on SQLite:

```text
CREATE TABLE t (a TEXT NOT NULL, loc TEXT, vi INTEGER NOT NULL,
                PRIMARY KEY (a, loc, vi));
INSERT INTO t VALUES ('p', NULL, 0);   -- accepted
INSERT INTO t VALUES ('p', NULL, 0);   -- accepted again: the PK did not collide
```

So a nullable component silently turns a declared key into **no constraint at
all** for exactly the rows where the column is `NULL`, which is precisely the
interesting case. Two consequences follow, and both are answered by the same rule:

1. **A logical key over a nullable column is a bug.** Either the constraint is
   split into partial indexes whose predicates pin the column, or the column
   carries a fachliche non-`NULL` encoding that does not collapse two different
   values. For `locale` (§9.3, §9.4) the answer is the partial split, because a
   `locale` has a real `NULL` meaning that no token may absorb.
2. **`NULL` in a key column is never used to mean "any".** A `NULL` is a value the
   schema happens to permit, not a wildcard; where a rule needs "any value",
   it is expressed as several partial indexes rather than one nullable key.

**Partial indexes are the escape hatch**, because they make the guarantee explicit
rather than relying on `NULL` semantics: a predicate such as
`WHERE scope_kind = 'System'` proves that `scope_system_id` is the key component
for exactly those rows. Each such predicate is paired with a `CHECK` (or, for
`locale`, the two predicates are exhaustive) so that the discriminator and the
populated column cannot disagree.

#### Locale is never encoded by a sentinel (binding)

> **Rule (binding).** A locale column is **`TEXT NULL`**, where `NULL` means
> *language-neutral* and every non-`NULL` value is a real BCP-47 tag. No reserved
> token, and in particular **no valid BCP-47 tag, may stand in for `NULL`.**
> Uniqueness over a locale is expressed by **two partial unique indexes** — one
> `WHERE locale IS NULL`, one `WHERE locale IS NOT NULL` — never by substituting a
> tag for the absent value.

An earlier revision carried a second, non-null *storage key* column that mapped
`NULL` to the tag `'und'` and pinned the pair with a `CHECK`. That was wrong on two
counts, and both matter:

- **`und` is itself a valid BCP-47 tag** meaning *undetermined language*. It is
  something a provider can legitimately return. Collapsing it onto `NULL` would
  make `locale = 'und'` and `locale = NULL` the same storage key, so two
  fachlich different rows would collide.
- **It destroyed data on read.** The round-3 text had the read path map
  `'und'` back to `None`, which would silently turn a genuinely `und`-tagged
  provider value into "language-neutral" — a value the provider never returned.

The partial split needs no token at all, so the collision is unrepresentable
rather than merely forbidden. It also keeps the fachliche column identical to the
storage column, which is what the domain type (`Option<Locale>`) already models.

**Locale canonicalisation (binding).** One locale has exactly **one** stored
spelling: the RFC 5646 canonical case of §3.2 (`en-US`, not `en-us` or `EN-us`).
The write path MUST canonicalise before comparing or storing, and the unique
indexes of §9.3/§9.4 additionally declare `COLLATE NOCASE` so that two rows
differing only in the case of a tag still collide even if a writer skips
canonicalisation. Locale comparison is therefore case-insensitive **by index**,
while the stored value is canonical — `en-US` and `en-us` are one locale, never
two keys. No further normalisation is performed: region and script subtags,
`und`, and tags with extensions are stored as the caller spelled them once
canonically cased, and no locale negotiation or fallback library is designed here
(the fallback *preference* is a product input, `PRODUCT.md` §15.12).

### 3.6 Text comparison and case sensitivity

The default comparison for text columns is **case-sensitive**. Case-insensitive
comparison is requested explicitly where a *user-visible* rule needs it, and then
through `COLLATE NOCASE` on that index or lookup only.

- **Titles and display names**: compared case-insensitively by the search
  projection (§17), never by a unique constraint.
- **Technical keys** (configuration keys, core option keys, catalog keys,
  component ids, library-relative paths): compared **case-sensitively**. These
  are identifiers whose spelling is meaningful, and `Mario.nes` and `mario.nes`
  are two different files on a case-sensitive volume.
- **Locale tags** are compared **case-insensitively by index** (`COLLATE NOCASE`,
  §3.5): BCP-47 is case-insensitive, so `en-US` and `en-us` are the same locale,
  while the stored value is the canonical-case spelling.
- **File-system case policy** belongs to the platform, not to the model. A
  content location records the path **verbatim as observed**; normalizing it
  would make BitArchive unable to find the file it indexed on a case-sensitive
  volume. Duplicate-by-path detection is therefore case-sensitive, and
  duplicate-by-content detection is hash-based and unaffected.

### 3.7 Constraints the model deliberately does not use

**Decision — no unnamed `ON DELETE CASCADE`.** Every foreign key states its
delete behavior explicitly, and a cascade is used only where the child rows are
*intrinsic parts* of the parent and are meaningless without it — currently only
`release_regions` and `release_languages` (§6.4). For everything else the rule
is expressed in §19, because the fachliche meaning of "delete a library source"
is not "delete every game that was ever seen there" (`PRODUCT.md` §7.3).

### 3.8 Audit of every key against the nullable-key rule

Every primary key and unique constraint in the model is listed here with the
classification required by §3.5. This is a complete audit, not a sample: a new key
that is not in this table is a modelling omission, and
`tools/check_data_model.py` fails when it finds one.

```text
A  safe — every key column is NOT NULL, and the key is a full (non-partial) key
B  safe — a partial predicate proves the key columns are non-NULL for the rows it covers
D  safe — the key is the table's single-column storage row identity, so NULL is impossible
```

Every key below is named with its kind, because the three kinds are not
interchangeable: `PK` is the storage primary key (§3.9), `U` is a full unique key
and the only kind a foreign key may target, and `U*` is a **partial** unique index,
which may never be a primary key and never a foreign-key target.

| Table | Storage primary key | Nested / unique keys | Class | Note |
|---|---|---|---|---|
| `schema_migrations` | `(version)` | — | A | `INTEGER`, never `NULL` |
| `library_sources` | `(id)` | `U (location, platform_locator_kind)` | A | both key columns `NOT NULL` |
| `systems` | `(system_id)` | `U (catalog_key)` | A | |
| `games` | `(id)` | — | A | `default_release_id` is nullable but **not** in a key |
| `releases` | `(id)` | `U (id, game_id)`; `U (game_id, release_key)` | A | `release_key` is `NOT NULL` |
| `release_regions` | `(release_id, region)` | — | A | |
| `release_languages` | `(release_id, language)` | — | A | |
| `contents` | `(id)` | — | A | no `archive_content_id` column at all (§6.3) |
| `content_locations` | `(row_id)` | `U* (source_id, relative_path) WHERE location_kind = 'File'`; `U* (archive_content_id) WHERE location_kind = 'ArchiveEntry'` | **B**/**D** | the `File` predicate pins `source_id`/`relative_path` and forbids the archive columns; the `ArchiveEntry` predicate pins `archive_content_id` and forbids source and path — one `CASE` check in §5.2 decides which. `archive_entry_path` is an attribute of the one entry row, not a key component |
| `content_derivations` | `(content_id, kind)` | — | A | |
| `content_derivation_members` | `(content_id, kind, source_content_id)` | `U (content_id, kind, member_index)` | A | |
| `content_fingerprints` | `(row_id)` | `U* (content_id, fingerprint_kind, algorithm) WHERE fingerprint_kind <> 'EntryList'`; `U* (content_id, fingerprint_kind, algorithm, entry_path) WHERE fingerprint_kind = 'EntryList'`; `U* (algorithm, digest) WHERE fingerprint_kind = 'Payload'` | **B**/**D** | the `EntryList` predicate is pinned by `CHECK ((fingerprint_kind = 'EntryList') = (entry_path IS NOT NULL))` |
| `scan_runs` | `(id)` | — | A | |
| `scan_run_issues` | `(scan_run_id, issue_index)` | — | A | |
| `metadata_fields` | `(field_key)` | — | A | |
| `metadata_providers` | `(provider_id)` | — | A | |
| `provider_values` | `(row_id)` | `U* (provider_id, subject_kind, subject_id, field_key, value_index) WHERE locale IS NULL`; `U* (provider_id, subject_kind, subject_id, field_key, locale, value_index) WHERE locale IS NOT NULL` | **B**/**D** | `locale` is the fachliche column; `NULL` = language-neutral. The two predicates are exhaustive, so no row escapes both keys and no sentinel exists (§3.5, §9.3) |
| `manual_overrides` | `(row_id)` | `U* (subject_kind, subject_id, field_key) WHERE locale IS NULL`; `U* (subject_kind, subject_id, field_key, locale) WHERE locale IS NOT NULL` | **B**/**D** | same split (§9.4) |
| `scrape_runs` | `(id)` | — | A | `scope_system_id`/`scope_game_id` are nullable but not keys |
| `scrape_run_items` | `(scrape_run_id, game_id)` | — | A | |
| `media_assets` | `(id)` | — | **A** | the former nullable-column unique constraint is **removed**; slot uniqueness lives in `media_asset_references` (§10.1) |
| `media_asset_references` | `(subject_kind, subject_id, logical_key)` | — | A | |
| `firmware_entries` | `(relative_path)` | — | A | an index key, not an identity |
| `firmware_index_state` | `(singleton)` | — | A | `CHECK (singleton = 1)` |
| `cores` | `(core_id)` | `U (component_key)` | A | |
| `core_selection_overrides` | `(row_id)` | `U* (scope_system_id) WHERE scope_kind = 'System'`; `U* (scope_game_id) WHERE scope_kind = 'Game'`; `U* (scope_release_id) WHERE scope_kind = 'Release'` | **B**/**D** | each predicate pins its scope column, and the scope check constraint requires that column for that kind |
| `core_versions` | `(core_version_id)` | `U (component_key, platform, build_id)`; `U (core_id, platform, build_id)` | A | `revision`/`artifact_digest` are nullable but not keys |
| `runtime_versions` | `(runtime_version_id)` | `U (component_key, platform, version)` | A | |
| `managed_components` | `(component_class, component_key, platform, version)` | — | A | the nullable anchors are governed by the §12.6 check constraint and are not keys |
| `component_index_state` | `(singleton)` | — | A | `CHECK (singleton = 1)` |
| `retroarch_setting_overrides` | `(row_id)` | `U* (setting_key) WHERE scope_kind = 'Global'`; `U* (scope_system_id, setting_key) WHERE scope_kind = 'System'`; `U* (scope_game_id, setting_key) WHERE scope_kind = 'Game'` | **B**/**D** | as above |
| `core_option_schemas` | `(core_version_id)` | — | A | |
| `core_option_definitions` | `(core_version_id, option_key)` | — | A | |
| `core_option_overrides` | `(row_id)` | `U* (core_version_id, option_key) WHERE scope_kind = 'CoreDefaults'`; `U* (core_version_id, scope_system_id, option_key) WHERE scope_kind = 'System'`; `U* (core_version_id, scope_game_id, option_key) WHERE scope_kind = 'Game'` | **B**/**D** | as above |
| `save_states` | `(id)` | `U (file_relative_path)` | **A** | one row per physical state file. `slot` is a technical *attribute*, not a key component (§15.1) |
| `sessions` | `(id)` | `U* (state) WHERE state = 'Active'` | **B** | the single-active-session index is partial over a constant |
| `search_index` | FTS5 `rowid` (implicit) | one row per `game_id`, a rebuild invariant (§17.4) | A | rebuildable projection; an FTS5 virtual table carries no declarative `UNIQUE` constraint |

**No case is left unclassified, and no constraint relies on `NULL` meaning "any".**
Three rules make the table short:

- where a nullable column genuinely distinguishes rows (a scope, a fingerprint
  kind, a location kind), the constraint is **partial** and its predicate is the
  proof;
- where a nullable column has exactly one fachliche meaning and no token may
  stand in for it (`locale`), the constraint is split into the two exhaustive
  partial indexes `IS NULL` / `IS NOT NULL` (§3.5); and
- where a table's uniqueness is entirely partial, its storage primary key is a
  `row_id` that carries no fachliche meaning at all (§3.9).

### 3.9 One real primary key per table (binding)

> **Rules (binding).**
>
> 1. Every persisted table declares **exactly one storage primary key**, and it
>    MUST be implementable as a SQLite `PRIMARY KEY`.
> 2. A **partial unique index is never a primary key.** SQLite has no partial
>    primary key, so a table whose uniqueness rules are all partial still needs
>    its own storage key — a UUIDv7 identity where the row is a fachliche entity,
>    otherwise `row_id INTEGER PRIMARY KEY` (§2.4). An earlier revision labelled
>    the scope indexes of §12.3, §13.1 and §14.1 "Primary key", which #109 could
>    not have implemented.
> 3. **Domain identity, storage primary key and business uniqueness are three
>    different things** and are stated separately for every table. A table may
>    have a domain identity and no business key (a game), a business key and no
>    domain identity (a release region), or a storage key and neither (a scope
>    override keyed only by partial indexes).
> 4. A **foreign key may only target a `PRIMARY KEY` or a full `UNIQUE` key of its
>    parent** — that is SQLite's own rule. A **partial** unique index is **not** a
>    valid foreign-key parent target: SQLite rejects the constraint with
>    "foreign key mismatch". Every foreign key in this model therefore targets a
>    non-partial key, and `tools/check_data_model.py` fails when one does not.
> 5. A storage `row_id` is never the target of a fachliche foreign key, never
>    appears in a domain type, and never leaves the persistence layer (§2.4).

Every table section states its keys in the same three-line form
(`**Domain identity.**`, `**Storage primary key.**`, `**Business uniqueness.**`),
and §20.1 repeats them as the schema-v1 checklist. The distinction is what lets
#109 write `CREATE TABLE … PRIMARY KEY …` without re-deciding any identity, and
what keeps `UNIQUE` constraints from being read as identities.

---

## 4. Ownership and lifecycle

### 4.1 Lifecycle vocabulary

`ARCHITECTURE.md` §35.1 defines four artifact lifetimes. This document applies
them to **data**, one level down:

| Lifetime | Meaning for database rows |
|---|---|
| **Persistent** | User state. Survives restarts, is part of a backup, and is only removed by an explicit user action or an explicit rule in §19. |
| **Rebuildable** | An index or projection over persistent state or over an external owner. May be deleted wholesale at any time and reconstructed without user-visible loss of meaning. |
| **SessionScoped** | Meaningful only while a session exists, and deleted with it. |
| **Temporary** | Work state that may be removed by startup cleanup with no user-visible effect. |

A row that mixes lifetimes MUST be split, so that a rebuild of the rebuildable
part cannot touch the persistent part.

**The split is by owner, not by label.** Where one table's rows genuinely span two
owners, the model names the persistent side as the structure's lifetime and treats
the other side as the separate artifact it is:

```text
save_states        Persistent rows   over  files RetroArch owns
media_assets       Persistent rows   over  blobs in the managed media store
content_derivations + members   Rebuildable rows and the file they name
```

So a `Lifetime` column in §19.7 always reads as exactly one of the four classes, and
a structure that looks like it has two is a repository-port signal that the two
halves have different owners — not a licence to give one table two lifetimes.

### 4.2 Every persisted structure has an owner

For each structure this document states:

```text
Purpose
Identity / primary key
Columns and logical types
Nullable columns and what NULL means
Foreign keys and their delete behavior
Unique constraints
Indexes
Owner
Lifecycle
Retention
Invariant protected
```

### 4.3 The four ownership classes

```text
A. SQLite authoritative (Persistent)
   fachliche identity, relationships, manual overrides, settings,
   scan/scrape history, session history, user flags

B. File-system authoritative (Persistent, indexed by SQLite)
   user ROM/ISO/ZIP files, user firmware, managed component binaries,
   RetroArch save-state files and thumbnails, media blobs

C. Rebuildable projections and indexes
   FTS5 search index, firmware index, managed component index,
   managed media file consistency, generated managed playlists

   (The library's scanned index — content locations and run history — is
    cleared by a library rebuild and re-derived by the re-scan, but its
    recognition evidence, the payload fingerprints, is Persistent; see §19.3.)

D. Generated artifacts (SessionScoped / Temporary)
   sessions/<session-id>/retroarch.cfg, core-options.cfg, launch.json,
   stdout.log, stderr.log, managed .m3u playlists, staging, downloads
```

**Rules (binding).**

- Class A is never rebuilt from class B or C. Losing class A is data loss.
- Class B is never modified by BitArchive except through an explicit,
  user-confirmed product action. No internal cleanup touches it (invariant 20).
- Class C may be dropped and rebuilt at any time without changing a fachliche
  answer.
- Class D never becomes a fachliche source (`ARCHITECTURE.md` §53.7). A `.cfg` is
  output; the setting table of §13 is truth.

### 4.4 Backup

`PRODUCT.md` §39 and `ARCHITECTURE.md` §34 define the backup content. The model
classifies each structure accordingly:

| Structure | In the backup |
|---|---|
| Every class A table | yes |
| `firmware_entries`, `firmware_index_state` | no — rebuildable |
| `managed_components`, `component_index_state` | no — rebuildable from the component store |
| `search_index` (FTS5) | no — rebuildable |
| Media assets: the **rows** | yes (metadata); cover/screenshot blobs are `Rebuildable` per `ARCHITECTURE.md` §34.3 |
| Session rows | yes |
| Save-state metadata rows | yes; save-state **files** only when the user checks the optional box |
| Session artifacts, staging, downloads | no |

No table in this model stores ROM, ISO, ZIP, or firmware bytes; no table stores a
secret in clear text.

---

## 5. Library sources and content locations

### 5.1 `library_sources`

**Purpose.** One configured place BitArchive looks for content. Library Sources
are stable fachliche entities (`ARCHITECTURE.md` §10.1), so a source keeps its
identity across restarts, re-adds, and availability changes.

**Domain identity.** `LibrarySourceId` (UUIDv7 `BLOB(16)`).

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `id` | UUIDv7 | no | Primary key |
| `display_name` | TEXT | no | User-visible name; a label, not an identity |
| `location` | TEXT | no | The source root as the platform reports it |
| `platform_locator_kind` | TEXT enum | no | `Path` today; a later Android build may add `DocumentTree` (`ARCHITECTURE.md` §10.4) |
| `fixed_system_id` | UUIDv7 | yes | `NULL` = detect per file; set = every file of this source belongs to this system (`PRODUCT.md` §7.5) |
| `availability` | TEXT enum | no | `Available`, `Offline`, `PermissionDenied`, `Missing` (`ARCHITECTURE.md` §10.3) |
| `availability_checked_at` | timestamp | yes | When availability was last observed; `NULL` = never observed yet |
| `created_at` | timestamp | no | When the source was added |
| `removed_at` | timestamp | yes | `NULL` = active; set = removed from the active library |

**Foreign keys.** `fixed_system_id → systems.system_id`, delete behavior
`RESTRICT`.

**Storage primary key.** `(id)`.

**Business uniqueness.**

- `UNIQUE (location, platform_locator_kind)` — adding the same root twice would
  create two sources that see the same files and would then scan them twice and
  treat each other's findings as duplicates. This is the rule that protects
  against it. It is deliberately **not** enforced on `display_name`: two sources
  may carry the same label.

**Indexes.** `(availability)` — the startup availability pass and the
"Source Offline" UI state query it. `(removed_at)` — active-library queries.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** Removing a source sets `removed_at`; it does **not** delete the
row and does **not** touch the file system (`PRODUCT.md` §7.3). Files remain
untouched because BitArchive never owned them. The row stays so that re-adding
the same root can be recognized and so that the history of what came from where
stays legible. See §19.2 for the full removal procedure.

**Invariant protected.** Source removal must never delete user files
(invariant 20); an offline source must never delete games
(`ARCHITECTURE.md` §10.3, invariant 18).

**Decision — removability is a timestamp on the source, not a delete.** A hard
delete would immediately break every `content_locations` row that points at the
source, or force a cascade that violates §19. A `removed_at` column keeps the
history intact and makes "re-add the same source" a recognizable, cheap
operation.

**Decision — availability is stored on the source row, not derived per scan.**
Availability is observed outside a scan (at startup, and through platform volume
events), and launch readiness has to answer `SourceOffline` without running a
scan (`ARCHITECTURE.md` §10.3, `ARCHITECTURE.md` §21). `availability_checked_at` is stored next to
it so that a stale observation is distinguishable from a fresh one.

### 5.2 `content_locations`

**Purpose.** Where one `Content` was actually observed. A location is a
**fact about the file system**, not part of the content's identity
(`ARCHITECTURE.md` §2.3, §14.1).

A location is one of exactly two shapes, mirroring
`ARCHITECTURE.md` §14.1's `ContentLocation` sum type: a **`File`** location says
*where a physical file exists*, and an **`ArchiveEntry`** location says *which entry
exists inside which `ArchiveContainer`*.

**Domain identity.** None. A location is not a fachliche entity (invariant 2).

**Storage primary key.** `(row_id)` — `row_id INTEGER PRIMARY KEY`, a storage
convenience only (§2.4, §3.9). The row number is never a domain identity, never
appears in a UI contract, and is never a foreign-key target.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `row_id` | INTEGER | no | Storage primary key; internal only |
| `content_id` | UUIDv7 | no | The content found here |
| `location_kind` | TEXT enum | no | `File` or `ArchiveEntry`; decides which column set is populated |
| `source_id` | UUIDv7 | yes | **`File` only.** The source this path is relative to; `NULL` for `ArchiveEntry` |
| `relative_path` | TEXT | yes | **`File` only.** Path **relative to the source root**, verbatim as observed; `NULL` for `ArchiveEntry` |
| `archive_content_id` | UUIDv7 | yes | **`ArchiveEntry` only.** The `ArchiveContainer` content this entry lives in; `NULL` for `File` |
| `archive_entry_path` | TEXT | yes | **`ArchiveEntry` only.** Entry path inside that container; `NULL` for `File` |
| `observed_filename` | TEXT | no | The file name as observed; a label, not an identity |
| `observed_size` | INTEGER | yes | Last observed size in bytes; `NULL` = not observed |
| `observed_mtime` | timestamp | yes | Last observed modification time; `NULL` = not observed |
| `state` | TEXT enum | no | `Present`, `Missing`, `Unreadable`, `PermissionDenied` |
| `last_seen_scan_run_id` | UUIDv7 | yes | The run that last observed this location; `NULL` = never observed by a persisted run |
| `first_seen_at` | timestamp | no | When this location was first indexed |
| `last_seen_at` | timestamp | yes | When it was last observed present |

**The two shapes are exclusive, and the check constraint is what makes them so.**
This is the round-4 correction: an `ArchiveEntry` location MUST NOT also carry a
`source_id`/`relative_path`, because those describe one *physical file*, and an
entry is identified by its container, not by whichever copy of that container a
scan happened to see first. `#109` can implement this verbatim:

```sql
CHECK (
    CASE location_kind
        WHEN 'File'         THEN source_id IS NOT NULL
                                 AND relative_path IS NOT NULL
                                 AND archive_content_id IS NULL
                                 AND archive_entry_path IS NULL
        WHEN 'ArchiveEntry' THEN source_id IS NULL
                                 AND relative_path IS NULL
                                 AND archive_content_id IS NOT NULL
                                 AND archive_entry_path IS NOT NULL
    END
)
```

`location_kind` is `NOT NULL` and constrained to those two values, so the `CASE`
always has a branch to take. The check is also what makes the two partial unique
indexes below sound: for every row each index covers, the predicate *and* this
check together prove that no key component is `NULL` (§3.5, §3.8).

**Why an `ArchiveEntry` must not be pinned to one file.** An `ArchiveContainer`
content may itself have several `File` locations — the same ZIP found in two
sources, or twice in one source:

```text
ArchiveContainer A
   ├── File location   Source 1 / games/foo.zip
   └── File location   Source 2 / backup/foo.zip

   ArchiveEntry location   A / game.gba   ← one entry, one content, independent of
                                            which copy of the archive was observed
```

The entry `A / game.gba` is the *same entry* whichever copy of `A` is on disk. Had
the entry row also stored `source_id`/`relative_path`, that relationship would have
been a claim about one copy — redundant with the container's own `File` location,
and able to contradict it the moment the archive moved, was renamed, or was
observed in a second source. The physical location of the container is resolved
separately and always the same way:

```text
ArchiveEntry location
    ↓ archive_content_id
ArchiveContainer content
    ↓ its own content_locations rows WHERE location_kind = 'File'
the physical file(s) that currently hold the entry
```

**Foreign keys.**

- `content_id → contents.id` — `RESTRICT`.
- `archive_content_id → contents.id` — `RESTRICT`, nullable (required for
  `ArchiveEntry`, forbidden for `File` by the check above).
- `source_id → library_sources.id` — `RESTRICT`, nullable (required for `File`,
  forbidden for `ArchiveEntry` by the check above).
- `last_seen_scan_run_id → scan_runs.id` — `SET NULL`, nullable (provenance only;
  see §19.7).

The rule that `archive_content_id` must reference a content of kind
`ArchiveContainer` cannot be a foreign key — it is a predicate on the *parent row*,
not on the reference — so it is a **write-path rule and a required integrity test**
(§20.3 test 5).

**Business uniqueness.** Exactly two partial unique indexes:

- **`UNIQUE (source_id, relative_path) WHERE location_kind = 'File'` — one
  physical file belongs to one content.** Without this rule two contents could both
  claim `source X / games/foo.rom` at the same time, so a launch would have two
  different answers for one file and reconciliation could not say which content it
  had just observed. This index alone also gives "the same content has one location
  record per path": a second row agreeing on `(source_id, relative_path)` collides
  whatever its `content_id` is.
- **`UNIQUE (archive_content_id) WHERE location_kind = 'ArchiveEntry'` — one
  supported archive holds at most one imported playable entry.** This is
  `PRODUCT.md` §10 expressed as a database constraint, and §6.3 states the product
  rule it mirrors: a ZIP is supported only when it contains **exactly one**
  unambiguously playable content, so BitArchive imports **at most one** playable
  entry per `ArchiveContainer` content. Without this rule one container could carry
  two `ArchiveEntry` rows — two entry paths for the same payload, or two different
  playable contents — and the launch contract `ArchiveEntry { archive, content }`
  (§5.4) would have more than one row to choose from.

  ```text
  one ArchiveContainer content
      → at most one imported playable ArchiveEntry location
      → therefore at most one row for any (archive, content) pair
  ```

  The rule is about **what BitArchive imports**, not about what a ZIP physically
  contains. A ZIP may hold N files and BitArchive still records **N** `EntryList`
  fingerprints for it (§7.1, §7.2) — diagnostics stay complete. What is bounded is
  the number of *playable entry locations* this archive contributes:

  ```text
  0 unambiguous playable entries   → unsupported / unknown, no ArchiveEntry location
  1 unambiguous playable entry     → imported: exactly one ArchiveEntry location
  2+ playable entries              → ambiguous: not regularly imported
  ```

  `archive_entry_path` therefore stays exactly what it is — the technical entry path
  of the one imported entry row — and is deliberately **not** a key component of any
  `ArchiveEntry` key.

Two keys from an earlier revision are **removed**:

- `(content_id, source_id, relative_path)` was redundant — it contained the `File`
  index above as a prefix-set, so it could never reject a row the smaller key
  accepts.
- `(content_id, archive_content_id, archive_entry_path)` was the `ArchiveEntry` key
  of review round 4. The container-level rule above now decides the same question
  strictly earlier: a second `ArchiveEntry` row for one `archive_content_id` is
  refused whatever its `content_id` **and** whatever its `archive_entry_path`, so
  the entry path no longer needs to appear in a key at all. Keeping it would have
  been a *weaker* key than the one that protects the invariant (it admits exactly
  the two rows the product rule forbids), and no `UNIQUE` constraint is kept without
  a fachliche reason.

A redundant unique index is not free — it is a second thing to keep in step with the
first — so the smallest set that protects both invariants is kept.

Both remaining keys are **partial unique indexes over `location_kind`**, because the
discriminator is what makes each key meaningful, and a partial index may never be
mistaken for the table's primary key (§3.9). Neither depends on SQLite's
`NULL`-are-distinct behaviour: the `File` predicate pins `source_id`/`relative_path`,
the `ArchiveEntry` predicate pins `archive_content_id`, and the `CASE` check above
forbids the archive columns for every `File` row (§3.5, §3.8).

**Decision — the container relationship lives on the location, not on the
content.** An earlier revision put `contents.archive_content_id` on the content,
which made a content belong to exactly one container. That is wrong for the same
reason a content may have several locations at all: identical payload bytes are one
content, so a game may legitimately be observed as a loose file *and* inside one or
more archives at the same time.

```text
Playable content C   (one identity, whatever the packaging)
   ├── File location            source X / loose.gba
   ├── ArchiveEntry location    container A / ROMS/loose.gba
   └── ArchiveEntry location    container B / ROMS/loose.gba
```

With the relationship on the location, all three rows point at the same
`ContentId`, the container is named by identity rather than re-derived from a path,
and adding a second archive that happens to contain the same payload creates a
location instead of a duplicate content. `ARCHITECTURE.md` §14.1 defines exactly
this shape — `ArchiveEntry { archive_id: ContentId, entry_path: String }` is a
variant of `ContentLocation`, not of `Content` — so the location is also where the
architecture puts it, and the round-4 correction above is what made the two agree
column by column.

**This is a location constraint, not a path identity.** It constrains *observations
of files*, not fachliche identity: the content's identity stays its `ContentId`,
two different files with the same bytes remain two locations of one content
(`PRODUCT.md` §7.7), and moving a file removes one location row and adds another
without touching any identity (§7.4). Nothing outside `content_locations` may key
on a path (invariant 2).

**Indexes.**

- `(source_id)` — reconciliation and source removal walk all locations of a
  source. `File` rows only, by the predicate `location_kind = 'File'`.
- `(content_id)` — "where can this content be found" and launch content
  resolution.
- `(state)` — the "missing content" and maintenance-queue queries.

No separate index on `archive_content_id` is declared: the partial unique index
`UNIQUE (archive_content_id) WHERE location_kind = 'ArchiveEntry'` already answers
"the one imported entry of this container" — which is what launch resolution, source
removal and container removal all ask — so a second index over the same column would
be a duplicate of it.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** A location is a *discovery*. It is marked `Missing`, never deleted
while its content still exists (§19.4), and it is deleted only when a library
rebuild clears the scanned index (`PRODUCT.md` §40, §19.3) or when **its source** is
removed — and a source removal deletes `File` rows only, because only they belong to
a source. An `ArchiveEntry` location has no `source_id`, is never deleted by a source
removal, and becomes unreachable rather than deleted when its container loses its
last `File` location (§19.2).

**Invariant protected.** Paths are locations, not identities (invariant 2); a
content can be found in several places at once (`PRODUCT.md` §7.7); a missing
file does not delete a game (invariant 18).

**Decision — relative paths, with the source root stored once.** Storing the
absolute path per location would duplicate the root into every row and would make
"the volume is mounted somewhere else today" look like "every file moved"
(`ARCHITECTURE.md` §10.2). The absolute path is computed as
`library_sources.location + relative_path`. `relative_path` MUST be relative,
MUST NOT escape the source root, and is stored exactly as observed (§3.6). It
applies to `File` locations only; an `ArchiveEntry` has no path of its own.

**Decision — the path is not normalized beyond making it relative.** Case
normalization, Unicode normalization, and symlink resolution are all
platform-dependent, and any of them can turn a path that BitArchive indexed into
a path that does not resolve. Identity comes from the content fingerprint (§7);
the path only has to be good enough to open the file again.

**Decision — no separate table for archive contents.** A ZIP entry is a location,
not a content: it is inside a file that is itself indexed, and `PRODUCT.md` §10
does not require BitArchive to keep an inventory of every entry. Only the entries
BitArchive actually identified are recorded, which also keeps a library scan from
having to persist a row per file inside every archive. Which container an entry
belongs to is not re-derived from these paths — it is the `archive_content_id`
column on the location, so the relationship survives the archive moving, being
renamed, or going offline.

### 5.3 Multi-disc and playlists

A multi-disc release stores one `Content` per disc and, when a `.m3u` exists,
one more `Content` of type `Playlist` whose locations are the `.m3u` files
(`ARCHITECTURE.md` §14.3, `PRODUCT.md` §9).

The rule is:

- An existing, valid `.m3u` is preferred as `LaunchContent` and becomes the
  release's playlist content.
- If none exists, BitArchive may generate a managed `.m3u`. That file lives in
  the BitArchive data area, is `Rebuildable`, and is therefore **not** a
  `content_locations` row of a configured source. It is recorded as a
  `content_derivations` row (§6.6) with one `content_derivation_members` row per
  disc it lists.
- Original files are never modified, and the original files are never the
  generated playlist.

### 5.4 What `LaunchContent` resolves to

`ARCHITECTURE.md` §14.2 models the launch input as:

```rust
pub enum LaunchContent {
    File(ContentId),
    ArchiveEntry { archive: ContentId, content: ContentId },
    ExistingPlaylist { playlist: ContentId },
    ManagedPlaylist { path: PathBuf, members: Vec<ContentId> },
}
```

The model supplies every one of those identities from tables in this document,
**with no entry identity and no path matching**:

| `LaunchContent` variant | This model |
|---|---|
| `File(content)` | the `Content` of kind `Content`; its `Present` location supplies source and relative path |
| `ArchiveEntry { archive, content }` | `content` is the playable entry content and `archive` is the `ArchiveContainer`; together they name **the** `content_locations` row whose `location_kind = 'ArchiveEntry'`, whose `content_id` **is** `content` and whose `archive_content_id` **is** `archive` (§5.2). Both halves are identities, and the row supplies the technical `archive_entry_path` |
| `ExistingPlaylist(content)` | the `Playlist` content whose location is the user's `.m3u` |
| `ManagedPlaylist { path, members }` | a `content_derivations` row (§6.6): `content_id` is the managed playlist, `relative_path` is `path`, and the `content_derivation_members` rows are `members` in `member_index` order |

**Decision — no `ArchiveEntryId`.** An `ArchiveEntryId` would be a fourth identity
for something that is already fully identified by (container content, playable
content, entry path) — all three of which the `ArchiveEntry` location carries
directly (§5.2). Introducing it would add an identity type with no fachliche
content, which §3.1 forbids, and `ARCHITECTURE.md` §14.2 used to name such a type.
That was a genuine source-of-truth contradiction, and it is resolved here in favour
of the model: the architecture's variant now names the container and the playable
content, and the third component — the entry path — is read from the location row
that the two identities select. `ARCHITECTURE.md` §14.2 was corrected in the same
change (`AGENTS.md` §1), and ADR 0005 records the boundary.

**Decision — `archive` alone already identifies the entry location.** §5.2's
`UNIQUE (archive_content_id) WHERE location_kind = 'ArchiveEntry'` is the product
rule of `PRODUCT.md` §10 in relational form: at most one playable entry is imported
per `ArchiveContainer`. The two identities of the launch contract are therefore not
two halves of a *composite* key that could still be ambiguous — the container half
is already unique, and the playable content half only has to **agree** with the row
that the container half selects:

```text
ArchiveEntry { archive, content }
        ↓
archive_content_id = archive  AND  content_id = content
        ↓
0 rows   the archive contributes no imported playable entry, or the pair is wrong
         → not available / readiness failure (ContentUnavailable, §21)
1 row    use its archive_entry_path
2+ rows  impossible by the partial unique index of §5.2
```

An archive that holds the same payload at two entry paths therefore never yields two
rows: the second import is refused by the constraint, so the model needs no
entry-path ordering and §5.4's resolution contains none.

#### Launch resolution (binding)

Resolution is a sequence of identity lookups, never a search over paths. It is the
same procedure for every variant:

```text
Playable ContentId  +  optional Archive ContentId
        ↓  1. the content's location rows
content_locations WHERE content_id = <playable>  [AND archive_content_id = <archive>]
        ↓  2. for an ArchiveEntry the container half is already unique:
        ↓     UNIQUE (archive_content_id) WHERE location_kind = 'ArchiveEntry'
        ↓     admits 0 or 1 row for the pair, never 2 (§5.2), and the row's
        ↓     archive_entry_path is read rather than chosen
   location_kind = 'File'          location_kind = 'ArchiveEntry'
        ↓                                  ↓ archive_content_id
   source_id + relative_path        ArchiveContainer content
        ↓                                  ↓ its File locations
        │                          source_id + relative_path of the container
        └──────────────┬───────────────────┘
                       ↓
        absolute path = library_sources.location + relative_path
                       ↓
        launch: file path            launch: archive path + archive_entry_path
```

> **Rule (binding) — one present location is chosen deterministically.**
>
> 1. Only locations with `state = 'Present'` whose source `availability` is
>    `Available` are launch candidates. A location that is `Missing`,
>    `Unreadable` or `PermissionDenied`, or whose source is not available, is never
>    silently used.
> 2. If **several** candidates exist — the same content found in two sources, or an
>    archive observed at two paths — the resolver picks the smallest by
>    `(source_id, relative_path)` under the case-sensitive text comparison of §3.6.
>    The order is over stable identities and stored paths, so it is a pure function
>    of the database and cannot depend on scan order, filesystem enumeration order
>    or list position (`ARCHITECTURE.md` §53.22, §36.3).
> 3. The `ArchiveEntry` row itself is **not** subject to that order. The container
>    half of `ArchiveEntry { archive, content }` is unique across the entries one
>    archive imports (§5.2), so the pair matches **0 or 1** row and its
>    `archive_entry_path` is read, never chosen. Several entry paths for one
>    `(archive, content)` pair are **impossible by database constraint**, not a
>    legitimate state with a tie-break: an archive holding two playable entries is
>    ambiguous and is not regularly imported at all (`PRODUCT.md` §10, §6.3). Step 2
>    therefore only ever orders *physically identical copies of the container*, and
>    no entry-path ordering exists anywhere in this model.
> 4. If **no** candidate exists, resolution fails with the corresponding readiness
>    issue (`ContentUnavailable`, or `SourceOffline` when the only locations belong
>    to unavailable sources, `ARCHITECTURE.md` §21). It never falls back to a
>    different content, to a path guess, or to a stale observation.
>
> Choosing among several *physically identical* copies changes nothing fachlich:
> they all hold the same payload, so the launched bytes are the same whichever is
> chosen, and the choice is recorded in the session's `content_location_kind` and
> its launch artifacts rather than becoming a new identity.

This rule is stated once, here, and the readiness and session sections reference it
rather than restating it. It is the only place in the model where more than one row
can answer "where is this content", and the answer is a total order rather than an
arbitrary pick. Note what the order covers: **several copies of one payload, never
several entries of one archive** — for `ArchiveEntry` the database admits one row,
so there is nothing to order and no entry-path tie-break exists.

---

## 6. Games, releases and contents

### 6.1 `games`

**Purpose.** The logical work (`ARCHITECTURE.md` §13.1, `PRODUCT.md` §8.1).

**Domain identity.** `GameId`.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `id` | UUIDv7 | no | Storage primary key |
| `first_seen_at` | timestamp | no | When this game entered the library |
| `default_release_id` | UUIDv7 | yes | **Only** the user's explicit default-release override; `NULL` = the user has chosen none |
| `is_favorite` | boolean | no | Favorite flag; default `0` |
| `is_hidden` | boolean | no | "Hidden from library" (`PRODUCT.md` §41) |
| `is_ignored` | boolean | no | "Ignore" (`PRODUCT.md` §41) |

**Foreign keys.** `default_release_id → releases.id`, delete behavior `SET NULL`.

**Storage primary key.** `(id)`.

**Business uniqueness.** **None.** A game has **no natural key**: titles collide
and change, and two games may legitimately share a title.

**Indexes.** `(is_favorite)`, `(is_hidden, is_ignored)` — the standard library
views filter on both (`ARCHITECTURE.md` §36.6).

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** A game is never deleted by a scan, by a scrape, by an import, by a
provider refresh, by a source becoming unavailable, by a **library rebuild**, or by
a per-game index reset. It is removed only by the **full reset** of `PRODUCT.md`
§40 (§19.3). There is **no per-game delete action** in the MVP.

**Invariant protected.** A game does not disappear because a source is offline
(`ARCHITECTURE.md` §53.18, `PRODUCT.md` §7.2); invalid or refreshed provider data
does not delete user state.

**Decision — `default_release_id` is a user override and nothing else; a scan
never writes it.** `PRODUCT.md` §8.6 defines the default-release priority as

```text
1. explicit default release of the game
2. preferred game language
3. preferred region
4. defined fallback rule
```

Step 1 is the **only** step this column represents. Every other step is a rule the
Default Release Resolver applies, so it is never materialised into a row.

Consequences of that reading, all of them binding:

- `default_release_id` is `NULL` for every game the user has never explicitly
  configured, **including a game with exactly one release**.
- A scan, a re-scan, a completed run, an import or a source re-add MUST NOT set it.
  Discovering a release is not choosing one.
- The resolver is always run. When `default_release_id` is `NULL`, steps 2–4 decide
  and the language/region preferences stay effective; they are not silently
  short-circuited by a stored default.
- Clearable: setting it back to `NULL` is the documented "reset to automatic"
  action, and it must be a real state rather than a sentinel release.

**Why not materialise the resolved release.** The obvious shortcut — store the
resolved release once so the resolver has less to do — would be a second truth.
`PRODUCT.md` §8.6 says the user can *override* the automatic choice, which only
means something if the automatic choice keeps being recomputed: if the user later
changes the preferred language or region, a materialised default would keep
answering with the old release while the product claims to honour the preference.
The resolver is deterministic and separately testable (`ARCHITECTURE.md` §13.4), so
recomputing it is cheap and always correct. This is §2.2 applied: the preferred
release is derivable, so it is derived.

**Decision — favorite, hidden and ignored live on the game, not in a separate
"library state" table.** They are properties of the logical work, they are
one-per-game, and all three survive a source removal (`PRODUCT.md` §7.3, `PRODUCT.md` §7.6).
A separate table would add a join to every library query for no modeling gain.
`is_hidden` and `is_ignored` are separate flags because the product separates
them: a hidden game is out of the active library, an ignored one is also excluded
from future scans' "new game" handling. Both are reversible, and neither ever
deletes a file.

### 6.2 `releases`

**Purpose.** A concrete publication or variant of a game
(`ARCHITECTURE.md` §13.2, `PRODUCT.md` §8.2).

**Domain identity.** `ReleaseId`.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `id` | UUIDv7 | no | Storage primary key |
| `game_id` | UUIDv7 | no | The game this release belongs to |
| `system_id` | UUIDv7 | no | The curated system this release targets |
| `release_title` | TEXT | yes | The release's own title, when it differs from the game's; `NULL` = use the effective game title |
| `release_date` | TEXT | yes | The release date as it is known. Partial dates are allowed and stored as a prefix (`1996`, `1996-03`); `NULL` = unknown |
| `revision` | TEXT | yes | e.g. `Rev 1`, `v1.1`; `NULL` = unknown |
| `release_type` | TEXT enum | no | `Official`, `RomHack`, `FanTranslation`, `Homebrew`, `Prototype`, `Unlicensed`; default `Official` |
| `release_key` | `BLOB(32)` | no | The normalized fachliche release identity as a SHA-256 digest, computed by the resolver; see the decision below |
| `archive_kind` | TEXT enum | yes | `Zip` for a container-derived release, `NULL` otherwise |
| `created_at` | timestamp | no | When the release was indexed |

**Foreign keys.**

- `game_id → games.id` — `RESTRICT`.
- `system_id → systems.system_id` — `RESTRICT`.
- `(id, game_id)` carries a separate `UNIQUE` key below, because `contents`
  references that pair (§6.3).

**Storage primary key.** `(id)`.

**Business uniqueness.**

- **Fachliche uniqueness is `(game_id, system_id, release_type, region set,
  language set, revision)`**, computed by the resolver during a scan. Because the
  region and language sets live in child tables, the enforceable part of this key
  is materialized as `release_key`: a `NOT NULL` digest computed by the resolver
  over the normalized release identity (game, system, type, sorted region tags,
  sorted language tags, revision).
  - `UNIQUE (game_id, release_key)`.
  - **Decision — release identity is a computed digest, because SQLite cannot
    index a multi-row set.** The alternative is to store the region and language
    tags as a delimited string column, which would make "all European releases"
    or "all German releases" a `LIKE` scan and would put two representations of
    the same fact into the database. `release_key` is derived from the same
    normalized inputs the resolver already uses, so it is not a second truth — it
    is the *key* form of the one truth, and §20.3 requires it to be recomputed
    whenever a tag changes.
  - `release_key` is a **fingerprint of the release's fachliche identity**, so it
    MUST NOT be used as the primary key (invariant 3).
- `UNIQUE (id, game_id)` — the **foreign-key target** for `contents` (§6.3), so
  that the database itself can prove that a content's release belongs to the
  content's game. It is redundant as a business rule (it contains the primary key)
  but it is required as a key: SQLite can only reference a `PRIMARY KEY` or a full
  `UNIQUE` key, and `(release_id, game_id)` on the child must match the parent's
  key column *set*. §3.9 rule 4.

**Indexes.** `(game_id)`, `(system_id)`, `(system_id, release_type)` for the
filter in `ARCHITECTURE.md` §36.1, `(release_date)` for the `ReleaseDate` sort.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** A release disappears only with its game, which happens only in the
**full reset** (`PRODUCT.md` §40, §19.3). A library rebuild keeps it: releases are
re-derivable from a scan in principle, but keeping them keeps the identity that a
release-scoped core override (§12.3), a save state (§15.1) and a session (§16.1)
reference stable across the rebuild.

**Invariant protected.** `Game ≠ Release` (invariant 1); a game has many releases
(`PRODUCT.md` §8.2).

### 6.3 `contents`

**Purpose.** One concrete technical content unit of a release
(`ARCHITECTURE.md` §13.3, §14.1).

**Domain identity.** `ContentId`.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `id` | UUIDv7 | no | Storage primary key |
| `release_id` | UUIDv7 | no | The release this content belongs to |
| `game_id` | UUIDv7 | no | Redundant-by-design parent for the integrity rule below |
| `content_kind` | TEXT enum | no | `Content`, `ArchiveContainer`, `Playlist` |
| `format` | TEXT | yes | Normalized format token, e.g. `nes`, `chd`, `cue`, `bin`, `gb`; `NULL` = not classified |
| `validation_state` | TEXT enum | no | `Unvalidated`, `Valid`, `Suspect`, `Invalid` |
| `validation_detail` | TEXT | yes | Machine-readable reason for `Suspect`/`Invalid` (`PRODUCT.md` §11); `NULL` = none |
| `disc_index` | INTEGER | yes | 1-based position in a multi-disc release; `NULL` = not part of a multi-disc set |
| `created_at` | timestamp | no | When the content was indexed |

**Foreign keys.**

- `(release_id, game_id) → releases(id, game_id)` — `RESTRICT`. This composite
  foreign key is what makes "a content's release belongs to the content's game" a
  database fact rather than a hope.
- `release_id → releases.id` — covered by the composite key above.

**There is deliberately no `archive_content_id` here.** Which container an entry
was found in is a property of a **location**, not of the content, and it is
modelled in `content_locations` (§5.2). A content may be present loose and inside
several archives at once, so a single container column on the content would make
all but one of those observations unrepresentable. See the decision in §5.2.

**Storage primary key.** `(id)`.

**Business uniqueness.** **None**, and deliberately so. There is **no** unique
constraint on `(release_id, disc_index)`: a multi-disc release may legitimately
hold a disc and an alternate dump of the same disc (a `.bin` and its `.cue`,
`PRODUCT.md` §9), and both are separate contents. Duplicate *files* are a
location-level fact (§5.2), and duplicate *payloads* are a fingerprint-level
fact (§7).

**Indexes.** `(release_id)`, `(game_id)`, `(validation_state)`.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** A content disappears with its release, which happens only in the
**full reset** (`PRODUCT.md` §40, §19.3). A library rebuild clears
`content_locations` but **keeps** the content and its canonical `Payload`
fingerprint, because that fingerprint is the recognition evidence a re-scan needs
to bind a rediscovered payload back to this `ContentId` (§19.3). Losing its last
location does **not** delete a content either: the fingerprint is also what allows
BitArchive to recognize it when it comes back (`PRODUCT.md` §7.6).

**Invariant protected.** `Release ≠ Content` (invariant 1); one release has many
contents.

**Decision — `ArchiveContainer` is a content.** `PRODUCT.md` §10 says the ZIP is
only a container and the hash of the *contained* ROM decides the release
identity. That makes the archive and the ROM two different things that both need
an identity: the ROM is the `Content` the release is identified by, and the
archive is the `ArchiveContainer` whose locations the scanner actually saw. The
alternative — modelling the ZIP as a location of the ROM content — would lose the
distinction between "the file BitArchive opens" and "the entry inside it", which
is exactly what launch content resolution needs (`ARCHITECTURE.md` §14.2, §14.4).

**Decision — the entry-to-container link is an identity, and it lives on the
location.** `ARCHITECTURE.md` §14.1 defines the location as

```rust
ArchiveEntry {
    archive_id: ContentId,
    entry_path: String,
}
```

so the container is named by **identity**, never by a path. A path could not do
this job: the archive's location is a fact about the file system (§5.2) and changes
when the ZIP moves or when its source is offline, while the entry-to-container
relationship must survive both. Reconstituting "which archive does this entry
belong to?" by matching `relative_path` would make the relationship depend on a
location, which invariant 2 forbids, and it would silently produce a second,
possibly different answer after a move.

`content_locations` therefore carries both `archive_content_id` (the container,
`NOT NULL` for `ArchiveEntry`) and `archive_entry_path` (the path inside it,
`NOT NULL` for `ArchiveEntry`), governed by the check constraint in §5.2. Because
the link sits on the location, the same playable payload observed loose and in two
archives is **one content with three locations**, not three contents.

**Decision — no `ArchiveEntryId`.** See §5.4: the `entry` half of
`LaunchContent::ArchiveEntry` is the entry **content**, so no additional identity
type is introduced.

**Decision — at most one playable entry is imported per archive, and the database
enforces it.** `PRODUCT.md` §10 states the product rule exactly: a ZIP is supported
only when it contains **exactly one unambiguously playable content**, and an archive
with several ROMs is ambiguous and is not regularly imported. The three cases are:

```text
0 unambiguous playable entries   → unsupported / unknown, not a release
1 unambiguous playable entry     → imported, one ArchiveEntry location
2+ playable entries              → ambiguous, not regularly imported
```

A ZIP in either of the first two senses is recorded as an unsupported or unknown
file rather than as a release, and BitArchive never extracts an archive
persistently. The rule is relational, not a convention: §5.2 declares

```text
UNIQUE (archive_content_id)
WHERE location_kind = 'ArchiveEntry'
```

so one `ArchiveContainer` content can hold **at most one** imported playable
`ArchiveEntry` location, and a second one is refused by the schema whatever its
`content_id` and whatever its `archive_entry_path`. `archive_entry_path` stays the
ordinary technical entry path of that one row — it is not a key component anywhere.

Note what this does **not** say, in either direction:

- It limits how many entries BitArchive *imports from one* archive, **not** how many
  archives may contain the same playable payload. The same payload in containers A
  and B is one content with two `ArchiveEntry` locations (§5.2), each container
  holding its own single imported entry.
- It limits imported **playable entry locations**, **not** the fingerprint record of
  what the archive contains. An ambiguous ZIP may still carry **N** `EntryList`
  fingerprints in `content_fingerprints` — one per physical entry — because those are
  diagnosis, not import (§7.1, §7.2), and no constraint counts them.

### 6.4 `release_regions`, `release_languages`

**Purpose.** The multi-valued region and language tags of a release
(`PRODUCT.md` §8.4, §8.5).

**`release_regions`**

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `release_id` | UUIDv7 | no | The release |
| `region` | TEXT enum | no | `Europe`, `Usa`, `Japan`, `World`, `Asia`, `Unknown` |

**Domain identity.** None. A region tag is an intrinsic part of its release, not a
separately nameable entity.

**Storage primary key.** `(release_id, region)` — the same region twice on one
release is meaningless. Both components are `NOT NULL`, so this is a real, full
primary key (§3.9).

**Business uniqueness.** None beyond the storage primary key.

**`release_languages`**

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `release_id` | UUIDv7 | no | The release |
| `language` | TEXT | no | BCP-47 primary subtag in canonical case, e.g. `de`, `en`, `ja` (§3.2) |

**Domain identity.** None, for the same reason.

**Storage primary key.** `(release_id, language)`. Both components are `NOT NULL`,
so this is a real, full primary key (§3.9).

**Business uniqueness.** None beyond the storage primary key.

**Foreign keys.** `release_id → releases.id` — **`ON DELETE CASCADE`**. This is
the one place a cascade is correct: a region tag has no meaning without its
release, holds no user data, and cannot be referenced by anything. Deleting the
release without its tags would leave unreachable rows that a later join could
resurrect as a phantom region.

**Indexes.** `(region)`, `(language)` — the "preferred region" and "preferred
language" resolution and the corresponding filters read tags across releases.

**Owner.** SQLite. **Lifecycle.** Persistent (as part of the release).

**Retention.** Follows the release, and only the release.

**Invariant protected.** Region and language are *sets*, and
`ARCHITECTURE.md` §13.4/§20.4 resolution reads them as sets. A single string
column would make a multi-region release unrepresentable.

### 6.5 `systems`

**Purpose.** The stable identity of a curated system, so that a persisted release
can carry a `SystemId` without the database owning the curated catalogue
(ADR 0004).

**Domain identity.** `SystemId`.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `system_id` | UUIDv7 | no | Storage primary key |
| `catalog_key` | TEXT | no | The curated slug, e.g. `gba`, `gbc`, `gb` |
| `created_at` | timestamp | no | When this system was first materialized |

**Storage primary key.** `(system_id)`.

**Business uniqueness.** `UNIQUE (catalog_key)`.

**Indexes.** `(catalog_key)` — resolving a curated key to its stable identity on
every startup and scan.

**Owner.** SQLite, for **identity binding only**. **Lifecycle.** Persistent.

**Retention.** Never deleted while a release references it. Removing a curated
system is a catalogue change that requires a migration, not a delete.

**Invariant protected.** Paths and slugs are not domain identities (invariant 2).
The curated slug is curated data; `SystemId` is the fachliche identity a persisted
release carries (`bitarchive-domain::system` documents exactly this gap).

**Decision — the app materializes one row per curated system lazily, on first
need, and never stores curated metadata in it.** The system's display name,
manufacturer, release year, artwork, accepted formats and recommended core stay
in the curated catalogue (§12.2). Storing them here would create a second copy of
curated data that a later catalogue update would have to keep in sync — the
database would either go stale or start overriding reviewed data.

### 6.6 `content_derivations` and `content_derivation_members`

**Purpose.** Anything BitArchive derived from a content and wrote into its own
data area — today the managed multi-disc `.m3u`
(`ARCHITECTURE.md` §14.3, `PRODUCT.md` §9).

The structure is a **header plus members**, because one derivation can reference
2..N source contents in a defined order and a single table cannot: a primary key
that identifies one derivation can only hold one member row.

#### `content_derivations`: the generated artifact

**Domain identity.** None. A derivation is not a fachliche entity, so it gets no
UUID of its own; it is identified by the content it produces.

**Storage primary key.** `(content_id, kind)` — one generated artifact per content
and kind. Both components are `NOT NULL`, so this is a real, full primary key.

**Business uniqueness.** None beyond the storage primary key.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `content_id` | UUIDv7 | no | The derived content (the managed playlist) |
| `kind` | TEXT enum | no | `ManagedPlaylist` today |
| `relative_path` | TEXT | no | Location **inside the BitArchive data area**, relative to the generated-files root |
| `member_count` | INTEGER | no | Number of members; `>= 2` for `ManagedPlaylist`. See the decision below |
| `created_at` | timestamp | no | When it was generated |

**Check constraint.** `member_count >= 2`. A managed playlist is BitArchive's
multi-disc fallback and exists for no other purpose, so a one-member playlist is not
a degenerate valid case — it is a modelling error (see the decision below).

**Foreign keys.** `content_id → contents.id` — `CASCADE`. The header is an
intrinsic part of the content it produces: a derivation without its playlist
content is meaningless, and only a content deletion can remove it.

**Indexes.** `(kind)` — "which managed playlists exist" for a rebuild pass.

**Cross-row invariant (not expressible as a declarative constraint).**
`member_count` MUST equal the number of `content_derivation_members` rows for the
same `(content_id, kind)`. SQLite cannot express "a column equals the row count of
another table" without a trigger, so §20.3 test 26 requires it as a write-path rule
**and** as a check performed when a derivation is read back or regenerated.

**Why `member_count` is stored although it is derivable.** It is a deliberate,
bounded exception to §2.2, justified by what it protects rather than by
convenience: it makes a *truncated* derivation detectable. A playlist file whose
members were half-written is dangerous — RetroArch would happily start disc 1 of a
3-disc game and the user would discover the loss later — and comparing a stored
count against the actual member rows turns that into a detectable inconsistency
instead of a silent one. The column is cheap (one integer per derivation), it is
written in the same transaction as the members, and the invariant above is tested,
so it cannot drift unnoticed. It is not a second source of truth for *which*
members exist; only for *how many* were intended.

**Owner.** SQLite (the row) / file system (the file, BitArchive-owned).
**Lifecycle.** **Rebuildable** — the row and the file it names can both be
recreated from the member contents. This is the header half of one structure; the
members half carries the same lifetime (§6.6 decision).

**Retention.** Deleted and regenerated freely. Its members cascade with it.

#### `content_derivation_members`: the ordered members

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `content_id` | UUIDv7 | no | The derivation header this member belongs to |
| `kind` | TEXT enum | no | The derivation header's kind |
| `source_content_id` | UUIDv7 | no | One content the derivation was built from |
| `member_index` | INTEGER | no | The member's **1-based** position in the derivation; `>= 1` |

**Domain identity.** None. The structure is identified by the derivation it
belongs to.

**Storage primary key.** `(content_id, kind, source_content_id)` — **one row per
member, and the same member cannot appear twice in one derivation.** A playlist
that listed the same disc twice would be a defect, not a feature, so the composite
key refuses it.

**Business uniqueness.**

- `UNIQUE (content_id, kind, member_index)` — **the member order is unambiguous.**
  The primary key alone would allow two members to claim position 1, which would
  make the playlist's order depend on row insertion order. Both constraints
  together are what make the order well defined.

**Foreign keys.**

- `(content_id, kind) → content_derivations(content_id, kind)` — `CASCADE`: a
  member row has no meaning without its derivation header, and this is the one
  place in the model where a multi-column cascade is exactly right.
- `source_content_id → contents.id` — `RESTRICT`: a derivation must not be
  able to destroy, or silently lose, a content it was built from.

**Indexes.** `(source_content_id)` — "which derived files depend on this disc"
before deleting anything. `(content_id, kind, member_index)` — reading a playlist
in order.

**Owner.** SQLite (the rows) / file system (the file, BitArchive-owned).
**Lifecycle.** **Rebuildable**, the same single classification as its header: a
member row is never persistent on its own, and header plus members are dropped and
regenerated as one unit. Neither half is `Persistent`, so the structure carries
exactly one lifetime rather than two.

**Retention.** Deleted and regenerated freely. Deleting a derivation never touches
`source_content_id`'s locations, because those are user files.

**Invariant protected.** Original files are never modified (`PRODUCT.md` §9);
generated files never become authoritative (`ARCHITECTURE.md` §53.7).

**Decision — header plus members, rather than one wide row of member columns.** A
fixed number of member columns would cap the disc count arbitrarily and make "disc
7" a different kind of fact from "disc 1". The two-table form states the real
cardinality (1 derivation → N ordered members) and keeps the order in a column
that a unique constraint can protect, which a serialized member list could not.

**Decision — the member count is 2..N, not 1..N.** `PRODUCT.md` §9, `PRODUCT.md`
§10 and `ARCHITECTURE.md` §14.3 together leave no room for a one-member managed
playlist:

```text
PRODUCT.md §9      managed .m3u exists for multi-disc releases
ARCHITECTURE §14.3 it is the THIRD-priority fallback, reached only when no
                   valid .m3u exists AND a known multi-disc structure does
PRODUCT.md §10     a ZIP holds exactly one playable entry, so a single-content
                   release is launched directly and needs no playlist
```

A release with one content is launched from that content (`LaunchContent::File`,
§5.4); it has no reason to acquire a generated playlist. Allowing `member_count = 1`
"just in case" would admit a row no code path creates and no product requirement
describes, so the constraint is `>= 2`. If a future requirement genuinely needs a
single-member playlist, it arrives with a migration that changes this check — not
speculatively now.

### 6.7 The relationship model

```text
systems ──1:N──> releases <──N:1── games
                     │                 ▲
                     │                 │ default_release_id
                     │                 │ (SET NULL, user override only)
                     │ 1:N             │
                     ▼                 │
                  contents ────────────┘  (game_id, integrity only)
                     │ 1:N
                     ▼
             content_locations ──N:1──> library_sources

contents ──1:N──> content_locations ──0:N──> contents
                                   (archive_content_id: entry location → its container)
games ──1:N──> releases ──1:N──> contents ──1:N──> save_states
```

A game is **not** reachable from a location by a path. Every traversal goes
through identities, which is what lets a file move without changing a single
fachliche relationship (`PRODUCT.md` §7.6).

The preferred release is deliberately absent from this diagram as a stored value:
it is the output of the Default Release Resolver, recomputed from
`default_release_id` and the language/region preferences on every read (§6.1).

---

## 7. Content fingerprints

### 7.1 `content_fingerprints`

**Purpose.** Recognize a content again after it moves, is renamed, is copied, or
reappears in another source (`PRODUCT.md` §7.6, §7.7). A fingerprint is evidence
*about* a content, never the content's identity (invariant 3).

**Domain identity.** None. A fingerprint is evidence *about* a content, not an
entity, so it gets no UUID of its own.

**Storage primary key.** `(row_id)` — `row_id INTEGER PRIMARY KEY`, a storage
convenience only (§2.4, §3.9). An earlier revision declared two alternative
"primary keys" here, one of which contained the nullable `entry_path`; neither was
a SQLite primary key, and #109 could not have implemented the table from it.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `row_id` | INTEGER | no | Storage primary key; internal only |
| `content_id` | UUIDv7 | no | The content this fingerprint describes |
| `fingerprint_kind` | TEXT enum | no | `Payload`, `Container`, `EntryList` |
| `algorithm` | TEXT enum | no | **`Sha256` is the only value in schema v1.** The column exists so a later algorithm can be added without a migration, not to admit one now |
| `digest` | `BLOB(32)` | no | The 32 raw bytes of the SHA-256 digest |
| `entry_path` | TEXT | yes | The archive entry this digest belongs to; `NOT NULL` only for `EntryList` rows |
| `byte_size` | INTEGER | yes | The size the digest covers; `NULL` = not recorded. An **invalidation hint only** — it is deliberately not part of any uniqueness rule (see below) |
| `computed_at` | timestamp | no | When it was computed |
| `source_scan_run_id` | UUIDv7 | yes | The run that computed it; `NULL` = computed outside a persisted run, or its run was pruned (§19.7) |

**Check constraint.** `entry_path` is populated for exactly one kind, which is what
makes the `EntryList` uniqueness rule below effective — `entry_path` is nullable,
and a nullable key component would otherwise leave that rule unenforced (§3.5):

```sql
CHECK ((fingerprint_kind = 'EntryList') = (entry_path IS NOT NULL))
```

A `Payload` or `Container` row therefore never carries an entry path, and an
`EntryList` row always does. §20.3 test 33 asserts it.

**Foreign keys.** `content_id → contents.id` — `CASCADE` (a fingerprint
without its content is meaningless). `source_scan_run_id → scan_runs.id` —
`SET NULL`, nullable: the run is **provenance**, not an owner, and a fingerprint
stays valid and usable when the run that computed it has been pruned (§19.7).

**Business uniqueness.**

- `UNIQUE (content_id, fingerprint_kind, algorithm) WHERE fingerprint_kind IN
  ('Payload', 'Container')` — **one `Payload` and one `Container` row per
  content.** This is what makes "the canonical payload fingerprint" well defined
  rather than a convention (§19.3).
- `UNIQUE (content_id, fingerprint_kind, algorithm, entry_path) WHERE
  fingerprint_kind = 'EntryList'` — **one `EntryList` row per entry of one
  content.** The predicate is pinned by the check constraint above, so
  `entry_path` is known non-`NULL` for every row it covers.
- **`UNIQUE (algorithm, digest) WHERE fingerprint_kind = 'Payload'` — the
  recognition key.** This single partial index is what guarantees the property the
  whole re-identification path rests on:

  ```text
  Payload / Sha256 / digest abc   →  at most one ContentId
  ```

  It answers "have I seen these bytes before?", which is what moved-file
  recognition, duplicate detection and content matching all need. Two contents can
  therefore never both claim the same payload digest, and identical payload files
  are two locations of one content (`PRODUCT.md` §7.7).

`(content_id)` and `(source_scan_run_id)` are ordinary indexes, listed below; there
is no key on `entry_path` alone and none on `byte_size`.

**The recognition key is exactly the lookup key (binding).** §19.3's rescan looks a
payload up by `(fingerprint_kind = 'Payload', algorithm, digest)` and nothing else,
so the constraint above covers **precisely** those columns. An earlier revision
declared `UNIQUE (algorithm, fingerprint_kind, digest, byte_size)` globally and
looked up on three of those four columns, which guaranteed nothing:

- `byte_size` is **nullable**, and SQLite treats `NULL` values in a unique index as
  distinct from one another. Two rows
  `Payload / Sha256 / abc / NULL` could therefore coexist, the lookup would return
  two contents, and "digest → exactly one `ContentId`" would be false.
- The constraint was **global**, so it also applied to `EntryList` rows and forbade
  the same entry bytes appearing in two different archives — which is legitimate:
  two ZIPs may contain the same playable ROM, and each archive needs its own
  `EntryList` row for that entry.

The rule is now:

| Fingerprint kind | Uniqueness |
|---|---|
| `Payload` | **globally unique** on `(algorithm, digest)`: a payload digest belongs to one content |
| `Container` | not globally unique: the same archive bytes may be observed as two container contents |
| `EntryList` | only per `(content_id, entry_path)` via the primary key; **the same entry bytes may occur in many archives** |

`byte_size` stays as an **invalidation hint** (§7.3): it is compared to shortcut a
re-hash, and it never narrows or widens the recognition identity. §20.3 test 31
asserts that the lookup columns and the constraint columns are identical.

**Cardinality, stated because two consumers depend on it.**

- The `UNIQUE (content_id, fingerprint_kind, algorithm) WHERE fingerprint_kind IN
  ('Payload', 'Container')` index admits **exactly one `Payload` row per content**.
  "The canonical payload fingerprint" is therefore well defined rather than a
  convention, which is what §19.3's re-identification path relies on.
- `Payload` and `Container` are **one row each per content**; `EntryList` may have
  **N rows**, one per entry, distinguished by `entry_path` in its own partial
  unique index.
- A second `Payload` digest for one content is refused rather than stored: it would
  make "which bytes identify this content?" ambiguous, and §19.3's lookup would
  stop being a function.

**Indexes.** `(content_id)` — reading a content's own fingerprints.
`(source_scan_run_id)` — reporting what a run computed. Neither is a key.

**Owner.** SQLite (class A for the *link* to a content) / derived from class B
bytes. **Lifecycle.** **Persistent.** This table is recognition evidence, not an index:
it is the only thing that lets a rescan bind a rediscovered payload back to an
existing `ContentId` after `content_locations` has been reset (§19.3), and that is
not derivable from anything else in the database.

*Individual rows* are recomputable — while the file is still present, a row may be
recomputed and replaced without changing a fachliche answer — but that is a
per-row property, not a second lifetime for the table. §4.1 requires a structure to
carry exactly one lifetime, and for this one it is `Persistent`.

**Decision — SHA-256 only, and the fixed `BLOB(32)` is what enforces it.** An
earlier revision allowed `Crc32`, `Md5` and `Sha1` while fixing the storage at 32
bytes, which is inconsistent on its face: a CRC-32 is 4 bytes and an MD5 is 16.
Two ways out existed, and the simpler one is chosen:

- **Chosen:** the persistable algorithm is **SHA-256 alone**, so `digest` is
  exactly 32 bytes on every row and the schema needs no length rule. This matches
  `ARCHITECTURE.md` §11, which makes SHA-256 *the* canonical content hash and says
  other hashes are computed "only when external providers or reference data
  require them".
- Rejected: an algorithm-dependent length. It would make `digest` a variable-width
  column whose meaning depends on `algorithm`, add a `CHECK` that must be kept in
  step with the enum, and admit four algorithms into schema v1 so that a
  reference-ROM system the MVP explicitly excludes would have somewhere to land.

`Crc32`, `Md5` and `Sha1` are therefore **out of scope for this fingerprint
table**. When a provider or a reference dataset genuinely requires one, it is
added by a migration that also decides its storage width, together with the
consumer that needs it — not speculatively now. Upstream values that are merely
*compared* (for example the change-hint CRC-32 of a core build channel, ADR 0002)
are not content fingerprints and do not belong here.

**Retention.** Recomputation replaces the row. A fingerprint is deleted with its
content. A fingerprint is **never** the target of a foreign key from anywhere
else in the model.

**Invariant protected.** Hashes are fingerprints, not identities or primary keys
(invariant 3); SHA-256 is the canonical content hash
(`ARCHITECTURE.md` §11).

### 7.2 What each fingerprint kind means

```text
Payload      the playable content itself.
             For a plain ROM/ISO: the file's bytes.
             For a ZIP: the bytes of the one playable entry, NOT the container.
             → This is the fingerprint that identifies a release's content.

Container    the bytes of the archive file as a whole.
             → Detects "the same ZIP file moved", cheaply, without re-reading members.

EntryList    one digest per entry inside an archive, with its entry_path.
             → Lets a change inside an archive be detected without hashing
               everything, and makes an ambiguous archive's contents diagnosable.
```

`PRODUCT.md` §10 is explicit that the container is not the identity: **a
release's content identity is always the `Payload` fingerprint**, and the
`Container` fingerprint only ever identifies the file.

### 7.3 Hashing and invalidation

- Hashing is **streaming** (`ARCHITECTURE.md` §11). No implementation detail
  belongs here, but `byte_size` and `observed_mtime` (§5.2) exist precisely so
  that a re-scan can decide whether a full re-hash is necessary.
- `(observed_size, observed_mtime)` is an **invalidation hint, not a truth**. If
  it is unchanged, the existing fingerprint may be reused; the moment either
  changes, the content MUST be re-hashed.
- **Decision — the model does not treat size and mtime as proof.** A file can
  change without its size changing, and mtime is user-settable. Treating them as
  proof would make BitArchive report a stale fingerprint as current, which is
  worse than re-hashing.

### 7.4 Recognition, movement and duplication

| Question | How the model answers it |
|---|---|
| Did this file move? | The scan computes the `Payload` fingerprint, looks it up in the recognition index, finds an existing content, and adds a `content_locations` row for the new path instead of creating a second game. |
| Is this a duplicate? | Same `Payload` fingerprint, different `content_locations` rows. Duplicates are shown once in the library (`PRODUCT.md` §7.7). |
| Which entry of this ZIP is the game? | The `Payload` row whose `content_id` is the `Content` of the release; the archive is the separate `ArchiveContainer` content. |
| Is this the same archive I saw before? | The `Container` fingerprint. |

Recognition is limited to **exact payload matches**. `PRODUCT.md` §11 excludes
full ROM integrity verification against reference databases from the MVP, so:

> **Rule (binding).** The model MUST NOT invent a reference ROM database, a
> known-good hash set, or an integrity verdict that BitArchive cannot derive from
> the user's own bytes.

---

## 8. Scan runs

### 8.1 `scan_runs`

**Purpose.** A scan is a fachliche event with a history, not a transient job:
"when was this library last fully scanned successfully" is what decides whether
reconciliation is allowed (invariant 18).

**Domain identity.** `ScanRunId`.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `id` | UUIDv7 | no | Storage primary key |
| `target_kind` | TEXT enum | no | `FullLibrary`, `LibrarySource`, `Rebuild` |
| `target_source_id` | UUIDv7 | yes | Required for `LibrarySource`; `NULL` otherwise |
| `status` | TEXT enum | no | `Running`, `Completed`, `Cancelled`, `Failed` |
| `started_at` | timestamp | no | Run start |
| `finished_at` | timestamp | yes | `NULL` while running |
| `reconciliation_eligible` | boolean | no | Whether this run covered its whole target without a gap; default `0` |
| `discovered_count` | INTEGER | no | Files observed |
| `imported_count` | INTEGER | no | Contents newly indexed |
| `updated_count` | INTEGER | no | Contents whose location or fingerprint changed |
| `rediscovered_count` | INTEGER | no | Contents recognized at a new path |
| `missing_count` | INTEGER | no | Locations this run marked missing |
| `unknown_count` | INTEGER | no | Files not recognized as a supported content |
| `unsupported_count` | INTEGER | no | Files of a known-unsupported kind |
| `problem_count` | INTEGER | no | Entries in `scan_run_issues` |

**Foreign keys.** `target_source_id → library_sources.id` — `RESTRICT`, nullable.

**Storage primary key.** `(id)`.

**Business uniqueness.** **None.** A run is an event: two identical runs are two
events.

**Indexes.** `(started_at)` — the activity view and "most recent successful full
scan". `(target_source_id, started_at)` — the same question per source.
`(status)` — recovery of a run that a crash left `Running`.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** Scan runs are **kept**, because they are the evidence that a
reconciliation was legitimate. §19.7 defines the pruning rule.

**Invariant protected.** Destructive reconciliation only after a successful full
scan (invariant 18). The `status` and `reconciliation_eligible` columns are what
make that checkable instead of assumed:

> **Rule (binding).** A run may perform destructive reconciliation only if
> `status = Completed` **and** `reconciliation_eligible = 1`. A run that was
> cancelled, failed, was interrupted, or skipped an unreadable subtree MUST leave
> every location untouched.

**Decision — coverage is a stored flag, not inferred from the counters.** A run
can complete with plenty of unknown files and still be a complete observation of
its target; conversely a run that never opened a permission-denied directory
looks successful by counter alone. The flag records the fact that matters, and
the reason a run is not eligible is recorded as a `scan_run_issues` row.

**Decision — the run row is written when the run starts.** Writing it only at the
end would mean an interrupted scan leaves no trace at all, so a crash would be
indistinguishable from "no scan was running" — and the next startup could not
tell whether reconciliation was ever completed.

### 8.2 `scan_run_issues`

**Purpose.** Per-item problems and the unknown/unsupported inventory of a run
(`PRODUCT.md` §12 reports them separately in the scan result).

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `scan_run_id` | UUIDv7 | no | The run |
| `issue_index` | INTEGER | no | Stable order within the run |
| `kind` | TEXT enum | no | `UnknownFile`, `UnsupportedFile`, `Unreadable`, `PermissionDenied`, `InvalidArchive`, `AmbiguousArchive`, `SourceOffline` |
| `source_id` | UUIDv7 | yes | The source, when the issue is source-scoped |
| `relative_path` | TEXT | yes | The path, when the issue is file-scoped |
| `detail` | TEXT | yes | Machine-readable detail, e.g. the missing companion file of a `BIN/CUE` pair |
| `byte_size` | INTEGER | yes | Observed size, when known |
| `prevents_reconciliation` | boolean | no | Whether this issue is what made the run ineligible |

**Domain identity.** None. An issue is an intrinsic part of its run.

**Storage primary key.** `(scan_run_id, issue_index)`. Both components are
`NOT NULL`, so this is a real, full primary key.

**Business uniqueness.** None beyond the storage primary key.

**Foreign keys.** `scan_run_id → scan_runs.id` — `CASCADE` (an issue has no
meaning without its run). `source_id → library_sources.id` — `RESTRICT`,
nullable.

**Indexes.** `(scan_run_id, kind)` — the grouped scan summary. `(prevents_reconciliation)`
— explaining why a run may not reconcile.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** Follows its run (§19.7).

**Invariant protected.** Unknown and unsupported files are reported, never
silently absorbed (`PRODUCT.md` §12); uncertain classification is never
guessed (`PRODUCT.md` §7.5).

**Decision — the unknown/unsupported inventory is stored as run issues rather
than as a permanent "unknown files" table.** The product shows the inventory as
the result *of a run*. A permanent table would need its own reconciliation
semantics and would become a second, competing answer to "what is in my
library?"; a run-scoped row is exactly as long-lived as the fact it records.

### 8.3 What a scan run is not

A scan run is **not** the `JobManager`'s job state (`ARCHITECTURE.md` §30.4).
`JobManager` persistence covers every job type and belongs to the Issue that
implements job recovery. This model persists the *scan's fachliche history*
because reconciliation depends on it, and deliberately does not pre-empt the
generic job table.

---

## 9. Metadata and overrides

### 9.1 The three layers

```text
provider_values  (what a provider said, with provenance)
        +
manual_overrides (what the user decided)
        ↓
effective metadata = resolve(manual override, then provider value, then local)
```

`ARCHITECTURE.md` §16.2 makes this the priority order. It is expressed in the
model as **two tables plus a resolver**, never as one table with a "current
value" column.

### 9.2 `metadata_fields`

**Purpose.** The closed set of metadata fields BitArchive understands, so that a
metadata value can never be an arbitrary string key that nothing can render.

**Domain identity.** None. A field key is curated vocabulary, not a minted
identity (`ARCHITECTURE.md` §53.2's principle that a technical key is not a domain
identity).

**Storage primary key.** `(field_key)`.

**Business uniqueness.** None beyond the storage primary key.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `field_key` | TEXT | no | Storage primary key, e.g. `title`, `description`, `genre`, `developer`, `publisher`, `release_date`, `players`, `region`, `alternate_title` |
| `value_kind` | TEXT enum | no | `Text`, `Date`, `Integer`, `TagList` |
| `is_multi_valued` | boolean | no | Whether several values may coexist for one subject and locale |
| `is_localizable` | boolean | no | Whether the field may carry a locale |

**Owner.** Populated from the curated field definition set shipped with the app.
**Lifecycle.** Persistent (small, curated, migration-managed).

**Invariant protected.** Provider data and user input are external; they MUST
land in a known field, not in a free-form bag.

### 9.3 `provider_values`

**Purpose.** One value a metadata provider returned, with full provenance
(`ARCHITECTURE.md` §16.3, `PRODUCT.md` §15.9).

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `row_id` | INTEGER | no | Storage primary key; internal only |
| `provider_id` | TEXT | no | e.g. `screenscraper` |
| `subject_kind` | TEXT enum | no | `Game`, `Release` |
| `subject_id` | UUIDv7 | no | The subject's identity; interpretation depends on `subject_kind` |
| `field_key` | TEXT | no | One of `metadata_fields.field_key` |
| `locale` | TEXT | yes | The fachliche locale: a BCP-47 tag in canonical case (§3.5), or `NULL` for **language-neutral**. It **is** part of the uniqueness rules, through the two partial indexes below |
| `value_index` | INTEGER | no | The value's position within this provider field. **Always set, even for a single-valued field** |
| `value_text` | TEXT | yes | The value as text |
| `value_number` | INTEGER | yes | The value when the field is numeric |
| `provider_item_id` | TEXT | yes | The provider's own item identifier. **Provenance only** |
| `fetched_at` | timestamp | no | When the provider returned it |
| `scrape_run_id` | UUIDv7 | yes | The run that fetched it; `NULL` = fetched outside a persisted run |

**`value_index` — definition (this column is part of the uniqueness rules and of the
resolver's tie-break, so it is defined explicitly rather than implied).**

- **Logical type:** `INTEGER`.
- **Nullability:** `NOT NULL`. A `NULL` in a key component is not comparable in
  SQLite, so a nullable `value_index` would silently disable the uniqueness rule
  that is supposed to detect duplicates.
- **Valid range:** `0 <= value_index`. Zero-based, because it is a position in a
  list and the first value of a field is the common case.
- **Meaning:** the identity of *this* value among the several values the same
  provider returned for the same `(subject, field_key, locale)`. It is not a
  ranking, not a relevance score and not a timestamp.
- **Deterministic assignment (binding):** for one
  `(provider_id, subject_kind, subject_id, field_key, locale)`, a write assigns
  `0, 1, 2, …` in the order the provider adapter yielded the values for that field.
  The order MUST come from the provider's own response order, so the same response
  persisted twice produces the same indices. A refresh replaces the whole set for
  the provider/field/locale it refreshed, so indices never accumulate across
  refreshes.
- **Cardinality rule:** `metadata_fields.is_multi_valued = 0` means only
  `value_index = 0` may exist for that `(subject, field, locale)`; a multi-valued
  field may have any number of consecutive indices. The schema MUST enforce the
  single-valued case and the write path enforces consecutiveness.
- **Not a surrogate for order of discovery:** it deliberately does **not** encode
  "which value arrived first across refreshes". Provenance timing is
  `fetched_at`, and mixing the two would make the key depend on network timing.

**Domain identity.** None. A provider value is provenance about a subject, not an
entity of its own.

**Storage primary key.** `(row_id)` — `row_id INTEGER PRIMARY KEY`, a storage
convenience only (§2.4, §3.9). The row number carries no fachliche meaning and is
never a foreign-key target.

**Business uniqueness.** Two **exhaustive** partial unique indexes, one per
locale shape:

```sql
UNIQUE (provider_id, subject_kind, subject_id, field_key, value_index)
    WHERE locale IS NULL                      -- language-neutral values
UNIQUE (provider_id, subject_kind, subject_id, field_key, locale, value_index)
    WHERE locale IS NOT NULL                  -- localised values
```

Every row matches exactly one of the two predicates, so no row escapes both rules,
and each rule's column list contains no nullable component for the rows it covers
(§3.5, §3.8). The rules say: for one provider, one subject, one field and one
locale, a value index identifies exactly one value.

**Decision — the nullable locale is carried by partial indexes, not by a sentinel
column.** The fachliche column `locale` is genuinely nullable: a provider value may
be language-neutral. Putting it directly in one composite key would have left that
key **unenforced for exactly the language-neutral rows**, because SQLite treats
`NULL` values in a unique index as distinct.

An earlier revision solved that with a second, non-null storage-key column that
mapped `NULL` to the tag `'und'` and pinned the pair with a `CHECK`. **That was
wrong and is removed:**

- **`und` is a real BCP-47 tag** with the meaning *undetermined language*, and it
  is a value a provider can legitimately return. Using it as the token for "no
  locale" collapsed two different fachliche values — `locale IS NULL` and
  `locale = 'und'` — onto one key, so a genuine `und` value and a
  language-neutral value could not coexist.
- **It would have destroyed the real tag on read.** The previous text had the read
  path map `'und'` back to `None`, which silently rewrites a `und`-tagged provider
  value into "language-neutral" — a value the provider never returned.

The partial split removes the need for any token: `NULL` stays `NULL`, `'und'`
stays a real locale, and the two are different keys by construction. `locale`
therefore remains the single locale column, in the shape the domain type
(`Option<Locale>`) already has.

**Locale canonicalisation (binding).** A writer MUST store the RFC 5646 canonical
case of the tag (§3.2, §3.5), and both indexes declare `COLLATE NOCASE` on
`locale`, so `en-US` and `en-us` are one locale rather than two keys even if a
writer skips canonicalisation. `und` is stored and compared like any other tag: it
is a locale, not a marker.

**Foreign keys.** `field_key → metadata_fields.field_key` — `RESTRICT`.
`scrape_run_id → scrape_runs.id` — `SET NULL`, nullable (provenance only; see
§19.7).

**Indexes.**

- `(subject_kind, subject_id, field_key)` — the resolver's read path.
- `(scrape_run_id)` — reporting a run's effect.

**Owner.** SQLite. **Lifecycle.** Persistent, **replaceable per provider**.

**Retention.** A provider refresh **replaces** the rows of that provider for the
subject and field it refreshed, and **never touches `manual_overrides`**.

**Invariant protected.** Provider data never destroys manual overrides
(`PRODUCT.md` §15.11); provenance is preserved (`ARCHITECTURE.md` §16.3).

**Decision — a single polymorphic `subject_kind`/`subject_id` pair instead of
separate game and release metadata tables.** The product scrapes games and shows
release-specific values (`PRODUCT.md` §32), the resolver's logic is identical for
both, and one table keeps the resolver, the effective-value view and the search
projection from being written twice. The cost is that the subject reference
cannot be a declarative foreign key. That cost is bounded deliberately:
**only `Game` and `Release` are ever subjects**, and no other kind may be added
without changing this section. §20.3 lists the referential check that replaces
the foreign key, and it is a required test of schema v1.

### 9.4 `manual_overrides`

**Purpose.** A field the user set by hand (`PRODUCT.md` §15.11). This is user
data and the single most protected class A content in the model.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `row_id` | INTEGER | no | Storage primary key; internal only |
| `subject_kind` | TEXT enum | no | `Game`, `Release` |
| `subject_id` | UUIDv7 | no | The subject's identity |
| `field_key` | TEXT | no | One of `metadata_fields.field_key` |
| `locale` | TEXT | yes | The fachliche locale: a BCP-47 tag in canonical case, or `NULL` for **language-neutral** (§9.3) |
| `value_text` | TEXT | yes | The override value |
| `value_number` | INTEGER | yes | The override value when numeric |
| `created_at` | timestamp | no | When the user set it |
| `updated_at` | timestamp | no | When it was last changed |

**Domain identity.** None. An override is user data about a subject, not an entity
of its own.

**Storage primary key.** `(row_id)` — `row_id INTEGER PRIMARY KEY`, a storage
convenience only (§2.4, §3.9).

**Business uniqueness.** The same two exhaustive partial unique indexes as §9.3,
for the same reason: a nullable `locale` in the key would let two identical
language-neutral overrides exist, and the second would be unreachable or would
silently shadow the first.

```sql
UNIQUE (subject_kind, subject_id, field_key)
    WHERE locale IS NULL
UNIQUE (subject_kind, subject_id, field_key, locale)
    WHERE locale IS NOT NULL
```

Both indexes declare `COLLATE NOCASE` on `locale`, so a case variant of one tag
collides with its canonical spelling (§3.5). There is **no** second storage-locale
column and **no** sentinel: `NULL` is language-neutral, and `'und'` is the real
BCP-47 tag it spells (§9.3).

**Foreign keys.** `field_key → metadata_fields.field_key` — `RESTRICT`.

**The subject reference is polymorphic and therefore not a foreign key.** As in
§9.3, `(subject_kind, subject_id)` names either a `games` or a `releases` row and
no single declarative constraint can express that. It is bounded the same way:
**only `Game` and `Release` are ever subjects**, the kind is `CHECK`-constrained to
those two values, and the reference is enforced by the **write path** and verified
by §20.3 test 19 — a required test of schema v1, not an optional one. An override
whose subject no longer exists is reported (invariant 19), never silently deleted.

**Indexes.** `(subject_kind, subject_id)` — the resolver's first lookup.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** Exactly two things remove a `manual_overrides` row:

```text
1. the explicit user reset of that field          (PRODUCT.md §15.11)
2. the full reset                                 (PRODUCT.md §40, §19.3)
```

> **Rule (binding).** **No scan, no library rebuild, no scrape, no provider
> refresh, no migration and no cleanup routine may delete or overwrite a
> `manual_overrides` row.** A library rebuild explicitly **keeps** it (§19.3). A
> provider refresh may rewrite `provider_values`; it may not touch overrides. A
> migration that cannot map an override to the current `metadata_fields` set MUST
> preserve the row and report it, not drop it (invariant 19).

**Invariant protected.** Invalid or stale user overrides are never silently
deleted (invariant 19); manual metadata survives re-scans and re-scrapes.

**Decision — "reset to provider value" is a delete of the override row, not a
copy of the provider value.** Copying the provider value into the override would
make the field permanently manual, so a later provider refresh could never
improve it. Deleting the override restores the documented priority order.

### 9.5 Effective metadata resolution

Derived, in this order per `(subject, field_key, requested locale)`. The queries
use the fachliche `locale` column directly, in the two shapes the partial indexes of
§9.3/§9.4 define — `locale IS NULL` for language-neutral and `locale = <tag>` for a
localised row:

```text
1. manual_overrides   where locale IS NOT NULL AND locale = requested
2. manual_overrides   where locale IS NULL                (language-neutral)
3. provider_values    where locale IS NOT NULL AND locale = requested
4. provider_values    where locale IS NOT NULL AND locale = fallback  (PRODUCT.md §15.12)
5. provider_values    where locale IS NULL
6. provider_values    where locale IS NOT NULL AND locale <> requested
                        AND locale <> fallback,
                        lowest tag                        (deterministic tie-break)
7. absent             → the field is simply not shown
```

**Why the steps use `locale` and not a normalised key.** `NULL` means
language-neutral, which is a fachliche value in its own right, and every step above
names exactly one of the two shapes: an equality on a real tag, or `IS NULL`. The
queries are therefore shaped like the specification, and no step needs a token to
stand in for "no locale" — which is what makes a genuine `und` tag usable: it is
compared like any other tag in steps 1, 3, 4 and 6, and it is **never** treated as
language-neutral.

**Rules.**

- Each step is only consulted when the previous one produced nothing.
- Every step that can return more than one row has a total tie-break: ascending
  `locale`, then ascending `provider_id`, then ascending `value_index`. Reading
  the same data twice MUST produce the same effective value
  (`ARCHITECTURE.md` §36.3). Steps 1–5 are already unique by their partial unique
  index (§3.8), so the tie-break decides step 6.
- **`NULL` is never a wildcard, and `und` is never a marker.** A row with
  `locale IS NULL` is reachable only through the dedicated language-neutral steps
  (2 and 5); a row tagged `und` is reachable only through the ordinary tag
  comparisons, and only when the requested or fallback tag actually is `und`.
  A user who prefers German is therefore never served a language-neutral value
  *because* it matched, and never served an `und`-tagged value at all unless they
  requested it. `tools/check_data_model.py` asserts that the resolver contains no
  sentinel and no comparison that could conflate the two.
- For a **multi-valued** field (`metadata_fields.is_multi_valued = 1`) the
  resolved value is the set of that field's values in ascending `value_index`
  order; the *step* is still chosen by the rules above, and it is the step that
  selects the provider/locale, not the individual value. A field that is not
  multi-valued resolves to the single row with `value_index = 0` (§9.3).
- The effective value is **never written to a column**. There is no
  `effective_title` field, because it would be a second truth that a later
  override, locale change or provider refresh would have to remember to update.
- Search indexes the **effective title** (§17), which is why the resolution order
  above is part of the binding contract and not an implementation detail.

### 9.6 `scrape_runs` and `scrape_run_items`

**Purpose.** Scraping runs are persistent jobs with per-game status, and they can
be paused and resumed (`ARCHITECTURE.md` §16.4, `PRODUCT.md` §15.5).

**`scrape_runs`**

**Domain identity.** `ScrapeRunId`.

**Storage primary key.** `(id)`.

**Business uniqueness.** None.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `id` | UUIDv7 | no | Storage primary key |
| `provider_id` | TEXT | no | The provider used |
| `mode` | TEXT enum | no | `Automatic`, `Interactive` |
| `scope_kind` | TEXT enum | no | `Library`, `System`, `Game` |
| `scope_system_id` | UUIDv7 | yes | Required for `System` |
| `scope_game_id` | UUIDv7 | yes | Required for `Game` |
| `only_missing` | boolean | no | "only games without metadata" |
| `fields_missing` | TEXT | yes | Field selection, stored as the canonical serialized field-key list; `NULL` = all fields |
| `status` | TEXT enum | no | `Queued`, `Running`, `Paused`, `Completed`, `Cancelled`, `Failed` |
| `started_at` | timestamp | no | Run start |
| `finished_at` | timestamp | yes | `NULL` while unfinished |
| `updated_count` | INTEGER | no | Games updated |
| `no_match_count` | INTEGER | no | No match |
| `multiple_match_count` | INTEGER | no | Several matches |
| `error_count` | INTEGER | no | Network/API errors |
| `skipped_count` | INTEGER | no | Skipped |
| `override_protected_count` | INTEGER | no | Fields left alone because an override exists (`PRODUCT.md` §15.6) |

**Foreign keys.** `scope_system_id → systems.system_id` — `RESTRICT`, nullable.
`scope_game_id → games.id` — `RESTRICT`, nullable. `provider_id →
metadata_providers.provider_id` — `RESTRICT`.

**Indexes.** `(status)` — resume after a crash. `(started_at)`.

**`scrape_run_items`**

**Domain identity.** None. An item is the run's record about one game.

**Storage primary key.** `(scrape_run_id, game_id)`. Both components are
`NOT NULL`, so this is a real, full primary key.

**Business uniqueness.** None beyond the storage primary key.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `scrape_run_id` | UUIDv7 | no | The run |
| `game_id` | UUIDv7 | no | The game |
| `status` | TEXT enum | no | `Pending`, `Updated`, `NoMatch`, `MultipleMatches`, `Error`, `Skipped` |
| `candidate_count` | INTEGER | no | How many candidates the provider returned |
| `accepted_provider_item_id` | TEXT | yes | The provider item the run accepted; provenance only |
| `error_detail` | TEXT | yes | Machine-readable error detail; **never** a credential (§9.7) |
| `updated_at` | timestamp | no | When the item reached this status |

**Foreign keys.** `scrape_run_id → scrape_runs.id` — `CASCADE`.
`game_id → games.id` — `RESTRICT`.

**Indexes.** `(scrape_run_id, status)` — the resume set and the run summary.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** Scrape runs and their items are kept as job history (§19.7). They
are the audit trail for "why did this field change", and provider values
reference them.

**Invariant protected.** Scraping is separate from library scanning
(invariant 14); a paused run resumes per game (`PRODUCT.md` §15.5); a scrape
never overwrites a manual override (invariant 19).

**Decision — a run's status is persisted, and `Running` after a crash is
recovered, not trusted.** The row records what the run *was doing*; the recovery
step at startup converts a stale `Running` into `Paused` (resumable) or `Failed`.
Without the persisted status, resuming a multi-hour scrape would be impossible
without re-scraping everything already done.

### 9.7 `metadata_providers`

**Purpose.** The providers BitArchive may use, so that a stored
`provider_id` is a known key rather than a free string.

**Domain identity.** None. A provider id is curated vocabulary, not a minted
identity.

**Storage primary key.** `(provider_id)`.

**Business uniqueness.** None beyond the storage primary key.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `provider_id` | TEXT | no | Storage primary key, e.g. `screenscraper` |
| `display_name` | TEXT | no | User-visible name |
| `is_enabled` | boolean | no | Whether the provider may be used |

**Owner.** Populated from the curated provider list. **Lifecycle.** Persistent.

**Invariant protected.** `ARCHITECTURE.md` §53.15: network activity belongs to
explicit subsystems. A provider that is not in this table cannot be referenced.

> **Rule (binding).** **No credential, API key, token, password or secret of any
> kind is ever stored in the database.** Provider credentials live only in the
> `SecretStore` boundary (`ARCHITECTURE.md` §32, `PRODUCT.md` §15.8). These tables
> may reference a provider by `provider_id`; they MUST NOT carry a secret column,
> a secret value, or a secret passed through in `error_detail`.

---

## 10. Media assets

### 10.1 `media_assets`

**Purpose.** Metadata and a reference for one media file in the managed media
store. **SQLite stores no media bytes** (`ARCHITECTURE.md` §18.1).

**Domain identity.** `MediaAssetId`. A record is user-visible state (which cover
belongs to which game), so it is an entity and not merely a mapping.

**Storage primary key.** `(id)`.

**Business uniqueness.** **None.** See the decision below.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `id` | UUIDv7 | no | Storage primary key |
| `media_type` | TEXT enum | no | `Cover`, `Screenshot`, `SaveStateThumbnail` |
| `locale` | TEXT | yes | Locale the asset belongs to; `NULL` = locale-neutral |
| `provider_id` | TEXT | yes | Where it came from; `NULL` = user-supplied |
| `source_provider_item_id` | TEXT | yes | Provider's own identifier; provenance only |
| `managed_relative_path` | TEXT | yes | Location inside the managed media store; `NULL` = not materialized yet |
| `content_digest` | `BLOB(32)` | yes | SHA-256 of the stored bytes; `NULL` = not hashed yet |
| `byte_size` | INTEGER | yes | Size of the stored file |
| `pixel_width` | INTEGER | yes | Intrinsic width, when known |
| `pixel_height` | INTEGER | yes | Intrinsic height, when known |
| `state` | TEXT enum | no | `Pending`, `Available`, `Missing`, `Unusable` |
| `created_at` | timestamp | no | When the asset record was created |
| `last_verified_at` | timestamp | yes | When the file was last confirmed present |

**Foreign keys.** `provider_id → metadata_providers.provider_id` — `RESTRICT`,
nullable.

**Unique constraints.** None are declared, and the storage primary key is the only
key this table has.

**Decision — no natural-key unique constraint, because its columns would be
nullable.** An earlier revision declared
`UNIQUE (media_type, locale, provider_id, content_digest)`. Three of those four
columns are nullable (`locale`, `provider_id`, `content_digest`), so under SQLite's
`NULL`-are-distinct rule it constrained nothing at all for exactly the common rows
— a user-supplied, locale-neutral asset with no digest yet — and two identical rows
could coexist. It also could not have expressed what it claimed: the columns are
nullable *independently*, so a "key" containing them has no single meaning.

The uniqueness that the product actually needs is **which asset occupies a logical
slot**, and that is already enforced where it belongs, by the primary key of
`media_asset_references`:

```text
media_asset_references PRIMARY KEY (subject_kind, subject_id, logical_key)
    → one asset per slot per subject
```

Beyond that, duplicate *records* are harmless by design: the managed media store is
content-addressed (`ARCHITECTURE.md` §18.1), so several records may point at one
blob, and **`content_digest` is deliberately not unique** — deduplicating the *file*
MUST NOT force deduplicating the *record*, or a release-scoped screenshot and a
game-scoped cover of the same image would collapse into one row that can only
belong to one of them. The blob-level de-duplication happens in the store, not in
the database.

A partial index cannot repair the old constraint either: with three independently
nullable components there is no single predicate that makes them all non-`NULL`,
and inventing one would encode a business rule ("an asset must have a provider and a
digest") that the model does not hold — a user-provided cover has neither.

**Indexes.** `(media_type)`, `(content_digest)`, `(state)` — the garbage
collector walks unreferenced available assets.

**Owner.** SQLite (the record) / managed media store (the bytes).
**Lifecycle.** **Persistent.** This row is a record of user-visible state — which
cover belongs to which game, from which provider, in which language, and the user's
own thumbnail relationship — and §4.1 forbids one structure carrying two lifetimes.

The *bytes* the row points at are a different artifact with a different owner and a
different lifetime, documented in §10.3 and §18.1 as `Rebuildable`: the managed
media store can be re-populated by re-scraping, and the controlled garbage
collector may drop a blob no reference needs any more. The split is therefore by
owner, not by table: `media_assets` is a persistent record that *references* a
rebuildable artifact, which is exactly the shape §4.1 asks for.

**Retention.** The record survives a missing file (`state = Missing`) so that a
later re-scrape or a restored media store can reattach to it. The bytes are
removed only by the controlled garbage-collection step
(`ARCHITECTURE.md` §18.3), which runs **after** the reference rows are gone.

**Invariant protected.** No media bytes in SQLite; media files live in the managed
store, never beside the user's ROMs (`PRODUCT.md` §15.10).

### 10.2 `media_asset_references`

**Purpose.** Which game or release an asset belongs to, and in which logical slot.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `media_asset_id` | UUIDv7 | no | The asset |
| `subject_kind` | TEXT enum | no | `Game`, `Release` |
| `subject_id` | UUIDv7 | no | The subject |
| `logical_key` | TEXT | no | The slot, e.g. `box-front`, `screen-1` |

**Domain identity.** None. A reference is a mapping from a logical slot to an
asset; the slot belongs to the subject, not to an entity of its own.

**Storage primary key.** `(subject_kind, subject_id, logical_key)` — one asset per
slot per subject, which is what makes "replace the cover" a replace rather than an
append. All three components are `NOT NULL`, so this is a real, full primary key.

**Business uniqueness.** None beyond the storage primary key.

**Foreign keys.** `media_asset_id → media_assets.id` — `RESTRICT`: the reference
is protected, but see the deletion rule below.

**The subject reference is polymorphic and therefore not a foreign key.**
`(subject_kind, subject_id)` names either a `games` or a `releases` row — a cover
belongs to a game, a screenshot to a release — so no single declarative constraint
covers it. As in §9.3, the reference is bounded to those two kinds by a `CHECK`, and
it is enforced by the **write path** and verified by §20.3 test 19.

**Indexes.** `(media_asset_id)` — finding unreferenced assets for garbage
collection. `(subject_kind, subject_id)` — loading a game's or release's media.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** A reference for a subject that is reset away is deleted **with**
that subject (full reset, §19.3), and the asset record it pointed at becomes a
garbage-collection candidate but is not removed immediately — the same bytes may
still be referenced elsewhere. A **library rebuild does not remove media
references**: `PRODUCT.md` §15.10 wants the library to stay offline-displayable
after a rebuild, and media is not one of the things `PRODUCT.md` §40's
"Index zurücksetzen"
names (§19.3).

**Invariant protected.** One logical slot has one asset; content-addressed
deduplication does not become content-identity confusion.

### 10.3 The transient image cache

Thumbnails and render caches live under the OS cache path
(`ARCHITECTURE.md` §18.2) and are **not modelled at all**. They are `Temporary`,
have no rows, and may be deleted at any time. A derived thumbnail is never a
`media_assets` row, because a row would make a disposable file look like user
state.

---

## 11. Firmware index

### 11.1 `firmware_entries`

**Purpose.** An index of the user's one central firmware folder
(`ARCHITECTURE.md` §25.1, `PRODUCT.md` §20.1). Firmware remains an external,
user-owned resource; this table is an **index over it**.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `relative_path` | TEXT | no | Path relative to the configured firmware root |
| `filename` | TEXT | no | The file name as observed |
| `byte_size` | INTEGER | yes | Size in bytes; `NULL` = not observed (see decision) |
| `mtime` | timestamp | yes | Modification time; `NULL` = not observed (see decision) |
| `digest` | `BLOB(32)` | yes | **The local SHA-256 of the file's bytes.** `NULL` = not computed because the file could not be read |
| `digest_computed_at` | timestamp | yes | When the digest was computed; `NULL` = never |
| `read_state` | TEXT enum | no | `Readable`, `Unreadable`, `Missing` |
| `first_seen_at` | timestamp | no | When the entry was first indexed |
| `last_seen_at` | timestamp | no | When it was last observed present |

**Domain identity.** None. A firmware entry is an index row over the user's
firmware folder, not a fachliche entity: nothing references it and the whole table
may be deleted and rebuilt.

**Storage primary key.** `(relative_path)`. The relative path is the index's key
**and explicitly not a domain identity**.

**Foreign keys.** None. The table is self-contained and points at nothing.

**Business uniqueness.** None beyond the storage primary key. `(digest)` is
**not** unique — two identical firmware dumps under different names are a
legitimate, reportable state ("content correct, file name wrong",
`PRODUCT.md` §20.4).

**Nullability rule (binding).** `digest` is `NOT NULL` exactly when the entry was
read successfully, and `byte_size` and `mtime` are `NOT NULL` exactly when the
entry was observed at all:

| `read_state` | `byte_size` | `mtime` | `digest` | `digest_computed_at` |
|---|---|---|---|---|
| `Readable` | `NOT NULL` | `NOT NULL` | `NOT NULL` | `NOT NULL` |
| `Unreadable` | nullable | nullable | `NULL` | `NULL` |
| `Missing` | `NULL` | `NULL` | `NULL` | `NULL` |

An unreadable file is a **first-class state**, not a `NULL` digest with no
explanation: `PRODUCT.md` §20.4 and `ARCHITECTURE.md` §25.3 distinguish "missing"
from "present but wrong", and a file BitArchive could not open is neither. It is
indexed so the user can see it, and it is never silently treated as absent.

**Indexes.** `(filename)` — matching a core's `expected_filenames`;
`(digest)` — matching a requirement's accepted hashes; `(read_state)`.

**Owner.** Index over class B. **Lifecycle.** **Rebuildable** — drop and rescan.

**Retention.** Removed when the file disappears (`read_state = Missing` first,
then pruned on the next successful firmware scan), and never the other way round.
BitArchive **never** copies, renames, moves, downloads, repairs or distributes a
firmware file (`PRODUCT.md` §20.2, §20.3, invariant 20).

**Invariant protected.** Firmware requirements belong to the core
(invariant 9); firmware rows are not the firmware (invariant 4).

**Decision — the local digest is computed unconditionally; it does not depend on
#89.** Two completely different things were conflated in an earlier revision of
this document, and separating them is binding:

```text
A. firmware_entries.digest
   "which bytes does the user's file contain?"
   A measurement of the user's own file.
   Owned by the firmware index. Always computable.
   Depends on nothing external.
        ≠
B. accepted / trusted reference hashes
   "which bytes would be correct?"
   A curated statement about the world, used to judge A.
   Belongs to the CORE REQUIREMENTS and the curated catalogue (§12.2),
   never to this index.
   Its provenance is the open decision of #89.
```

Consequences:

- `ARCHITECTURE.md` §25.1 lists `SHA-256` among the indexed fields, and that is
  satisfied: the index stores the digest of every readable file. The index does
  **not** wait for #89, and it does not store an empty column "until a hash source
  exists".
- **A local digest alone never yields a verdict.** Comparing it against a curated
  hash requires B, which #89 owns. Until a reviewed hash source exists, the
  firmware readiness states that need B (`WrongContent`,
  `CorrectContentWrongFilename`, `ARCHITECTURE.md` §25.3) remain underivable, and
  the model MUST NOT claim them.
- **`NULL` digest means "could not read the file"**, never "wrong content" and
  never "not yet hashed because the digest source is unknown".
- **#89 changes nothing in this table.** If #89 decides BitArchive may not ship
  commercial firmware digests, the local digest is still computed and still
  useful (identical-content detection, a user-supplied digest, diagnostics); only
  B is affected, and B lives in the curated catalogue and the core requirements.
- Hashing is not made a startup cost by this: the firmware index is `Rebuildable`
  and is filled by an explicit firmware scan, which may hash lazily and may reuse
  `(byte_size, mtime)` as an invalidation hint exactly like §7.3.

### 11.2 `firmware_index_state`

**Purpose.** When the firmware folder was last successfully scanned, so that
readiness can report "stale index" instead of guessing, and so that an
unscanned folder is distinguishable from an empty one.

**Domain identity.** None. The state row is a singleton observation record.

**Storage primary key.** `(singleton)` — `INTEGER`, `CHECK (singleton = 1)`, one
row only.

**Business uniqueness.** None beyond the storage primary key. The `CHECK` is what
makes the singleton a database fact rather than a convention.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `singleton` | INTEGER | no | Storage primary key, `CHECK (singleton = 1)` — one row only |
| `last_scan_at` | timestamp | yes | `NULL` = never scanned |
| `root_location` | TEXT | yes | The root that was scanned; detects "the user moved the folder" |
| `entry_count` | INTEGER | no | Entries found |
| `scan_status` | TEXT enum | no | `Never`, `Complete`, `Partial`, `Failed` |

**Owner.** SQLite. **Lifecycle.** **Rebuildable.**

**Retention.** Reset on the next firmware scan.

**Invariant protected.** Readiness is derived from the index and the core
requirements (`ARCHITECTURE.md` §25.3), never from the existence of a file the
index has not observed.

---

## 12. Managed components

### 12.1 The three things that are easy to conflate

Runtime and core management involves three genuinely different things. Keeping
them apart is what makes the rest of this section consistent:

```text
A. The curated definition          (code + catalog data, §12.2)
   "which core builds exist, and which bytes are reviewed?"
   Owned by the app. Never in the database.

B. The fachliche version anchor    (persistent, SQLite-owned — §12.4, §12.5)
   "which concrete core build / runtime version does this fact refer to?"
   Referenced by save states, sessions, core-option schemas and overrides.

C. The installed-state index       (rebuildable — §12.6)
   "which artifacts are on disk right now?"
   Owned by the component store. Droppable and rebuildable at any time.
```

> **Rule (binding).** Persistent data MUST reference **B**, never **C**. A
> rebuildable table owns no identity that persistent data depends on.

An earlier revision of this document blurred B and C into one rebuildable table
with a minted `ManagedComponentId`, which meant a full component-index rebuild
could have destroyed the core-version binding of every save state, session and
core-option override. That is fixed by the split: §12.4 and §12.5 are persistent
fachliche anchors, and §12.6 is the droppable index that points at them.

### 12.2 The curated catalogue is not in the database

> **Rule (binding).** The curated system/format/core catalogue — systems,
  manufacturers, release years, accepted formats, detection rules, recommended
  and alternative cores, capabilities, firmware requirements, launch rules,
  component definitions and their pins — is **shipped with the app as reviewed
  data and constants**. It is **not** stored in SQLite.

Consequences:

- There are no `catalog_systems`, `catalog_cores`, `capabilities`,
  `detection_rules`, `core_definitions` or `runtime_definitions` tables.
- `systems`, `cores` and the version anchors of §12.4/§12.5 exist only to **bind a
  stable identity** to a curated key, which a persisted row needs and the
  catalogue cannot supply.
- The database never overrides a curated definition. A user's preference is a
  *scope assignment* (§13, §14, §12.3), never an edited catalogue entry.
- **A curated core is identifiable even when no build of it is installed.** The
  existence of a `cores` row and the existence of an installed artifact are
  independent (see the decision in §12.3).

**Why.** `ARCHITECTURE.md` §15 makes the catalogue versioned, data-driven, CI
validated, and delivered with the app. Mirroring it into SQLite would create a
second copy that a catalogue update could not reliably refresh, and the curation
would stop being reviewable in one place. The separation is also exactly the
"Curated Catalog vs. Online Distribution Manifest" line from the backlog audit,
extended to the database: **curated catalogue, distribution manifest and database
are three separate things.**

### 12.3 `cores` and core selection overrides

**`cores`** binds a `CoreId` to a curated core component so that a scope can
select it.

**Domain identity.** `CoreId` — the fachliche core identity a scope selects.

**Storage primary key.** `(core_id)`.

**Business uniqueness.** `UNIQUE (component_key)` — one fachliche core per curated
component. This is precisely the distinction ADR 0002 §7 draws between `CoreId`
("which core did the user configure for this game") and the distribution slug
("which distributable thing does BitArchive install").

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `core_id` | UUIDv7 | no | Storage primary key — the fachliche core identity |
| `component_key` | TEXT | no | The curated core component, e.g. `mgba` |
| `created_at` | timestamp | no | When the binding was first needed |

**Foreign keys.** **None.** This is deliberate and is the core of Finding 3:

- An earlier revision pointed `(component_key, platform)` at
  `managed_components(component_key, platform)`. That foreign key was **invalid**:
  several builds of one core component can be installed for one platform, so the
  referenced column pair is not unique and SQLite would have rejected the
  constraint (or, worse, silently accepted it had it been declared without the
  matching unique index).
- It was also **fachlich wrong**, and the invalidity merely made the wrongness
  visible. A foreign key into the installed-state index would make the existence
  of a fachliche core assignment depend on an artifact currently being installed.
  A user may configure a core that is not installed yet, and `PRODUCT.md` §17
  wants the assignment remembered so that installing it later resolves. The
  curated catalogue is the authority for "this core exists"; the store is the
  authority for "this build is installed".
- `component_key` is therefore a **curated key**, validated against the curated
  core allowlist on write and on read — exactly like `systems.catalog_key` in
  §6.5. The catalogue is code (§12.2), so a foreign key is not the right
  instrument; §20.3 makes the validation a required test.

**Platform is not part of the core identity.** A curated core is one fachliche
core; the platform decides which *build* is installable and resolvable
(§12.4 decides *which build*, and §12.6 decides *whether it is installed*), not whether the core exists. Modelling `(component_key, platform)` as the
identity would mean a game could resolve to different fachliche cores depending on
which machine the library was built on, and a platform change would invalidate
user assignments. Platform compatibility is answered by the curated matrix and
reported as `UnsupportedSystemOrCore` at readiness time
(ADR 0004 §3), never by the identity.

**Retention.** A `cores` row is never deleted while a `core_selection_overrides`,
`core_versions` or `save_states` row references it. Removing a curated core is a
catalogue change requiring a migration, not a delete.

**`core_selection_overrides`** — the persisted core selection.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `row_id` | INTEGER | no | Storage primary key; internal only |
| `scope_kind` | TEXT enum | no | `System`, `Game`, `Release` |
| `scope_system_id` | UUIDv7 | yes | Required for `System` |
| `scope_game_id` | UUIDv7 | yes | Required for `Game` |
| `scope_release_id` | UUIDv7 | yes | Required for `Release` |
| `core_id` | UUIDv7 | no | The selected core |
| `created_at` | timestamp | no | When it was set |

**Domain identity.** None. A core selection is configuration, not an entity.

**Storage primary key.** `(row_id)` — `row_id INTEGER PRIMARY KEY`, a storage
convenience only (§2.4, §3.9). The previous revision called the three indexes below
the "primary key", which SQLite cannot implement: a primary key cannot be partial,
and the three indexes are mutually exclusive by construction, so no single one of
them covers the table.

**Business uniqueness.** Three partial unique indexes, mirroring the shape used for
settings (§13.1):

- `UNIQUE (scope_system_id) WHERE scope_kind = 'System'`
- `UNIQUE (scope_game_id) WHERE scope_kind = 'Game'`
- `UNIQUE (scope_release_id) WHERE scope_kind = 'Release'`

Each predicate pins exactly the scope column the check constraint below requires
for that kind, so no nullable component is ever part of a key for the rows the index
covers (§3.5). The check is what makes the predicate sound: without it a row could
claim `scope_kind = 'System'` while `scope_system_id` is `NULL`, and the partial key
would then be unenforced for exactly that row.

```sql
CHECK (
    CASE scope_kind
        WHEN 'System'  THEN scope_system_id IS NOT NULL
                            AND scope_game_id IS NULL AND scope_release_id IS NULL
        WHEN 'Game'    THEN scope_game_id IS NOT NULL
                            AND scope_system_id IS NULL AND scope_release_id IS NULL
        WHEN 'Release' THEN scope_release_id IS NOT NULL
                            AND scope_system_id IS NULL AND scope_game_id IS NULL
    END
)
```

**Foreign keys.** `scope_system_id → systems.system_id` — `CASCADE`;
`scope_game_id → games.id` — `CASCADE`; `scope_release_id → releases.id` —
`CASCADE`; `core_id → cores.core_id` — `RESTRICT`.

**Indexes.** `(core_id)` — "which assignments use this core" before removing one.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** Removed by an explicit user reset, or with the scope it belongs
to.

**Invariant protected.** Invariant 21: core resolution follows
`System → Game → optional Release` and there is no global core default. There is
deliberately **no `Global` scope value in the enum**, so a global default is not
merely unimplemented — it is unrepresentable.

> **Rule (binding).** A `Release`-scoped override may only exist while the
> selected core is compatible with the release's system and content
> (`ARCHITECTURE.md` §23.5). Compatibility is checked by the resolver; the model
> records the assignment, and an assignment that is no longer compatible is
> **reported, not deleted** (invariant 19).

**Decision — core selection gets its own table, not a row in the settings
table.** `ARCHITECTURE.md` §23.5 requires core selection to be persisted
separately from RetroArch settings and core options. They also differ in shape:
core selection has a `Release` scope and RetroArch settings deliberately do not
(§13.4).

### 12.4 `core_versions`: the persistent core-build anchor

**Purpose.** The stable fachliche identity of one **concrete core build**, so that
save states, sessions, core-option schemas and core-option overrides have a
persistent anchor that does not depend on the droppable installed-state index.

**Domain identity.** `CoreVersionId`.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `core_version_id` | UUIDv7 | no | Storage primary key |
| `core_id` | UUIDv7 | no | The fachliche core this build belongs to |
| `component_key` | TEXT | no | The curated core component, e.g. `mgba` (redundant with `core_id`, kept so a version is interpretable without a join) |
| `platform` | TEXT | no | The platform the build is for, e.g. `macos-arm64` |
| `build_id` | TEXT | no | The reviewed **build identity** verbatim, e.g. `mgba-0.11-212-7a12d6d` (ADR 0002 §6) |
| `revision` | TEXT | yes | The upstream revision the build id names; `NULL` = not recorded |
| `artifact_digest` | `BLOB(32)` | yes | The pinned reviewed artifact digest, when one is known |
| `first_seen_at` | timestamp | no | When the build was first observed |
| `last_seen_at` | timestamp | no | When it was last resolved or observed |

**Storage primary key.** `(core_version_id)`.

**Business uniqueness.**

- **`UNIQUE (component_key, platform, build_id)`** — one fachliche build identity.
  This is the reviewed identity, so two rows for one build would be a modelling
  error.
- `UNIQUE (core_id, platform, build_id)` — the same rule expressed through the
  fachliche core.

**Foreign keys.** `core_id → cores.core_id` — `RESTRICT`. A version without its
core is meaningless.

**Indexes.** `(core_id, platform)` — "which builds of this core do I know about".
`(component_key, platform)` — resolving a store directory to its anchor.

**Owner.** SQLite. **Lifecycle.** **Persistent** — this is class A data, not an
index.

**Retention.** A row is removed only when nothing references it any more, and even
then only by an explicit cleanup: it is an **anchor**, so deleting it would orphan
save states and sessions. If a build is uninstalled, the row stays — the user's
save states still belong to that build. When a core update removes the last
reference, the row may be pruned.

**Invariant protected.** Save states belong to `Release + Core + Core Version`
(invariant 12); runtime and cores are separately versioned components
(invariant 10).

**Decision — a persistent anchor table, instead of storing the build id as loose
text, and instead of referencing the rebuildable index.** The three options were:

| Option | Why it was rejected or chosen |
|---|---|
| Reference `managed_components` (the rebuildable index) | **Rejected.** A droppable index must not own an identity that persistent data depends on (§12.1). A rebuild could re-mint identities and silently detach every save state and core-option schema from its build. |
| Store `(component_key, platform, build_id, revision)` as loose columns on every referencing table | **Rejected.** Four columns repeated across `save_states`, `sessions`, `core_option_schemas` and `core_option_overrides`, no single place to record the pinned digest, and no way to express "these two save states are for the same build" without comparing four text columns. |
| **A persistent `core_versions` anchor** | **Chosen.** One row per reviewed build, one 16-byte foreign key from each referencing table, and a stable target for the core-update impact report of `ARCHITECTURE.md` §23.2. |

**Why this is not "a second authoritative component registry".** It records no
installed state and no store path, so it cannot contradict the store: it says
*which build a fachliche fact refers to*, never *whether that build is on disk*.
Installed state stays exclusively in §12.6, and the store stays authoritative for
it. ADR 0002 rejected a database **core registry** that would have answered
"which core is active/installed"; that question is still answered only by the
store and by invariant 21's resolver.

**Decision — a row is materialized when a fachliche fact first needs it, not when
a build is installed.** A build that was installed and then removed keeps its
anchor, because save states written with it still exist. A build that is known
from the curated catalogue but never installed has no row until something refers to
it — the catalogue already knows the definition, so the database does not need to
mirror it (§12.2).

### 12.5 `runtime_versions`: the persistent runtime-version anchor

**Purpose.** The same anchor role for the managed **runtime**, so that sessions
can reference the runtime version they ran on without depending on the
installed-state index.

**Domain identity.** `RuntimeVersionId`.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `runtime_version_id` | UUIDv7 | no | Storage primary key |
| `component_key` | TEXT | no | The curated runtime component, e.g. `retroarch` |
| `platform` | TEXT | no | The platform the runtime version is for, e.g. `macos-universal` |
| `version` | TEXT | no | The runtime version verbatim, e.g. `1.22.2` |
| `artifact_digest` | `BLOB(32)` | yes | The pinned reviewed artifact digest, when one is known |
| `first_seen_at` | timestamp | no | When the version was first observed |
| `last_seen_at` | timestamp | no | When it was last resolved or observed |

**Storage primary key.** `(runtime_version_id)`.

**Business uniqueness.** **`UNIQUE (component_key, platform, version)`** — one
fachliche runtime version.

**Foreign keys.** None. A runtime version belongs to a curated component
(§12.2), not to a `cores` row: runtime and cores are separate component classes
(invariant 10, `ARCHITECTURE.md` §23.4).

**Indexes.** `(component_key, platform)`.

**Owner.** SQLite. **Lifecycle.** **Persistent.**

**Retention.** As §12.4: removed only when nothing references it. A session that
ran on a since-removed runtime version keeps its anchor, so the history stays
interpretable.

**Invariant protected.** Runtime and cores are separately versioned components
(invariant 10); app updates never update them implicitly (invariant 11).

**Decision — runtime and core anchors are two tables, not one polymorphic
table.** They share a shape, but a shared table would have to leave `core_id` and
the build-id semantics nullable for one class and meaningless for the other, and it
would re-open the door to a single "component" notion that
`ARCHITECTURE.md` §23.4 and invariant 10 explicitly refuse. Two small tables with
one foreign key each are simpler to constrain correctly.

### 12.6 `managed_components`: the rebuildable installed-state index

**Purpose.** A database-visible **index** of the component store, so that "which
builds are installed", "which runtime version is present" and the core-update
impact report of `ARCHITECTURE.md` §23.2 can be answered without rescanning the
file system on every read (#41). The store remains authoritative.

The store layout this index describes (ADR 0001 §4, ADR 0002 §6):

```text
<component root>/
├── runtime/<platform>/<version>/…                ← installed runtime
├── runtime/<id>/active                           ← the activation record
├── cores/<component-key>/<platform>/<build-id>/… ← installed core
├── staging/install-…/                            ← transient, removed on every path
└── artifacts/<…>                                 ← verified downloads, rebuildable
```

**Domain identity.** None. There is deliberately **no minted UUID** here — an
identity owned by a droppable table must not be referenced by anything (§12.1), so
none is offered. The persistent anchors are `CoreVersionId` and
`RuntimeVersionId`, which belong to tables SQLite owns (§12.4, §12.5).

**Storage primary key.** `(component_class, component_key, platform, version)` —
**the artifact itself**, and the only identity this index has. All four components
are `NOT NULL`, so this is a real, full primary key.

**Business uniqueness.** None beyond the storage primary key.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `component_class` | TEXT enum | no | `Runtime`, `Core` — storage primary key part 1 |
| `component_key` | TEXT | no | The distribution slug, e.g. `retroarch`, `mgba` — part 2 |
| `platform` | TEXT | no | The platform segment of the store layout — part 3 |
| `version` | TEXT | no | The store's version segment **verbatim**: the runtime version or the core build id — part 4 |
| `core_version_id` | UUIDv7 | yes | For a core: the anchor in §12.4 this directory realizes |
| `runtime_version_id` | UUIDv7 | yes | For a runtime: the anchor in §12.5 this directory realizes |
| `relative_path` | TEXT | no | Location inside the component store |
| `library_relative_path` | TEXT | yes | For a core: the loadable library below `relative_path` |
| `executable_relative_path` | TEXT | yes | For a runtime: the executable below `relative_path` |
| `integrity_state` | TEXT enum | no | `Verified`, `Unknown`, `Missing`, `Unusable` |
| `first_seen_at` | timestamp | no | When the index first observed it |
| `last_seen_at` | timestamp | no | When the store was last confirmed to contain it |

**One index row per installed artifact** is what keeps a re-scan idempotent instead
of appending a duplicate row per pass, and the storage primary key above is what
enforces it.

**Foreign keys.**

- `core_version_id → core_versions.core_version_id` — `SET NULL`, nullable. The
  index points **at** the persistent anchor, never the other way round, so
  dropping this table costs the anchor nothing.
- `runtime_version_id → runtime_versions.runtime_version_id` — `SET NULL`,
  nullable.

**Check constraint (single, unambiguous).** The two anchor columns are governed by
one rule, expressed so that #109 can translate it into SQL without interpreting
prose. An earlier revision said "exactly one … which is why the rule is at most
one", which stated two different things in two sentences; this replaces it.

```sql
CHECK (
    -- 1. The two anchors are mutually exclusive. Nothing is ever both.
    NOT (core_version_id IS NOT NULL AND runtime_version_id IS NOT NULL)
    AND
    -- 2. The anchor must match the component class.
    CASE component_class
        WHEN 'Core'    THEN runtime_version_id IS NULL
        WHEN 'Runtime' THEN core_version_id    IS NULL
    END
)
```

The four permitted shapes, and the three that are refused:

| `component_class` | `core_version_id` | `runtime_version_id` | Verdict |
|---|---|---|---|
| `Core` | set | `NULL` | allowed — a core directory bound to its build anchor |
| `Core` | `NULL` | `NULL` | allowed — a core directory not yet bound to an anchor |
| `Runtime` | `NULL` | set | allowed — a runtime directory bound to its version anchor |
| `Runtime` | `NULL` | `NULL` | allowed — a runtime directory not yet bound |
| `Core` | `NULL` | set | **refused** by clause 2 |
| `Runtime` | set | `NULL` | **refused** by clause 2 |
| either | set | set | **refused** by clause 1 |

**Why both anchors may be `NULL`, and why that is not a gap.** The index is
reconciled from the store (§12.7), and binding a directory to a persistent anchor
is a separate step from observing that the directory exists. A directory whose
inspection has not completed — or whose `component_key`/`platform`/`version` does
not (yet) match any anchor, for example a build the curated catalogue does not know
— is indexed with `NULL` anchors and `integrity_state = 'Unknown'`. Such a row is
useful (it answers "what is on disk?") and harmless (nothing references it). Note
that `component_class` itself is `NOT NULL` and constrained to the two values, so
clause 2 always has a branch to take.

**No stricter rule is imposed, deliberately.** Requiring
`integrity_state = 'Verified'` to imply a non-`NULL` anchor would be derivable, but
it would forbid the legitimate intermediate state above and would make
`integrity_state` and the anchors two names for one fact — exactly the kind of
second truth §2.2 removes. `integrity_state` answers "did the bytes check out?";
the anchors answer "which build is this?". They are independent, and a partially
inspected directory can legitimately be `Unknown` or `Unusable` while still being
attributable to a build.

**Indexes.** `(component_class, component_key, platform)` — "which builds of this
core are installed". `(integrity_state)`. `(core_version_id)`,
`(runtime_version_id)` — resolving an anchor to its installed directory.

**Owner.** Index over class B. **Lifecycle.** **Rebuildable** — the whole table
may be dropped and rebuilt from the store at any time.

**Retention.** An entry whose directory no longer exists is marked `Missing` and
then removed on reconciliation (§12.7). A full rebuild may simply drop every row.

**Invariant protected.** Runtime and cores are separately versioned components
(invariant 10); app updates never update them implicitly (invariant 11).

**Decision — a persisted component index is not a contradiction of
ADR 0001/0002, because it is `Rebuildable` and off the critical path.** ADR 0001
rejected "introduce SQLite for the component registry **now**" and ADR 0002
rejected a database core registry because "the installed set is derivable from the
store layout". Both stay true: the store is authoritative, the installed set is
derived by listing directories, and this table is a *cache* that may be dropped at
any time. It exists because two questions genuinely need a join that the file
system cannot answer — the core-update impact report of `ARCHITECTURE.md` §23.2
("how many save states become a version mismatch") and orphan detection across the
session and save-state history. A caller that finds this table empty must fall
back to the store, never to an error.

**Decision — for a core, the version segment is the build id.** ADR 0002
established that the observed version string is **not monotonic** (a rebuild
produced a lower number) and that "the commit count inside it is not identity: the
revision is". `version` therefore holds the store's directory name verbatim — the
only thing that can resolve a path — and the reviewed `build_id` and `revision`
live on the §12.4 anchor, so "is this the build I reviewed?" is answerable without
parsing a directory name. This is also why the unique key here is the store
segment and not the build identity: an index row describes a *directory*, while
§12.4 describes a *build*.

**Decision — no `is_active` column, and the runtime's active version is not
stored here.** ADR 0001 makes the active runtime a **file-system registry**
(`components/runtime/<id>/active`) that may only point at an installed version;
ADR 0002 states there is no activation record, no "current core", and no default
core, because core selection is answered by `System → Game → optional Release`
(invariant 21). A database `is_active` column would be a second, contradictory
answer to the runtime question and would imply an active-core concept that must
not exist. The index reports what is *on disk*; which runtime is *active* comes
from the store's own record. (#41 asks for an active marker for the runtime; the
store record is that marker, and the read model may surface it by reading the
store rather than by duplicating it.)

**Decision — no shared "component" table for runtime and core.** They share
download/verify/stage/install/activate/rollback infrastructure but are different
fachliche component types (`ARCHITECTURE.md` §23.4, invariant 10). A single index
table with a class discriminator is used because it genuinely stores the same
artifact observations for both; the fachliche separation is preserved by the
`component_class` key part, by the two separate anchor tables, and by the fact
that only runtimes have an activation record.

### 12.7 Reconciliation with the component store

> **Rule (binding).** For every managed component the **component store is
> authoritative**. The installed-state index of §12.6 is reconciled to it, never
> the reverse.

```text
store directory present, index row absent   → add index row (Verified after check)
index row present, store directory absent   → mark Missing, then remove on rebuild
store directory present, index row stale    → update from the store
store directory binds to a known build      → set core_version_id / runtime_version_id
```

A component directory is **never** deleted because a database row says so, and
BitArchive never deletes, rewrites or repoints an installed component to satisfy
the index. The only deletions in the component store are the explicit
retention/rollback rules of `ARCHITECTURE.md` §23.3 (keep active and previous),
which belong to the component subsystem and not to the database.

**What a rebuild does and does not touch.** A rebuild drops and re-derives the
rows of §12.6 only. It never writes to `core_versions`, `runtime_versions`,
`cores`, `save_states`, `sessions`, `core_option_schemas` or
`core_option_overrides`. Because §12.6 owns no identity that any of those tables
reference, this is a total statement: after a rebuild, every persistent reference
still resolves and the installed view is reconstructed.

**`component_index_state`** records `last_scan_at` and `last_scan_status` so that
"the index has never been built" is distinguishable from "the store is actually
empty".

**Domain identity.** None. The state row is a singleton observation record.

**Storage primary key.** `(singleton)` — `INTEGER`, `CHECK (singleton = 1)`, one
row only.

**Business uniqueness.** None beyond the storage primary key.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `singleton` | INTEGER | no | Storage primary key, `CHECK (singleton = 1)` — one row only |
| `last_scan_at` | timestamp | yes | `NULL` = never scanned |
| `last_scan_status` | TEXT enum | no | `Never`, `Complete`, `Partial`, `Failed` |
| `indexed_entry_count` | INTEGER | no | Index rows written by the last scan |

**Owner.** Index over class B. **Lifecycle.** **Rebuildable.**

**Retention.** Reset by the next reconciliation.

---

## 13. RetroArch settings

### 13.1 `retroarch_setting_overrides`

**Purpose.** The structured, typed RetroArch settings BitArchive manages, with
the inheritance hierarchy `Global → System → Game`
(`ARCHITECTURE.md` §26.1, `PRODUCT.md` §21.1). **No `.cfg` file is the source of
truth** (`ARCHITECTURE.md` §53.7).

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `row_id` | INTEGER | no | Storage primary key; internal only |
| `scope_kind` | TEXT enum | no | `Global`, `System`, `Game` |
| `scope_system_id` | UUIDv7 | yes | Required for `System`, otherwise `NULL` |
| `scope_game_id` | UUIDv7 | yes | Required for `Game`, otherwise `NULL` |
| `setting_key` | TEXT | no | The technical key, e.g. `video_vsync` |
| `value_type` | TEXT enum | no | `Bool`, `Integer`, `Float`, `Choice`, `Text` |
| `value_bool` | INTEGER | yes | Set only for `Bool`; `0`/`1` |
| `value_integer` | INTEGER | yes | Set only for `Integer` |
| `value_float` | REAL | yes | Set only for `Float` |
| `value_text` | TEXT | yes | Set for `Choice` and `Text` |
| `created_at` | timestamp | no | When the override was set |
| `updated_at` | timestamp | no | When it was last changed |

**Domain identity.** None. A setting override is configuration, not an entity.

**Storage primary key.** `(row_id)` — `row_id INTEGER PRIMARY KEY`, a storage
convenience only (§2.4, §3.9). The three indexes below are **partial**, and a
partial index can never be a primary key (§3.9 rule 2); the previous revision
labelled them "Primary key", which #109 could not have implemented.

**Business uniqueness.** Three partial unique indexes:

- `UNIQUE (setting_key) WHERE scope_kind = 'Global'`
- `UNIQUE (scope_system_id, setting_key) WHERE scope_kind = 'System'`
- `UNIQUE (scope_game_id, setting_key) WHERE scope_kind = 'Game'`

Each predicate names exactly the scope column its kind requires, so the key has no
nullable component for the rows it covers (§3.5). The check constraint below is what
makes that true rather than assumed:

```sql
CHECK (
    CASE scope_kind
        WHEN 'Global' THEN scope_system_id IS NULL AND scope_game_id IS NULL
        WHEN 'System' THEN scope_system_id IS NOT NULL AND scope_game_id IS NULL
        WHEN 'Game'   THEN scope_game_id IS NOT NULL AND scope_system_id IS NULL
    END
)
```

**Foreign keys.** `scope_system_id → systems.system_id` — `CASCADE`;
`scope_game_id → games.id` — `CASCADE`.

**Indexes.** `(scope_kind, scope_system_id)`, `(scope_kind, scope_game_id)`.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** Removed when the user resets a value to "inherited", or with the
scope it belongs to. **Not** removed because it became invalid.

**Invariant protected.** `Global → System → Game` inheritance with explicit
overrides only (`ARCHITECTURE.md` §26.1, §53.7).

### 13.2 Typed values

The `value_type` discriminator plus one populated value column is what keeps a
boolean from being stored three ways (`true`, `yes`, `1`) and keeps a generator
from having to guess how to render a value. `bitarchive-domain::config` already
models exactly this set (`Bool`, `Integer`, `Float`, `Choice`, `Text`) and the
scope precedence as a property of the scope rather than of the caller's order; the
column set above is its storage form.

**Rule (binding).** Exactly one value column is populated, matching
`value_type`. The schema MUST enforce this, not merely document it.

### 13.3 No definition table

`ARCHITECTURE.md` §26.2 mentions a `SettingDefinition` describing type, default
and supported scopes. **That registry is curated code, not database content**
(§12.2), so there is no `setting_definitions` table. `setting_key` is validated
against the curated registry when a value is written and when it is read.

**Decision — validity is derived at load time and not stored.** A stored
`is_valid` column would freeze the verdict of whatever registry version wrote the
row, so a RetroArch or BitArchive update that *fixes* a definition could never
make an existing override usable again without a data migration, and one that
*breaks* a definition would leave a stale "valid" flag behind. Validation is a
pure function of (stored value, current definition, current core version), which
is exactly the shape §2.2 says to derive.

### 13.4 Release scope

> **Rule (binding).** There is **no `Release` scope for RetroArch settings and no
> `Release` scope for core options.** The `scope_kind` enums of §13.1 and §14.1
> MUST NOT contain a `Release` variant.

`ARCHITECTURE.md` §20.4 states this explicitly: a release-specific core override
is part of core resolution only, and RetroArch settings and core options stay on
their own `Global/System/Game` and `Core Defaults/System/Game` scopes.

### 13.5 Generated configuration

The `.cfg` files under `sessions/<session-id>/` are **session artifacts**
(`ARCHITECTURE.md` §28) and are not modelled as data at all. They are generated
from the effective configuration, and the effective configuration is derived from
this table. `PRODUCT.md` §21.2 excludes a generic `.cfg` editor, so a `.cfg` is
never an input.

---

## 14. Core options

### 14.1 `core_option_overrides`

**Purpose.** Core-specific options, a **separate configuration domain** from
RetroArch settings (`ARCHITECTURE.md` §27, `PRODUCT.md` §21.4), with the
hierarchy `Core Defaults → System → Game`.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `row_id` | INTEGER | no | Storage primary key; internal only |
| `core_version_id` | UUIDv7 | no | The concrete core build this override belongs to (§12.4) |
| `scope_kind` | TEXT enum | no | `CoreDefaults`, `System`, `Game` |
| `scope_system_id` | UUIDv7 | yes | Required for `System`, otherwise `NULL` |
| `scope_game_id` | UUIDv7 | yes | Required for `Game`, otherwise `NULL` |
| `option_key` | TEXT | no | The core's technical option key |
| `value_text` | TEXT | no | The stored value, as text |
| `created_at` | timestamp | no | When the override was set |
| `updated_at` | timestamp | no | When it was last changed |

**Domain identity.** None. A core option override is configuration, not an entity.

**Storage primary key.** `(row_id)` — `row_id INTEGER PRIMARY KEY`, a storage
convenience only (§2.4, §3.9).

**Business uniqueness.** Three partial unique indexes, one per scope:

- `UNIQUE (core_version_id, option_key) WHERE scope_kind = 'CoreDefaults'`
- `UNIQUE (core_version_id, scope_system_id, option_key) WHERE scope_kind = 'System'`
- `UNIQUE (core_version_id, scope_game_id, option_key) WHERE scope_kind = 'Game'`

The same scope check as §13.1, with `CoreDefaults` in place of `Global`:

```sql
CHECK (
    CASE scope_kind
        WHEN 'CoreDefaults' THEN scope_system_id IS NULL AND scope_game_id IS NULL
        WHEN 'System'       THEN scope_system_id IS NOT NULL AND scope_game_id IS NULL
        WHEN 'Game'         THEN scope_game_id IS NOT NULL AND scope_system_id IS NULL
    END
)
```

**Foreign keys.** `core_version_id → core_versions.core_version_id` —
`RESTRICT`; `scope_system_id → systems.system_id` — `CASCADE`;
`scope_game_id → games.id` — `CASCADE`.

**Indexes.** `(core_version_id)`, `(scope_kind, scope_game_id)`.

**Why the anchor is `core_versions`, not the installed-state index.** A
`CoreDefaults` override is the bottom of the hierarchy and outlives any particular
installation: the user set it for a build, and it must stay readable when that
build is temporarily uninstalled. Referencing §12.6 would make the override depend
on an artifact being on disk and would let a component-index rebuild orphan it.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** Removed by an explicit user reset or with its scope. **Never**
removed because a core update made it invalid.

**Invariant protected.** Core options are not RetroArch settings
(`ARCHITECTURE.md` §27); invalid stored overrides are preserved (invariant 19).

**Decision — values are stored as text, with the schema supplying the typing.**
A core option's value domain is defined by the *core*: its type and its allowed
values come from the core's schema, which is only known after introspection and
is not curated by BitArchive. Storing a guessed `value_type` would freeze a
guess. Text plus the schema's own validation is the honest form, and it is the
one shape that survives a core version whose option changed type.

### 14.2 `core_option_schemas` and `core_option_definitions`

**Purpose.** The options a concrete core version actually offers
(`ARCHITECTURE.md` §27.1). Schemas belong to a **concrete core version**.

**`core_option_schemas`**

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `core_version_id` | UUIDv7 | no | Storage primary key — one schema per core version |
| `introspected_at` | timestamp | no | When introspection ran |
| `definition_count` | INTEGER | no | Options found |
| `introspection_status` | TEXT enum | no | `Complete`, `Partial`, `Failed` |

**Domain identity.** None. A schema is an introspection result about a core build.

**Storage primary key.** `(core_version_id)`. `NOT NULL`, so this is a real, full
primary key.

**Business uniqueness.** None beyond the storage primary key.

**`core_option_definitions`**

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `core_version_id` | UUIDv7 | no | The core version |
| `option_key` | TEXT | no | The core's technical option key |
| `value_type` | TEXT enum | no | `Bool`, `Integer`, `Float`, `Choice`, `Text` |
| `default_value` | TEXT | yes | The core's own default |
| `allowed_values` | TEXT | yes | Canonical serialized list of allowed values for a `Choice`; `NULL` otherwise |
| `display_name` | TEXT | yes | The core's label, if it provides one |
| `definition_index` | INTEGER | no | The option's order in the core's own list |

**Domain identity.** None. A definition is one option the schema declares.

**Storage primary key.** `(core_version_id, option_key)`. Both components are
`NOT NULL`, so this is a real, full primary key.

**Business uniqueness.** None beyond the storage primary key.

**Foreign keys.** `core_version_id → core_versions.core_version_id` —
`CASCADE`. A schema without its core version is meaningless, and the cascade is the
single mechanism by which a schema disappears: only when its anchor goes. Since the
anchor is persistent and is removed only when nothing references it (§12.4), a
schema cannot be lost merely because a build was uninstalled.

**Note on `core_version_id`.** It references one `core_versions` row (§12.4),
whose `build_id` column holds the reviewed build identity for a core (ADR 0002
§6). An option schema is therefore bound to a **build**, not to a core and not to
a floating version string, which is what `ARCHITECTURE.md` §27.1 requires. Bound
to a build also means bound to something the store cannot change under it: the
anchor is persistent, while the installed-state index that points at it is not.

**Owner.** SQLite. **Lifecycle.** **Persistent** — one classification, not two.
The stored schema is a *historical introspection result*, not a cache.

**Decision — the stored schema is persistent history, not a rebuildable index.**
An earlier revision called it "Persistent, and rebuildable by re-running
introspection", which broke §4.1's rule that a structure carries exactly one
lifetime and made a promise the model cannot keep: **an old schema usually cannot
be re-introspected at all**, because the core build it describes may no longer be
installed. Once a build is gone, the only surviving evidence of what options it
offered is the row itself.

The core binary remains the **technical authority** for what was discovered — it
is what introspection read. But what introspection *wrote down* is thereafter
BitArchive's persistent record of that discovery, and it is treated like any other
persistent user-facing fact. The distinction matters because three consumers
depend on it long after the build is uninstalled:

| Consumer | Why it needs the schema to survive |
|---|---|
| Stored overrides (§14.1, §14.5) | An override for an old build must stay interpretable and must be reported as valid or invalid, never as "unknown because we forgot" (invariant 19). |
| Diagnosis | The user asks why an option changed or disappeared after a core update. |
| Core-update impact (`ARCHITECTURE.md` §23.2) | The pre-update comparison needs the *old* schema to say which overrides would become invalid. |

**Re-introspection of the same build.** If the same concrete build is
introspected again — the store was restored, the row was lost, a first attempt was
`Partial` or `Failed` and is retried — the stored result **may be updated or
replaced** when introspection produces a new consistent result. That is a refresh
of a persistent row, exactly like a provider refresh replacing `provider_values`
(§9.3), and it is why re-introspection remains possible without the table being
"rebuildable": the *table* has one lifetime, and individual rows may be refreshed.

**Retention.**

- Schemas are **kept per core version**, not dropped when a core update arrives,
  so overrides written against an older build stay interpretable.
- **Uninstalling a build does NOT delete its schema.** The component store is not
  the owner of this row, and §12.7's reconciliation never touches it.
- A schema row is removed only when its `core_versions` anchor can itself be
  removed — that is, when no override, no diagnostic reference and no update-impact
  comparison needs it any more (see the anchor retention rule in §12.4). The
  `CASCADE` below is the mechanism, and it fires only on that anchor's removal.
- A **failed** introspection does not overwrite a previously good schema with an
  empty one: `introspection_status` records the failure and the last good
  definitions stay, so a transient failure cannot degrade a user's stored overrides
  into "invalid".

**Not changed by this decision.** The component store does not become a database
authority: nothing here records installed state, and §12.7 still reconciles the
index from the store. Schemas are **not** re-attached to `managed_components`, and
no stored override is ever deleted because its schema changed (invariant 19).

**Invariant protected.** Core options belong to a concrete core version
(`ARCHITECTURE.md` §27.1); introspection happens after
installation/activation, not when the settings screen opens
(`ARCHITECTURE.md` §27.2).

### 14.3 Scope identity

An option override is keyed by `(core_version_id, scope, option_key)` — **not**
by core identity. That is the whole point of `ARCHITECTURE.md` §27.1: an option
set is a property of a *version*, because a core update may rename, retype,
re-default or remove options.

`CoreDefaults` is the bottom of the hierarchy and therefore needs no
`scope_system_id`/`scope_game_id`: it is already version-scoped.

### 14.4 Inheritance and effective value

```text
Game  >  System  >  CoreDefaults
```

The most specific scope that offers a **defined** option for this core version
wins. An override for an option the core version does not define is simply not
applied.

### 14.5 Invalid overrides

> **Rule (binding).** A stored override that no longer validates against the
> current schema for its core version is:
>
> 1. **kept** — not deleted, not migrated away, not nulled;
> 2. **not applied** — it does not become an effective value;
> 3. **reported** — surfaced as a warning the user can see and resolve
>    (`ARCHITECTURE.md` §27.3, invariant 19).

Because validity is derived (§13.3), this needs no status column: both sides of
the comparison are available at read time. A later core version that reintroduces
a compatible option therefore heals the override automatically, which a stored
`is_invalid` flag could not do.

An invalid stored override is **not** a content problem. A launch that is blocked
because of one is reported as `InvalidConfiguration` and never as
`InvalidContent` (`ARCHITECTURE.md` §21).

---

## 15. Save states

### 15.1 `save_states`

**Purpose.** BitArchive's management and metadata layer over the save-state files
**RetroArch** owns (`ARCHITECTURE.md` §29, `PRODUCT.md` §23). BitArchive indexes
them; it does not own them.

**Domain identity.** `SaveStateId`.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `id` | UUIDv7 | no | Storage primary key |
| `release_id` | UUIDv7 | no | The release this state belongs to |
| `core_id` | UUIDv7 | no | The core that wrote it |
| `core_version_id` | UUIDv7 | yes | The concrete core version, when it is known |
| `core_version_label` | TEXT | yes | The version as observed in the file name/label, when the version cannot be bound to an installed build |
| `slot` | INTEGER | yes | The RetroArch **state slot** as encoded in the file name; `-1` is RetroArch's own `Auto` slot. `NULL` = the file name does not encode a slot. **Not part of any key** — see below |
| `created_at` | timestamp | no | The state's timestamp as observed |
| `file_relative_path` | TEXT | no | The state file's location, relative to the RetroArch save-state root |
| `file_size` | INTEGER | yes | Observed size |
| `file_mtime` | timestamp | yes | Observed modification time |
| `display_name` | TEXT | yes | The user's name for this state. **BitArchive-only** |
| `thumbnail_media_asset_id` | UUIDv7 | yes | The optional thumbnail; `NULL` = none |
| `state` | TEXT enum | no | `Present`, `Missing`, `Unreadable` |
| `last_seen_at` | timestamp | no | When the file was last confirmed present |

**Foreign keys.**

- `release_id → releases.id` — `RESTRICT`.
- `core_id → cores.core_id` — `RESTRICT`.
- `core_version_id → core_versions.core_version_id` — `RESTRICT`, nullable.
- `thumbnail_media_asset_id → media_assets.id` — `SET NULL`, nullable.

A save state deliberately carries **no** runtime reference: a save-state file is
written by a core, never by the runtime process, so a runtime version is not part
of its identity or its compatibility (`ARCHITECTURE.md` §29.1: `Release + Core +
Core-Version`).

**Storage primary key.** `(id)`.

**Business uniqueness.** Exactly one rule:

- `UNIQUE (file_relative_path)` — **one indexed row per physical state file.** This
  is what makes discovery idempotent: the same file observed twice is refused
  instead of stacked, whatever else about the row changed.

**Decision — three different things, kept apart.** The previous revision merged
them into one key list, and that is how a sentinel got in. They answer different
questions:

| Question | Answered by | Is it a key? |
|---|---|---|
| *Which state does RetroArch's UI mean?* (technical addressing) | `slot` | **no** — an attribute of the file, not an identity |
| *May this state be loaded with the active setup?* (compatibility) | `release_id`, `core_id`, `core_version_id` / `core_version_label` | **no** — ownership dimensions, derived into a verdict by §15.3 |
| *Is this the same file I indexed before?* (physical identity) | `file_relative_path` | **yes** — the one unique key |

**Decision — `slot` is `INTEGER` and nullable, and it is not a key.** RetroArch
addresses save states by an integer slot and by nothing else. Verified against the
RetroArch source:

- the config key is `state_slot` (integer, default `0`) and the menu, hotkeys and
  CLI expose the range `-1 … 999`, where **`-1` renders as `Auto`**
  (`libretro/RetroArch` `v1.22.2`: `menu/menu_setting.c` range registration,
  `menu/menu_setting.c` `-1 → "Auto"`, `retroarch.c` `--entryslot` validation);
- the file name is derived from the slot, with slot `0` producing no suffix
  (`runloop.c`, `runloop_get_savestate_path`).
- there is **no** `savestate_max_slots` setting; `savestate_max_keep` bounds
  auto-increment garbage collection, not addressing.

*Sources* (checked at tag `v1.22.2`, cross-checked on `master`):
`runloop.c` — `runloop_get_savestate_path` and `runloop_path_set_names`;
`configuration.c` — the `state_slot` registration and the integer-settings loader;
`menu/menu_setting.c` — the `-1 … 999` range and the `-1 → "Auto"` label;
`retroarch.c` — `--entryslot` validation; `command.c` — the `.auto` name and the
auto-index scanner's `state*` extension filter; `tasks/task_screenshot.c` and
`menu/drivers/xmb.c` — the thumbnail path. On `master` the config rows moved into
`settings/settings_def_*.h` and the thumbnail path into
`gfx/gfx_thumbnail.c`, with the same semantics.

The stored file naming is therefore:

```text
slot 0        <content>.state
slot N > 0    <content>.stateN
slot -1       <content>.state.auto        (RetroArch's "Auto" state)
thumbnail     <the state file's path>.png (a separate file, §15.4)
```

`slot` is `NULL` for a file whose name does not encode a slot. That is a
**fachliche** value — "this file is not slot-addressed, or its slot could not be
determined" — and not a storage sentinel, which is why no token stands in for it
and why `NULL` needs no normalisation: the column is in no key, so SQLite's
`NULL`-are-distinct behaviour cannot weaken anything (§3.5, §3.9).

**Why the empty-string sentinel is gone.** The previous revision made `slot` a
`NOT NULL` `TEXT` column with `''` meaning "not slot-derived", justified by the
claim that "RetroArch slots are digits **or names**". That claim is not true, and
the sentinel existed only to keep a key enforced. No RetroArch setting, UI element,
hotkey, network command or CLI option addresses a save state by a name; the
save/load API is path-based internally, and the only name-like artifacts are
third-party frontend conventions (`game.state.1` from MinUI/NextUI), which are not
RetroArch behaviour. Since no user-facing name exists, there is nothing to record,
and `state_name` is removed as well: `display_name` below is *BitArchive's* name for
the state, invented by the user and never written back to the file
(`ARCHITECTURE.md` §29.5).

**Why `slot` is not unique.** One physical state file per slot is what RetroArch's
own naming implies, but the model does not enforce it as a key, for two reasons:

- The range is a **UI/CLI contract, not a storage invariant.** A hand-edited
  `state_slot = 5000` is loaded without clamping (RetroArch's integer-settings
  loader assigns the value as read), so a file name may legitimately encode a slot
  outside `-1 … 999`.
- The physical identity of a state is its **file**, and a state file can be
  *moved*: after a move the new path is a new row while the old row stays `Missing`
  (§19.4's rule, applied to states by §15.5). Both rows would carry the same slot,
  and a `UNIQUE (release, core, version, slot)` would then refuse to index the file
  that actually exists — turning the "file system wins" rule into a constraint
  violation. `file_relative_path` is the honest key for "did I already index this
  file", and slot duplication across two paths is a truthful observation rather
  than a defect.

**Decision — `core_version_label` stays nullable and is not a key component.**
It records the version text observed for a state that cannot be bound to an
installed build (§15.2). Nothing needs to be unique on it: a second file with the
same observed label is a second file, and `file_relative_path` already separates
them.

**Indexes.**

- `(release_id, created_at DESC)` — the timeline of a release, which is the
  product's primary presentation (`PRODUCT.md` §23.5).
- `(slot)` — "which states occupy which technical slot", the question the
  Quick-Menu-style listing asks. Non-unique, per the decision above.
- `(core_id, core_version_id)` — version-mismatch counting for the core-update
  impact report (`ARCHITECTURE.md` §23.2).
- `(state)` — the "state file vanished" maintenance query.

**Owner.** SQLite (metadata) / RetroArch and the file system (the files).
**Lifecycle.** Persistent metadata over class B files.

**Retention.** A state row is removed when the user confirms deletion, **and only
after** the file operations succeeded (§19.5). A missing file sets
`state = Missing`; it does not delete the row, because the user's state may be
temporarily unavailable (an unmounted volume) and a silently forgotten save state
is the worst possible outcome for a user.

**Invariant protected.** Save states belong to `Release + Core + Core Version`
(invariant 12). Normal RetroArch saves are outside the model (invariant 13).

> **Rule (binding).** This model covers save states only. SRAM, memory cards and
> every other regular RetroArch save file MUST NOT be modelled, indexed, listed or
> deleted by BitArchive (`PRODUCT.md` §23).

### 15.2 Which core version a state belongs to

A save-state file does not carry a machine-readable core version. The model
therefore allows both:

- `core_version_id` — set when the state can be bound to a known core build
  (§12.4), resolved from RetroArch's own naming and the curated build identities.
  This is a **persistent anchor**, so the binding survives the build being
  uninstalled and survives a component-index rebuild;
- `core_version_label` — the version text as observed, used when no binding is
  possible.

**Decision — an unbound version label is stored as text rather than forcing a
binding.** Inventing a `core_version_id` would attribute a state to a version
that did not write it, and the compatibility verdict would then be confidently
wrong. Keeping the observed label lets the UI say "version unknown, treat as
potentially incompatible", which is the conservative answer the product asks for
(`PRODUCT.md` §23.4).

### 15.3 Compatibility is derived, never stored

```text
core_id equals the active core AND core_version_id equals the active version
        → Compatible
core_id equals the active core AND version differs (or is unknown)
        → VersionMismatch
core_id differs
        → IncompatibleCore
```

> **Rule (binding).** Compatibility MUST be computed from `core_id` and the core
> version. There is **no** stored compatibility column, and no stored
> "compatible" list.

The "active version" this comparison uses is the version of the **installed and
selected** core, read from the component store via §12.6; the save state's side of
the comparison is its persistent `core_version_id` anchor. The two sides therefore
have different owners on purpose: the state's history is SQLite's, the installed
state is the store's.

A stored verdict would become a second truth the moment the active core or its
version changed, and it would be unverifiable: nothing could tell whether it was
still true. Deriving it makes `Continue` — "the newest `Compatible` state for the
active release, core and core version" — a query instead of a synchronization
problem.

`Continue` therefore resolves to the newest `Present` row with `state = 'Present'`
and a `Compatible` verdict, ordered by `created_at DESC` with `id` as the
tie-breaker, so that two rows with the same timestamp cannot make the answer depend
on row order (`ARCHITECTURE.md` §36.3). A `Missing` row is never a `Continue`
candidate even if it is the newest, because its file is not there to load.

### 15.4 Thumbnails and display names

- The thumbnail is a `SaveStateThumbnail` media asset (§10), referenced by
  `thumbnail_media_asset_id`. Thumbnails remain **technically separate files**
  (`ARCHITECTURE.md` §29.5): RetroArch writes them as `<state file>.png` next to
  the state (`tasks/task_screenshot.c`, `menu/drivers/xmb.c`), so the thumbnail of
  `<content>.state1` is `<content>.state1.png`. BitArchive indexes the pair but
  never renames either file.
- `display_name` lives **only** in BitArchive. Physical state files are never
  renamed to carry it (`PRODUCT.md` §23.2, `ARCHITECTURE.md` §29.5), and there is
  **no** `state_name` column, because RetroArch has no named save states to record:
  the only addressing dimension it has is the numeric slot above.

### 15.5 Discovery

Discovery runs at startup, after a session ends, and on explicit refresh
(`ARCHITECTURE.md` §29.2). It reconciles the index with the file system with the
same authority rule as §12.7: the **file system wins**. No permanent file watcher
exists in the MVP, so the index is only ever as fresh as the last discovery pass.

What discovery writes is exactly `file_relative_path`, the observed file metadata,
the `slot` parsed from the name, and the compatibility dimensions it can bind
(`release_id`, `core_id`, `core_version_id`/`core_version_label`). Two consequences
follow from the key choice of §15.1 and are stated rather than implied:

- **The same file is never indexed twice.** `UNIQUE (file_relative_path)` refuses
  the second row, so a repeated discovery pass is idempotent.
- **A moved state file is a new row, and the old row stays.** The old
  `file_relative_path` is not present any more, so its row becomes `Missing`
  (§19.4's rule, unchanged for states) while the file at its new path is indexed as
  a new `SaveStateId`. Both rows describe the same RetroArch slot, and that is
  honest: one file was there and is not, another is there now. Nothing is deleted,
  because a silently forgotten save state is the worst outcome for a user (§15.1
  retention), and the user's `display_name` and thumbnail relationship stay on the
  row they were set on.

---

## 16. Sessions and statistics

### 16.1 `sessions`

**Purpose.** One emulation session — the record that makes exclusivity,
recovery, playtime and statistics possible (`ARCHITECTURE.md` §22).

**Domain identity.** `SessionId`.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `id` | UUIDv7 | no | Storage primary key |
| `game_id` | UUIDv7 | no | The launched game |
| `release_id` | UUIDv7 | yes | The release that was launched |
| `content_id` | UUIDv7 | yes | The content that was launched |
| `content_location_kind` | TEXT enum | yes | `File` or `ArchiveEntry`, mirroring the resolved launch content |
| `core_id` | UUIDv7 | yes | The core that was used |
| `core_version_id` | UUIDv7 | yes | The concrete core version |
| `core_version_label` | TEXT | yes | The observed version, when it could not be bound (§15.2) |
| `runtime_version_id` | UUIDv7 | yes | The managed runtime version this session ran on (§12.5) |
| `runtime_version` | TEXT | yes | The runtime version verbatim, as a **deliberate, documented duplicate** of the anchor (see the decision below) |
| `started_at` | timestamp | no | Session start |
| `ended_at` | timestamp | yes | `NULL` while active |
| `state` | TEXT enum | no | `Active`, `Finalized`, `Recovered`, `Abandoned` |
| `duration_ms` | INTEGER | yes | Total session duration; `NULL` while active |
| `active_duration_ms` | INTEGER | yes | Foreground/observed play time when it differs from wall clock |
| `process_id` | INTEGER | yes | PID of the emulator process |
| `process_started_at` | timestamp | yes | Process start time, for identity |
| `process_executable` | TEXT | yes | Executable identity, for recovery |
| `exit_kind` | TEXT enum | yes | `ClosedGracefully`, `ForceKilled`, `Crashed`, `ProcessGone`, `Unknown` |
| `exit_code` | INTEGER | yes | Raw exit code when available |
| `exit_signal` | TEXT | yes | Terminating signal, when that is how it ended |
| `recovery_note` | TEXT | yes | Machine-readable note about how a stale session was resolved |
| `session_directory` | TEXT | yes | Location of the session's generated artifacts, relative to the generated-files root |
| `created_at` | timestamp | no | Row creation |

**Foreign keys.** `game_id → games.id` — `RESTRICT`. `release_id →
releases.id` — `RESTRICT`, nullable. `content_id → contents.id` —
`RESTRICT`, nullable. `core_id → cores.core_id` — `RESTRICT`, nullable.
`core_version_id → core_versions.core_version_id` — `RESTRICT`, nullable.
`runtime_version_id → runtime_versions.runtime_version_id` — `RESTRICT`, nullable.

**Storage primary key.** `(id)`.

**Business uniqueness.**

- **At most one active session:**
  `UNIQUE (state) WHERE state = 'Active'`. A partial unique index over a
  constant means "there may be at most one row in this state", which is exactly
  the MVP rule (`ARCHITECTURE.md` §22, `PRODUCT.md` §27.1). It is enforced by the
  database rather than by application discipline, so a second concurrent start
  cannot succeed even if a caller forgets to check.

**Indexes.**

- `(game_id, started_at DESC)` — the per-game session history and the derived
  statistics (§16.3).
- `(state)` — the active-session lookup at startup and before every launch.
- `(started_at)` — the activity view and retention.

**Owner.** SQLite. **Lifecycle.** Persistent.

**Retention.** Sessions are user history and are the source of playtime, so they
are kept (§19.7).

**Invariant protected.** Exactly one active session; a session is persistent so
recovery is possible (`PRODUCT.md` §27.6).

**Decision — the active session is also the recovery record, and `state`
distinguishes it from a stale row.** A session that has started is written
immediately with `state = 'Active'` and `ended_at = NULL`. At startup the process
identity (`process_id` + `process_started_at` + `process_executable`,
`ARCHITECTURE.md` §22.1) decides:

```text
process still running  → keep Active, resume
process gone           → finalize: state = Recovered, ended_at = now, exit_kind = ProcessGone
```

A separate `active_session` table would be a second truth that could disagree
with the session history, and it would make "exactly one active session" a rule
two tables have to keep.

**Decision — the session records the runtime through the persistent
`runtime_versions` anchor plus the readable version text, not through the
installed-state index.** The runtime a session ran on is a historical fact, and it
must survive two things: the runtime build being uninstalled, and a component-index
rebuild. A reference to §12.6 would survive neither, because that table owns no
identity (§12.1). The anchor gives a stable, referencable identity; the
`runtime_version` text is kept alongside it, and that duplication is
**deliberate and bounded**: it exists so a session row is readable in a diagnostic
dump, in a log line and in a support report without a join, and so the observed
fact survives even if the anchor is pruned after every referencing session is
gone. It is documentation, never a second authority — nothing resolves, compares
or filters on `runtime_version`, and §20.3 test 25 asserts the two agree on write
so they cannot drift.

The same "observed but not bindable" pattern appears in
`save_states.core_version_label` (§15.2), where it is unavoidable rather than a
convenience: a save-state file genuinely may not identify its build.

**Decision — recovery never invents an end time.** A session whose process is
gone is finalized with the time of the recovery and `state = Recovered`, not with
a guessed end. `duration_ms` therefore means "observed or recovery-bounded
duration", and `recovery_note` says so. A fabricated exact end time would be
unverifiable and would silently distort playtime.

### 16.2 Generated session artifacts

`retroarch.cfg`, `core-options.cfg`, `launch.json`, `stdout.log` and `stderr.log`
live under `sessions/<session-id>/` (`ARCHITECTURE.md` §28) and are
**SessionScoped artifacts, not fachliche data**.

> **Rule (binding).** No table stores the *content* of a generated session
> artifact. The session row references the **directory**
> (`session_directory`) at most. A generated `.cfg` MUST NOT be read back as an
> authoritative setting, and a log MUST NOT be parsed as domain state.

**Decision — the artifact directory is referenced from the session row rather
than modelled by its own rows.** The directory is derived from the session
identity, and a row per artifact would be a table of files whose authority is
explicitly denied. Keeping the path on the session row is enough to find the
artifacts for diagnosis and to delete them with the session under the
`SessionScoped` rule.

### 16.3 Statistics are derived from sessions

`PRODUCT.md` §27.4 requires `lastPlayedAt`, `totalPlayTime`, `playCount` and the
duration of the last session. All four are **pure functions of the session rows**:

| Value | Derivation |
|---|---|
| `lastPlayedAt` | `MAX(started_at)` over the game's finalized sessions |
| `totalPlayTime` | `SUM(COALESCE(active_duration_ms, duration_ms))` over the same |
| `playCount` | `COUNT(*)` over the same |
| `lastSessionDuration` | `duration_ms` of the session with the greatest `started_at` |

> **Rule (binding).** There is **no statistics table**. Statistics MUST be
> derived from `sessions`.

**Decision and its reason — derived, not stored.** A materialized statistics
table would be a second truth that every session write, crash recovery, session
deletion and library reset would have to keep in step, and its most likely
failure mode is silent: a playtime that is slightly wrong forever, with no way to
tell which of the two values is right. Deriving makes a wrong statistic
impossible by construction; the cost is an aggregate query. That cost is
acceptable because the query is a per-game `GROUP BY` over a table with a
`(game_id, started_at)` index, and the library views that need cross-game ranking
(`RecentlyPlayed`, `Playtime` in `ARCHITECTURE.md` §36.1) can be answered by the
same indexed aggregate.

The retained history is what makes this decision safe: **because sessions are
kept, the derivation stays total.** Pruning sessions would silently reduce
lifelong playtime, so §19.7 only permits pruning that preserves the lifetime
totals.

### 16.4 What is deliberately not modelled

- **No normal saves.** SRAM, memory cards and other regular RetroArch saves are
  outside BitArchive scope (`ARCHITECTURE.md` §53.13,
  `PRODUCT.md` §23). No table, no index, no reference.
- **No `JobManager` job table.** `ARCHITECTURE.md` §30.4 keeps job persistence
  with the Issue that implements job recovery; this document models the
  *fachliche* run histories that reconciliation and resume actually need (§8.3).
- **No window/focus/presentation state.** Focus is stored as a fachliche
  identity (`ARCHITECTURE.md` §36.3), never as a list index, and it belongs to
  UI state, not to this model.
- **No telemetry, no analytics, no user account, no sync state**
  (`ARCHITECTURE.md` §53.16).

---

## 17. Search / FTS projection

### 17.1 What is indexed

One FTS5 index over the library, providing title search
(`ARCHITECTURE.md` §36.4) and the filter/sort columns the library views need
(`ARCHITECTURE.md` §36.1).

| Indexed value | Derived from |
|---|---|
| Effective title | §9.5, layers 1–7 on `metadata_fields.field_key = 'title'` (and `alternate_title` as secondary text) |
| Effective release title | the same resolution for the release-scoped `title` field, when it differs |
| System display name | the curated catalogue, via `releases.system_id → systems.catalog_key` |
| Release region and language tags | `release_regions`, `release_languages` |
| Release type | `releases.release_type` |
| Favorite / hidden / ignored | `games.is_favorite`, `games.is_hidden`, `games.is_ignored` |
| Release date | `releases.release_date` |

### 17.2 Rules

> **Rules (binding).**
>
> 1. The index contains the **effective** title, including manual overrides.
>    A user who renames a game finds it under the new name.
> 2. FTS5 is a **projection and MUST NOT be a fachliche truth**. Nothing reads
>    the index to answer a fachliche question; the index only *locates* rows.
> 3. `search_index` MUST be rebuildable from class A tables alone, at any time,
>    with no user-visible loss of meaning. It is therefore `Rebuildable`.
> 4. The index returns a **stable `GameId`**, and the query layer joins back to
>    `games` for the authoritative row. Results MUST carry the `GameId`, never a
>    row offset or a list position (`ARCHITECTURE.md` §53.22).
> 5. Every sort has a deterministic tie-breaker over stable identities, so the
>    same query returns the same order twice (`ARCHITECTURE.md` §36.3).

### 17.3 Rebuild triggers

Rebuilding is required when:

- a `manual_overrides` row is created, changed or deleted;
- `provider_values` change for `title` or `alternate_title`;
- a release is added, removed, or has its region/language/type/date changed;
- `games.is_favorite`, `is_hidden` or `is_ignored` changes;
- the locale preference changes (the effective title may change with it);
- the index is missing or its version marker does not match the schema version;
- a migration changes any input to the projection.

**Decision — incremental maintenance is allowed, full rebuild is the contract.**
Triggers or explicit writes may keep the index warm for responsiveness, but the
binding guarantee is that a full rebuild from class A always reproduces the same
index. An incrementally maintained index whose full rebuild is untested would be
a truth of its own, which rule 2 forbids.

**Decision — the projection is keyed by `GameId`, and a game appears once.** A
game with six releases is one library entry (`PRODUCT.md` §7.7), so the index must
carry exactly one row per game for title search. Release-level attributes are
used for filtering and for display, not for multiplying search results.

### 17.4 No SQL

This section defines the projection conceptually. The FTS5 object, its tokenizer,
its columns and its triggers are schema-v1 work (Issue #109).

**`search_index`** — the projection's keys, stated in the same form as every other
table so that §20.1 and this section agree:

**Domain identity.** None. The projection is keyed by the identity it indexes.

**Storage primary key.** The FTS5 `rowid` (implicit). An FTS5 virtual table has no
declared primary key, so the projection MUST NOT be presented as having one; the
`rowid` is a storage handle inside the projection only and never leaves it
(§3.9 rule 5).

**Business uniqueness.** One row per `game_id` (§17.3). An FTS5 virtual table
carries no declarative `UNIQUE` constraint, so this rule is a **rebuild
invariant** enforced by the rebuild and asserted by §20.3 test 20, not by an index.
That is acceptable precisely because the table is `Rebuildable`: a violation is
repaired by dropping and rebuilding the projection, with no user-visible loss.

---

## 18. Schema versioning and migration rules

### 18.1 The migration ledger

**`schema_migrations`** is the single authoritative record of what has been
applied.

**Domain identity.** None. The ledger is an ordered record, not an entity.

**Storage primary key.** `(version)` — `INTEGER`, sequential, starting at 1.

**Business uniqueness.** None beyond the storage primary key.

| Column | Logical type | Null | Meaning |
|---|---|---|---|
| `version` | INTEGER | no | Storage primary key; sequential, starting at 1 |
| `description` | TEXT | no | Human-readable purpose |
| `checksum` | TEXT | no | Digest of the migration's own definition, to detect an edited released migration |
| `applied_at` | timestamp | no | When it was applied |
| `bitarchive_version` | TEXT | no | The app version that applied it |

**Owner.** SQLite. **Lifecycle.** Persistent. **Retention.** Never pruned; the
ledger is the history.

### 18.2 Binding rules

> **Rules (binding).**
>
> 1. Migrations are **sequential and versioned**. Version `n+1` may only be
>    applied after version `n`.
> 2. Migrations are **forward-only**. There is **no down-migration as a product
>    mechanism** (`ARCHITECTURE.md` §9).
> 3. An **older application MUST refuse** to open a database whose schema version
>    is newer than the newest migration that application knows.
> 4. Migrations run **before** any repository or service is constructed: the
>    database is migrated to the version the binary expects as part of bootstrap,
>    so no repository can ever observe a partially migrated database.
> 5. **Backup restore reuses the same migration runner.** A restored database is
>    migrated by the same code path as a freshly opened one
>    (`ARCHITECTURE.md` §9, §34.2).
> 6. Migrations may combine SQL and Rust-based data transformation.
> 7. A **released migration MUST NOT be edited** to be prettier. A correction is
>    a new migration (`AGENTS.md` §9). The `checksum` column makes a violation
>    detectable.
> 8. A migration that cannot map existing data MUST preserve it and report the
>    problem rather than dropping rows — in particular `manual_overrides`
>    (§9.4) and stored overrides (§14.5).

### 18.3 Refusing a newer database

The refusal is a **hard startup failure with an explicit message**, naming the
database's schema version and the application's supported version. It MUST NOT:

- open the database read-only and continue,
- ignore unknown tables or columns,
- attempt a best-effort partial read.

A newer database may contain rows whose meaning this application would
misinterpret, and a misinterpretation that writes is far worse than a refusal. The
failure is reported to the user as an actionable condition ("this library was
created by a newer BitArchive version"), not as a crash.

### 18.4 What the version number means

The **schema version** is the `MAX(version)` in `schema_migrations`. It is:

- recorded in the backup manifest (`ARCHITECTURE.md` §34), so a restore knows what
  it is restoring;
- **not** the app version. The two are independent, and a release that adds no
  migration keeps the schema version;
- the marker the FTS projection and the component index compare against to decide
  whether they need a rebuild (§17.3).

**Decision — the ledger is a table of applied migrations, not a single version
row.** A single row answers "what version is this?", which is all the refusal rule
needs, but it cannot answer "was migration 7 applied with a definition that has
since been edited?" or "in what order were Rust-based transformations applied?".
The log costs one small table and makes the migration history auditable, which is
the property `ARCHITECTURE.md` §9 is protecting.

---

## 19. Deletion and retention rules

### 19.1 The governing principles

1. **The database never deletes an external user file** (invariant 20). Every
   deletion of a file outside BitArchive's own data area requires an explicit,
   confirmed user action.
2. **Losing a file never deletes fachliche identity.** Going offline, unplugging
   a volume, moving a file and revoking a permission all change *observations*,
   never identities.
3. **Invalid user data is preserved** (invariant 19). A row that no longer
   validates is reported, not removed.
4. **A cascade must be justified by the parent owning the child.** No unnamed
   `ON DELETE CASCADE` (§3.7).
5. **Derived data may always be rebuilt, so it may always be dropped** — as long
   as the rebuild is from class A and reproduces the same answer.
6. **Destructive reconciliation requires a successful complete scan**
   (invariant 18).

### 19.2 Removing a Library Source

```text
1. Set library_sources.removed_at                    (the source is no longer active)
2. Delete that source's File content_locations rows   (they were paths under its root)
3. Re-evaluate every ArchiveEntry location whose container was affected:
   the container's remaining File locations decide whether the entry is
   still physically reachable
4. Diff each affected content against its remaining locations
5. A content with no location left            → keep the row; it is now "not found"
6. A game/release with no located content     → keep every row; it leaves the
                                                active library view, and its
                                                metadata, overlays, favorites and
                                                statistics all survive
7. Delete nothing outside the database
```

`PRODUCT.md` §7.3 is the specification: source files stay untouched, games that
came *only* from that source leave the active library, and metadata, favorites and
statistics survive so that re-adding the source can recognize the content again.

**Step 2 deletes `File` locations only, and step 3 is why.** An `ArchiveEntry`
location has **no** `source_id` of its own (§5.2): it names a container content and
an entry inside it. Deleting entry rows "because they were under source X" is
therefore not even expressible, and it would also be wrong — the same archive may
still be present through another source or another path.

> **Rules (binding).** The two location kinds are treated differently, and the
> difference is not a detail — it is the whole rule:
>
> 1. **Removing source X deletes exactly one kind of row:** the `content_locations`
>    rows with `location_kind = 'File' AND source_id = X`. Those are the paths under
>    X's root, and they are the only locations that name a source at all.
> 2. **Removing source X deletes no `ArchiveEntry` location, ever.** An
>    `ArchiveEntry` row has no `source_id` (§5.2), so it cannot be "a location of
>    source X" in the first place: it names a container content and the entry inside
>    it, and the same archive may still be present through another source or another
>    path. Deleting entry rows "because they were under source X" is therefore not
>    even expressible, and it would also be wrong.
> 3. Removing one `File` location of an archive MUST NOT destroy an `ArchiveEntry`
>    relationship. As long as the `ArchiveContainer` content still has **one**
>    remaining `File` location — in another source, or at another path — the entry
>    is still physically reachable and its `ArchiveEntry` location stays valid,
>    unchanged.
> 4. Only when the container has **no** `File` location left does the entry become
>    unreachable. Even then the entry location row is **kept**, and the observed
>    state is decided by the ordinary reconciliation rules of §19.4: the
>    container's remaining locations are not present, so the entry is reported as
>    not found, and the `ContentId`s on both sides survive untouched.
> 5. A source removal MUST NOT delete a `games`, `releases`, `contents`,
>    `manual_overrides`, `provider_values`, `sessions`, `save_states` or
>    `media_assets` row, and MUST NOT touch the file system. Together with rules 1
>    and 2 this is the complete deletion statement of this section: the `File`
>    locations of the removed source go, **nothing else does**.
>
> **The binding retention rule in one sentence.** Removing source X deletes only
> `File` `content_locations` belonging to X. It MUST NOT delete `ArchiveEntry`
> `content_locations` or any fachliche identity, user state, metadata history or
> external file.

**What happens to an `ArchiveEntry` when a source is removed.** An `ArchiveEntry`
has no `source_id` of its own, so its reachability is decided by its container's
`File` locations and by nothing else. The whole case analysis is:

```text
remove the File location   Source 1 / games/foo.zip   of ArchiveContainer A
        ↓
A still has another Present File location (Source 2 / backup/foo.zip)
        → the ArchiveEntry location of A stays reachable and unchanged

A has no File location left
        → the ArchiveEntry row REMAINS
        → it becomes unreachable / not found through normal reconciliation
        → both ContentIds and every fachliche row stay untouched
```

No `ArchiveEntry` row is ever deleted because of a source removal, and no
`ArchiveEntry` row is ever deleted because its container went offline. "Not found"
is a state (§19.4), exactly as it is for a loose file.

So "the container is gone" is a **state**, not a deletion, exactly as it is for a
plain file: re-adding the source or replugging the volume makes the same entry
reachable again without any identity having moved.

**Removing a source ≠ rebuilding the library.** A rebuild (`PRODUCT.md` §40) is a
different, explicit operation with different semantics, specified in §19.3: it
clears the scan-derived index — `content_locations`, scan history, scraped provider
values and the search projection — and then re-scans, while **keeping**
`GameId`/`ReleaseId`/`ContentId` (and therefore every `games`, `releases` and
`contents` row), the canonical `Payload` recognition evidence, and all user data.
It is not reachable from a source removal, and it is not a "forget everything"
operation: §19.3 is the single binding specification of what it clears and what it
keeps, and no other section may describe it differently.

> **Rule (binding).** No passage of this document may state that a library rebuild
> deletes `games`, `releases` or `contents`, or that it forgets content
> identities. A rebuild re-derives the *index*, never the *identity*.
> `tools/check_data_model.py` fails on any such statement anywhere in the document.

### 19.3 The three removal operations, and what each keeps

Three different operations are easy to conflate. They are separate, and only the
third destroys everything.

```text
per-game index reset   ≠   library rebuild   ≠   full reset
```

#### There is no per-game delete action

> **Rule (binding).** The MVP has **no user-facing "Delete Game" action**, and the
> data model MUST NOT be read as defining one. A `games` row is never removed by an
> action aimed at a single game.

`PRODUCT.md` §41 enumerates the MVP's per-game operations exhaustively:

```text
- aus Bibliothek ausblenden   → games.is_hidden  = 1
- ignorieren                 → games.is_ignored = 1
- Indexeintrag zurücksetzen   → see below
```

**Hide and ignore are flags, not deletions** (`games.is_hidden`,
`games.is_ignored`, §6.1). Both are reversible, neither touches a user file, and
neither removes the game's identity — which is what lets metadata, favorites and
statistics survive a source removal (`PRODUCT.md` §7.3) and re-identification
after a move (`PRODUCT.md` §7.6).

**"Indexeintrag zurücksetzen" is not a game deletion.** It resets what the index
derived for one entry so that a scan may rebuild it. `PRODUCT.md` §41 names the
action but does not specify it further, so this document records only what
retention consistency requires and deliberately does **not** invent its full
semantics: it removes no `GameId`, deletes no user file, and preserves
`manual_overrides`, `sessions` and the game's flags. Its exact scope belongs with
the scan-reconciliation work (#53), which owns the index-reset and rebuild
semantics, and it is **not** a library rebuild.

#### Library rebuild

**Specification.** `PRODUCT.md` §40 and `ARCHITECTURE.md` §49.1:

```text
Library Index zurücksetzen
gescrapte Zuordnungen zurücksetzen      (gemäß Produktverhalten)
abhängige rebuildbare Library-Daten
Sources anschließend erneut scannen
Originaldateien bleiben unangetastet
```

**Rule (binding).** A library rebuild destroys **no fachliche identity** and no
user or history data. It clears the scan-derived index and the scraped
assignments, then re-scans. The table below is the complete specification of what
it clears and what it keeps; §6.1, §6.2, §6.3, §6.4, §10.2 and §19.4 use the same
list and no other.

| Table | Library rebuild | Why |
|---|---|---|
| `content_locations` | **cleared** | The scanned "what is where" index, and exactly what "Index zurücksetzen" names. The re-scan re-derives it. |
| `scan_runs`, `scan_run_issues` | **cleared** | Run history is scan-derived. Clearing it is why `content_locations.last_seen_scan_run_id` is `SET NULL` (§19.7) and why the rebuild needs no eligibility flag. |
| `provider_values` | **cleared** | The scraped assignments of `PRODUCT.md` §40. A later scrape re-fetches them. |
| `content_derivation_members`, `content_derivations` | **cleared** | Generated managed playlists, regenerated on demand (§6.6). |
| `search_index` | **cleared** | A projection over the tables above (§17.3). |
| `content_fingerprints` | **kept** | **Recognition evidence** — the only thing that lets the re-scan bind a rediscovered payload back to an existing `ContentId`. See the decision below. |
| `games`, `releases`, `contents` | **kept** | Fachliche identities. Their metadata, favorites, flags and statistics must survive (`PRODUCT.md` §7.3, §7.6). |
| `release_regions`, `release_languages` | **kept** | Intrinsic parts of a kept release. |
| `manual_overrides` | **kept** | User data; only an explicit user action removes it (§19.6). |
| `sessions` | **kept** | History and the source of every statistic (§16.3). |
| `save_states` | **kept** | User-visible save-state metadata over files RetroArch owns. |
| `media_assets`, `media_asset_references` | **kept** | `PRODUCT.md` §15.10 wants the library offline-displayable after a rebuild; only the controlled garbage collector removes blobs (§10.1). |
| `library_sources` | **kept** | The rebuild re-scans *these* sources; `availability` is refreshed. |
| `systems`, `cores`, `core_versions`, `runtime_versions` | **kept** | Not scan-derived. |
| `core_selection_overrides`, `retroarch_setting_overrides`, `core_option_overrides`, `core_option_schemas`, `core_option_definitions` | **kept** | User configuration. |
| `firmware_entries`, `firmware_index_state`, `managed_components`, `component_index_state` | **kept** | Indexes over the firmware folder and the component store, not over the library. |
| `schema_migrations` | **kept** | Never. |

**Order and atomicity.** The cleared tables are removed child-first —
`content_derivation_members` → `content_derivations` → `scan_run_issues` →
`content_locations` → `provider_values` → `scan_runs` — and the whole rebuild runs
in **one transaction**. A rebuild that cleared the index and then failed must not
leave a half-reset library visible, and a partially cleared index would be worse
than either end state. No reference has to be cleared here: every cleared table is
a child in each of its remaining relationships, and the retained tables are the
parents. The mutual `games` ↔ `releases` reference is between two **retained**
tables and is not touched by a rebuild at all.

`content_fingerprints` is **not** in that sequence: it is the one table the rebuild
deliberately keeps (see the decision below). `search_index` is a projection and is
rebuilt after the transaction rather than cleared inside it (§17.3).

Note the direction of the dependencies: a derivation belongs to a `Content`, not to
a location, so clearing locations cannot orphan one — which is why the two are
cleared independently and why the `Payload` fingerprints survive untouched.

**Non-destructive by construction.** A rebuild touches no path outside BitArchive's
own data area. It never deletes, moves, renames or rewrites a ROM, ISO, ZIP,
firmware or save-state file (invariant 20, `ARCHITECTURE.md` §49.1).

**Decision — the canonical payload fingerprint is recognition evidence, not scan
index, so a rebuild keeps it.** This is the one non-obvious row of the table, and
it is what makes the rebuild *executable* rather than merely declared.

*The problem.* If a rebuild cleared `content_fingerprints` along with
`content_locations`, the retained `ContentId`s would be unreachable. A re-scan that
found `Foo.gba` with payload SHA-256 `abc` would have no way to learn that `abc`
already belonged to `Content C`, so it would either mint a new content — losing
every metadata, favorite and statistic attached to `C` — or fall back to matching
by path or file name, which invariant 2 forbids.

*The rule.* A `Payload` fingerprint is the content's **recognition evidence**, and
it is retained across a rebuild. `Container` and `EntryList` fingerprints are
observation-near descriptions of a specific archive and may be cleared and
re-derived by the re-scan, because re-identification needs the playable payload,
not the container's packaging (`PRODUCT.md` §10).

*The re-identification path*, which is deterministic and path-free:

```text
retained:  ContentId C  +  its canonical Payload fingerprint (Sha256, digest abc)
                    ↓
re-scan:   observes  <source>/Foo.gba  with payload digest abc
                    ↓
lookup:    content_fingerprints WHERE fingerprint_kind='Payload'
                                   AND algorithm='Sha256' AND digest=abc
                    ↓
match:     exactly one row → ContentId C        (new observation only)
           no row          → genuinely new content
                    ↓
result:    GameId / ReleaseId / ContentId stay stable;
           metadata, favorites and statistics remain attached
```

*The lookup is unambiguous by construction.* The lookup uses exactly
`(fingerprint_kind = 'Payload', algorithm, digest)`, and §7.1 declares a partial
**UNIQUE** index over exactly those columns. The constraint and the lookup are
therefore the same key, and the lookup returns one row or none, never a choice. And
because §7.1's primary key allows one `Payload` row per content, "the canonical
payload fingerprint" is well defined rather than a convention. Two files with
identical payloads are therefore two locations of one content — which is exactly
`PRODUCT.md` §7.7's duplicate rule, not a special case.

Note what the key deliberately does **not** include: `byte_size`. It is nullable,
SQLite treats `NULL`s in a unique index as distinct, and a constraint containing it
would not have guaranteed what the lookup assumes. `entry_path` is likewise absent,
so the same entry bytes may occur in several archives (§7.1).

*What this preserves and what it costs.* The rebuild keeps a bounded amount of
information — one 32-byte digest per content — in exchange for guaranteeing the
property `PRODUCT.md` §7.6 requires. That is the whole trade: the rebuild is still
a reset of the scanned index (locations, runs, scraped values, projections all go),
but it is no longer a reset of the knowledge that lets BitArchive recognize its own
content afterwards.

#### Full reset

**Rule (binding).** Only the full reset destroys all BitArchive-owned database
state. It is the *only* operation in this model that removes `games`, `releases`,
`contents`, `sessions` or `save_states` rows, and even it never touches a user file.

**The order below is verified, not asserted.** It was derived from the declared
foreign keys and then **executed against a real SQLite database with
`PRAGMA foreign_keys=ON`**, with one referencing row seeded for every edge, so
every `RESTRICT` actually bites. `tools/check_data_model.py` re-derives it from the
foreign keys in this document on every run and fails if the two disagree, so the
list cannot drift away from the model it claims to satisfy.

**No pointer has to be cleared first.** An earlier revision prefixed this plan with
`UPDATE games SET default_release_id = NULL` and
`UPDATE sessions SET content_id = NULL, release_id = NULL`. Both are unnecessary,
and removing them is what the verification showed:

- `games.default_release_id` is `SET NULL`, so deleting a release nulls the pointer
  itself. That reference therefore creates no ordering constraint at all; it would
  only do so if it were `RESTRICT`.
- `sessions.content_id` **is** `RESTRICT`, but `sessions` is deleted *before*
  `contents` in the order, so the reference is gone before its target is.

So the rule is simply: **delete child before parent, and one ordering does it.**
There is **no cycle of `RESTRICT` references in this model**, which is why no
pointer pre-clearing is needed anywhere. The only mutual reference is
`games` ↔ `releases`, and it is not a cycle in the ordering sense because one of its
two directions is `SET NULL`. Clear a nullable pointer explicitly only if a future
change turns such a reference into `RESTRICT` and makes an ordering impossible; the
model currently contains none, and `tools/check_data_model.py` re-derives that fact
from the declared foreign keys on every run.

```text
 1. DELETE core_option_overrides          (refs core_versions, games, systems)
    DELETE core_option_definitions        (refs core_versions)
    DELETE core_option_schemas            (refs core_versions)
 2. DELETE retroarch_setting_overrides    (refs games, systems)
    DELETE core_selection_overrides       (refs cores, games, releases, systems)
 3. DELETE save_states                    (refs releases, cores, core_versions, media_assets)
 4. DELETE content_derivation_members     (refs content_derivations, contents)
    DELETE content_derivations            (refs contents)
 5. DELETE content_fingerprints           (refs contents, scan_runs)
    DELETE content_locations              (refs contents, library_sources, scan_runs)
 6. DELETE sessions                       (refs games, releases, contents, cores,
                                           core_versions, runtime_versions)
 7. DELETE contents                       (refs releases; its locations,
                                           fingerprints, derivations and sessions
                                           are gone above)
 8. DELETE release_regions                (refs releases)
    DELETE release_languages              (refs releases)
    DELETE releases                       (refs games, systems; games released the
                                           default-release pointer via SET NULL)
 9. DELETE scrape_run_items               (refs games, scrape_runs)
    DELETE scrape_runs                    (refs games, metadata_providers, systems)
10. DELETE media_asset_references         (refs media_assets)
    DELETE media_assets                   (refs metadata_providers)
11. DELETE games                          (default_release_id already NULL;
                                           scrape_run_items and every override are gone)
12. DELETE provider_values                (refs metadata_fields, scrape_runs)
    DELETE manual_overrides               (refs metadata_fields)
13. DELETE scan_run_issues                (refs library_sources, scan_runs)
    DELETE scan_runs                      (refs library_sources)
    DELETE library_sources                (refs systems)
14. DELETE core_versions                  (refs cores)
    DELETE runtime_versions
    DELETE cores
15. DELETE managed_components             (refs core_versions, runtime_versions)
16. DELETE systems
17. DELETE metadata_providers             (refs nothing; curated)
    DELETE metadata_fields                (refs nothing; curated)
18. DELETE schema_migrations and the database file itself
```

**Why `games` is deleted so late.** It is referenced by `RESTRICT` from
`core_option_overrides`, `retroarch_setting_overrides`, `core_selection_overrides`,
`sessions`, `scrape_run_items` and `scrape_runs`, so it cannot go until all six are
gone. The previous plan deleted `games` at step 7 and then tried to delete the two
`scrape_*` tables — which would have failed with
`FOREIGN KEY constraint failed`. That is the concrete defect this order fixes.

**Why `cores` is deleted after `core_versions`.** `core_versions.core_id` is
`RESTRICT` and points at `cores`, so the versions go first; `managed_components`
follows because it only points *at* the versions, and with `SET NULL`.

`firmware_entries`, `firmware_index_state` and `component_index_state` are
deliberately absent: they are index-only tables that reference nothing and hold no
identity, so a full reset may truncate them at any point or simply delete the
database file in step 18. `search_index` is likewise a projection and needs no
explicit step.

**Rule (binding).** Step 18 is the only step that may touch anything outside the
database, and even it touches **only BitArchive's own data area**.
`PRODUCT.md` §41 states that BitArchive never deletes the underlying ROM/ISO files,
and `PRODUCT.md` §40 repeats it for the full reset: ROMs, ISOs and BIOS/firmware
remain untouched, and normal RetroArch saves are outside the model entirely
(invariant 13). A "reset" that deleted a save-state *file* would need the explicit
per-state confirmation of `ARCHITECTURE.md` §29.6, which is a different flow: a
full reset removes save-state *rows*, and the files stay until the user confirms
their deletion state by state (§19.5).

### 19.4 Missing content

| Observation | Effect |
|---|---|
| A location is not seen by an eligible scan | `content_locations.state = 'Missing'`; `last_seen_at` is not advanced |
| A source is `Offline` / `PermissionDenied` / `Missing` | **No** location is marked missing at all |
| A content has no `Present` location | It is "not found": it leaves the launchable set but keeps its identity, fingerprints and every referencing row |
| An `ArchiveContainer` loses its last `File` location | Its `ArchiveEntry` location row is **kept** and becomes unreachable, never deleted: the container has no present copy, so the entry is "not found" like any other location, and the playable `ContentId` is untouched (§19.2) |
| The file reappears | The next scan sets `state = 'Present'` and advances `last_seen_at`; because the identity was never dropped, all metadata and statistics are still attached |

> **Rule (binding) — the retention cases are not interchangeable.**
>
> 1. **An ordinary missing or unreachable observation never deletes a
>    `content_locations` row.** A location that is not seen, a source that is
>    offline, and an `ArchiveContainer` whose last `File` location is gone all
>    change a **state**, never the row count: the row stays and is reported as
>    `Missing`, unreachable or "not found", and the `ContentId` survives
>    (§19.4 table above).
> 2. **A source removal deletes only the removed source's `File` locations.** Its
>    scope is `location_kind = 'File' AND source_id = X` for the explicitly removed
>    source X, and it **never** deletes an `ArchiveEntry` location (§19.2).
> 3. **A library rebuild clears all `content_locations` rows** — not one source's
>    `File` rows — and then re-scans, as specified by §19.3 (`PRODUCT.md` §40). It
>    is an index reset and not a destructive one: `GameId`, `ReleaseId`, `ContentId`
>    and the retained `Payload` recognition evidence all survive the clear (§19.3).
> 4. **A full reset removes `content_locations` together with all other
>    BitArchive-owned database state.** It is the only operation here that destroys
>    fachliche identities, and even it touches no external ROM, ISO, firmware or
>    save-state file (§19.3).
> 5. **The per-game "Indexeintrag zurücksetzen" is delegated, not decided here.**
>    `PRODUCT.md` §41 names the action but does not specify its scope, so its exact
>    deletion semantics stay with the scan-reconciliation work (#53, §19.3).
>
> The five cases are stated so that no two of them can be merged, and each one names
> its own effect. This section therefore claims no total number of the operations
> that may delete a `content_locations` row: case 5 is undecided until #53, and any
> passage fixing such a total would claim more than this model knows today. A
> missing content never deletes a game, a release, or a content either, and neither
> does a library rebuild: only the **full reset** removes those rows, and even it
> touches nothing outside BitArchive's own data area (§19.3).

### 19.5 Deleting a Save State

The order is fixed by `ARCHITECTURE.md` §29.6 and is **file-system operation
first**:

```text
1. Delete the actual RetroArch state file
2. Delete the thumbnail file, if present
3. Only then update the database index
```

> **Rule (binding).** If step 1 or step 2 fails, the database row MUST remain and
> the failure MUST be reported. A row is never deleted for a file that still
> exists, because the next discovery pass would simply rediscover the state and
> the user's deletion would look like it silently failed.

**Note on the thumbnail.** Because the same bytes may back several media records
(§10.1), deleting the thumbnail *file* is only correct once no other media
reference still needs it. The deletion step therefore removes the reference and
lets the controlled garbage collection (`ARCHITECTURE.md` §18.3) drop the file
when the last reference is gone. Observed behavior for the user is unchanged — the
thumbnail disappears from BitArchive — and no other asset breaks.

### 19.6 Provider refresh and overrides

> **Rule (binding).** A provider refresh may **create, update and delete
> `provider_values` rows**. It MUST NOT create, update or delete a
> `manual_overrides` row. A field that has an override is reported as
> "override protected" (`PRODUCT.md` §15.6) instead of being overwritten.

### 19.7 Retention summary

| Structure | Lifetime | Retention rule |
|---|---|---|
| `library_sources` | Persistent | Kept; removal sets `removed_at` |
| `content_locations` | Persistent | Kept while the content exists; `Missing` is a state, not a deletion. A source removal deletes that source's `File` rows only and no `ArchiveEntry` row; a library rebuild clears the table and the re-scan re-derives it; the full reset drops it with the rest of the state (§19.2, §19.3) |
| `games`, `releases`, `contents` | Persistent | **Only a full reset** (§19.3). A library rebuild keeps all three |
| `release_regions`, `release_languages` | Persistent | Cascade with their release (intrinsic parts) |
| `content_fingerprints` | Persistent | **Recognition evidence.** `Payload` rows are kept across a library rebuild, because re-identification depends on them; `Container`/`EntryList` rows are re-derived. Rows are replaced on recompute and deleted with their content (§7.1, §19.3) |
| `content_derivations`, `content_derivation_members` | Rebuildable | Deleted and regenerated freely |
| `systems`, `cores` | Persistent | Never deleted while referenced |
| `core_versions` | Persistent (anchor) | Kept while any save state, session, schema or override references it |
| `runtime_versions` | Persistent (anchor) | Kept while any session references it |
| `core_selection_overrides`, `retroarch_setting_overrides`, `core_option_overrides` | Persistent | Explicit reset or scope deletion; never for invalidity |
| `core_option_schemas`, `core_option_definitions` | Persistent | Historical introspection results; refreshed when the same build is re-introspected; removed only with their `core_versions` anchor (§14.2) |
| `metadata_fields`, `metadata_providers` | Persistent | Curated; changed by migration |
| `provider_values` | Persistent | Replaced per provider refresh; deleted with the subject; cleared by a library rebuild, because `PRODUCT.md` §40 resets the scraped assignments (§19.3) |
| `manual_overrides` | Persistent | **Explicit user action only** |
| `scrape_runs`, `scrape_run_items` | Persistent | Job history; pruned only as a whole run (§19.7) |
| `scan_runs`, `scan_run_issues` | Persistent | Carve-outs always kept; older runs pruned (§19.7); a library rebuild clears run history (§19.3) |
| `media_assets`, `media_asset_references` | Persistent | The **record** is persistent; the blob it references is a separate `Rebuildable` artifact in the managed media store (see the split in §10.1). Bytes are removed only by the controlled garbage collector |
| `firmware_entries`, `firmware_index_state` | Rebuildable | Dropped and rebuilt on a firmware scan |
| `managed_components`, `component_index_state` | Rebuildable | Reconciled to the component store; may be dropped entirely |
| `save_states` | Persistent (metadata) | Only after a successful file deletion; `Missing` is a state |
| `sessions` | Persistent | **Not pruned** (lifetime totals, §16.3) |
| `search_index` | Rebuildable | Dropped and rebuilt (§17.3) |
| `schema_migrations` | Persistent | Never |

**Decision — the pruning horizon is 90 days for run history.** Run history exists
to make reconciliation and resume legitimate and to explain a scan to the user.
Both needs are bounded by recent history, so keeping every run of a library
scanned nightly forever would grow the database without adding an answer. The two
carve-outs are non-negotiable: the most recent **successful full scan** is the
evidence that the *next* reconciliation may be destructive, and the most recent
run per source is what the UI reports after a restart.

**Rule (binding) — every reference from long-lived data to a run is nullable and
`ON DELETE SET NULL`, except an intrinsic child.** Pruning a run must never be
blocked by, and must never damage, the data that outlives it:

| Reference | Delete behaviour | Why |
|---|---|---|
| `scan_run_issues.scan_run_id` | `CASCADE` | Intrinsic child: an issue is *part of* its run and has no meaning alone. It is pruned with the run, by definition. |
| `scrape_run_items.scrape_run_id` | `CASCADE` | Intrinsic child, same reasoning. |
| `content_locations.last_seen_scan_run_id` | `SET NULL` | Pure provenance. The location's validity comes from its `state` and `last_seen_at`, not from which run observed it. |
| `content_fingerprints.source_scan_run_id` | `SET NULL` | Pure provenance. A fingerprint stays valid and usable after its run is pruned. |
| `provider_values.scrape_run_id` | `SET NULL` | Pure provenance. The value and its `fetched_at` outlive the run record. |

An earlier revision marked the last three `RESTRICT`, which made §19.7's pruning
rule **unexecutable**: a rule that says "prune runs older than 90 days" cannot
coexist with a foreign key that refuses to delete a run any long-lived row still
mentions. The rule and the constraint had to be reconciled in one direction or the
other, and the table above is that reconciliation.

**Nothing that is not a run reference was weakened.** Ownership relations keep
their `RESTRICT` (`contents.release_id`, `save_states.release_id`,
`cores`… ), intrinsic parts keep their `CASCADE` (§6.4, §6.6, §12.4), and
`manual_overrides` remains undeletable by anything but an explicit user action
(§19.6).

**Decision — `save_states` carries no run reference at all.** It previously had a
`last_scan_run_id`. Discovery is a maintenance pass over the file system, not a
`scan_runs` event in the library-scan sense (§8.1 records *library scans*), and
`last_seen_at` already answers "when was this state last confirmed present". The
column was removed rather than re-pointed, because a reference that exists only to
be set to `NULL` on pruning carries no information.

**Decision — session history may keep a run provenance link `SET NULL`, and
need not.** Sessions are not pruned (§16.3), so they never force a run to survive
either; the model deliberately stores no session→run reference, because a session
is caused by a *launch*, not by a scan or a scrape.

**Decision — sessions are never pruned.** §16.3 derives `totalPlayTime` and
`playCount` from them. Pruning would make lifelong statistics shrink, which is
user-visible data loss. If the table ever needs bounding, the correct change is a
separate lifetime-totals store written *before* any pruning — a future Issue, not
a silent retention rule.

### 19.8 Generated artifacts

| Artifact | Lifetime | Rule |
|---|---|---|
| `sessions/<session-id>/retroarch.cfg`, `core-options.cfg`, `launch.json` | SessionScoped | Deleted with the session's artifacts; never read back as truth |
| `sessions/<session-id>/stdout.log`, `stderr.log` | SessionScoped | Prorated by the log retention policy; never parsed as domain state |
| Managed `.m3u` playlists | Rebuildable | Regenerated on demand |
| `Cache/downloads`, `Cache/staging`, `Cache/extracted`, `Cache/images` | Temporary | Removable by startup cleanup |
| Managed media blobs | Rebuildable | Removed only by the controlled garbage collector |

> **Rule (binding).** Startup cleanup removes only artifacts that are
> unambiguously BitArchive-owned, temporary, or expired (invariant 20,
> `ARCHITECTURE.md` §35.1). It MUST NOT touch a configured library source, the
> firmware folder, the managed component store, or a save-state file.

---

## 20. Schema-v1 derivation checklist

### 20.1 Tables

Every persisted table, with the three things §3.9 requires to be kept apart:
the **domain identity** (may be `none`), the **storage primary key** (always
exactly one), and the **business uniqueness** (the `UNIQUE` rules, which may be
`none`). This is the checklist #109 derives `CREATE TABLE` statements from, and it
contains no other authority: the per-table sections define the columns and the
constraints, and this table must agree with them.

| # | Table | Section | Domain identity | Storage primary key | Business uniqueness |
|---|---|---|---|---|---|
| 1 | `schema_migrations` | §18.1 | none | `(version)` `INTEGER` | none |
| 2 | `systems` | §6.5 | `SystemId` | `(system_id)` | `UNIQUE (catalog_key)` |
| 3 | `library_sources` | §5.1 | `LibrarySourceId` | `(id)` | `UNIQUE (location, platform_locator_kind)` |
| 4 | `games` | §6.1 | `GameId` | `(id)` | none |
| 5 | `releases` | §6.2 | `ReleaseId` | `(id)` | `UNIQUE (game_id, release_key)`; `UNIQUE (id, game_id)` (FK target for #8) |
| 6 | `release_regions` | §6.4 | none | `(release_id, region)` | none |
| 7 | `release_languages` | §6.4 | none | `(release_id, language)` | none |
| 8 | `contents` | §6.3 | `ContentId` | `(id)` | none (FK `(release_id, game_id)` → #5) |
| 9 | `content_locations` | §5.2 | none | `(row_id)` `INTEGER` | `UNIQUE (source_id, relative_path) WHERE location_kind = 'File'`; `UNIQUE (archive_content_id) WHERE location_kind = 'ArchiveEntry'` |
| 10 | `content_derivations` | §6.6 | none | `(content_id, kind)` | none |
| 11 | `content_derivation_members` | §6.6 | none | `(content_id, kind, source_content_id)` | `UNIQUE (content_id, kind, member_index)` |
| 12 | `content_fingerprints` | §7.1 | none | `(row_id)` `INTEGER` | `UNIQUE (content_id, fingerprint_kind, algorithm) WHERE fingerprint_kind IN ('Payload', 'Container')`; `UNIQUE (content_id, fingerprint_kind, algorithm, entry_path) WHERE fingerprint_kind = 'EntryList'`; `UNIQUE (algorithm, digest) WHERE fingerprint_kind = 'Payload'` |
| 13 | `scan_runs` | §8.1 | `ScanRunId` | `(id)` | none |
| 14 | `scan_run_issues` | §8.2 | none | `(scan_run_id, issue_index)` | none |
| 15 | `metadata_fields` | §9.2 | none | `(field_key)` | none |
| 16 | `metadata_providers` | §9.7 | none | `(provider_id)` | none |
| 17 | `provider_values` | §9.3 | none | `(row_id)` `INTEGER` | `UNIQUE (provider_id, subject_kind, subject_id, field_key, value_index) WHERE locale IS NULL`; `UNIQUE (provider_id, subject_kind, subject_id, field_key, locale, value_index) WHERE locale IS NOT NULL` |
| 18 | `manual_overrides` | §9.4 | none | `(row_id)` `INTEGER` | `UNIQUE (subject_kind, subject_id, field_key) WHERE locale IS NULL`; `UNIQUE (subject_kind, subject_id, field_key, locale) WHERE locale IS NOT NULL` |
| 19 | `scrape_runs` | §9.6 | `ScrapeRunId` | `(id)` | none |
| 20 | `scrape_run_items` | §9.6 | none | `(scrape_run_id, game_id)` | none |
| 21 | `media_assets` | §10.1 | `MediaAssetId` | `(id)` | none (no natural key; §10.1) |
| 22 | `media_asset_references` | §10.2 | none | `(subject_kind, subject_id, logical_key)` | none |
| 23 | `firmware_entries` | §11.1 | none | `(relative_path)` | none (an index key, not an identity) |
| 24 | `firmware_index_state` | §11.2 | none | `(singleton)` `CHECK (singleton = 1)` | none |
| 25 | `cores` | §12.3 | `CoreId` | `(core_id)` | `UNIQUE (component_key)` |
| 26 | `core_selection_overrides` | §12.3 | none | `(row_id)` `INTEGER` | `UNIQUE (scope_system_id) WHERE scope_kind = 'System'`; `UNIQUE (scope_game_id) WHERE scope_kind = 'Game'`; `UNIQUE (scope_release_id) WHERE scope_kind = 'Release'` |
| 27 | `core_versions` | §12.4 | `CoreVersionId` | `(core_version_id)` | `UNIQUE (component_key, platform, build_id)`; `UNIQUE (core_id, platform, build_id)` |
| 28 | `runtime_versions` | §12.5 | `RuntimeVersionId` | `(runtime_version_id)` | `UNIQUE (component_key, platform, version)` |
| 29 | `managed_components` | §12.6 | none | `(component_class, component_key, platform, version)` | none |
| 30 | `component_index_state` | §12.7 | none | `(singleton)` `CHECK (singleton = 1)` | none |
| 31 | `retroarch_setting_overrides` | §13.1 | none | `(row_id)` `INTEGER` | `UNIQUE (setting_key) WHERE scope_kind = 'Global'`; `UNIQUE (scope_system_id, setting_key) WHERE scope_kind = 'System'`; `UNIQUE (scope_game_id, setting_key) WHERE scope_kind = 'Game'` |
| 32 | `core_option_schemas` | §14.2 | none | `(core_version_id)` | none |
| 33 | `core_option_definitions` | §14.2 | none | `(core_version_id, option_key)` | none |
| 34 | `core_option_overrides` | §14.1 | none | `(row_id)` `INTEGER` | `UNIQUE (core_version_id, option_key) WHERE scope_kind = 'CoreDefaults'`; `UNIQUE (core_version_id, scope_system_id, option_key) WHERE scope_kind = 'System'`; `UNIQUE (core_version_id, scope_game_id, option_key) WHERE scope_kind = 'Game'` |
| 35 | `save_states` | §15.1 | `SaveStateId` | `(id)` | `UNIQUE (file_relative_path)` |
| 36 | `sessions` | §16.1 | `SessionId` | `(id)` | `UNIQUE (state) WHERE state = 'Active'` |
| 37 | `search_index` (FTS5 + its external-content/source tables) | §17 | none | FTS5 `rowid` (implicit) | one row per `game_id` — a rebuild invariant, §17.4 |

**Reading the table.** A table with a `row_id` primary key has **no** domain
identity: the row is a location, a fingerprint, an override or a mapping, and its
fachliche uniqueness is entirely in the `UNIQUE` column. Such a `row_id` is never
referenced by another table, never leaves the persistence layer and never appears
in a domain type (§2.4, §3.9). Conversely, a table with a domain identity and a
`none` in the last column is an entity whose only key is its identity.

**Foreign-key targets.** Only two kinds of key in this table may be referenced by a
foreign key: a storage primary key, and a **non-partial** business unique key. The
partial indexes (`WHERE …`) are **not** eligible: SQLite rejects a foreign key whose
parent key is a partial index with "foreign key mismatch". Every foreign key in
§20.2 respects this, and `tools/check_data_model.py` verifies it.

### 20.2 Referential creation order

Table 8 references `releases(id, game_id)`, and tables 26, 27, 29, 30, 31 and 35
reference the anchors of tables 25 and 27/28. Schema v1 therefore has
order-of-creation constraints that a naive alphabetical script would violate:

```text
cores                       before  core_selection_overrides, core_versions
core_versions               before  managed_components, core_option_schemas,
                                    core_option_overrides, save_states, sessions
runtime_versions            before  managed_components, sessions
releases (UNIQUE (id, game_id))  before  contents
scan_runs                   before  content_locations, content_fingerprints
content_derivations         before  content_derivation_members
```

Note what is **not** in this list: `managed_components` is referenced by nothing,
so it may be created last and dropped at any time.

### 20.3 Required integrity tests for schema v1

Each rule below is stated as a test that MUST fail when the rule is broken:

1. A 16-byte value that is not a UUIDv7 is **rejected** on read, per table, and
   never silently replaced (§3.3).
2. A digest column is never a primary key and never a foreign key target (§3.4).
3. `content_locations` rejects a **second `ArchiveEntry` row for one container**, and
   rejects two contents claiming the same `File` path of one source (§5.2). The
   `ArchiveEntry` half is asserted three times, because the constraint must bite
   regardless of which other column differs:
   - a second row for the same `archive_content_id` with the **same** `content_id`
     and a different `archive_entry_path` is **rejected**;
   - a second row for the same `archive_content_id` with a **different**
     `content_id` is **rejected**;
   - the **first** `ArchiveEntry` row for a container is **accepted**, so the
     constraint bounds the count without forbidding the supported case.
   This is `PRODUCT.md` §10 ("genau einen eindeutig spielbaren Content") and
   `§6.3`'s "at most one playable entry is imported per archive" expressed as a
   database fact. A second `ArchiveEntry` row with a *different* `archive_entry_path`
   is not a legitimate "same payload twice" state: the archive was ambiguous at scan
   time and is not regularly imported.
4. `contents` rejects a `release_id` whose release belongs to a different
   `game_id` (§6.3).
5. **Both location shapes are exactly the architecture's sum type** (§5.2). The
   `CASE` check is tested from both sides:
   - a **`File`** location refuses a non-`NULL` `archive_content_id` **or**
     `archive_entry_path`, and refuses a `NULL` `source_id` or `relative_path`;
   - an **`ArchiveEntry`** location refuses a `NULL` `archive_content_id` or
     `archive_entry_path`, and refuses a non-`NULL` `source_id` **or**
     `relative_path`.
   The test MUST assert all four directions, because "an `ArchiveEntry` also
   carries the source path it happened to be seen at" is exactly the redundant
   representation the model removed.
   Also: an `archive_content_id` that does not reference a content of kind
   `ArchiveContainer` is refused by the write path (the type check is a parent-row
   predicate, so it cannot be a foreign key).
6. `release_regions`/`release_languages` reject a duplicate tag and cascade only
   with their release (§6.4).
7. `content_derivation_members` stores **several** members for one derivation,
   rejects a duplicate member, and rejects two members claiming the same
   `member_index` (§6.6).
8. **Every partial unique index rejects a duplicate and accepts a row that
   differs only in the discriminating column** (§3.8). The test enumerates the
   actual partial indexes rather than a remembered list: the `File` and
   `ArchiveEntry` location indexes, the three fingerprint-kind indexes, the two
   `locale IS NULL` / `locale IS NOT NULL` pairs, and the `System`/`Game`/`Release`,
   `Global`/`System`/`Game` and `CoreDefaults`/`System`/`Game` scope indexes. In
   each case a row that differs **only** in the discriminator must be accepted, and
   an otherwise identical row must be refused.
9. `sessions` rejects a second `Active` row (§16.1).
10. `media_asset_references` rejects two assets in one logical slot and allows one
    asset in several slots (§10.2).
11. `cores` rejects two `CoreId`s for one `component_key` (§12.3), **and accepts a
    core row while no build of it is installed** — the test asserts that no
    foreign key into the installed-state index exists.
12. `core_versions` rejects two rows for one `(component_key, platform, build_id)`
    (§12.4).
13. `provider_values` rejects a `NULL` `value_index` and rejects a second
    `value_index = 0` for a field whose `metadata_fields.is_multi_valued = 0`
    (§9.3).
14. `firmware_entries` requires a non-`NULL` `digest`, `byte_size` and `mtime`
    exactly when `read_state = 'Readable'`, and `NULL` `digest` for `Unreadable`
    and `Missing` (§11.1).
15. Exactly one value column is populated per settings/override row, matching its
    `value_type` (§13.2).
16. A `Release` scope is **impossible** for RetroArch settings and core options
    (§13.4).
17. A `Global` scope is **impossible** for core selection (§12.3).
18. Deleting a `manual_overrides` row is never a side effect of any other
    deletion in this model (§19.6).
19. **Every polymorphic subject reference resolves.** `provider_values.subject_id`,
    `manual_overrides.subject_id` and `media_asset_references.subject_id` reference
    an existing `games` or `releases` row according to `subject_kind`, and every
    `subject_kind` is one of the two values the `CHECK` admits (§9.3, §9.4, §10.2).
    Because the reference is deliberately polymorphic, this MUST be enforced by the
    write path **and** verified by a test — for all three tables, not just the
    metadata ones.
20. `search_index` rebuilds from class A tables to an identical index (§17).
21. `firmware_entries` and `managed_components` rebuild from the file system to an
    equivalent index (§11, §12.6).
22. **Dropping and rebuilding the whole of §12.6 leaves every persistent reference
    resolvable.** After the rebuild, all `save_states.core_version_id`,
    `sessions.core_version_id`, `core_option_schemas.core_version_id` and
    `core_option_overrides.core_version_id` values are unchanged and still point
    at an existing `core_versions` row, and every `sessions.runtime_version_id`
    still points at an existing `runtime_versions` row (§12.1, §12.4, §12.5,
    §12.6). This is the test for the finding that motivated the §12 split.
23. **Pruning a run succeeds and damages nothing.** Deleting a `scan_runs` row
    older than the retention horizon cascades its `scan_run_issues`, sets the
    `content_locations.last_seen_scan_run_id` and
    `content_fingerprints.source_scan_run_id` of surviving rows to `NULL`, and
    leaves those rows valid (§19.7). A `RESTRICT` anywhere on those two columns
    would make this test fail, which is the point.
24. Opening a database whose `MAX(schema_migrations.version)` exceeds the
    application's supported version **fails** (§18.3).
25. `sessions.runtime_version` and `sessions.runtime_version_id` cannot drift: a
    session written through the write path carries the version text that belongs to
    its anchor (§16.1). The duplication is documentation for diagnostics, never a
    second authority.
26. `content_derivations.member_count` equals the number of
    `content_derivation_members` rows for the same `(content_id, kind)`, and
    `member_count >= 2` (§6.6).
27. `core_option_schemas` is **not** dropped when a `managed_components` row
    disappears: uninstalling a build leaves the schema and its overrides intact and
    interpretable (§14.2). Re-installing and re-introspecting the same build
    replaces the row rather than adding a second one.
28. **A library rebuild preserves the re-identification path.** After clearing
    `content_locations` and re-scanning, a content whose file reappears is matched
    back to its original `ContentId` — never to a new one — using the retained
    `Payload` fingerprint, and its metadata, favorites and statistics are still
    attached (§19.3). The test asserts the whole path: retained identity, retained
    evidence, rescan match, stable `GameId`/`ReleaseId`/`ContentId`.
29. A library rebuild removes **no** row from `games`, `releases`, `contents`,
    `manual_overrides`, `sessions`, `save_states`, `media_assets`,
    `media_asset_references`, `content_fingerprints` (`Payload`),
    `library_sources`, `systems`, `cores`, `core_versions`, `runtime_versions`,
    `core_selection_overrides`, `retroarch_setting_overrides`,
    `core_option_overrides`, `core_option_schemas` or `core_option_definitions`,
    and touches no path outside BitArchive's own data area (§19.3).
30. Only the **full reset** removes `games`, `releases`, `contents`, `sessions` or
    `save_states` rows, and it removes no file (§19.3).
31. **The recognition key and the lookup key are the same columns** (§7.1): the
    partial unique index covers exactly `(algorithm, digest)` for
    `fingerprint_kind = 'Payload'`, no nullable column is part of it, and a second
    `Payload` row with the same digest is refused. Assert **0 or 1** matching
    `ContentId` for any digest — never 2.
32. **Language-neutral metadata is relationally unique, and a real `und` is a
    different locale** (§9.3, §9.4, §3.5). Four assertions, all required:
    - two `provider_values` rows, or two `manual_overrides` rows, identical in
      every fachliche respect **including `locale IS NULL`**, are refused;
    - two rows identical except for a locale tag are accepted, because they are
      two locales;
    - a **language-neutral** row and a row tagged **`und`** coexist: neither
      collides with the other, and neither is rewritten into the other on read.
      A genuine BCP-47 `und` value MUST survive a store/read round trip *as
      `und`*;
    - `en-US` and `en-us` collide (one locale, not two), because the indexes
      compare `locale` with `COLLATE NOCASE` and the write path canonicalises.
33. **`content_fingerprints.entry_path` is populated for exactly one kind** (§7.1):
    a `Payload` or `Container` row with an entry path is refused, an `EntryList`
    row without one is refused, and two `EntryList` rows may share a digest as long
    as their `(content_id, entry_path)` differs.
34. **The same entry bytes may occur in several archives, and one archive may carry
    several `EntryList` fingerprints** (§7.1): two contents in two different
    containers may each carry an `EntryList` row with the same digest, since no
    global uniqueness applies to `EntryList`. And an archive may hold **N**
    `EntryList` rows — one per physical entry — in *addition to* its single imported
    playable `ArchiveEntry` location: the `EntryList` key stays
    `(content_id, fingerprint_kind, algorithm, entry_path)`, so no constraint reduces
    the fingerprint record of an archive to one row, and an ambiguous ZIP that is
    *not* imported still keeps its full multi-row `EntryList` for diagnosis
    (§6.3). The distinction the test asserts:

    ```text
    archive may contain N physical entries      → yes
    EntryList may store N fingerprints          → yes
    imported playable ArchiveEntry locations    → at most 1 (§5.2)
    ```
35. **`media_assets` declares no natural-key unique constraint** (§10.1): two asset
    rows may share a digest, a provider or a locale, and slot uniqueness is
    enforced by `media_asset_references` alone.
36. **The documented full-reset order executes with `foreign_keys=ON`** (§19.3):
    seeding one referencing row per declared edge and deleting in the documented
    order completes without a foreign-key error, with no pointer pre-clearing.
    Deleting `games` before `scrape_run_items`/`scrape_runs` MUST fail — that is
    the regression this test guards.
37. **No key depends on `NULL` semantics** (§3.5, §3.8): every table with a
    composite primary key or unique constraint either declares all key columns
    `NOT NULL`, or carries a check constraint / partial predicate that proves the
    key component is non-`NULL` for the rows it covers.
38. **Every table has exactly one implementable storage primary key** (§3.9): the
    schema can be created, and no table declares two primary keys, no primary key
    contains a nullable column, and no primary key is partial. `content_locations`,
    `content_fingerprints`, `provider_values`, `manual_overrides`,
    `core_selection_overrides`, `retroarch_setting_overrides` and
    `core_option_overrides` each have a `row_id INTEGER PRIMARY KEY`, and the
    schema-v1 migration creates them with it.
39. **No partial index is ever used as a primary key or as a foreign-key target**
    (§3.9): attempting to declare a foreign key whose parent key is one of the
    partial indexes of §20.1 fails with "foreign key mismatch". Assert that every
    declared foreign key's parent key is a storage primary key or a non-partial
    business unique key, and that the schema opens with `PRAGMA foreign_keys = ON`
    and every constraint is accepted.
40. **`ArchiveEntry` launch resolution matches `ARCHITECTURE.md` §14.2** (§5.4):
    the launch contract names a container `ContentId` and a playable `ContentId`,
    with **no entry identity**; resolution reaches the physical file through the
    `ArchiveEntry` location's `archive_content_id` → the container's `File`
    locations, and never by matching a path. Assert that with two `Present` file
    locations for one container the resolver returns the same, deterministic
    choice on repeated calls, and that with none it reports `ContentUnavailable`
    instead of guessing.
41. **A library rebuild keeps `contents` everywhere it is described** (§19.3,
    §19.2): the rebuild deletes no row from `games`, `releases` or `contents`, and
    no section of this document states otherwise. Test 29 asserts the table set;
    this test asserts the *statement*, so that a section drifting back to
    "a rebuild forgets the content identities" fails.
42. **`ArchiveEntry { archive, content }` matches 0 or 1 row, never 2** (§5.2, §5.4):
    a lookup on `archive_content_id = archive AND content_id = content` returns at
    most one row by the partial unique index, so its `archive_entry_path` is *read*
    rather than selected from a set. The required assertions are the whole case
    analysis from §5.4:

    - the pair matches **one** row for an imported entry → that row's
      `archive_entry_path` is the launch entry path;
    - the pair matches **zero** rows → the resolver reports the readiness result of
      §5.4 rule 4 (`ContentUnavailable`, or `SourceOffline` when the only container
      locations belong to unavailable sources) and never guesses;
    - the pair can match **two** rows only if the schema lost its uniqueness
      constraint, so the test asserts the schema makes `2+` impossible **and** that
      no resolver implementation contains a secondary ordering over entry paths.
      An `archive_entry_path` tie-break is not a permitted fallback for a missing
      constraint: the constraint is the rule, and a resolver that sorts entry paths
      is failing this test rather than handling a case.
43. **A source removal deletes the removed source's `File` locations and nothing
    else** (§19.2). Four assertions, all required:
    - removing source X deletes the `content_locations` rows with
      `location_kind = 'File' AND source_id = X`;
    - removing source X deletes **no** `ArchiveEntry` location — not one, not even
      for a container that had all of its `File` locations under X;
    - if an `ArchiveContainer` still has another `File` location (another source or
      another path), its `ArchiveEntry` location stays reachable, unchanged;
    - if its **final** `File` location disappears, the `ArchiveEntry` row
      **remains** and becomes unreachable / not found through normal
      reconciliation: the row is still there, readiness reports it as unavailable,
      and both `ContentId`s survive untouched. Re-adding the source makes the same
      entry reachable again with no identity having moved.
    The test also asserts that no other table loses a row: `games`, `releases`,
    `contents`, `manual_overrides`, `provider_values`, `sessions`, `save_states`
    and `media_assets` are unchanged, and no external file is touched.
44. **One playable payload may be loose and inside several archives at once**
    (§5.2): the same payload digest observed as `source X / loose.gba`, as
    `container A / ROMS/loose.gba` and as `container B / ROMS/loose.gba` is **one**
    content with **three** location rows — one `File` and two `ArchiveEntry` — and
    not three contents. Removing one of the container `File` locations leaves both
    `ArchiveEntry` rows valid (§19.2 rule 3); removing the container's last `File`
    location leaves the rows present but unreachable.
45. **Save-state addressing is technical and slot-based** (§15.1): a discovered
    file's slot is stored as the integer RetroArch uses — including `-1` for the
    `Auto` state — and a file whose name encodes no slot stores `slot IS NULL`
    rather than a substituted token. Discovery is idempotent: indexing the same
    `file_relative_path` twice is refused, while the same slot observed at two
    paths (a moved file) is **accepted** as two rows, because the file is the
    physical identity and the compatibility dimensions are not keys. There is no
    `state_name`, no sentinel, and no `UNIQUE` constraint over
    `(release, core, version, slot)`.
46. **`content_locations` retention operations remain distinct** (§19.2, §19.3,
    §19.4). Test 43 asserts the source-removal case and test 29 the rebuild table;
    this test asserts that the known cases stay separate, because merging any two of
    them is the defect that has to fail:

    | Operation | Effect on `content_locations` |
    |---|---|
    | an ordinary missing or unreachable observation | **no row is deleted** — the row stays and its `state` changes (`Missing`, unreachable, "not found") |
    | a source removal | deletes the `File` rows with `source_id = X` and **no** `ArchiveEntry` row |
    | a library rebuild | clears **all** `content_locations` rows and re-scans |
    | a full reset | removes `content_locations` with the rest of the BitArchive-owned database state |
    | the per-game "Indexeintrag zurücksetzen" | **no semantics fixed here** — its exact deletion scope belongs to #53 (§19.3) |

    Assert the four decided cases: a reconciliation pass that marks a location
    `Missing` leaves the row count unchanged; the source removal of X deletes
    exactly its `File` rows; the rebuild's clear leaves the table empty while every
    `GameId`/`ReleaseId`/`ContentId` and every `Payload` fingerprint survives; and
    the full reset drops `content_locations` with the rest of the state while
    touching no external file. In particular, **no** operation may delete an
    `ArchiveEntry` location except the library rebuild's whole-index clear and the
    full reset's total clear, and no passage may claim that the source removal is
    the only operation that deletes a location. The test fixes **no** total number
    of deletion operations: the per-game index reset is undecided until #53, so a
    passage asserting such a total fails this test rather than being enforced by it.

#### Structural checks that run against this document

Tests 1–46 above are assertions schema v1 and the repository ports must satisfy at
run time. A subset of the *document's own* consistency is checkable statically, and
it is checked by a script in the repository so that the review findings that
motivated it — a foreign key naming a column its own table does not own, a
partial index labelled as a primary key, a sentinel colliding with a real locale, a
retention rule contradicted by its own section — cannot recur silently:

```bash
python3 tools/check_data_model.py
```

The script verifies, against `DATA_MODEL.md` alone:

| Check | What it catches |
|---|---|
| every FK child table and **child column** exists | the round-2 defect: `save_states` declaring `runtime_version_id` |
| every FK parent table, parent column and parent key exists | a foreign key into a non-unique or non-existent target |
| **every FK parent key is a primary key or a full `UNIQUE` key** | a foreign key whose parent is only a **partial** unique index, which SQLite rejects with "foreign key mismatch" |
| **every table declares exactly one storage primary key**, and no declaration calls a partial index a primary key | the round-4 defect: three partial indexes labelled "Primary key" in §12.3, §13.1, §14.1 and §7.1 |
| **every declared key is classified** in §3.8, and no key component is nullable without a predicate or a split | a nullable key component silently disabling a constraint |
| **no locale sentinel exists** and no valid BCP-47 tag stands in for `NULL` | the round-4 defect: mapping a language-neutral locale to the real tag `und`, collapsing the two |
| the `content_locations` shape check pins both variants in **both** directions | an `ArchiveEntry` that also carries `source_id`/`relative_path` |
| **no section claims a library rebuild deletes `games`, `releases` or `contents`** | the round-4 defect: §19.2 calling a rebuild "forget everything" |
| **the `ArchiveEntry` location key is exactly `UNIQUE (archive_content_id)` over its predicate**, and no key over `archive_entry_path` exists | the round-5 defect: `UNIQUE (archive_content_id, archive_entry_path)` admitting two playable entry rows for one ZIP, so `LaunchContent::ArchiveEntry { archive, content }` could resolve to 2 rows |
| **no section orders several `archive_entry_path` values, and no entry-path tie-break is described as a legitimate state** | the round-5 defect: §5.4 resolving an impossible ambiguity by "smallest `archive_entry_path`" instead of relying on the constraint |
| **§19.2's binding source-removal rule deletes `File` locations only and forbids deleting an `ArchiveEntry` location** | the round-5 defect: the same section both deleting `content_locations` of the source and stating that a source removal MUST NOT delete a `content_locations` row |
| **the `EntryList` fingerprint key still contains `entry_path`**, so an archive's fingerprint record is not reduced to one row | the round-5 near-miss: copying the location rule onto `content_fingerprints` and allowing only one `EntryList` per archive |
| **§19.4 separates the retention cases and claims no closed total of the operations that may delete a `content_locations` row** — an ordinary missing/unreachable observation deletes nothing, a source removal deletes the removed source's `File` rows only and never an `ArchiveEntry` row, a library rebuild clears all rows, a full reset removes the table with the rest of the state, and the per-game index reset is delegated to #53 | the round-6 defect: §19.4 calling the source removal "the only deletion that touches `content_locations` at all", which contradicts the library rebuild; and the round-7 defect: §19.4 closing the set in the other direction, while §19.3's full reset deletes the same table |
| **no passage lets a missing, unreachable or offline observation delete a `content_locations` row** | the round-6 near-miss: turning "not found" into a deletion |
| **`ARCHITECTURE.md` contains no `ArchiveEntryId`** and its `LaunchContent` matches this document | a source-of-truth contradiction surviving a change |
| every key is declared as a `UNIQUE` tuple, not as free prose | a constraint that no `CREATE TABLE` could be derived from |
| every PK/UNIQUE key column is a defined column | a unique constraint on a column nobody declared |
| no persistent table references a rebuildable table | the §12.1 ownership rule |
| every declared lifecycle names exactly one lifetime | a structure labelled `Persistent, and Rebuildable` |
| every run reference is compatible with run retention | a `RESTRICT` that makes pruning impossible |
| a multi-row derivation can hold 2..N members | a primary key that admits only one member |
| the library-rebuild table clears and keeps the stated sets | rebuild semantics drifting from the retention sections |
| the re-identification path is present | the recognition evidence being dropped from §19.3 |
| §20.1 agrees with every per-table section | the checklist and the sections drifting apart |
| markdown structure, and the `ARCHITECTURE.md` §54 areas and §53 invariants | a missing traceability row or a broken table |

It is a documentation check, not a test of code: it reads this file and asserts the
model is internally consistent. It runs in the same place as the formatting and
lint checks a reviewer already runs — and, since this round, **in the same CI
workflow as those checks** (`Rust quality` runs `python3 tools/check_data_model.py`
as its own step, `.github/workflows/ci.yml`), so the document cannot drift on a
branch whose CI is green.

**The round-5, round-6 and round-7 rules were verified to fail on their own
mutations.** Each was applied to a scratch copy and reverted, never committed:

| Mutation | Expected |
|---|---|
| §19.2 forbids deleting a `content_locations` row again while still deleting the source's `File` rows | FAIL |
| the entry-path component is put back into the `ArchiveEntry` key — in §5.2, in §20.1 alone, or in §3.8 alone | FAIL |
| §5.4 lets one `(archive, content)` pair own several entry paths again and resolves them by ordering the paths | FAIL |
| the `EntryList` key loses `entry_path`, so an archive could hold only one fingerprint | FAIL |
| §19.4 calls the source removal the only deletion that can ever affect `content_locations`, excluding the library rebuild | FAIL |
| §19.4 says a missing or unreachable observation may delete an `ArchiveEntry` row | FAIL |
| §19.4 restores the round-7 sentence closing the set with "exactly those two explicit operations, and by no third one" | FAIL |
| §19.4 drops, or contradicts, the statement that a full reset removes `content_locations` | FAIL |
| §19.4 claims a fixed total of the operations that delete a `content_locations` row | FAIL |
| §19.4 says a missing or unreachable observation deletes its `content_locations` row | FAIL |
| unmutated document (positive control) | PASS |

### 20.4 Open points this document deliberately does not decide

These are product or cross-cutting questions owned elsewhere. They are recorded
here with their status so that schema v1 does not silently resolve them.

| Point | Status | Owner |
|---|---|---|
| Which reviewed reference digests BitArchive may ship and compare local firmware against — **not** whether the local digest is computed | **OPEN** — explicitly deferred | #89 (`needs-decision`); §11.1 separates the two |
| How `previous` is represented for the one-step rollback of `ARCHITECTURE.md` §23.3 | **OPEN** — no document defines a record, a column or a derivation for it | The runtime/core update Issue; §12.6 deliberately stores no `is_active`/`is_previous` flag |
| Support for archive formats beyond `.zip` | **DECIDED out of MVP** | `PRODUCT.md` §10, §43 |
| `JobManager` job persistence and job recovery | **OPEN** | The Issue implementing job recovery; §8.3 and §16.4 keep it out of scope here |
| Core option introspection mechanism (libretro host vs. RetroArch introspection) | **OPEN, internally interchangeable** | `ARCHITECTURE.md` §27.2 |
| Controller/input mapping persistence | **OPEN** | `PRODUCT.md` §22 keeps full remapping out of MVP |
| Whether `active_duration_ms` is ever distinct from `duration_ms` | **FAVORED** — the column exists so that a later session implementation can distinguish foreground time from wall clock without a migration | The session lifecycle Issue; not a product decision, and no structure depends on it |
| Cloud sync, accounts, RetroAchievements and any second device | **DECIDED out of MVP** | `ARCHITECTURE.md` §51 |

**Rule (binding).** No structure in this document may depend on an **OPEN**
point. Where an open point reaches the schema, the model stores the observation
(the `NULL`-able digest, the unbound version label) rather than a verdict.

**Note on the `CoreDefaults` scope.** §14.1 persists a `CoreDefaults` core-option
scope because `PRODUCT.md` §21.4 and `ARCHITECTURE.md` §27 fix the hierarchy as
`Core Defaults → System → Game`. Issue #38 currently lists stored core-option
overrides only for `System` and `Game`. That is a **backlog inconsistency in #38**,
not a defect in this model: the source documents are unambiguous, the hierarchy is
implemented here as specified, and #38 should be aligned after this document is
merged. #33 does not change another Issue's text.

### 20.5 Deliberate choice of scope key types

Three anchors appear in the source documents for "which system" and "which core",
and this document had to pick one per use. The choices are recorded here because
they are the kind of detail schema v1 would otherwise re-decide:

| Reference | Chosen key | Reason |
|---|---|---|
| A release's system (`releases.system_id`) | `SystemId` (UUIDv7) | `ARCHITECTURE.md` §7 makes it a domain identity, and a release is persisted fachliche data. `bitarchive-domain::system` records the same gap: "turning a system key into a release's `SystemId` is a persistence question". |
| A library source's fixed system (`library_sources.fixed_system_id`) | `SystemId` (UUIDv7) | `ARCHITECTURE.md` §10.1 declares `fixed_system: Option<SystemId>`. |
| A configuration or core-selection scope anchor | `SystemId` (UUIDv7) | All three scopes must resolve to the same stable anchor a release carries, or a system-scoped setting could not be found from a release. |
| The curated catalogue's naming of a system | `catalog_key` slug in `systems` | ADR 0004 §7 is explicit that the slug is not a `SystemId`; the slug is the bridge, not the identity. |
| A selected core (`core_selection_overrides.core_id`) | `CoreId` (UUIDv7) | ADR 0002 §7: `CoreId` answers "which core did the user configure for this game". |
| The distributable core a `CoreId` binds to (`cores.component_key`) | distribution slug | ADR 0002 §7: `CoreComponentId` (`mgba`) answers "which distributable thing does BitArchive install". The two are different types with no conversion, so the model stores both and lets the write path bind them. |

---

## Appendix A — Traceability to `ARCHITECTURE.md` §54

The first column repeats the §54 bullet **verbatim** (they are German in
`ARCHITECTURE.md`) so that the mapping is checkable rather than paraphrased.

| `ARCHITECTURE.md` §54 area | This document |
|---|---|
| Tabellen | §20.1, and each table's own section |
| Foreign Keys | §5.2, §6.1–§6.6, §7.1, §8, §9, §10, §11, §12, §13.1, §14.1, §15.1, §16.1 |
| Unique Constraints | §3.5, §3.8, §3.9, and every table's business-uniqueness list |
| Game-/Release-/Content-Beziehungen | §6, §6.7 |
| Provider Metadata | §9.3, §9.7 |
| Manual Overrides | §9.4, §9.5 |
| FTS5 | §17 |
| Scan Runs | §8 |
| Scrape Runs | §9.6 |
| Firmware Index | §11 |
| Components | §12 |
| Config Overrides | §13 |
| Core Option Schemas | §14.2 |
| Save States | §15 |
| Sessions | §16.1 |
| Statistics | §16.3 |
| Media Assets | §10 |
| Schema-Versionierung | §18 |

## Appendix B — Traceability to `ARCHITECTURE.md` §53 invariants

| Invariant | Where this document protects it |
|---|---|
| 1. `Game != Release != Content` | §2.1, §6 |
| 2. Paths are not domain IDs | §3.4, §5.2, §6.3 (`archive_content_id`), §6.7 |
| 3. Hashes are not primary keys | §3.4, §6.2, §7.1 (SHA-256 only, fixed 32 bytes) |
| 4. ROMs/BIOS are external resources | §2.3, §11.1, §19 |
| 5. Slint calls no infrastructure | not a persistence concern; no table stores presentation state (§16.4) |
| 6. Domain knows no SQLite/Slint/OS types | §1.4, §3.2 (storage types are a persistence concern, not a domain one) |
| 7. Generated config is never source of truth | §13.5, §16.2, §19.8 |
| 8. Emulation stays backend-neutral | §16.1 stores `runtime_version` as a fact, not a backend type |
| 9. Firmware requirements belong to the core | §11.1 (no firmware-requirement table; requirements are curated per core) |
| 10. Runtime and cores are separately versioned | §12.3, §12.4, §12.5, §12.6 (`component_class`) |
| 11. App updates never update runtime/cores implicitly | §12.4, §12.5, §12.7 |
| 12. Save states belong to Release + Core + Core Version | §12.4 (persistent anchor), §15.1, §15.3 |
| 13. Normal saves stay outside the model | §2.1, §15.1, §16.4 |
| 14. Scraping is not library scanning | §8, §9.5, §9.7 |
| 15. Network belongs to explicit subsystems | §9.7 |
| 16. No telemetry or crash upload | §16.4 |
| 17. Background tasks never block the UI | not a persistence concern |
| 18. Destructive reconciliation only after a successful scan | §8.1, §19.1, §19.3, §19.4 |
| 19. Invalid user overrides are not silently deleted | §9.4, §13.3, §14.5, §19.1 |
| 20. Internal cleanup never deletes external user files | §4.3, §19.1, §19.2, §19.3, §19.8 |
| 21. Core resolution `System → Game → optional Release`, no global default | §12.3 |
| 22. Focus uses stable IDs, not list indices | §3.1, §17.2 |
| 23. No app-navigation input during a foreground session | §16.1 records the active session that makes the rule enforceable |
