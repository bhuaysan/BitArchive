-- BitArchive schema v1 — the initial SQLite structure.
--
-- This is migration 1 of the forward-only catalogue the migration runner applies
-- (`crates/bitarchive-infrastructure/src/database/migrations.rs`), and it is the
-- whole of schema v1: every persisted object `DATA_MODEL.md` §20.1 lists, and
-- nothing else. The statements are derived from that document and are not a
-- second design — where this file and the model disagree, the model wins and this
-- file is wrong.
--
-- The conventions below are the model's, not SQLite's defaults:
--
--   * a UUIDv7 identity is a 16-byte `BLOB` (§3.2), never `TEXT`, and never
--     replaced by a row number;
--   * a SHA-256 digest is a 32-byte `BLOB`, never hex text, and is never a key of
--     identity — it is a fingerprint (§3.4, §7.1);
--   * a timestamp is `INTEGER` milliseconds since the Unix epoch, UTC; a duration
--     is `INTEGER` milliseconds (§3.2);
--   * an enum is `TEXT` spelled exactly as the document spells its variants, and
--     a boolean is `INTEGER` `0`/`1`, both pinned by a check constraint (§3.2);
--   * a locale is `TEXT NULL`, `NULL` meaning language-neutral, with no sentinel
--     and no tag standing in for it (§3.5);
--   * every key column is `NOT NULL`, or the key is a partial unique index whose
--     predicate proves the column is non-NULL for the rows it covers (§3.5);
--   * every foreign key states its delete behaviour explicitly (§3.7).
--
-- There is deliberately no `CREATE TABLE IF NOT EXISTS` here. A numbered migration
-- runs exactly once and is skipped through the ledger, so a conditional create
-- would only hide a real error behind a silent no-op (the runner's own ledger
-- creation is the exception, and it is not a numbered migration).
--
-- No rows are inserted. Schema v1 is structure: the curated `metadata_fields` and
-- `metadata_providers` sets are curated app data (§9.2, §9.7), and every other
-- table is filled by the subsystem that owns it. The single row this migration
-- causes is the ledger row the runner writes next to it, in the same transaction.

-- ---------------------------------------------------------------------------
-- §6.5 systems — the stable identity of a curated system.
-- ---------------------------------------------------------------------------
CREATE TABLE systems (
    system_id   BLOB    NOT NULL PRIMARY KEY,
    catalog_key TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,

    UNIQUE (catalog_key)
);

-- ---------------------------------------------------------------------------
-- §5.1 library_sources — one configured place BitArchive looks for content.
-- ---------------------------------------------------------------------------
CREATE TABLE library_sources (
    id                    BLOB    NOT NULL PRIMARY KEY,
    display_name          TEXT    NOT NULL,
    location              TEXT    NOT NULL,
    -- `Path` is the only locator kind schema v1 has (§5.1). A later Android build
    -- adds `DocumentTree` through a migration, which is what a closed enum column
    -- means here: the value set is part of the schema, not of a convention.
    platform_locator_kind TEXT    NOT NULL CHECK (platform_locator_kind IN ('Path')),
    fixed_system_id       BLOB    NULL REFERENCES systems (system_id) ON DELETE RESTRICT,
    availability          TEXT    NOT NULL
                                  CHECK (availability IN ('Available', 'Offline', 'PermissionDenied', 'Missing')),
    availability_checked_at INTEGER NULL,
    created_at            INTEGER NOT NULL,
    removed_at            INTEGER NULL,

    UNIQUE (location, platform_locator_kind)
);

CREATE INDEX library_sources_availability ON library_sources (availability);
CREATE INDEX library_sources_removed_at   ON library_sources (removed_at);

-- ---------------------------------------------------------------------------
-- §8.1 scan_runs — a scan as a fachliche event with a history.
-- ---------------------------------------------------------------------------
CREATE TABLE scan_runs (
    id                     BLOB    NOT NULL PRIMARY KEY,
    target_kind            TEXT    NOT NULL
                                   CHECK (target_kind IN ('FullLibrary', 'LibrarySource', 'Rebuild')),
    target_source_id       BLOB    NULL REFERENCES library_sources (id) ON DELETE RESTRICT,
    status                 TEXT    NOT NULL
                                   CHECK (status IN ('Running', 'Completed', 'Cancelled', 'Failed')),
    started_at             INTEGER NOT NULL,
    finished_at            INTEGER NULL,
    reconciliation_eligible INTEGER NOT NULL DEFAULT 0 CHECK (reconciliation_eligible IN (0, 1)),
    discovered_count       INTEGER NOT NULL,
    imported_count         INTEGER NOT NULL,
    updated_count          INTEGER NOT NULL,
    rediscovered_count     INTEGER NOT NULL,
    missing_count          INTEGER NOT NULL,
    unknown_count          INTEGER NOT NULL,
    unsupported_count      INTEGER NOT NULL,
    problem_count          INTEGER NOT NULL,

    -- The source is named for exactly one target kind and for no other (§8.1).
    CHECK ((target_kind = 'LibrarySource') = (target_source_id IS NOT NULL))
);

CREATE INDEX scan_runs_started_at             ON scan_runs (started_at);
CREATE INDEX scan_runs_target_source_id_started_at ON scan_runs (target_source_id, started_at);
CREATE INDEX scan_runs_status                 ON scan_runs (status);

-- ---------------------------------------------------------------------------
-- §8.2 scan_run_issues — per-item problems and the unknown/unsupported inventory.
-- ---------------------------------------------------------------------------
CREATE TABLE scan_run_issues (
    scan_run_id            BLOB    NOT NULL REFERENCES scan_runs (id) ON DELETE CASCADE,
    issue_index            INTEGER NOT NULL,
    kind                   TEXT    NOT NULL
                                   CHECK (kind IN ('UnknownFile', 'UnsupportedFile', 'Unreadable',
                                                   'PermissionDenied', 'InvalidArchive',
                                                   'AmbiguousArchive', 'SourceOffline')),
    source_id              BLOB    NULL REFERENCES library_sources (id) ON DELETE RESTRICT,
    relative_path          TEXT    NULL,
    detail                 TEXT    NULL,
    byte_size              INTEGER NULL CHECK (byte_size IS NULL OR byte_size >= 0),
    prevents_reconciliation INTEGER NOT NULL CHECK (prevents_reconciliation IN (0, 1)),

    PRIMARY KEY (scan_run_id, issue_index)
);

