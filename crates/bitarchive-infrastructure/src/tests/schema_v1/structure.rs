//! The structure of schema v1: which objects exist, what they are made of, and
//! how they are keyed.
//!
//! The expectations below are declared as data and compared against what SQLite
//! actually holds, so a missing column, a changed delete rule, or an extra index
//! is reported as the difference it is rather than as a failed assertion about
//! something else. `DATA_MODEL.md` is where every expectation comes from, and each
//! declaration names the section that fixes it.

use rusqlite::params;

use super::super::support::database::TempDatabase;
use super::{
    Column, LEDGER_TABLE, SEARCH_INDEX, columns, count, id, indexes, migrated, primary_key,
    table_names,
};
use crate::database::{
    BITARCHIVE_MIGRATIONS, DatabaseError, Migration, MigrationRunner, SchemaVersion,
};

/// A column as [`TABLES`] declares it: its name, its declared type, and whether
/// it is `NOT NULL`.
type ColumnSpec = (&'static str, &'static str, bool);

/// A table as [`TABLES`] declares it: its name, and its columns in declared order.
type TableSpec = (&'static str, &'static [ColumnSpec]);

/// A foreign key as [`FOREIGN_KEYS`] declares it: the referencing table, its
/// columns, the referenced table, the referenced table's columns, and the
/// `ON DELETE` action.
type ForeignKeySpec = (
    &'static str,
    &'static [&'static str],
    &'static str,
    &'static [&'static str],
    &'static str,
);

/// An index as [`NAMED_INDEXES`] declares it: its name, its table, its columns,
/// whether it enforces uniqueness, and the `WHERE` clause of a partial index.
type IndexSpec = (
    &'static str,
    &'static str,
    &'static [&'static str],
    bool,
    Option<&'static str>,
);