CREATE INDEX scan_run_issues_scan_run_id_kind           ON scan_run_issues (scan_run_id, kind);
CREATE INDEX scan_run_issues_prevents_reconciliation    ON scan_run_issues (prevents_reconciliation);

-- ---------------------------------------------------------------------------
-- §6.1 games — the logical work.
--
-- `games.default_release_id` and `releases.game_id` point at each other, so one of
-- the two tables has to be created before the table it references. SQLite resolves
-- a foreign key when it is used, not when it is declared, so this forward
-- reference is sound; both tables exist before any row is written (§20.2 orders
-- the rest).
-- ---------------------------------------------------------------------------
CREATE TABLE games (
    id                 BLOB    NOT NULL PRIMARY KEY,
    first_seen_at      INTEGER NOT NULL,
    -- The user's explicit default release override, and nothing else: a scan never
    -- writes it, and `NULL` is the documented "reset to automatic" state (§6.1).
    default_release_id BLOB    NULL REFERENCES releases (id) ON DELETE SET NULL,
    is_favorite        INTEGER NOT NULL DEFAULT 0 CHECK (is_favorite IN (0, 1)),
    is_hidden          INTEGER NOT NULL CHECK (is_hidden IN (0, 1)),
    is_ignored         INTEGER NOT NULL CHECK (is_ignored IN (0, 1))
);

CREATE INDEX games_is_favorite        ON games (is_favorite);
CREATE INDEX games_is_hidden_is_ignored ON games (is_hidden, is_ignored);

-- ---------------------------------------------------------------------------
-- §6.2 releases — a concrete publication or variant of a game.
-- ---------------------------------------------------------------------------
CREATE TABLE releases (
    id           BLOB    NOT NULL PRIMARY KEY,
    game_id      BLOB    NOT NULL REFERENCES games (id) ON DELETE RESTRICT,
    system_id    BLOB    NOT NULL REFERENCES systems (system_id) ON DELETE RESTRICT,
    release_title TEXT   NULL,
    -- A partial date is stored as a prefix (`1996`, `1996-03`); the column is text
    -- on purpose (§6.2).
    release_date TEXT    NULL,
    revision     TEXT    NULL,
    release_type TEXT    NOT NULL DEFAULT 'Official'
                         CHECK (release_type IN ('Official', 'RomHack', 'FanTranslation',
                                                 'Homebrew', 'Prototype', 'Unlicensed')),
    -- The normalized fachliche release identity as a digest computed by the
    -- resolver. It is a key of uniqueness and a fingerprint, never a primary key
    -- and never an identity (§6.2, invariant 3).
    release_key  BLOB    NOT NULL,
    archive_kind TEXT    NULL CHECK (archive_kind IS NULL OR archive_kind = 'Zip'),
    created_at   INTEGER NOT NULL,

    UNIQUE (game_id, release_key),
    -- Redundant as a business rule, required as a key: it is the target of
    -- `contents (release_id, game_id)`, which is what makes "a content's release
    -- belongs to the content's game" a database fact (§6.2, §6.3, §3.9 rule 4).
    UNIQUE (id, game_id)
);

CREATE INDEX releases_game_id                 ON releases (game_id);
CREATE INDEX releases_system_id               ON releases (system_id);
CREATE INDEX releases_system_id_release_type  ON releases (system_id, release_type);
CREATE INDEX releases_release_date            ON releases (release_date);

-- ---------------------------------------------------------------------------
-- §6.4 release_regions, release_languages — the multi-valued tags of a release.
-- ---------------------------------------------------------------------------
CREATE TABLE release_regions (
    release_id BLOB NOT NULL REFERENCES releases (id) ON DELETE CASCADE,
    region     TEXT NOT NULL
                      CHECK (region IN ('Europe', 'Usa', 'Japan', 'World', 'Asia', 'Unknown')),

    PRIMARY KEY (release_id, region)
);

CREATE INDEX release_regions_region ON release_regions (region);

CREATE TABLE release_languages (
    release_id BLOB NOT NULL REFERENCES releases (id) ON DELETE CASCADE,
    -- A BCP-47 primary subtag in RFC 5646 canonical case (§3.2).
    language   TEXT NOT NULL,

    PRIMARY KEY (release_id, language)
);

CREATE INDEX release_languages_language ON release_languages (language);

-- ---------------------------------------------------------------------------
-- §6.3 contents — one concrete technical content unit of a release.
-- ---------------------------------------------------------------------------
CREATE TABLE contents (
    id               BLOB    NOT NULL PRIMARY KEY,
    release_id       BLOB    NOT NULL,
    -- Redundant by design: the pair is what the composite foreign key below
    -- checks (§6.3).
    game_id          BLOB    NOT NULL,
    content_kind     TEXT    NOT NULL
                             CHECK (content_kind IN ('Content', 'ArchiveContainer', 'Playlist')),
    format           TEXT    NULL,
    validation_state TEXT    NOT NULL
                             CHECK (validation_state IN ('Unvalidated', 'Valid', 'Suspect', 'Invalid')),
    validation_detail TEXT   NULL,
    disc_index       INTEGER NULL CHECK (disc_index IS NULL OR disc_index >= 1),
    created_at       INTEGER NOT NULL,

    FOREIGN KEY (release_id, game_id) REFERENCES releases (id, game_id) ON DELETE RESTRICT
);

CREATE INDEX contents_release_id       ON contents (release_id);
CREATE INDEX contents_game_id          ON contents (game_id);
CREATE INDEX contents_validation_state ON contents (validation_state);

-- ---------------------------------------------------------------------------
-- §5.2 content_locations — where one content was actually observed.
--
-- One of exactly two shapes, mirroring ARCHITECTURE.md §14.1's sum type, and the
-- check constraint is what makes them exclusive: a `File` location names a
-- physical file, an `ArchiveEntry` location names an entry inside a container and
-- carries no source and no path of its own.
-- ---------------------------------------------------------------------------
CREATE TABLE content_locations (
    row_id                INTEGER NOT NULL PRIMARY KEY,
    content_id            BLOB    NOT NULL REFERENCES contents (id) ON DELETE RESTRICT,
    location_kind         TEXT    NOT NULL CHECK (location_kind IN ('File', 'ArchiveEntry')),
    source_id             BLOB    NULL REFERENCES library_sources (id) ON DELETE RESTRICT,
    relative_path         TEXT    NULL,
    archive_content_id    BLOB    NULL REFERENCES contents (id) ON DELETE RESTRICT,
    archive_entry_path    TEXT    NULL,
    observed_filename     TEXT    NOT NULL,
    observed_size         INTEGER NULL CHECK (observed_size IS NULL OR observed_size >= 0),
    observed_mtime        INTEGER NULL,
    state                 TEXT    NOT NULL
                                  CHECK (state IN ('Present', 'Missing', 'Unreadable', 'PermissionDenied')),
    last_seen_scan_run_id BLOB    NULL REFERENCES scan_runs (id) ON DELETE SET NULL,
    first_seen_at         INTEGER NOT NULL,
    last_seen_at          INTEGER NULL,

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
);