/// Every table schema v1 declares, with its columns in declared order.
///
/// Each column is `(name, declared type, NOT NULL)`. `DATA_MODEL.md` §20.1 is the
/// inventory of tables; §6–§17 define each table's columns, their types, and their
/// nullability, and §3.2 fixes the encoding those types express — a UUIDv7 as a
/// 16-byte `BLOB`, a digest as a 32-byte `BLOB`, a timestamp as an `INTEGER` of
/// milliseconds since the Unix epoch, an enum as `TEXT`, and a boolean as an
/// `INTEGER` of 0 or 1.
///
/// The ledger is not here — the foundation owns it — and neither is the FTS5
/// projection, which is a virtual table and has no declared column types; both
/// have tests of their own below.
const TABLES: &[TableSpec] = &[
    (
        "systems",
        &[
            ("system_id", "BLOB", true),
            ("catalog_key", "TEXT", true),
            ("created_at", "INTEGER", true),
        ],
    ),
    (
        "library_sources",
        &[
            ("id", "BLOB", true),
            ("display_name", "TEXT", true),
            ("location", "TEXT", true),
            ("platform_locator_kind", "TEXT", true),
            ("fixed_system_id", "BLOB", false),
            ("availability", "TEXT", true),
            ("availability_checked_at", "INTEGER", false),
            ("created_at", "INTEGER", true),
            ("removed_at", "INTEGER", false),
        ],
    ),
    (
        "scan_runs",
        &[
            ("id", "BLOB", true),
            ("target_kind", "TEXT", true),
            ("target_source_id", "BLOB", false),
            ("status", "TEXT", true),
            ("started_at", "INTEGER", true),
            ("finished_at", "INTEGER", false),
            ("reconciliation_eligible", "INTEGER", true),
            ("discovered_count", "INTEGER", true),
            ("imported_count", "INTEGER", true),
            ("updated_count", "INTEGER", true),
            ("rediscovered_count", "INTEGER", true),
            ("missing_count", "INTEGER", true),
            ("unknown_count", "INTEGER", true),
            ("unsupported_count", "INTEGER", true),
            ("problem_count", "INTEGER", true),
        ],
    ),
    (
        "scan_run_issues",
        &[
            ("scan_run_id", "BLOB", true),
            ("issue_index", "INTEGER", true),
            ("kind", "TEXT", true),
            ("source_id", "BLOB", false),
            ("relative_path", "TEXT", false),
            ("detail", "TEXT", false),
            ("byte_size", "INTEGER", false),
            ("prevents_reconciliation", "INTEGER", true),
        ],
    ),
    (
        "games",
        &[
            ("id", "BLOB", true),
            ("first_seen_at", "INTEGER", true),
            ("default_release_id", "BLOB", false),
            ("is_favorite", "INTEGER", true),
            ("is_hidden", "INTEGER", true),
            ("is_ignored", "INTEGER", true),
        ],
    ),
    (
        "releases",
        &[
            ("id", "BLOB", true),
            ("game_id", "BLOB", true),
            ("system_id", "BLOB", true),
            ("release_title", "TEXT", false),
            ("release_date", "TEXT", false),
            ("revision", "TEXT", false),
            ("release_type", "TEXT", true),
            ("release_key", "BLOB", true),
            ("archive_kind", "TEXT", false),
            ("created_at", "INTEGER", true),
        ],
    ),
    (
        "release_regions",
        &[("release_id", "BLOB", true), ("region", "TEXT", true)],
    ),
    (
        "release_languages",
        &[("release_id", "BLOB", true), ("language", "TEXT", true)],
    ),
    (
        "contents",
        &[
            ("id", "BLOB", true),
            ("release_id", "BLOB", true),
            ("game_id", "BLOB", true),
            ("content_kind", "TEXT", true),
            ("format", "TEXT", false),
            ("validation_state", "TEXT", true),
            ("validation_detail", "TEXT", false),
            ("disc_index", "INTEGER", false),
            ("created_at", "INTEGER", true),
        ],
    ),
    (
        "content_locations",
        &[
            ("row_id", "INTEGER", true),
            ("content_id", "BLOB", true),
            ("location_kind", "TEXT", true),
            ("source_id", "BLOB", false),
            ("relative_path", "TEXT", false),
            ("archive_content_id", "BLOB", false),
            ("archive_entry_path", "TEXT", false),
            ("observed_filename", "TEXT", true),
            ("observed_size", "INTEGER", false),
            ("observed_mtime", "INTEGER", false),
            ("state", "TEXT", true),
            ("last_seen_scan_run_id", "BLOB", false),
            ("first_seen_at", "INTEGER", true),
            ("last_seen_at", "INTEGER", false),
        ],
    ),
    (
        "content_derivations",
        &[
            ("content_id", "BLOB", true),
            ("kind", "TEXT", true),
            ("relative_path", "TEXT", true),
            ("member_count", "INTEGER", true),
            ("created_at", "INTEGER", true),
        ],
    ),
    (
        "content_derivation_members",
        &[
            ("content_id", "BLOB", true),
            ("kind", "TEXT", true),
            ("source_content_id", "BLOB", true),
            ("member_index", "INTEGER", true),
        ],
    ),
    (
        "content_fingerprints",
        &[
            ("row_id", "INTEGER", true),
            ("content_id", "BLOB", true),
            ("fingerprint_kind", "TEXT", true),
            ("algorithm", "TEXT", true),
            ("digest", "BLOB", true),
            ("entry_path", "TEXT", false),
            ("byte_size", "INTEGER", false),
            ("computed_at", "INTEGER", true),
            ("source_scan_run_id", "BLOB", false),
        ],
    ),
    (
        "metadata_fields",
        &[
            ("field_key", "TEXT", true),
            ("value_kind", "TEXT", true),
            ("is_multi_valued", "INTEGER", true),
            ("is_localizable", "INTEGER", true),
        ],
    ),
    (
        "metadata_providers",
        &[
            ("provider_id", "TEXT", true),
            ("display_name", "TEXT", true),
            ("is_enabled", "INTEGER", true),
        ],
    ),
    (
        "scrape_runs",
        &[
            ("id", "BLOB", true),
            ("provider_id", "TEXT", true),
            ("mode", "TEXT", true),
            ("scope_kind", "TEXT", true),
            ("scope_system_id", "BLOB", false),
            ("scope_game_id", "BLOB", false),
            ("only_missing", "INTEGER", true),
            ("fields_missing", "TEXT", false),
            ("status", "TEXT", true),
            ("started_at", "INTEGER", true),
            ("finished_at", "INTEGER", false),
            ("updated_count", "INTEGER", true),
            ("no_match_count", "INTEGER", true),
            ("multiple_match_count", "INTEGER", true),
            ("error_count", "INTEGER", true),
            ("skipped_count", "INTEGER", true),
            ("override_protected_count", "INTEGER", true),
        ],
    ),
    (
        "scrape_run_items",
        &[
            ("scrape_run_id", "BLOB", true),
            ("game_id", "BLOB", true),
            ("status", "TEXT", true),
            ("candidate_count", "INTEGER", true),
            ("accepted_provider_item_id", "TEXT", false),
            ("error_detail", "TEXT", false),
            ("updated_at", "INTEGER", true),
        ],
    ),
    (
        "provider_values",
        &[
            ("row_id", "INTEGER", true),
            ("provider_id", "TEXT", true),
            ("subject_kind", "TEXT", true),
            ("subject_id", "BLOB", true),
            ("field_key", "TEXT", true),
            ("locale", "TEXT", false),
            ("value_index", "INTEGER", true),
            ("value_text", "TEXT", false),
            ("value_number", "INTEGER", false),
            ("provider_item_id", "TEXT", false),
            ("fetched_at", "INTEGER", true),
            ("scrape_run_id", "BLOB", false),
        ],
    ),
    (
        "manual_overrides",
        &[
            ("row_id", "INTEGER", true),
            ("subject_kind", "TEXT", true),
            ("subject_id", "BLOB", true),
            ("field_key", "TEXT", true),
            ("locale", "TEXT", false),
            ("value_text", "TEXT", false),
            ("value_number", "INTEGER", false),
            ("created_at", "INTEGER", true),
            ("updated_at", "INTEGER", true),
        ],
    ),
    (
        "media_assets",
        &[
            ("id", "BLOB", true),
            ("media_type", "TEXT", true),
            ("locale", "TEXT", false),
            ("provider_id", "TEXT", false),
            ("source_provider_item_id", "TEXT", false),
            ("managed_relative_path", "TEXT", false),
            ("content_digest", "BLOB", false),
            ("byte_size", "INTEGER", false),
            ("pixel_width", "INTEGER", false),
            ("pixel_height", "INTEGER", false),
            ("state", "TEXT", true),
            ("created_at", "INTEGER", true),
            ("last_verified_at", "INTEGER", false),
        ],
    ),
    (
        "media_asset_references",
        &[
            ("media_asset_id", "BLOB", true),
            ("subject_kind", "TEXT", true),
            ("subject_id", "BLOB", true),
            ("logical_key", "TEXT", true),
        ],
    ),
    (
        "firmware_entries",
        &[
            ("relative_path", "TEXT", true),
            ("filename", "TEXT", true),
            ("byte_size", "INTEGER", false),
            ("mtime", "INTEGER", false),
            ("digest", "BLOB", false),
            ("digest_computed_at", "INTEGER", false),
            ("read_state", "TEXT", true),
            ("first_seen_at", "INTEGER", true),
            ("last_seen_at", "INTEGER", true),
        ],
    ),
    (
        "firmware_index_state",
        &[
            ("singleton", "INTEGER", true),
            ("last_scan_at", "INTEGER", false),
            ("root_location", "TEXT", false),
            ("entry_count", "INTEGER", true),
            ("scan_status", "TEXT", true),
        ],
    ),
    (
        "cores",
        &[
            ("core_id", "BLOB", true),
            ("component_key", "TEXT", true),
            ("created_at", "INTEGER", true),
        ],
    ),
    (
        "core_selection_overrides",
        &[
            ("row_id", "INTEGER", true),
            ("scope_kind", "TEXT", true),
            ("scope_system_id", "BLOB", false),
            ("scope_game_id", "BLOB", false),
            ("scope_release_id", "BLOB", false),
            ("core_id", "BLOB", true),
            ("created_at", "INTEGER", true),
        ],
    ),
    (
        "core_versions",
        &[
            ("core_version_id", "BLOB", true),
            ("core_id", "BLOB", true),
            ("component_key", "TEXT", true),
            ("platform", "TEXT", true),
            ("build_id", "TEXT", true),
            ("revision", "TEXT", false),
            ("artifact_digest", "BLOB", false),
            ("first_seen_at", "INTEGER", true),
            ("last_seen_at", "INTEGER", true),
        ],
    ),
    (
        "runtime_versions",
        &[
            ("runtime_version_id", "BLOB", true),
            ("component_key", "TEXT", true),
            ("platform", "TEXT", true),
            ("version", "TEXT", true),
            ("artifact_digest", "BLOB", false),
            ("first_seen_at", "INTEGER", true),
            ("last_seen_at", "INTEGER", true),
        ],
    ),
    (
        "managed_components",
        &[
            ("component_class", "TEXT", true),
            ("component_key", "TEXT", true),
            ("platform", "TEXT", true),
            ("version", "TEXT", true),
            ("core_version_id", "BLOB", false),
            ("runtime_version_id", "BLOB", false),
            ("relative_path", "TEXT", true),
            ("library_relative_path", "TEXT", false),
            ("executable_relative_path", "TEXT", false),
            ("integrity_state", "TEXT", true),
            ("first_seen_at", "INTEGER", true),
            ("last_seen_at", "INTEGER", true),
        ],
    ),
    (
        "component_index_state",
        &[
            ("singleton", "INTEGER", true),
            ("last_scan_at", "INTEGER", false),
            ("last_scan_status", "TEXT", true),
            ("indexed_entry_count", "INTEGER", true),
        ],
    ),
    (
        "retroarch_setting_overrides",
        &[
            ("row_id", "INTEGER", true),
            ("scope_kind", "TEXT", true),
            ("scope_system_id", "BLOB", false),
            ("scope_game_id", "BLOB", false),
            ("setting_key", "TEXT", true),
            ("value_type", "TEXT", true),
            ("value_bool", "INTEGER", false),
            ("value_integer", "INTEGER", false),
            ("value_float", "REAL", false),
            ("value_text", "TEXT", false),
            ("created_at", "INTEGER", true),
            ("updated_at", "INTEGER", true),
        ],
    ),
    (
        "core_option_schemas",
        &[
            ("core_version_id", "BLOB", true),
            ("introspected_at", "INTEGER", true),
            ("definition_count", "INTEGER", true),
            ("introspection_status", "TEXT", true),
        ],
    ),
    (
        "core_option_definitions",
        &[
            ("core_version_id", "BLOB", true),
            ("option_key", "TEXT", true),
            ("value_type", "TEXT", true),
            ("default_value", "TEXT", false),
            ("allowed_values", "TEXT", false),
            ("display_name", "TEXT", false),
            ("definition_index", "INTEGER", true),
        ],
    ),
    (
        "core_option_overrides",
        &[
            ("row_id", "INTEGER", true),
            ("core_version_id", "BLOB", true),
            ("scope_kind", "TEXT", true),
            ("scope_system_id", "BLOB", false),
            ("scope_game_id", "BLOB", false),
            ("option_key", "TEXT", true),
            ("value_text", "TEXT", true),
            ("created_at", "INTEGER", true),
            ("updated_at", "INTEGER", true),
        ],
    ),
    (
        "save_states",
        &[
            ("id", "BLOB", true),
            ("release_id", "BLOB", true),
            ("core_id", "BLOB", true),
            ("core_version_id", "BLOB", false),
            ("core_version_label", "TEXT", false),
            ("slot", "INTEGER", false),
            ("created_at", "INTEGER", true),
            ("file_relative_path", "TEXT", true),
            ("file_size", "INTEGER", false),
            ("file_mtime", "INTEGER", false),
            ("display_name", "TEXT", false),
            ("thumbnail_media_asset_id", "BLOB", false),
            ("state", "TEXT", true),
            ("last_seen_at", "INTEGER", true),
        ],
    ),
    (
        "sessions",
        &[
            ("id", "BLOB", true),
            ("game_id", "BLOB", true),
            ("release_id", "BLOB", false),
            ("content_id", "BLOB", false),
            ("content_location_kind", "TEXT", false),
            ("core_id", "BLOB", false),
            ("core_version_id", "BLOB", false),
            ("core_version_label", "TEXT", false),
            ("runtime_version_id", "BLOB", false),
            ("runtime_version", "TEXT", false),
            ("started_at", "INTEGER", true),
            ("ended_at", "INTEGER", false),
            ("state", "TEXT", true),
            ("duration_ms", "INTEGER", false),
            ("active_duration_ms", "INTEGER", false),
            ("process_id", "INTEGER", false),
            ("process_started_at", "INTEGER", false),
            ("process_executable", "TEXT", false),
            ("exit_kind", "TEXT", false),
            ("exit_code", "INTEGER", false),
            ("exit_signal", "TEXT", false),
            ("recovery_note", "TEXT", false),
            ("session_directory", "TEXT", false),
            ("created_at", "INTEGER", true),
        ],
    ),
];

/// The primary key of every table, as `DATA_MODEL.md` §3.8's key audit fixes it.
///
/// Three kinds appear, and they are not interchangeable: a domain UUID where the
/// table has an identity of its own, a composite key where the identity *is* the
/// combination, and `row_id` — a sequential storage key with no domain meaning —
/// where the table has no identity at all: an observation, a fingerprint, a
/// provider value, an override, the ordered members, and the run-item rows.
///
/// A path is never here (invariant 2), a hash is never here (invariant 3), and a
/// list position is never here.
const PRIMARY_KEYS: &[(&str, &[&str])] = &[
    ("systems", &["system_id"]),
    ("library_sources", &["id"]),
    ("scan_runs", &["id"]),
    ("scan_run_issues", &["scan_run_id", "issue_index"]),
    ("games", &["id"]),
    ("releases", &["id"]),
    ("release_regions", &["release_id", "region"]),
    ("release_languages", &["release_id", "language"]),
    ("contents", &["id"]),
    ("content_locations", &["row_id"]),
    ("content_derivations", &["content_id", "kind"]),
    (
        "content_derivation_members",
        &["content_id", "kind", "source_content_id"],
    ),
    ("content_fingerprints", &["row_id"]),
    ("metadata_fields", &["field_key"]),
    ("metadata_providers", &["provider_id"]),
    ("scrape_runs", &["id"]),
    ("scrape_run_items", &["scrape_run_id", "game_id"]),
    ("provider_values", &["row_id"]),
    ("manual_overrides", &["row_id"]),
    ("media_assets", &["id"]),
    (
        "media_asset_references",
        &["subject_kind", "subject_id", "logical_key"],
    ),
    ("firmware_entries", &["relative_path"]),
    ("firmware_index_state", &["singleton"]),
    ("cores", &["core_id"]),
    ("core_selection_overrides", &["row_id"]),
    ("core_versions", &["core_version_id"]),
    ("runtime_versions", &["runtime_version_id"]),
    (
        "managed_components",
        &["component_class", "component_key", "platform", "version"],
    ),
    ("component_index_state", &["singleton"]),
    ("retroarch_setting_overrides", &["row_id"]),
    ("core_option_schemas", &["core_version_id"]),
    (
        "core_option_definitions",
        &["core_version_id", "option_key"],
    ),
    ("core_option_overrides", &["row_id"]),
    ("save_states", &["id"]),
    ("sessions", &["id"]),
];

/// Every foreign key schema v1 declares, as
/// `(child, child columns, parent, parent columns, ON DELETE)`.
///
/// `DATA_MODEL.md` §3.7 fixes the delete behaviour of each reference, §19.7
/// tabulates it for the run references, and §20.2 gives the creation order the
/// keys require. `CASCADE` appears exactly twice — the release tags and the rows
/// that belong to a derivation — because those are the cases where the child has
/// no meaning without its parent. Everything else either refuses the delete
/// (`RESTRICT`) or is a reference kept for provenance (`SET NULL`).
const FOREIGN_KEYS: &[ForeignKeySpec] = &[
    (
        "library_sources",
        &["fixed_system_id"],
        "systems",
        &["system_id"],
        "RESTRICT",
    ),
    (
        "scan_runs",
        &["target_source_id"],
        "library_sources",
        &["id"],
        "RESTRICT",
    ),
    (
        "scan_run_issues",
        &["scan_run_id"],
        "scan_runs",
        &["id"],
        "CASCADE",
    ),
    (
        "scan_run_issues",
        &["source_id"],
        "library_sources",
        &["id"],
        "RESTRICT",
    ),
    (
        "games",
        &["default_release_id"],
        "releases",
        &["id"],
        "SET NULL",
    ),
    ("releases", &["game_id"], "games", &["id"], "RESTRICT"),
    (
        "releases",
        &["system_id"],
        "systems",
        &["system_id"],
        "RESTRICT",
    ),
    (
        "release_regions",
        &["release_id"],
        "releases",
        &["id"],
        "CASCADE",
    ),
    (
        "release_languages",
        &["release_id"],
        "releases",
        &["id"],
        "CASCADE",
    ),
    (
        "contents",
        &["release_id", "game_id"],
        "releases",
        &["id", "game_id"],
        "RESTRICT",
    ),
    (
        "content_locations",
        &["content_id"],
        "contents",
        &["id"],
        "RESTRICT",
    ),
    (
        "content_locations",
        &["archive_content_id"],
        "contents",
        &["id"],
        "RESTRICT",
    ),
    (
        "content_locations",
        &["source_id"],
        "library_sources",
        &["id"],
        "RESTRICT",
    ),
    (
        "content_locations",
        &["last_seen_scan_run_id"],
        "scan_runs",
        &["id"],
        "SET NULL",
    ),
    (
        "content_derivations",
        &["content_id"],
        "contents",
        &["id"],
        "CASCADE",
    ),
    (
        "content_derivation_members",
        &["content_id", "kind"],
        "content_derivations",
        &["content_id", "kind"],
        "CASCADE",
    ),
    (
        "content_derivation_members",
        &["source_content_id"],
        "contents",
        &["id"],
        "RESTRICT",
    ),
    (
        "content_fingerprints",
        &["content_id"],
        "contents",
        &["id"],
        "CASCADE",
    ),
    (
        "content_fingerprints",
        &["source_scan_run_id"],
        "scan_runs",
        &["id"],
        "SET NULL",
    ),
    (
        "scrape_runs",
        &["provider_id"],
        "metadata_providers",
        &["provider_id"],
        "RESTRICT",
    ),
    (
        "scrape_runs",
        &["scope_game_id"],
        "games",
        &["id"],
        "RESTRICT",
    ),
    (
        "scrape_runs",
        &["scope_system_id"],
        "systems",
        &["system_id"],
        "RESTRICT",
    ),
    (
        "scrape_run_items",
        &["game_id"],
        "games",
        &["id"],
        "RESTRICT",
    ),
    (
        "scrape_run_items",
        &["scrape_run_id"],
        "scrape_runs",
        &["id"],
        "CASCADE",
    ),
    (
        "provider_values",
        &["field_key"],
        "metadata_fields",
        &["field_key"],
        "RESTRICT",
    ),
    (
        "provider_values",
        &["scrape_run_id"],
        "scrape_runs",
        &["id"],
        "SET NULL",
    ),
    (
        "manual_overrides",
        &["field_key"],
        "metadata_fields",
        &["field_key"],
        "RESTRICT",
    ),
    (
        "media_assets",
        &["provider_id"],
        "metadata_providers",
        &["provider_id"],
        "RESTRICT",
    ),
    (
        "media_asset_references",
        &["media_asset_id"],
        "media_assets",
        &["id"],
        "RESTRICT",
    ),
    (
        "core_selection_overrides",
        &["core_id"],
        "cores",
        &["core_id"],
        "RESTRICT",
    ),
    (
        "core_selection_overrides",
        &["scope_game_id"],
        "games",
        &["id"],
        "CASCADE",
    ),
    (
        "core_selection_overrides",
        &["scope_release_id"],
        "releases",
        &["id"],
        "CASCADE",
    ),
    (
        "core_selection_overrides",
        &["scope_system_id"],
        "systems",
        &["system_id"],
        "CASCADE",
    ),
    (
        "core_versions",
        &["core_id"],
        "cores",
        &["core_id"],
        "RESTRICT",
    ),
    (
        "managed_components",
        &["core_version_id"],
        "core_versions",
        &["core_version_id"],
        "SET NULL",
    ),
    (
        "managed_components",
        &["runtime_version_id"],
        "runtime_versions",
        &["runtime_version_id"],
        "SET NULL",
    ),
    (
        "retroarch_setting_overrides",
        &["scope_game_id"],
        "games",
        &["id"],
        "CASCADE",
    ),
    (
        "retroarch_setting_overrides",
        &["scope_system_id"],
        "systems",
        &["system_id"],
        "CASCADE",
    ),
    (
        "core_option_schemas",
        &["core_version_id"],
        "core_versions",
        &["core_version_id"],
        "CASCADE",
    ),
    (
        "core_option_definitions",
        &["core_version_id"],
        "core_versions",
        &["core_version_id"],
        "CASCADE",
    ),
    (
        "core_option_overrides",
        &["core_version_id"],
        "core_versions",
        &["core_version_id"],
        "RESTRICT",
    ),
    (
        "core_option_overrides",
        &["scope_game_id"],
        "games",
        &["id"],
        "CASCADE",
    ),
    (
        "core_option_overrides",
        &["scope_system_id"],
        "systems",
        &["system_id"],
        "CASCADE",
    ),
    (
        "save_states",
        &["release_id"],
        "releases",
        &["id"],
        "RESTRICT",
    ),
    (
        "save_states",
        &["core_id"],
        "cores",
        &["core_id"],
        "RESTRICT",
    ),
    (
        "save_states",
        &["core_version_id"],
        "core_versions",
        &["core_version_id"],
        "RESTRICT",
    ),
    (
        "save_states",
        &["thumbnail_media_asset_id"],
        "media_assets",
        &["id"],
        "SET NULL",
    ),
    ("sessions", &["game_id"], "games", &["id"], "RESTRICT"),
    ("sessions", &["release_id"], "releases", &["id"], "RESTRICT"),
    ("sessions", &["content_id"], "contents", &["id"], "RESTRICT"),
    ("sessions", &["core_id"], "cores", &["core_id"], "RESTRICT"),
    (
        "sessions",
        &["core_version_id"],
        "core_versions",
        &["core_version_id"],
        "RESTRICT",
    ),
    (
        "sessions",
        &["runtime_version_id"],
        "runtime_versions",
        &["runtime_version_id"],
        "RESTRICT",
    ),
];