-- One physical file belongs to one content. The predicate pins both key columns
-- and the check above forbids the archive columns for every row it covers (§3.5).
CREATE UNIQUE INDEX content_locations_source_id_relative_path_file
    ON content_locations (source_id, relative_path)
    WHERE location_kind = 'File';

-- One supported archive holds at most one imported playable entry (§5.2, §6.3).
-- The entry path is deliberately not a key component: a second entry row for one
-- container is refused whatever its content and whatever its entry path.
CREATE UNIQUE INDEX content_locations_archive_content_id_archive_entry
    ON content_locations (archive_content_id)
    WHERE location_kind = 'ArchiveEntry';

CREATE INDEX content_locations_source_id_file
    ON content_locations (source_id)
    WHERE location_kind = 'File';
CREATE INDEX content_locations_content_id ON content_locations (content_id);
CREATE INDEX content_locations_state      ON content_locations (state);

-- ---------------------------------------------------------------------------
-- §6.6 content_derivations — a generated artifact BitArchive owns (the managed
-- multi-disc playlist).
-- ---------------------------------------------------------------------------
CREATE TABLE content_derivations (
    content_id   BLOB    NOT NULL REFERENCES contents (id) ON DELETE CASCADE,
    kind         TEXT    NOT NULL CHECK (kind IN ('ManagedPlaylist')),
    relative_path TEXT   NOT NULL,
    member_count INTEGER NOT NULL CHECK (member_count >= 2),
    created_at   INTEGER NOT NULL,

    PRIMARY KEY (content_id, kind)
);

CREATE INDEX content_derivations_kind ON content_derivations (kind);

-- ---------------------------------------------------------------------------
-- §6.6 content_derivation_members — the ordered members of a derivation.
-- ---------------------------------------------------------------------------
CREATE TABLE content_derivation_members (
    content_id        BLOB    NOT NULL,
    kind              TEXT    NOT NULL CHECK (kind IN ('ManagedPlaylist')),
    source_content_id BLOB    NOT NULL REFERENCES contents (id) ON DELETE RESTRICT,
    member_index      INTEGER NOT NULL CHECK (member_index >= 1),

    PRIMARY KEY (content_id, kind, source_content_id),
    -- The primary key alone would let two members claim position 1, which would
    -- make the playlist's order depend on insertion order (§6.6).
    UNIQUE (content_id, kind, member_index),

    FOREIGN KEY (content_id, kind) REFERENCES content_derivations (content_id, kind)
        ON DELETE CASCADE
);

CREATE INDEX content_derivation_members_source_content_id
    ON content_derivation_members (source_content_id);

-- ---------------------------------------------------------------------------
-- §7.1 content_fingerprints — recognition evidence about a content.
--
-- A fingerprint is never an identity: the digest is a value, indexed where it is
-- looked up, and it is never a primary key and never a foreign-key target.
-- ---------------------------------------------------------------------------
CREATE TABLE content_fingerprints (
    row_id             INTEGER NOT NULL PRIMARY KEY,
    content_id         BLOB    NOT NULL REFERENCES contents (id) ON DELETE CASCADE,
    fingerprint_kind   TEXT    NOT NULL
                               CHECK (fingerprint_kind IN ('Payload', 'Container', 'EntryList')),
    -- `Sha256` is the only value schema v1 persists. The column exists so a later
    -- algorithm arrives without a migration, which is why it carries no check
    -- constraint (§7.1).
    algorithm          TEXT    NOT NULL,
    digest             BLOB    NOT NULL,
    entry_path         TEXT    NULL,
    byte_size          INTEGER NULL CHECK (byte_size IS NULL OR byte_size >= 0),
    computed_at        INTEGER NOT NULL,
    source_scan_run_id BLOB    NULL REFERENCES scan_runs (id) ON DELETE SET NULL,

    -- The entry path is populated for exactly one kind, which is what makes the
    -- `EntryList` uniqueness rule below effective (§3.5, §7.1).
    CHECK ((fingerprint_kind = 'EntryList') = (entry_path IS NOT NULL))
);

-- One `Payload` and one `Container` row per content: "the canonical payload
-- fingerprint" is a database fact, not a convention.
CREATE UNIQUE INDEX content_fingerprints_content_id_kind_algorithm_payload_container
    ON content_fingerprints (content_id, fingerprint_kind, algorithm)
    WHERE fingerprint_kind IN ('Payload', 'Container');

-- One `EntryList` row per entry of one content. The predicate is pinned by the
-- check above, so `entry_path` is never NULL for a row this index covers.
CREATE UNIQUE INDEX content_fingerprints_content_id_kind_algorithm_entry_path_entry_list
    ON content_fingerprints (content_id, fingerprint_kind, algorithm, entry_path)
    WHERE fingerprint_kind = 'EntryList';

-- The recognition key, and exactly the lookup key: a payload digest belongs to one
-- content, so "have I seen these bytes before?" is a function (§7.1).
CREATE UNIQUE INDEX content_fingerprints_algorithm_digest_payload
    ON content_fingerprints (algorithm, digest)
    WHERE fingerprint_kind = 'Payload';

CREATE INDEX content_fingerprints_content_id         ON content_fingerprints (content_id);
CREATE INDEX content_fingerprints_source_scan_run_id ON content_fingerprints (source_scan_run_id);

-- ---------------------------------------------------------------------------
-- §9.2 metadata_fields — the closed set of metadata fields.
-- ---------------------------------------------------------------------------
CREATE TABLE metadata_fields (
    field_key       TEXT    NOT NULL PRIMARY KEY,
    value_kind      TEXT    NOT NULL CHECK (value_kind IN ('Text', 'Date', 'Integer', 'TagList')),
    is_multi_valued INTEGER NOT NULL CHECK (is_multi_valued IN (0, 1)),
    is_localizable  INTEGER NOT NULL CHECK (is_localizable IN (0, 1))
);