/// The unique keys declared as `UNIQUE` clauses rather than as named indexes.
///
/// `DATA_MODEL.md` §3.8 lists them with the key audit; §3.9 rule 4 is why two of
/// them exist at all — `releases (id, game_id)` is what makes the composite
/// foreign key from `contents` possible, and `content_derivations (content_id,
/// kind)` does the same for its members.
const UNIQUE_KEYS: &[(&str, &[&str])] = &[
    ("systems", &["catalog_key"]),
    ("library_sources", &["location", "platform_locator_kind"]),
    ("releases", &["game_id", "release_key"]),
    ("releases", &["id", "game_id"]),
    (
        "content_derivation_members",
        &["content_id", "kind", "member_index"],
    ),
    ("metadata_providers", &["provider_id"]),
    ("cores", &["component_key"]),
    ("core_versions", &["component_key", "platform", "build_id"]),
    ("core_versions", &["core_id", "platform", "build_id"]),
    (
        "runtime_versions",
        &["component_key", "platform", "version"],
    ),
    ("save_states", &["file_relative_path"]),
];

/// Every index schema v1 declares with a name of its own, as
/// `(name, table, columns, unique, predicate)`.
///
/// `DATA_MODEL.md` §3.9 rule 5 is the reason most of these exist: a unique key
/// whose columns can be `NULL` is not enforceable as a `UNIQUE` clause — SQLite
/// treats `NULL`s as distinct — so it is declared as a partial unique index whose
/// predicate states that the key columns are non-NULL for exactly the rows it
/// covers. The remaining entries are the ordinary lookups the model documents.
///
/// A predicate is part of the assertion and not decoration: an index whose
/// predicate says the wrong thing protects the wrong invariant while still
/// looking like it is there.
const NAMED_INDEXES: &[IndexSpec] = &[
    (
        "library_sources_availability",
        "library_sources",
        &["availability"],
        false,
        None,
    ),
    (
        "library_sources_removed_at",
        "library_sources",
        &["removed_at"],
        false,
        None,
    ),
    (
        "scan_runs_started_at",
        "scan_runs",
        &["started_at"],
        false,
        None,
    ),
    (
        "scan_runs_target_source_id_started_at",
        "scan_runs",
        &["target_source_id", "started_at"],
        false,
        None,
    ),
    ("scan_runs_status", "scan_runs", &["status"], false, None),
    (
        "scan_run_issues_scan_run_id_kind",
        "scan_run_issues",
        &["scan_run_id", "kind"],
        false,
        None,
    ),
    (
        "scan_run_issues_prevents_reconciliation",
        "scan_run_issues",
        &["prevents_reconciliation"],
        false,
        None,
    ),
    ("games_is_favorite", "games", &["is_favorite"], false, None),
    (
        "games_is_hidden_is_ignored",
        "games",
        &["is_hidden", "is_ignored"],
        false,
        None,
    ),
    ("releases_game_id", "releases", &["game_id"], false, None),
    (
        "releases_system_id",
        "releases",
        &["system_id"],
        false,
        None,
    ),
    (
        "releases_system_id_release_type",
        "releases",
        &["system_id", "release_type"],
        false,
        None,
    ),
    (
        "releases_release_date",
        "releases",
        &["release_date"],
        false,
        None,
    ),
    (
        "release_regions_region",
        "release_regions",
        &["region"],
        false,
        None,
    ),
    (
        "release_languages_language",
        "release_languages",
        &["language"],
        false,
        None,
    ),
    (
        "contents_release_id",
        "contents",
        &["release_id"],
        false,
        None,
    ),
    ("contents_game_id", "contents", &["game_id"], false, None),
    (
        "contents_validation_state",
        "contents",
        &["validation_state"],
        false,
        None,
    ),
    (
        "content_locations_source_id_relative_path_file",
        "content_locations",
        &["source_id", "relative_path"],
        true,
        Some("location_kind = 'File'"),
    ),
    (
        "content_locations_archive_content_id_archive_entry",
        "content_locations",
        &["archive_content_id"],
        true,
        Some("location_kind = 'ArchiveEntry'"),
    ),
    (
        "content_locations_source_id_file",
        "content_locations",
        &["source_id"],
        false,
        Some("location_kind = 'File'"),
    ),
    (
        "content_locations_content_id",
        "content_locations",
        &["content_id"],
        false,
        None,
    ),
    (
        "content_locations_state",
        "content_locations",
        &["state"],
        false,
        None,
    ),
    (
        "content_derivations_kind",
        "content_derivations",
        &["kind"],
        false,
        None,
    ),
    (
        "content_derivation_members_source_content_id",
        "content_derivation_members",
        &["source_content_id"],
        false,
        None,
    ),
    (
        "content_fingerprints_content_id_kind_algorithm_payload_container",
        "content_fingerprints",
        &["content_id", "fingerprint_kind", "algorithm"],
        true,
        Some("fingerprint_kind IN ('Payload', 'Container')"),
    ),
    (
        "content_fingerprints_content_id_kind_algorithm_entry_path_entry_list",
        "content_fingerprints",
        &["content_id", "fingerprint_kind", "algorithm", "entry_path"],
        true,
        Some("fingerprint_kind = 'EntryList'"),
    ),
    (
        "content_fingerprints_algorithm_digest_payload",
        "content_fingerprints",
        &["algorithm", "digest"],
        true,
        Some("fingerprint_kind = 'Payload'"),
    ),
    (
        "content_fingerprints_content_id",
        "content_fingerprints",
        &["content_id"],
        false,
        None,
    ),
    (
        "content_fingerprints_source_scan_run_id",
        "content_fingerprints",
        &["source_scan_run_id"],
        false,
        None,
    ),
    (
        "provider_values_provider_id_subject_id_field_key_value_index_neutral",
        "provider_values",
        &[
            "provider_id",
            "subject_kind",
            "subject_id",
            "field_key",
            "value_index",
        ],
        true,
        Some("locale IS NULL"),
    ),
    (
        "provider_values_provider_id_subject_id_field_key_locale_value_index_localised",
        "provider_values",
        &[
            "provider_id",
            "subject_kind",
            "subject_id",
            "field_key",
            "locale",
            "value_index",
        ],
        true,
        Some("locale IS NOT NULL"),
    ),
    (
        "provider_values_subject_kind_subject_id_field_key",
        "provider_values",
        &["subject_kind", "subject_id", "field_key"],
        false,
        None,
    ),
    (
        "provider_values_scrape_run_id",
        "provider_values",
        &["scrape_run_id"],
        false,
        None,
    ),
    (
        "manual_overrides_subject_kind_subject_id_field_key_neutral",
        "manual_overrides",
        &["subject_kind", "subject_id", "field_key"],
        true,
        Some("locale IS NULL"),
    ),
    (
        "manual_overrides_subject_kind_subject_id_field_key_locale_localised",
        "manual_overrides",
        &["subject_kind", "subject_id", "field_key", "locale"],
        true,
        Some("locale IS NOT NULL"),
    ),
    (
        "manual_overrides_subject_kind_subject_id",
        "manual_overrides",
        &["subject_kind", "subject_id"],
        false,
        None,
    ),
    (
        "scrape_runs_status",
        "scrape_runs",
        &["status"],
        false,
        None,
    ),
    (
        "scrape_runs_started_at",
        "scrape_runs",
        &["started_at"],
        false,
        None,
    ),
    (
        "scrape_run_items_scrape_run_id_status",
        "scrape_run_items",
        &["scrape_run_id", "status"],
        false,
        None,
    ),
    (
        "media_assets_media_type",
        "media_assets",
        &["media_type"],
        false,
        None,
    ),
    (
        "media_assets_content_digest",
        "media_assets",
        &["content_digest"],
        false,
        None,
    ),
    (
        "media_assets_state",
        "media_assets",
        &["state"],
        false,
        None,
    ),
    (
        "media_asset_references_media_asset_id",
        "media_asset_references",
        &["media_asset_id"],
        false,
        None,
    ),
    (
        "firmware_entries_filename",
        "firmware_entries",
        &["filename"],
        false,
        None,
    ),
    (
        "firmware_entries_digest",
        "firmware_entries",
        &["digest"],
        false,
        None,
    ),
    (
        "firmware_entries_read_state",
        "firmware_entries",
        &["read_state"],
        false,
        None,
    ),
    (
        "core_selection_overrides_scope_system_id_system",
        "core_selection_overrides",
        &["scope_system_id"],
        true,
        Some("scope_kind = 'System'"),
    ),
    (
        "core_selection_overrides_scope_game_id_game",
        "core_selection_overrides",
        &["scope_game_id"],
        true,
        Some("scope_kind = 'Game'"),
    ),
    (
        "core_selection_overrides_scope_release_id_release",
        "core_selection_overrides",
        &["scope_release_id"],
        true,
        Some("scope_kind = 'Release'"),
    ),
    (
        "core_selection_overrides_core_id",
        "core_selection_overrides",
        &["core_id"],
        false,
        None,
    ),
    (
        "managed_components_integrity_state",
        "managed_components",
        &["integrity_state"],
        false,
        None,
    ),
    (
        "managed_components_core_version_id",
        "managed_components",
        &["core_version_id"],
        false,
        None,
    ),
    (
        "managed_components_runtime_version_id",
        "managed_components",
        &["runtime_version_id"],
        false,
        None,
    ),
    (
        "retroarch_setting_overrides_setting_key_global",
        "retroarch_setting_overrides",
        &["setting_key"],
        true,
        Some("scope_kind = 'Global'"),
    ),
    (
        "retroarch_setting_overrides_scope_system_id_setting_key_system",
        "retroarch_setting_overrides",
        &["scope_system_id", "setting_key"],
        true,
        Some("scope_kind = 'System'"),
    ),
    (
        "retroarch_setting_overrides_scope_game_id_setting_key_game",
        "retroarch_setting_overrides",
        &["scope_game_id", "setting_key"],
        true,
        Some("scope_kind = 'Game'"),
    ),
    (
        "retroarch_setting_overrides_scope_kind_scope_system_id",
        "retroarch_setting_overrides",
        &["scope_kind", "scope_system_id"],
        false,
        None,
    ),
    (
        "retroarch_setting_overrides_scope_kind_scope_game_id",
        "retroarch_setting_overrides",
        &["scope_kind", "scope_game_id"],
        false,
        None,
    ),
    (
        "core_option_overrides_core_version_id_option_key_core_defaults",
        "core_option_overrides",
        &["core_version_id", "option_key"],
        true,
        Some("scope_kind = 'CoreDefaults'"),
    ),
    (
        "core_option_overrides_core_version_id_scope_system_id_option_key_system",
        "core_option_overrides",
        &["core_version_id", "scope_system_id", "option_key"],
        true,
        Some("scope_kind = 'System'"),
    ),
    (
        "core_option_overrides_core_version_id_scope_game_id_option_key_game",
        "core_option_overrides",
        &["core_version_id", "scope_game_id", "option_key"],
        true,
        Some("scope_kind = 'Game'"),
    ),
    (
        "core_option_overrides_core_version_id",
        "core_option_overrides",
        &["core_version_id"],
        false,
        None,
    ),
    (
        "core_option_overrides_scope_kind_scope_game_id",
        "core_option_overrides",
        &["scope_kind", "scope_game_id"],
        false,
        None,
    ),
    (
        "save_states_release_id_created_at",
        "save_states",
        &["release_id", "created_at"],
        false,
        None,
    ),
    ("save_states_slot", "save_states", &["slot"], false, None),
    (
        "save_states_core_id_core_version_id",
        "save_states",
        &["core_id", "core_version_id"],
        false,
        None,
    ),
    ("save_states_state", "save_states", &["state"], false, None),
    (
        "sessions_game_id_started_at",
        "sessions",
        &["game_id", "started_at"],
        false,
        None,
    ),
    ("sessions_state", "sessions", &["state"], false, None),
    (
        "sessions_started_at",
        "sessions",
        &["started_at"],
        false,
        None,
    ),
    (
        "sessions_state_active",
        "sessions",
        &["state"],
        true,
        Some("state = 'Active'"),
    ),
];

/// Every digest column, as `(table, column)`.
///
/// `DATA_MODEL.md` §3.2 fixes a SHA-256 digest as a 32-byte `BLOB`. The release
/// key is one too, even though it is not a file's digest: §6.2 defines it as the
/// digest of the release's normalized identity. None of them is an identity.
const DIGEST_COLUMNS: &[(&str, &str)] = &[
    ("releases", "release_key"),
    ("content_fingerprints", "digest"),
    ("media_assets", "content_digest"),
    ("firmware_entries", "digest"),
    ("core_versions", "artifact_digest"),
    ("runtime_versions", "artifact_digest"),
];

/// The FTS5 projection's columns, in the order the rebuild writes them.
///
/// `DATA_MODEL.md` §17.1 lists the values the projection holds: the four
/// searchable text values first and the filter and sort values after them, which
/// are `UNINDEXED` because they are compared and never matched.
const SEARCH_INDEX_COLUMNS: &[&str] = &[
    "game_id",
    "title",
    "alternate_title",
    "release_title",
    "system_name",
    "regions",
    "languages",
    "release_type",
    "release_date",
    "is_favorite",
    "is_hidden",
    "is_ignored",
];

// ---------------------------------------------------------------------------
// Version and ledger
// ---------------------------------------------------------------------------

/// A new database is at schema version 1, and schema v1 is what it holds.
#[test]
fn a_new_database_migrates_to_schema_version_one() {
    let (temporary, database) = migrated();

    assert_eq!(
        database
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::new(1),
        "applying the shipped catalogue brings a new database to version 1"
    );

    let connection = super::inspect(&temporary);

    assert!(
        table_names(&connection).contains(&"games".to_owned()),
        "version 1 means the schema is there, not only that the ledger says so"
    );

    drop(database);
}