-- ---------------------------------------------------------------------------
-- §9.7 metadata_providers — the providers BitArchive may use.
--
-- No credential, key or token is ever stored here or anywhere else in the schema:
-- secrets live only in the secret-storage boundary (§9.7).
-- ---------------------------------------------------------------------------
CREATE TABLE metadata_providers (
    provider_id  TEXT    NOT NULL PRIMARY KEY,
    display_name TEXT    NOT NULL,
    is_enabled   INTEGER NOT NULL CHECK (is_enabled IN (0, 1))
);

-- ---------------------------------------------------------------------------
-- §9.6 scrape_runs — a metadata scraping run, resumable per game.
-- ---------------------------------------------------------------------------
CREATE TABLE scrape_runs (
    id                      BLOB    NOT NULL PRIMARY KEY,
    provider_id             TEXT    NOT NULL
                                    REFERENCES metadata_providers (provider_id) ON DELETE RESTRICT,
    mode                    TEXT    NOT NULL CHECK (mode IN ('Automatic', 'Interactive')),
    scope_kind              TEXT    NOT NULL CHECK (scope_kind IN ('Library', 'System', 'Game')),
    scope_system_id         BLOB    NULL REFERENCES systems (system_id) ON DELETE RESTRICT,
    scope_game_id           BLOB    NULL REFERENCES games (id) ON DELETE RESTRICT,
    only_missing            INTEGER NOT NULL CHECK (only_missing IN (0, 1)),
    fields_missing          TEXT    NULL,
    status                  TEXT    NOT NULL
                                    CHECK (status IN ('Queued', 'Running', 'Paused', 'Completed',
                                                      'Cancelled', 'Failed')),
    started_at              INTEGER NOT NULL,
    finished_at             INTEGER NULL,
    updated_count           INTEGER NOT NULL,
    no_match_count          INTEGER NOT NULL,
    multiple_match_count    INTEGER NOT NULL,
    error_count             INTEGER NOT NULL,
    skipped_count           INTEGER NOT NULL,
    override_protected_count INTEGER NOT NULL,

    -- Each scope names exactly the anchor it needs and no other (§9.6).
    CHECK (
        CASE scope_kind
            WHEN 'Library' THEN scope_system_id IS NULL AND scope_game_id IS NULL
            WHEN 'System'  THEN scope_system_id IS NOT NULL AND scope_game_id IS NULL
            WHEN 'Game'    THEN scope_game_id IS NOT NULL AND scope_system_id IS NULL
        END
    )
);

CREATE INDEX scrape_runs_status     ON scrape_runs (status);
CREATE INDEX scrape_runs_started_at ON scrape_runs (started_at);

-- ---------------------------------------------------------------------------
-- §9.6 scrape_run_items — the run's record about one game.
-- ---------------------------------------------------------------------------
CREATE TABLE scrape_run_items (
    scrape_run_id            BLOB    NOT NULL REFERENCES scrape_runs (id) ON DELETE CASCADE,
    game_id                  BLOB    NOT NULL REFERENCES games (id) ON DELETE RESTRICT,
    status                   TEXT    NOT NULL
                                     CHECK (status IN ('Pending', 'Updated', 'NoMatch',
                                                       'MultipleMatches', 'Error', 'Skipped')),
    candidate_count          INTEGER NOT NULL,
    accepted_provider_item_id TEXT   NULL,
    -- Machine-readable, and never a credential (§9.6).
    error_detail             TEXT    NULL,
    updated_at               INTEGER NOT NULL,

    PRIMARY KEY (scrape_run_id, game_id)
);

CREATE INDEX scrape_run_items_scrape_run_id_status ON scrape_run_items (scrape_run_id, status);

-- ---------------------------------------------------------------------------
-- §9.3 provider_values — one value a provider returned, with full provenance.
--
-- The subject reference is polymorphic and therefore not a foreign key; only
-- `Game` and `Release` are ever subjects (§9.3).
-- ---------------------------------------------------------------------------
CREATE TABLE provider_values (
    row_id           INTEGER NOT NULL PRIMARY KEY,
    provider_id      TEXT    NOT NULL,
    subject_kind     TEXT    NOT NULL CHECK (subject_kind IN ('Game', 'Release')),
    subject_id       BLOB    NOT NULL,
    field_key        TEXT    NOT NULL REFERENCES metadata_fields (field_key) ON DELETE RESTRICT,
    -- The fachliche locale: a BCP-47 tag in canonical case, or NULL for
    -- language-neutral. It is part of the uniqueness rules below, and no token
    -- stands in for the absent value (§3.5, §9.3).
    locale           TEXT    NULL,
    value_index      INTEGER NOT NULL CHECK (value_index >= 0),
    value_text       TEXT    NULL,
    value_number     INTEGER NULL,
    provider_item_id TEXT    NULL,
    fetched_at       INTEGER NOT NULL,
    scrape_run_id    BLOB    NULL REFERENCES scrape_runs (id) ON DELETE SET NULL
);

-- The two predicates are exhaustive, so no row escapes both rules, and neither
-- rule has a nullable key component for the rows it covers (§3.8).
--
-- `COLLATE NOCASE` is declared on the index rather than on the column, so
-- `en-US` and `en-us` are one locale even if a writer skips canonicalisation
-- while the stored value stays canonical (§3.5).
CREATE UNIQUE INDEX provider_values_provider_id_subject_id_field_key_value_index_neutral
    ON provider_values (provider_id, subject_kind, subject_id, field_key, value_index)
    WHERE locale IS NULL;

CREATE UNIQUE INDEX provider_values_provider_id_subject_id_field_key_locale_value_index_localised
    ON provider_values (provider_id, subject_kind, subject_id, field_key, locale COLLATE NOCASE, value_index)
    WHERE locale IS NOT NULL;

CREATE INDEX provider_values_subject_kind_subject_id_field_key
    ON provider_values (subject_kind, subject_id, field_key);
CREATE INDEX provider_values_scrape_run_id ON provider_values (scrape_run_id);