/// A database the foundation created on its own upgrades to schema version 1.
///
/// This is the second start state of Issue #109: a database that already exists
/// with the ledger in place and no migration applied. The upgrade is the same
/// migration as a fresh install — there is no separate path, and none is needed.
#[test]
fn a_foundation_only_database_upgrades_to_schema_version_one() {
    let temporary = TempDatabase::new();

    let foundation = temporary
        .open_with(MigrationRunner::new(&[]))
        .expect("a database with no migration must open");

    assert_eq!(
        foundation
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::NONE,
        "the foundation alone is version 0"
    );
    assert_eq!(
        table_names(&super::inspect(&temporary)),
        vec![LEDGER_TABLE.to_owned()],
        "a version 0 database carries the ledger and nothing else"
    );

    drop(foundation);

    // The same file, opened by a build that carries schema v1.
    let upgraded = temporary.open().expect("the upgrade must apply");

    assert_eq!(
        upgraded
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::new(1),
        "the foundation-only database must upgrade to version 1"
    );

    let connection = super::inspect(&temporary);

    for (table, _) in TABLES {
        assert!(
            table_names(&connection).contains(&(*table).to_owned()),
            "the upgrade must create `{table}`"
        );
    }

    assert_eq!(
        count(&connection, LEDGER_TABLE),
        1,
        "the upgrade must record exactly the migration it applied"
    );

    drop(upgraded);
}

/// The ledger records exactly migration 1, and records it completely.
#[test]
fn the_ledger_records_exactly_migration_one() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    let rows: Vec<(i64, String, String, i64, String)> = connection
        .prepare(
            "SELECT version, description, checksum, applied_at, bitarchive_version \
             FROM schema_migrations ORDER BY version",
        )
        .expect("the ledger must be queryable")
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .expect("the ledger must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable");

    assert_eq!(
        rows.len(),
        1,
        "schema v1 is one migration, so the ledger holds one row: {rows:?}"
    );

    let (version, description, checksum, applied_at, bitarchive_version) = &rows[0];

    assert_eq!(*version, 1, "the recorded migration is number 1");
    assert!(
        !description.is_empty() && !description.contains("fixture"),
        "the ledger records the migration's purpose: {description}"
    );
    assert_eq!(
        checksum.len(),
        64,
        "the checksum is the hex SHA-256 of the migration's definition: {checksum}"
    );
    assert!(
        checksum
            .chars()
            .all(|digit| digit.is_ascii_hexdigit() && !digit.is_ascii_uppercase()),
        "the checksum is lower-case hex: {checksum}"
    );
    assert!(*applied_at > 0, "the ledger records when the migration ran");
    assert_eq!(
        bitarchive_version,
        env!("CARGO_PKG_VERSION"),
        "the ledger records which build applied the migration"
    );

    drop(database);
}

/// Opening a migrated database again applies nothing at all.
///
/// Idempotence is not "the second run happened to produce the same schema": the
/// assertion that matters is that nothing ran. The ledger row is compared
/// byte-for-byte including `applied_at`, so a migration that was re-applied — or
/// re-recorded — is caught even when the resulting schema looks identical.
#[test]
fn a_second_open_applies_nothing() {
    let temporary = TempDatabase::new();

    let first = temporary.open().expect("the first open must migrate");

    assert_eq!(
        first.schema_version().expect("readable"),
        SchemaVersion::new(1)
    );

    let after_first = {
        let connection = super::inspect(&temporary);

        (
            super::schema_objects(&connection),
            count(&connection, LEDGER_TABLE),
            count(&connection, SEARCH_INDEX),
        )
    };

    drop(first);

    let second = temporary.open().expect("the second open must succeed");

    assert_eq!(
        second.schema_version().expect("readable"),
        SchemaVersion::new(1),
        "a second open leaves the version where it was"
    );

    let after_second = {
        let connection = super::inspect(&temporary);

        (
            super::schema_objects(&connection),
            count(&connection, LEDGER_TABLE),
            count(&connection, SEARCH_INDEX),
        )
    };

    assert_eq!(
        after_first, after_second,
        "a second open must not create an object, re-apply a migration, or \
         duplicate a ledger row"
    );

    drop(second);
}

/// Schema v1 stays applied when a later migration fails.
///
/// The foundation's own tests cover the transaction with fixture migrations. This
/// one covers the interaction that matters for schema v1: migration 1 is one large
/// multi-statement migration, and a failure in a *later* step must not be able to
/// take any of it back. The failure rolls back only its own work, so the schema is
/// complete at version 1 and a retry has nothing to redo.
#[test]
fn schema_v1_survives_a_later_migration_failing() {
    let catalogue: &'static [Migration] = Box::leak(
        vec![
            BITARCHIVE_MIGRATIONS[0],
            Migration {
                version: 2,
                description: "fixture: a migration that fails halfway",
                sql: "CREATE TABLE fixture_orphans (id INTEGER PRIMARY KEY NOT NULL);\n\
                      INSERT INTO fixture_no_such_table (id) VALUES (1);\n",
            },
        ]
        .into_boxed_slice(),
    );

    let temporary = TempDatabase::new();

    let error = temporary
        .open_with(MigrationRunner::new(catalogue))
        .expect_err("the failing migration must fail the open");

    match error {
        DatabaseError::MigrationFailed { version, .. } => assert_eq!(
            version,
            SchemaVersion::new(2),
            "the failure is reported against the migration that failed"
        ),
        other => panic!("expected a migration failure, got {other:?}"),
    }

    let connection = super::inspect(&temporary);

    let recorded: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("the ledger must be readable");

    assert_eq!(
        recorded, 1,
        "schema v1 is applied and recorded; only the failed migration was rolled back"
    );

    let tables = table_names(&connection);

    for (table, _) in TABLES {
        assert!(
            tables.contains(&(*table).to_owned()),
            "the failure of a later migration must not undo `{table}`"
        );
    }

    assert!(
        !tables.contains(&"fixture_orphans".to_owned()),
        "the table the failed migration created before it failed must be gone"
    );
}

// ---------------------------------------------------------------------------
// Objects and columns
// ---------------------------------------------------------------------------

/// Every object schema v1 declares exists, and no undeclared table exists.
///
/// The comparison is exact, which is what makes it worth having: a table that
/// schema v1 does not declare is a schema decision that was not reviewed, and a
/// repository that grew a cache table of its own would be caught here. The
/// projection's shadow tables are excluded, because they are SQLite's
/// implementation of a declared virtual table rather than objects — the FTS5 test
/// below checks them.
#[test]
fn the_schema_holds_every_declared_object_and_nothing_else() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    let mut expected = vec![LEDGER_TABLE.to_owned()];
    expected.extend(TABLES.iter().map(|(table, _)| (*table).to_owned()));
    expected.push(SEARCH_INDEX.to_owned());
    expected.sort();

    assert_eq!(
        table_names(&connection),
        expected,
        "the schema must hold exactly the tables schema v1 declares"
    );

    let others: Vec<(String, String)> = super::schema_objects(&connection)
        .into_iter()
        .filter(|(kind, _)| kind != "table" && kind != "index")
        .collect();

    assert!(
        others.is_empty(),
        "schema v1 declares no view, trigger, or virtual table beyond the \
         projection: {others:?}"
    );

    drop(database);
}

/// Every table has exactly the declared columns, in the declared order, with the
/// declared types and nullability.
///
/// This is the contract the schema makes to every reader and writer above it, and
/// it is where a column that should not exist would show up: no table gains a
/// place to put a ROM's bytes, a path becomes an identity, or a timestamp becomes
/// a text date. `DATA_MODEL.md` §3.2 is the encoding, and each table's section is
/// the column list.
#[test]
fn every_table_has_exactly_the_declared_columns() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    for (table, expected) in TABLES {
        let expected: Vec<Column> = expected
            .iter()
            .map(|(name, declared_type, not_null)| Column {
                name: (*name).to_owned(),
                declared_type: (*declared_type).to_owned(),
                not_null: *not_null,
            })
            .collect();

        assert_eq!(
            columns(&connection, table),
            expected,
            "`{table}` must have exactly the columns the data model gives it"
        );
    }

    drop(database);
}

/// Primary keys are the ones the model fixes.
#[test]
fn primary_keys_match_the_data_model() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    assert_eq!(
        PRIMARY_KEYS.len(),
        TABLES.len(),
        "every table's key is asserted, so a table added without one is caught"
    );

    for (table, expected) in PRIMARY_KEYS {
        let expected: Vec<String> = expected.iter().map(|column| (*column).to_owned()).collect();

        assert_eq!(
            primary_key(&connection, table),
            expected,
            "`{table}` must have exactly this primary key"
        );
    }

    drop(database);
}

/// Foreign keys are the ones the model fixes, including their delete behaviour.
///
/// The comparison is exact in both directions: a reference the model does not
/// declare is as much a defect as a missing one, and a `CASCADE` where the model
/// says `RESTRICT` would silently delete rows the model protects.
#[test]
fn foreign_keys_match_the_data_model() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    for (table, _) in TABLES {
        let declared: Vec<super::ForeignKey> = FOREIGN_KEYS
            .iter()
            .filter(|(child, ..)| child == table)
            .map(
                |(_, columns, parent, parent_columns, on_delete)| super::ForeignKey {
                    columns: columns.iter().map(|column| (*column).to_owned()).collect(),
                    parent: (*parent).to_owned(),
                    parent_columns: parent_columns
                        .iter()
                        .map(|column| (*column).to_owned())
                        .collect(),
                    on_delete: (*on_delete).to_owned(),
                    on_update: "NO ACTION".to_owned(),
                },
            )
            .collect::<Vec<_>>();

        let mut declared = declared;
        declared.sort_by(|left, right| {
            (&left.parent, &left.columns).cmp(&(&right.parent, &right.columns))
        });

        let found = super::foreign_keys(&connection, table);

        assert_eq!(
            found, declared,
            "`{table}` must declare exactly these foreign keys"
        );

        // A key column in this schema is an identity, and an identity does not
        // change, so there is nothing for an update action to do. The model fixes
        // no `ON UPDATE` behaviour anywhere, and the default is what that means.
        for key in &found {
            assert_eq!(
                key.on_update,
                "NO ACTION",
                "`{table}` must not declare an ON UPDATE action: {}",
                key.columns.join(", ")
            );
        }
    }

    drop(database);
}

/// Every unique key the model declares is a real key.
///
/// `DATA_MODEL.md` §3.8 lists them; some are `UNIQUE` clauses and some are the
/// partial unique indexes above. Both forms are checked here so that a key which
/// exists in the document and not in the schema cannot hide behind the other
/// representation.
#[test]
fn every_declared_unique_key_is_a_unique_index() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    for (table, key) in UNIQUE_KEYS {
        let expected: Vec<String> = key.iter().map(|column| (*column).to_owned()).collect();

        let found = indexes(&connection, table)
            .into_iter()
            .any(|index| index.unique && index.columns == expected);

        assert!(
            found,
            "`{table}` must carry a unique index over ({})",
            key.join(", ")
        );
    }

    drop(database);
}

/// Every declared index exists under its own name, over the declared columns and
/// with the declared predicate.
#[test]
fn required_indexes_exist_with_their_columns() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    for (name, table, expected_columns, unique, predicate) in NAMED_INDEXES {
        let expected_columns: Vec<String> = expected_columns
            .iter()
            .map(|column| (*column).to_owned())
            .collect();
        let expected_predicate = predicate.map(str::to_owned);

        let index = indexes(&connection, table)
            .into_iter()
            .find(|index| &index.name == name)
            .unwrap_or_else(|| panic!("`{name}` must exist on `{table}`"));

        assert_eq!(
            index.columns, expected_columns,
            "`{name}` must index exactly these columns, in this order"
        );
        assert_eq!(
            index.unique,
            *unique,
            "`{name}` has the wrong uniqueness: it is {}",
            if index.unique { "unique" } else { "not unique" }
        );
        assert_eq!(
            index.partial,
            expected_predicate.is_some(),
            "`{name}` must cover {}",
            if index.partial {
                "only the rows its predicate selects"
            } else {
                "every row"
            }
        );
        assert_eq!(
            index.predicate, expected_predicate,
            "`{name}` must select exactly the rows the data model gives it"
        );
    }

    drop(database);
}

/// No index exists that the model does not declare.
///
/// SQLite implements a `PRIMARY KEY` or `UNIQUE` clause as an index of its own,
/// so those are accounted for by matching them against the declared keys. An index
/// that matches neither a declared named index nor a declared key is an index
/// nobody asked for — the speculative performance index this schema deliberately
/// does not carry.
#[test]
fn no_undeclared_index_exists() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    let named: Vec<(&str, &str)> = NAMED_INDEXES
        .iter()
        .map(|(name, table, ..)| (*name, *table))
        .collect();

    for (table, _) in TABLES {
        for index in indexes(&connection, table) {
            if named.contains(&(index.name.as_str(), *table)) {
                continue;
            }

            assert!(
                index.name.starts_with("sqlite_autoindex_"),
                "`{}` on `{table}` is not an index the data model declares",
                index.name
            );

            let key = primary_key(&connection, table);
            let is_a_declared_key = index.columns == key
                || UNIQUE_KEYS
                    .iter()
                    .filter(|(declared, _)| declared == table)
                    .any(|(_, declared)| {
                        *declared == index.columns.iter().map(String::as_str).collect::<Vec<_>>()
                    });

            assert!(
                is_a_declared_key,
                "`{}` on `{table}` enforces a key the data model does not declare: ({})",
                index.name,
                index.columns.join(", ")
            );
        }
    }

    drop(database);
}

/// The two documented descending sort orders are declared as such.
///
/// `DATA_MODEL.md` §15.1 and §16.1 give two lookups as `… , created_at DESC` and
/// `… , started_at DESC`. `PRAGMA index_info` does not report the direction, so it
/// is read from the statement that created the index.
#[test]
fn the_documented_descending_sort_orders_are_declared() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    for (name, column) in [
        ("save_states_release_id_created_at", "created_at"),
        ("sessions_game_id_started_at", "started_at"),
    ] {
        let sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [name],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| panic!("`{name}` must exist"));

        assert!(
            sql.contains(&format!("{column} DESC")),
            "`{name}` must sort `{column}` descending: {sql}"
        );
    }

    drop(database);
}

// ---------------------------------------------------------------------------
// Identities, digests, and what is not stored
// ---------------------------------------------------------------------------

/// Every `BLOB` column is an identity or a digest, and nothing else.
///
/// This is the structural half of two invariants at once. Identities are 16-byte
/// UUIDv7 values (invariant 2, `DATA_MODEL.md` §3.2) and digests are 32-byte
/// SHA-256 values (invariant 3, §3.4); there is no third kind of `BLOB`, which is
/// what rules out a column that would carry a ROM, an ISO, a firmware payload, or
/// an archive's bytes into the database. Content stays in the files the user owns.
#[test]
fn blob_columns_hold_identities_and_digests_only() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    for (table, _) in TABLES {
        for column in columns(&connection, table) {
            if column.declared_type != "BLOB" {
                continue;
            }

            let is_identity = column.name == "id" || column.name.ends_with("_id");
            let is_digest = DIGEST_COLUMNS
                .iter()
                .any(|(owner, name)| owner == table && *name == column.name);

            assert!(
                is_identity || is_digest,
                "`{table}.{}` is a BLOB that is neither an identity nor a digest",
                column.name
            );
        }
    }

    drop(database);
}

/// A digest is never an identity.
///
/// `DATA_MODEL.md` §3.4 and invariant 3: a hash is a fingerprint. No digest column
/// is part of any primary key, and the one key a digest appears in — the
/// recognition key of §7.1 — is a unique index over `(algorithm, digest)` on a
/// table whose key is a storage one. A hash is also never a foreign-key target,
/// which the foreign-key test above covers.
#[test]
fn no_digest_column_is_a_primary_key() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    for (table, column) in DIGEST_COLUMNS {
        let declared_type = columns(&connection, table)
            .into_iter()
            .find(|declared| declared.name == *column)
            .unwrap_or_else(|| panic!("`{table}.{column}` must exist"))
            .declared_type;

        assert_eq!(
            declared_type, "BLOB",
            "`{table}.{column}` is a digest, and a digest is a BLOB"
        );

        assert!(
            !primary_key(&connection, table).contains(&(*column).to_owned()),
            "`{table}.{column}` is a fingerprint, not an identity"
        );
    }

    // The recognition key is a key, but a recognition key: it is a unique index
    // over the digest and the algorithm, on a table keyed by `row_id`.
    let recognition = indexes(&connection, "content_fingerprints")
        .into_iter()
        .find(|index| index.name == "content_fingerprints_algorithm_digest_payload")
        .expect("the recognition key must exist");

    assert!(
        recognition.unique && recognition.columns == vec!["algorithm", "digest"],
        "the recognition key is a unique index over (algorithm, digest), and it is \
         not the table's identity"
    );

    drop(database);
}