-- ---------------------------------------------------------------------------
-- §9.4 manual_overrides — a field the user set by hand. The most protected
-- content in the model: nothing but an explicit user action removes a row.
-- ---------------------------------------------------------------------------
CREATE TABLE manual_overrides (
    row_id       INTEGER NOT NULL PRIMARY KEY,
    subject_kind TEXT    NOT NULL CHECK (subject_kind IN ('Game', 'Release')),
    subject_id   BLOB    NOT NULL,
    field_key    TEXT    NOT NULL REFERENCES metadata_fields (field_key) ON DELETE RESTRICT,
    locale       TEXT    NULL,
    value_text   TEXT    NULL,
    value_number INTEGER NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE UNIQUE INDEX manual_overrides_subject_kind_subject_id_field_key_neutral
    ON manual_overrides (subject_kind, subject_id, field_key)
    WHERE locale IS NULL;

CREATE UNIQUE INDEX manual_overrides_subject_kind_subject_id_field_key_locale_localised
    ON manual_overrides (subject_kind, subject_id, field_key, locale COLLATE NOCASE)
    WHERE locale IS NOT NULL;

CREATE INDEX manual_overrides_subject_kind_subject_id
    ON manual_overrides (subject_kind, subject_id);

-- ---------------------------------------------------------------------------
-- §10.1 media_assets — metadata and a reference for one media file. SQLite stores
-- no media bytes.
-- ---------------------------------------------------------------------------
CREATE TABLE media_assets (
    id                       BLOB    NOT NULL PRIMARY KEY,
    media_type               TEXT    NOT NULL
                                     CHECK (media_type IN ('Cover', 'Screenshot', 'SaveStateThumbnail')),
    locale                   TEXT    NULL,
    provider_id              TEXT    NULL
                                     REFERENCES metadata_providers (provider_id) ON DELETE RESTRICT,
    source_provider_item_id  TEXT    NULL,
    managed_relative_path    TEXT    NULL,
    -- Deliberately not unique: the managed media store is content-addressed, so
    -- several records may point at one blob (§10.1).
    content_digest           BLOB    NULL,
    byte_size                INTEGER NULL CHECK (byte_size IS NULL OR byte_size >= 0),
    pixel_width              INTEGER NULL,
    pixel_height             INTEGER NULL,
    state                    TEXT    NOT NULL
                                     CHECK (state IN ('Pending', 'Available', 'Missing', 'Unusable')),
    created_at               INTEGER NOT NULL,
    last_verified_at         INTEGER NULL
);

CREATE INDEX media_assets_media_type     ON media_assets (media_type);
CREATE INDEX media_assets_content_digest ON media_assets (content_digest);
CREATE INDEX media_assets_state          ON media_assets (state);

-- ---------------------------------------------------------------------------
-- §10.2 media_asset_references — which subject an asset belongs to, in which
-- logical slot.
-- ---------------------------------------------------------------------------
CREATE TABLE media_asset_references (
    media_asset_id BLOB NOT NULL REFERENCES media_assets (id) ON DELETE RESTRICT,
    subject_kind   TEXT NOT NULL CHECK (subject_kind IN ('Game', 'Release')),
    subject_id     BLOB NOT NULL,
    logical_key    TEXT NOT NULL,

    -- One asset per slot per subject, which makes "replace the cover" a replace
    -- rather than an append.
    PRIMARY KEY (subject_kind, subject_id, logical_key)
);

CREATE INDEX media_asset_references_media_asset_id ON media_asset_references (media_asset_id);

-- ---------------------------------------------------------------------------
-- §11.1 firmware_entries — an index over the user's own firmware folder, not the
-- firmware. BitArchive never copies, renames, moves or deletes such a file.
-- ---------------------------------------------------------------------------
CREATE TABLE firmware_entries (
    relative_path      TEXT    NOT NULL PRIMARY KEY,
    filename           TEXT    NOT NULL,
    byte_size          INTEGER NULL CHECK (byte_size IS NULL OR byte_size >= 0),
    mtime              INTEGER NULL,
    digest             BLOB    NULL,
    digest_computed_at INTEGER NULL,
    read_state         TEXT    NOT NULL CHECK (read_state IN ('Readable', 'Unreadable', 'Missing')),
    first_seen_at      INTEGER NOT NULL,
    last_seen_at       INTEGER NOT NULL,

    -- An unreadable file is a first-class state, not a NULL digest with no
    -- explanation, so the nullability of the columns is pinned per state (§11.1).
    CHECK (
        CASE read_state
            WHEN 'Readable'   THEN byte_size IS NOT NULL AND mtime IS NOT NULL
                                   AND digest IS NOT NULL AND digest_computed_at IS NOT NULL
            WHEN 'Unreadable' THEN digest IS NULL AND digest_computed_at IS NULL
            WHEN 'Missing'    THEN byte_size IS NULL AND mtime IS NULL
                                   AND digest IS NULL AND digest_computed_at IS NULL
        END
    )
);

CREATE INDEX firmware_entries_filename   ON firmware_entries (filename);
CREATE INDEX firmware_entries_digest     ON firmware_entries (digest);
CREATE INDEX firmware_entries_read_state ON firmware_entries (read_state);

-- ---------------------------------------------------------------------------
-- §11.2 firmware_index_state — the singleton that separates "never scanned" from
-- "the folder is empty".
-- ---------------------------------------------------------------------------
CREATE TABLE firmware_index_state (
    singleton    INTEGER NOT NULL PRIMARY KEY CHECK (singleton = 1),
    last_scan_at INTEGER NULL,
    root_location TEXT   NULL,
    entry_count  INTEGER NOT NULL,
    scan_status  TEXT    NOT NULL CHECK (scan_status IN ('Never', 'Complete', 'Partial', 'Failed'))
);

-- ---------------------------------------------------------------------------
-- §12.3 cores — binds a CoreId to a curated core component.
--
-- No foreign key into the installed-state index, deliberately: a user may
-- configure a core that is not installed yet (§12.3).
-- ---------------------------------------------------------------------------
CREATE TABLE cores (
    core_id       BLOB    NOT NULL PRIMARY KEY,
    component_key TEXT    NOT NULL,
    created_at    INTEGER NOT NULL,

    UNIQUE (component_key)
);

-- ---------------------------------------------------------------------------
-- §12.3 core_selection_overrides — the persisted core selection. There is no
-- global scope, so a global core default is unrepresentable, not merely
-- unimplemented (invariant 21).
-- ---------------------------------------------------------------------------
CREATE TABLE core_selection_overrides (
    row_id          INTEGER NOT NULL PRIMARY KEY,
    scope_kind      TEXT    NOT NULL CHECK (scope_kind IN ('System', 'Game', 'Release')),
    scope_system_id BLOB    NULL REFERENCES systems (system_id) ON DELETE CASCADE,
    scope_game_id   BLOB    NULL REFERENCES games (id) ON DELETE CASCADE,
    scope_release_id BLOB   NULL REFERENCES releases (id) ON DELETE CASCADE,
    core_id         BLOB    NOT NULL REFERENCES cores (core_id) ON DELETE RESTRICT,
    created_at      INTEGER NOT NULL,

    -- Each scope requires exactly its own column, which is what makes the partial
    -- predicates below sound (§12.3).
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
);

CREATE UNIQUE INDEX core_selection_overrides_scope_system_id_system
    ON core_selection_overrides (scope_system_id)
    WHERE scope_kind = 'System';
CREATE UNIQUE INDEX core_selection_overrides_scope_game_id_game
    ON core_selection_overrides (scope_game_id)
    WHERE scope_kind = 'Game';
CREATE UNIQUE INDEX core_selection_overrides_scope_release_id_release
    ON core_selection_overrides (scope_release_id)
    WHERE scope_kind = 'Release';

CREATE INDEX core_selection_overrides_core_id ON core_selection_overrides (core_id);

-- ---------------------------------------------------------------------------
-- §12.4 core_versions — the persistent core-build anchor that save states,
-- sessions, schemas and overrides reference.
-- ---------------------------------------------------------------------------
CREATE TABLE core_versions (
    core_version_id BLOB    NOT NULL PRIMARY KEY,
    core_id         BLOB    NOT NULL REFERENCES cores (core_id) ON DELETE RESTRICT,
    component_key   TEXT    NOT NULL,
    platform        TEXT    NOT NULL,
    build_id        TEXT    NOT NULL,
    revision        TEXT    NULL,
    artifact_digest BLOB    NULL,
    first_seen_at   INTEGER NOT NULL,
    last_seen_at    INTEGER NOT NULL,

    UNIQUE (component_key, platform, build_id),
    UNIQUE (core_id, platform, build_id)
);

-- §12.4 documents two further lookups, `(core_id, platform)` and
-- `(component_key, platform)`. Both are leftmost prefixes of the unique keys
-- above, so the indexes those constraints already create answer them; a second
-- index over the same leading columns would duplicate one of them (§5.2).

-- ---------------------------------------------------------------------------
-- §12.5 runtime_versions — the same anchor role for the managed runtime.
-- ---------------------------------------------------------------------------
CREATE TABLE runtime_versions (
    runtime_version_id BLOB    NOT NULL PRIMARY KEY,
    component_key      TEXT    NOT NULL,
    platform           TEXT    NOT NULL,
    version            TEXT    NOT NULL,
    artifact_digest    BLOB    NULL,
    first_seen_at      INTEGER NOT NULL,
    last_seen_at       INTEGER NOT NULL,

    UNIQUE (component_key, platform, version)
);

-- §12.5's `(component_key, platform)` lookup is the leftmost prefix of the unique
-- key above and is answered by its index.

-- ---------------------------------------------------------------------------
-- §12.6 managed_components — the rebuildable installed-state index. It owns no
-- identity: it points at the persistent anchors, never the other way round.
-- ---------------------------------------------------------------------------
CREATE TABLE managed_components (
    component_class         TEXT    NOT NULL CHECK (component_class IN ('Runtime', 'Core')),
    component_key           TEXT    NOT NULL,
    platform                TEXT    NOT NULL,
    version                 TEXT    NOT NULL,
    core_version_id         BLOB    NULL REFERENCES core_versions (core_version_id) ON DELETE SET NULL,
    runtime_version_id      BLOB    NULL REFERENCES runtime_versions (runtime_version_id) ON DELETE SET NULL,
    relative_path           TEXT    NOT NULL,
    library_relative_path   TEXT    NULL,
    executable_relative_path TEXT   NULL,
    integrity_state         TEXT    NOT NULL
                                    CHECK (integrity_state IN ('Verified', 'Unknown', 'Missing', 'Unusable')),
    first_seen_at           INTEGER NOT NULL,
    last_seen_at            INTEGER NOT NULL,

    -- The artifact itself is the only identity this index has.
    PRIMARY KEY (component_class, component_key, platform, version),

    -- The two anchors are mutually exclusive, and the anchor must match the
    -- component class. Both may be NULL: an index row may describe a directory
    -- that is not yet bound to a build (§12.6).
    CHECK (
        NOT (core_version_id IS NOT NULL AND runtime_version_id IS NOT NULL)
        AND
        CASE component_class
            WHEN 'Core'    THEN runtime_version_id IS NULL
            WHEN 'Runtime' THEN core_version_id    IS NULL
        END
    )
);

-- §12.6's `(component_class, component_key, platform)` lookup is the leftmost
-- prefix of the primary key above and is answered by its index.

CREATE INDEX managed_components_integrity_state    ON managed_components (integrity_state);
CREATE INDEX managed_components_core_version_id    ON managed_components (core_version_id);
CREATE INDEX managed_components_runtime_version_id ON managed_components (runtime_version_id);

-- ---------------------------------------------------------------------------
-- §12.7 component_index_state — the singleton for the installed-state index.
-- ---------------------------------------------------------------------------
CREATE TABLE component_index_state (
    singleton           INTEGER NOT NULL PRIMARY KEY CHECK (singleton = 1),
    last_scan_at        INTEGER NULL,
    last_scan_status    TEXT    NOT NULL
                                CHECK (last_scan_status IN ('Never', 'Complete', 'Partial', 'Failed')),
    indexed_entry_count INTEGER NOT NULL
);

-- ---------------------------------------------------------------------------
-- §13.1 retroarch_setting_overrides — the typed RetroArch settings BitArchive
-- manages, with the inheritance Global -> System -> Game. There is deliberately no
-- release scope (§13.4).
-- ---------------------------------------------------------------------------
CREATE TABLE retroarch_setting_overrides (
    row_id          INTEGER NOT NULL PRIMARY KEY,
    scope_kind      TEXT    NOT NULL CHECK (scope_kind IN ('Global', 'System', 'Game')),
    scope_system_id BLOB    NULL REFERENCES systems (system_id) ON DELETE CASCADE,
    scope_game_id   BLOB    NULL REFERENCES games (id) ON DELETE CASCADE,
    setting_key     TEXT    NOT NULL,
    value_type      TEXT    NOT NULL
                            CHECK (value_type IN ('Bool', 'Integer', 'Float', 'Choice', 'Text')),
    value_bool      INTEGER NULL CHECK (value_bool IS NULL OR value_bool IN (0, 1)),
    value_integer   INTEGER NULL,
    value_float     REAL    NULL,
    value_text      TEXT    NULL,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,

    -- Each scope names exactly the column its kind requires, which is what makes
    -- the partial predicates below sound (§13.1).
    CHECK (
        CASE scope_kind
            WHEN 'Global' THEN scope_system_id IS NULL AND scope_game_id IS NULL
            WHEN 'System' THEN scope_system_id IS NOT NULL AND scope_game_id IS NULL
            WHEN 'Game'   THEN scope_game_id IS NOT NULL AND scope_system_id IS NULL
        END
    ),

    -- Exactly one value column is populated, matching `value_type`. This is what
    -- keeps a boolean from being stored three ways (§13.2).
    CHECK (
        CASE value_type
            WHEN 'Bool'    THEN value_bool IS NOT NULL AND value_integer IS NULL
                                  AND value_float IS NULL AND value_text IS NULL
            WHEN 'Integer' THEN value_integer IS NOT NULL AND value_bool IS NULL
                                  AND value_float IS NULL AND value_text IS NULL
            WHEN 'Float'   THEN value_float IS NOT NULL AND value_bool IS NULL
                                  AND value_integer IS NULL AND value_text IS NULL
            WHEN 'Choice'  THEN value_text IS NOT NULL AND value_bool IS NULL
                                  AND value_integer IS NULL AND value_float IS NULL
            WHEN 'Text'    THEN value_text IS NOT NULL AND value_bool IS NULL
                                  AND value_integer IS NULL AND value_float IS NULL
        END
    )
);

CREATE UNIQUE INDEX retroarch_setting_overrides_setting_key_global
    ON retroarch_setting_overrides (setting_key)
    WHERE scope_kind = 'Global';
CREATE UNIQUE INDEX retroarch_setting_overrides_scope_system_id_setting_key_system
    ON retroarch_setting_overrides (scope_system_id, setting_key)
    WHERE scope_kind = 'System';
CREATE UNIQUE INDEX retroarch_setting_overrides_scope_game_id_setting_key_game
    ON retroarch_setting_overrides (scope_game_id, setting_key)
    WHERE scope_kind = 'Game';

CREATE INDEX retroarch_setting_overrides_scope_kind_scope_system_id
    ON retroarch_setting_overrides (scope_kind, scope_system_id);
CREATE INDEX retroarch_setting_overrides_scope_kind_scope_game_id
    ON retroarch_setting_overrides (scope_kind, scope_game_id);

-- ---------------------------------------------------------------------------
-- §14.2 core_option_schemas — the options a concrete core version offers. A
-- historical introspection result, not a cache.
-- ---------------------------------------------------------------------------
CREATE TABLE core_option_schemas (
    core_version_id      BLOB    NOT NULL PRIMARY KEY
                                 REFERENCES core_versions (core_version_id) ON DELETE CASCADE,
    introspected_at      INTEGER NOT NULL,
    definition_count     INTEGER NOT NULL,
    introspection_status TEXT    NOT NULL
                                 CHECK (introspection_status IN ('Complete', 'Partial', 'Failed'))
);

-- ---------------------------------------------------------------------------
-- §14.2 core_option_definitions — one option a schema declares.
-- ---------------------------------------------------------------------------
CREATE TABLE core_option_definitions (
    core_version_id BLOB    NOT NULL REFERENCES core_versions (core_version_id) ON DELETE CASCADE,
    option_key      TEXT    NOT NULL,
    value_type      TEXT    NOT NULL
                            CHECK (value_type IN ('Bool', 'Integer', 'Float', 'Choice', 'Text')),
    default_value   TEXT    NULL,
    allowed_values  TEXT    NULL,
    display_name    TEXT    NULL,
    definition_index INTEGER NOT NULL,

    PRIMARY KEY (core_version_id, option_key)
);

-- ---------------------------------------------------------------------------
-- §14.1 core_option_overrides — core-specific options, a separate configuration
-- domain with the hierarchy Core Defaults -> System -> Game. There is
-- deliberately no release scope (§13.4).
-- ---------------------------------------------------------------------------
CREATE TABLE core_option_overrides (
    row_id          INTEGER NOT NULL PRIMARY KEY,
    core_version_id BLOB    NOT NULL REFERENCES core_versions (core_version_id) ON DELETE RESTRICT,
    scope_kind      TEXT    NOT NULL CHECK (scope_kind IN ('CoreDefaults', 'System', 'Game')),
    scope_system_id BLOB    NULL REFERENCES systems (system_id) ON DELETE CASCADE,
    scope_game_id   BLOB    NULL REFERENCES games (id) ON DELETE CASCADE,
    option_key      TEXT    NOT NULL,
    value_text      TEXT    NOT NULL,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,

    CHECK (
        CASE scope_kind
            WHEN 'CoreDefaults' THEN scope_system_id IS NULL AND scope_game_id IS NULL
            WHEN 'System'       THEN scope_system_id IS NOT NULL AND scope_game_id IS NULL
            WHEN 'Game'         THEN scope_game_id IS NOT NULL AND scope_system_id IS NULL
        END
    )
);

CREATE UNIQUE INDEX core_option_overrides_core_version_id_option_key_core_defaults
    ON core_option_overrides (core_version_id, option_key)
    WHERE scope_kind = 'CoreDefaults';
CREATE UNIQUE INDEX core_option_overrides_core_version_id_scope_system_id_option_key_system
    ON core_option_overrides (core_version_id, scope_system_id, option_key)
    WHERE scope_kind = 'System';
CREATE UNIQUE INDEX core_option_overrides_core_version_id_scope_game_id_option_key_game
    ON core_option_overrides (core_version_id, scope_game_id, option_key)
    WHERE scope_kind = 'Game';

CREATE INDEX core_option_overrides_core_version_id ON core_option_overrides (core_version_id);
CREATE INDEX core_option_overrides_scope_kind_scope_game_id
    ON core_option_overrides (scope_kind, scope_game_id);

-- ---------------------------------------------------------------------------
-- §15.1 save_states — BitArchive's management layer over the save-state files
-- RetroArch owns. The files stay outside the database and outside BitArchive's
-- ownership; normal RetroArch saves are not modelled at all.
-- ---------------------------------------------------------------------------
CREATE TABLE save_states (
    id                       BLOB    NOT NULL PRIMARY KEY,
    release_id               BLOB    NOT NULL REFERENCES releases (id) ON DELETE RESTRICT,
    core_id                  BLOB    NOT NULL REFERENCES cores (core_id) ON DELETE RESTRICT,
    core_version_id          BLOB    NULL REFERENCES core_versions (core_version_id) ON DELETE RESTRICT,
    core_version_label       TEXT    NULL,
    -- The RetroArch state slot as encoded in the file name; `-1` is RetroArch's own
    -- `Auto` slot. An attribute of the file, never a key, and deliberately not
    -- range-checked: the `-1 … 999` range is a UI/CLI contract, not a storage
    -- invariant (§15.1).
    slot                     INTEGER NULL,
    created_at               INTEGER NOT NULL,
    file_relative_path       TEXT    NOT NULL,
    file_size                INTEGER NULL CHECK (file_size IS NULL OR file_size >= 0),
    file_mtime               INTEGER NULL,
    display_name             TEXT    NULL,
    thumbnail_media_asset_id BLOB    NULL REFERENCES media_assets (id) ON DELETE SET NULL,
    state                    TEXT    NOT NULL CHECK (state IN ('Present', 'Missing', 'Unreadable')),
    last_seen_at             INTEGER NOT NULL,

    -- One indexed row per physical state file, which is what makes discovery
    -- idempotent.
    UNIQUE (file_relative_path)
);

CREATE INDEX save_states_release_id_created_at ON save_states (release_id, created_at DESC);
CREATE INDEX save_states_slot                  ON save_states (slot);
CREATE INDEX save_states_core_id_core_version_id ON save_states (core_id, core_version_id);
CREATE INDEX save_states_state                 ON save_states (state);

-- ---------------------------------------------------------------------------
-- §16.1 sessions — one emulation session, and the recovery record for it.
-- ---------------------------------------------------------------------------
CREATE TABLE sessions (
    id                    BLOB    NOT NULL PRIMARY KEY,
    game_id               BLOB    NOT NULL REFERENCES games (id) ON DELETE RESTRICT,
    release_id            BLOB    NULL REFERENCES releases (id) ON DELETE RESTRICT,
    content_id            BLOB    NULL REFERENCES contents (id) ON DELETE RESTRICT,
    content_location_kind TEXT    NULL CHECK (content_location_kind IN ('File', 'ArchiveEntry')),
    core_id               BLOB    NULL REFERENCES cores (core_id) ON DELETE RESTRICT,
    core_version_id       BLOB    NULL REFERENCES core_versions (core_version_id) ON DELETE RESTRICT,
    core_version_label    TEXT    NULL,
    runtime_version_id    BLOB    NULL REFERENCES runtime_versions (runtime_version_id) ON DELETE RESTRICT,
    -- A deliberate, documented duplicate of the anchor so a session row is
    -- readable without a join. Nothing resolves, compares or filters on it (§16.1).
    runtime_version       TEXT    NULL,
    started_at            INTEGER NOT NULL,
    ended_at              INTEGER NULL,
    state                 TEXT    NOT NULL
                                  CHECK (state IN ('Active', 'Finalized', 'Recovered', 'Abandoned')),
    duration_ms           INTEGER NULL,
    active_duration_ms    INTEGER NULL,
    process_id            INTEGER NULL,
    process_started_at    INTEGER NULL,
    process_executable    TEXT    NULL,
    exit_kind             TEXT    NULL
                                  CHECK (exit_kind IS NULL OR exit_kind IN ('ClosedGracefully',
                                                                            'ForceKilled', 'Crashed',
                                                                            'ProcessGone', 'Unknown')),
    exit_code             INTEGER NULL,
    exit_signal           TEXT    NULL,
    recovery_note         TEXT    NULL,
    session_directory     TEXT    NULL,
    created_at            INTEGER NOT NULL,

    -- A session is written when it starts, so "still active" and "has an end" are
    -- the same fact stated twice (§16.1).
    CHECK (state <> 'Active' OR (ended_at IS NULL AND duration_ms IS NULL))
);

-- At most one active session, enforced by the database rather than by caller
-- discipline (invariant: the single-active-session MVP rule).
CREATE UNIQUE INDEX sessions_state_active ON sessions (state) WHERE state = 'Active';

CREATE INDEX sessions_game_id_started_at ON sessions (game_id, started_at DESC);
CREATE INDEX sessions_state              ON sessions (state);
CREATE INDEX sessions_started_at         ON sessions (started_at);

-- ---------------------------------------------------------------------------
-- §17 search / FTS5 projection — a rebuildable projection over the library, and
-- never a fachliche truth. It locates rows; the query layer joins the returned
-- GameId back to `games` for the authoritative row.
--
-- Shape, and what is deliberately absent:
--
--   * `game_id` carries the projection's key. It is the 16-byte UUIDv7 identity,
--     stored as the projection's own copy and returned as a `GameId` — never a
--     row offset and never a list position.
--   * the four text columns are the searchable values of §17.1; the remaining
--     columns are the filter and sort values the library views need, stored
--     `UNINDEXED` because they are compared, not matched.
--   * there is no `UNIQUE` constraint, because an FTS5 virtual table cannot carry
--     one. "One row per game" is a rebuild invariant (§17.4), repaired by dropping
--     and rebuilding the projection.
--   * there are no triggers. The model allows incremental maintenance but binds
--     only the full rebuild (§17.3); the rebuild belongs to the query layer, and
--     pre-empting its design here would put a second, untested maintenance path
--     into schema v1.
--
-- The tokenizer is FTS5's `unicode61` with case folding on (its default) and
-- diacritics folded at the strongest setting, so title search compares titles
-- case-insensitively as §3.6 requires.
-- ---------------------------------------------------------------------------
CREATE VIRTUAL TABLE search_index USING fts5(
    game_id          UNINDEXED,
    title,
    alternate_title,
    release_title,
    system_name,
    regions          UNINDEXED,
    languages        UNINDEXED,
    release_type     UNINDEXED,
    release_date     UNINDEXED,
    is_favorite      UNINDEXED,
    is_hidden        UNINDEXED,
    is_ignored       UNINDEXED,
    tokenize = "unicode61 remove_diacritics 2"
);