/// An identity comes back as a 16-byte blob and a digest as a 32-byte blob.
///
/// The declared type is half of the contract; this is the other half, on a row
/// that was really written. It is also what shows that the encoding survives the
/// round trip rather than being coerced into something else — a UUID that came
/// back as text would mean the read path had to parse it everywhere.
#[test]
fn identities_and_digests_round_trip_as_blobs_of_their_documented_length() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    let fixture = super::seed(&connection);

    let (identity_type, identity_length): (String, i64) = connection
        .query_row(
            "SELECT typeof(id), length(id) FROM games WHERE id = ?1",
            [&fixture.game],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the seeded game must be readable");

    assert_eq!(identity_type, "blob", "an identity is stored as a blob");
    assert_eq!(identity_length, 16, "a UUIDv7 identity is 16 bytes");

    connection
        .execute(
            "INSERT INTO content_fingerprints \
             (content_id, fingerprint_kind, algorithm, digest, entry_path, byte_size, \
              computed_at, source_scan_run_id) \
             VALUES (?1, 'Payload', 'Sha256', ?2, NULL, 1024, 0, NULL)",
            params![&fixture.content, super::digest(0x22)],
        )
        .expect("a fingerprint must be insertable");

    let (digest_type, digest_length): (String, i64) = connection
        .query_row(
            "SELECT typeof(digest), length(digest) FROM content_fingerprints",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the fingerprint must be readable");

    assert_eq!(digest_type, "blob", "a digest is stored as a blob");
    assert_eq!(digest_length, 32, "a SHA-256 digest is 32 bytes");

    drop(database);
}

// ---------------------------------------------------------------------------
// The FTS5 projection
// ---------------------------------------------------------------------------

/// The search projection is an FTS5 table with the declared columns, and the
/// shadow tables SQLite implements it with.
#[test]
fn the_search_projection_is_an_fts5_table_with_the_declared_columns() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    let sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [SEARCH_INDEX],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| panic!("`{SEARCH_INDEX}` must exist"));

    assert!(
        sql.to_lowercase().contains("using fts5"),
        "the projection must be an FTS5 table: {sql}"
    );

    let declared: Vec<String> = columns(&connection, SEARCH_INDEX)
        .into_iter()
        .map(|column| column.name)
        .collect();

    assert_eq!(
        declared,
        SEARCH_INDEX_COLUMNS
            .iter()
            .map(|column| (*column).to_owned())
            .collect::<Vec<_>>(),
        "the projection must hold exactly the values the data model lists"
    );

    // The projection's storage is SQLite's, and it is checked by looking it up
    // directly: `table_names` deliberately leaves the shadow tables out, because
    // they are the implementation of a declared object rather than an object.
    let objects = super::schema_objects(&connection);

    for shadow in super::FTS_SHADOW_TABLES {
        assert!(
            objects
                .iter()
                .any(|(kind, name)| kind == "table" && name == shadow),
            "FTS5 must implement the projection with its own `{shadow}` table"
        );
    }

    assert!(
        super::indexes(&connection, SEARCH_INDEX).is_empty(),
        "an FTS5 table carries no index of its own, and therefore no declarative \
         unique constraint: one row per game is a rebuild invariant (§17.4)"
    );

    drop(database);
}

/// The projection indexes text, finds it case-insensitively, and keeps the values
/// it is given.
///
/// The row is written the way the rebuild writes one: the game's identity as a
/// 16-byte blob, the searchable text, and the filter values. What is asserted is
/// what the query layer depends on — a match finds the row, the filter columns
/// come back unchanged, and the identity comes back as the bytes that went in.
#[test]
fn the_search_projection_indexes_and_finds_text() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    let game = id(0x31);

    connection
        .execute(
            "INSERT INTO search_index \
             (game_id, title, alternate_title, release_title, system_name, regions, \
              languages, release_type, release_date, is_favorite, is_hidden, is_ignored) \
             VALUES (?1, 'Pokemon Rot', 'Pocket Monsters', 'Pokemon Rot (Europe)', \
                     'Nintendo Entertainment System', 'Europe', 'en', 'Official', '1996', \
                     1, 0, 0)",
            [&game],
        )
        .expect("the projection must accept a row");

    // The row comes back the way the query layer needs it: the game identity as
    // the bytes that went in, so it can be joined back to `games`, and the filter
    // values unchanged.
    let matched = |term: &str| -> Vec<(String, i64, Vec<u8>, Option<i64>)> {
        let mut statement = connection
            .prepare(
                "SELECT typeof(game_id), length(game_id), game_id, is_favorite \
                 FROM search_index WHERE search_index MATCH ?1",
            )
            .expect("the projection must be queryable");

        statement
            .query_map([term], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .expect("the projection must be queryable")
            .collect::<Result<Vec<_>, _>>()
            .expect("every row must be readable")
    };

    let found = matched("rot");

    assert_eq!(found.len(), 1, "a word in the title must be found");
    assert_eq!(
        found[0].0, "blob",
        "the identity must come back as bytes, not as text to parse"
    );
    assert_eq!(found[0].1, 16, "a UUIDv7 identity is 16 bytes");
    assert_eq!(
        found[0].2, game,
        "the projection must return the identity that was written"
    );
    assert_eq!(
        found[0].3,
        Some(1),
        "the filter values must come back as they were written"
    );

    assert_eq!(matched("ROT").len(), 1, "search is case-insensitive (§3.6)");
    assert_eq!(
        matched("monsters").len(),
        1,
        "the alternate title is searchable too (§17.1)"
    );
    assert_eq!(matched("Metroid").len(), 0, "an absent word finds nothing");
    assert_eq!(
        matched("rot*").len(),
        1,
        "a prefix query is what a search field needs (§17.2)"
    );

    // Diacritics are folded at the strongest setting, so a search typed without
    // them finds the title as it was written.
    let second = id(0x32);

    connection
        .execute(
            "INSERT INTO search_index (game_id, title) VALUES (?1, 'Stadium Edition')",
            [&second],
        )
        .expect("a second projection row must be insertable");

    assert_eq!(
        matched("edition").len(),
        1,
        "a word in the second row is found as well"
    );

    connection
        .execute(
            "UPDATE search_index SET title = 'Stadium Édition' WHERE game_id = ?1",
            [&second],
        )
        .expect("the projection row must be updatable");

    assert_eq!(
        matched("edition").len(),
        1,
        "diacritics are folded, so `edition` finds `Édition`"
    );

    drop(database);
}

/// The projection holds no domain identity of its own.
///
/// `DATA_MODEL.md` §17.4: the projection is rebuilt wholesale, and "one row per
/// game" is an invariant of that rebuild rather than a constraint the schema can
/// state — FTS5 tables carry no unique constraint at all. The identity the
/// projection stores is a copy for the join back, which is why the schema itself
/// is allowed to hold two rows for one game: the rebuild is what makes that
/// impossible, and a declaration that pretended otherwise would be a false
/// promise.
#[test]
fn the_search_projection_keeps_no_domain_identity() {
    let (temporary, database) = migrated();
    let connection = super::inspect(&temporary);

    let game = id(0x41);

    for title in ["First projection row", "Second projection row"] {
        connection
            .execute(
                "INSERT INTO search_index (game_id, title) VALUES (?1, ?2)",
                params![&game, title],
            )
            .expect("the projection must accept a row");
    }

    assert_eq!(
        count(&connection, SEARCH_INDEX),
        2,
        "the projection has no declarative uniqueness; the rebuild is what \
         guarantees one row per game"
    );

    drop(database);
}
