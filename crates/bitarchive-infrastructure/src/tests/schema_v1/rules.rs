//! The rules schema v1 enforces, asserted by what SQLite accepts and refuses.
//!
//! A constraint that is written down and not in force protects nothing, so every
//! rule here is checked twice where both halves are meaningful: the row the model
//! allows is inserted, and the row the model forbids is *refused by SQLite* — not
//! by a caller remembering to check. A refusal is only accepted as a refusal when
//! SQLite reports a constraint failure, so a statement that failed for a typo
//! fails the test instead of passing it.
//!
//! `DATA_MODEL.md` is the source of every rule. A `CHECK` in schema v1 exists only
//! where the model fixes an invariant unambiguously — a shape, an enum, a
//! pairing — and the tests below are grouped the way those rules are: the shapes a
//! row may take, the keys it must respect, and what happens to it when the row it
//! points at goes away.
//!
//! # Why some deletions are tested here
//!
//! `ON DELETE` is a rule like any other, and it is the one where an invented
//! cascade would do the most damage: a `CASCADE` the model does not declare
//! deletes a user's data with no error to show for it. Each delete behaviour is
//! therefore asserted in the direction it was declared — the rows that really
//! belong to their parent go with it, the rows that outlive it are kept and their
//! reference cleared, and the rows the model protects refuse the delete outright.

use rusqlite::{Connection, Params, params};

use super::super::support::database::TempDatabase;
use super::{accepts, digest, id, inspect, migrated, refuses, seed};

/// Inserts a row a test needs as scaffolding.
///
/// These are not the rows under test, so a failure reports the statement and
/// SQLite's own message rather than an assertion about the schema.
#[track_caller]
fn insert<P: Params>(connection: &Connection, sql: &str, parameters: P) {
    if let Err(error) = connection.execute(sql, parameters) {
        panic!("the row a test needs must be insertable, but SQLite reported: {error}\n{sql}");
    }
}

/// Returns whether `query` selects at least one row.
#[track_caller]
fn exists<P: Params>(connection: &Connection, query: &str, parameters: P) -> bool {
    connection
        .query_row(&format!("SELECT EXISTS ({query})"), parameters, |row| {
            row.get(0)
        })
        .expect("the existence query must be runnable")
}

/// Returns whether the value `query` selects is `NULL`.
#[track_caller]
fn is_null<P: Params>(connection: &Connection, query: &str, parameters: P) -> bool {
    connection
        .query_row(&format!("SELECT ({query}) IS NULL"), parameters, |row| {
            row.get(0)
        })
        .expect("the null check must be runnable")
}

/// A `File` content location, which is the shape that names a source and a path.
const A_FILE_LOCATION: &str = "INSERT INTO content_locations \
     (content_id, location_kind, source_id, relative_path, archive_content_id, \
      archive_entry_path, observed_filename, observed_size, observed_mtime, state, \
      last_seen_scan_run_id, first_seen_at, last_seen_at) \
     VALUES (?1, 'File', ?2, ?3, NULL, NULL, ?3, 4096, 0, 'Present', NULL, 0, 0)";

/// An `ArchiveEntry` content location, which is the shape that names a container.
const AN_ARCHIVE_ENTRY: &str = "INSERT INTO content_locations \
     (content_id, location_kind, source_id, relative_path, archive_content_id, \
      archive_entry_path, observed_filename, observed_size, observed_mtime, state, \
      last_seen_scan_run_id, first_seen_at, last_seen_at) \
     VALUES (?1, 'ArchiveEntry', NULL, NULL, ?2, ?3, ?3, 4096, 0, 'Present', NULL, 0, 0)";

/// A `Content` row of the seeded release, so a test can have a second one.
const A_CONTENT: &str = "INSERT INTO contents \
     (id, release_id, game_id, content_kind, format, validation_state, \
      validation_detail, disc_index, created_at) \
     VALUES (?1, ?2, ?3, 'Content', 'nes', 'Valid', NULL, NULL, 0)";

/// A `Payload` fingerprint of the seeded content.
const A_FINGERPRINT: &str = "INSERT INTO content_fingerprints \
     (content_id, fingerprint_kind, algorithm, digest, entry_path, byte_size, \
      computed_at, source_scan_run_id) \
     VALUES (?1, 'Payload', 'Sha256', ?2, NULL, 1024, 0, NULL)";

// ---------------------------------------------------------------------------
// The shapes a row may take
// ---------------------------------------------------------------------------

/// A content location has exactly one of the two shapes the model gives it.
///
/// `DATA_MODEL.md` §5.2 and ARCHITECTURE.md §14.1: a `File` location names a
/// physical file through its source and its relative path, and an
/// `ArchiveEntry` location names an entry inside a container and carries neither
/// a source nor a path of its own. A row that mixes the two is not a location
/// with an odd extra column — it is a row whose meaning is undefined, which is why
/// the schema refuses it rather than leaving the rule to every reader.
#[test]
fn a_file_location_names_its_source_and_its_path() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    accepts(
        &connection,
        A_FILE_LOCATION,
        params![&fixture.content, &fixture.source, "Game (Europe).nes"],
        "a file location naming its source and its path",
    );

    refuses(
        &connection,
        "INSERT INTO content_locations \
         (content_id, location_kind, source_id, relative_path, archive_content_id, \
          archive_entry_path, observed_filename, state, first_seen_at) \
         VALUES (?1, 'File', ?2, 'Mixed shape.nes', ?3, 'Mixed shape.nes', \
                 'Mixed shape.nes', 'Present', 0)",
        params![&fixture.content, &fixture.source, &fixture.content],
        "a file location that also names an archive entry",
    );

    refuses(
        &connection,
        "INSERT INTO content_locations \
         (content_id, location_kind, source_id, relative_path, archive_content_id, \
          archive_entry_path, observed_filename, state, first_seen_at) \
         VALUES (?1, 'File', NULL, 'No source.nes', NULL, NULL, 'No source.nes', \
                 'Present', 0)",
        params![&fixture.content],
        "a file location with no source",
    );

    refuses(
        &connection,
        "INSERT INTO content_locations \
         (content_id, location_kind, source_id, relative_path, archive_content_id, \
          archive_entry_path, observed_filename, state, first_seen_at) \
         VALUES (?1, 'File', ?2, NULL, NULL, NULL, 'No path.nes', 'Present', 0)",
        params![&fixture.content, &fixture.source],
        "a file location with no relative path",
    );

    refuses(
        &connection,
        "INSERT INTO content_locations \
         (content_id, location_kind, source_id, relative_path, archive_content_id, \
          archive_entry_path, observed_filename, state, first_seen_at) \
         VALUES (?1, 'Device', ?2, 'From a device.nes', NULL, NULL, 'From a device.nes', \
                 'Present', 0)",
        params![&fixture.content, &fixture.source],
        "a location of a kind schema v1 does not have",
    );

    drop(database);
}

/// An archive entry location names its container, and one container holds one.
#[test]
fn an_archive_entry_location_names_its_container() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let container = id(0x07);

    insert(
        &connection,
        "INSERT INTO contents \
         (id, release_id, game_id, content_kind, format, validation_state, \
          validation_detail, disc_index, created_at) \
         VALUES (?1, ?2, ?3, 'ArchiveContainer', 'zip', 'Valid', NULL, NULL, 0)",
        params![&container, &fixture.release, &fixture.game],
    );

    accepts(
        &connection,
        AN_ARCHIVE_ENTRY,
        params![&fixture.content, &container, "Game (Europe).nes"],
        "an archive entry naming its container and its entry path",
    );

    refuses(
        &connection,
        "INSERT INTO content_locations \
         (content_id, location_kind, source_id, relative_path, archive_content_id, \
          archive_entry_path, observed_filename, state, first_seen_at) \
         VALUES (?1, 'ArchiveEntry', NULL, NULL, ?2, NULL, 'No entry.nes', 'Present', 0)",
        params![&fixture.content, &container],
        "an archive entry with no entry path",
    );

    refuses(
        &connection,
        "INSERT INTO content_locations \
         (content_id, location_kind, source_id, relative_path, archive_content_id, \
          archive_entry_path, observed_filename, state, first_seen_at) \
         VALUES (?1, 'ArchiveEntry', ?2, NULL, ?3, 'With source.nes', 'With source.nes', \
                 'Present', 0)",
        params![&fixture.content, &fixture.source, &container],
        "an archive entry that also names a library source",
    );

    refuses(
        &connection,
        "INSERT INTO content_locations \
         (content_id, location_kind, source_id, relative_path, archive_content_id, \
          archive_entry_path, observed_filename, state, first_seen_at) \
         VALUES (?1, 'ArchiveEntry', NULL, 'With path.nes', ?2, 'With path.nes', \
                 'With path.nes', 'Present', 0)",
        params![&fixture.content, &container],
        "an archive entry that also names a path of its own",
    );

    refuses(
        &connection,
        "INSERT INTO content_locations \
         (content_id, location_kind, source_id, relative_path, archive_content_id, \
          archive_entry_path, observed_filename, state, first_seen_at) \
         VALUES (?1, 'ArchiveEntry', NULL, NULL, NULL, 'No container.nes', \
                 'No container.nes', 'Present', 0)",
        params![&fixture.content],
        "an archive entry with no container",
    );

    // One supported archive holds at most one imported playable entry, and the
    // entry path is deliberately not part of what makes it unique (§5.2).
    let second = id(0x08);

    insert(
        &connection,
        A_CONTENT,
        params![&second, &fixture.release, &fixture.game],
    );

    refuses(
        &connection,
        AN_ARCHIVE_ENTRY,
        params![&second, &container, "Another entry.nes"],
        "a second entry for one archive container, claimed by another content",
    );

    // The same container, claimed by the *same* content, and differing only in the
    // entry path. The key is the container alone, so this is refused too: an
    // archive the scanner had to guess about is not a second location, and an
    // entry path in the key would make it look like one.
    refuses(
        &connection,
        AN_ARCHIVE_ENTRY,
        params![&fixture.content, &container, "Another entry.nes"],
        "a second entry for one archive container, claimed by the same content",
    );

    drop(database);
}

/// A scan run names a source for exactly one target kind.
///
/// `DATA_MODEL.md` §8.1: a `LibrarySource` scan names its source, and a
/// `FullLibrary` or `Rebuild` scan names none. The two halves are one rule, which
/// is why they are one check.
#[test]
fn a_scan_run_names_its_source_for_exactly_one_target_kind() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    const A_RUN: &str = "INSERT INTO scan_runs \
         (id, target_kind, target_source_id, status, started_at, finished_at, \
          reconciliation_eligible, discovered_count, imported_count, updated_count, \
          rediscovered_count, missing_count, unknown_count, unsupported_count, \
          problem_count) \
         VALUES (?1, ?2, ?3, 'Running', 0, NULL, 0, 0, 0, 0, 0, 0, 0, 0, 0)";

    accepts(
        &connection,
        A_RUN,
        params![id(0x09), "LibrarySource", &fixture.source],
        "a source scan naming its source",
    );

    accepts(
        &connection,
        A_RUN,
        params![id(0x0a), "Rebuild", Option::<&[u8]>::None],
        "a rebuild naming no source",
    );

    refuses(
        &connection,
        A_RUN,
        params![id(0x0b), "LibrarySource", Option::<&[u8]>::None],
        "a source scan with no source",
    );

    refuses(
        &connection,
        A_RUN,
        params![id(0x0c), "FullLibrary", &fixture.source],
        "a full-library scan that names a source",
    );

    refuses(
        &connection,
        A_RUN,
        params![id(0x0d), "Incremental", Option::<&[u8]>::None],
        "a scan of a target kind schema v1 does not have",
    );

    drop(database);
}

/// A core selection names exactly the scope column its kind requires.
///
/// `DATA_MODEL.md` §12.3 and invariant 21: the scopes are `System`, `Game`, and
/// optional `Release`. There is no global scope, so a global core default is not
/// merely unimplemented — there is no row shape that could express one.
#[test]
fn a_core_selection_override_names_exactly_its_scope_column() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let core = id(0x0e);

    insert(
        &connection,
        "INSERT INTO cores (core_id, component_key, created_at) VALUES (?1, 'mesen', 0)",
        [&core],
    );

    const AN_OVERRIDE: &str = "INSERT INTO core_selection_overrides \
         (scope_kind, scope_system_id, scope_game_id, scope_release_id, core_id, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 0)";

    accepts(
        &connection,
        AN_OVERRIDE,
        params![
            "System",
            &fixture.system,
            None::<&[u8]>,
            None::<&[u8]>,
            &core
        ],
        "a system core selection",
    );

    accepts(
        &connection,
        AN_OVERRIDE,
        params!["Game", None::<&[u8]>, &fixture.game, None::<&[u8]>, &core],
        "a game core selection",
    );

    accepts(
        &connection,
        AN_OVERRIDE,
        params![
            "Release",
            None::<&[u8]>,
            None::<&[u8]>,
            &fixture.release,
            &core
        ],
        "a release core selection",
    );

    refuses(
        &connection,
        AN_OVERRIDE,
        params![
            "Release",
            None::<&[u8]>,
            &fixture.game,
            None::<&[u8]>,
            &core
        ],
        "a release core selection that names the release as its game",
    );

    refuses(
        &connection,
        AN_OVERRIDE,
        params![
            "Library",
            None::<&[u8]>,
            None::<&[u8]>,
            None::<&[u8]>,
            &core
        ],
        "a core selection of a scope schema v1 does not have",
    );

    refuses(
        &connection,
        AN_OVERRIDE,
        params!["System", None::<&[u8]>, None::<&[u8]>, None::<&[u8]>, &core],
        "a core selection with no scope at all",
    );

    drop(database);
}

/// One core selection per scope.
#[test]
fn one_core_selection_override_per_scope() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let core = id(0x0e);
    let replacement = id(0x0f);

    insert(
        &connection,
        "INSERT INTO cores (core_id, component_key, created_at) VALUES (?1, 'mesen', 0)",
        [&core],
    );
    insert(
        &connection,
        "INSERT INTO cores (core_id, component_key, created_at) VALUES (?1, 'nestopia', 0)",
        [&replacement],
    );

    const A_SYSTEM_OVERRIDE: &str = "INSERT INTO core_selection_overrides \
         (scope_kind, scope_system_id, scope_game_id, scope_release_id, core_id, created_at) \
         VALUES ('System', ?1, NULL, NULL, ?2, 0)";

    insert(
        &connection,
        A_SYSTEM_OVERRIDE,
        params![&fixture.system, &core],
    );

    // Replacing a selection is a write to the row that is there, not a second row:
    // the schema makes the second row unrepresentable (§12.3).
    refuses(
        &connection,
        A_SYSTEM_OVERRIDE,
        params![&fixture.system, &replacement],
        "a second core selection for one system",
    );

    // The scopes are independent, so the same core can be selected per system and
    // per game at once.
    accepts(
        &connection,
        "INSERT INTO core_selection_overrides \
         (scope_kind, scope_system_id, scope_game_id, scope_release_id, core_id, created_at) \
         VALUES ('Game', NULL, ?1, NULL, ?2, 0)",
        params![&fixture.game, &replacement],
        "a game core selection alongside the system one",
    );

    drop(database);
}

/// A RetroArch setting names exactly the scope column its kind requires.
///
/// `DATA_MODEL.md` §13.1 and §13.4: the inheritance is `Global → System → Game`,
/// and there is deliberately no release scope, so a release-scoped setting has no
/// row shape.
#[test]
fn a_retroarch_setting_names_exactly_its_scope_column() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    const A_SETTING: &str = "INSERT INTO retroarch_setting_overrides \
         (scope_kind, scope_system_id, scope_game_id, setting_key, value_type, \
          value_bool, value_integer, value_float, value_text, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, NULL, 0, 0)";

    accepts(
        &connection,
        A_SETTING,
        params![
            "Global",
            None::<&[u8]>,
            None::<&[u8]>,
            "video_vsync",
            "Bool",
            1
        ],
        "a global setting with no scope column",
    );

    accepts(
        &connection,
        A_SETTING,
        params![
            "System",
            &fixture.system,
            None::<&[u8]>,
            "video_vsync",
            "Bool",
            0
        ],
        "a system setting",
    );

    accepts(
        &connection,
        A_SETTING,
        params![
            "Game",
            None::<&[u8]>,
            &fixture.game,
            "video_vsync",
            "Bool",
            1
        ],
        "a game setting",
    );

    refuses(
        &connection,
        A_SETTING,
        params![
            "System",
            None::<&[u8]>,
            &fixture.game,
            "video_vsync",
            "Bool",
            1
        ],
        "a system setting that names a game instead of a system",
    );

    refuses(
        &connection,
        A_SETTING,
        params![
            "Release",
            None::<&[u8]>,
            None::<&[u8]>,
            "video_vsync",
            "Bool",
            1
        ],
        "a release-scoped setting, which §13.4 deliberately does not have",
    );

    drop(database);
}

/// A RetroArch setting stores its value in the column its type names, and in no
/// other.
///
/// `DATA_MODEL.md` §13.2: a boolean is not stored three ways, and a float is not
/// stored as text. The stored type is what the generated configuration will be
/// written from, so an unrepresentable combination is refused when it is written
/// rather than when a `.cfg` is generated.
#[test]
fn a_retroarch_setting_stores_its_value_in_the_column_its_type_names() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    seed(&connection);

    const A_SETTING: &str = "INSERT INTO retroarch_setting_overrides \
         (scope_kind, scope_system_id, scope_game_id, setting_key, value_type, \
          value_bool, value_integer, value_float, value_text, created_at, updated_at) \
         VALUES ('Global', NULL, NULL, ?1, ?2, ?3, ?4, ?5, ?6, 0, 0)";

    accepts(
        &connection,
        A_SETTING,
        params![
            "video_vsync",
            "Bool",
            1,
            None::<i64>,
            None::<f64>,
            None::<&str>
        ],
        "a boolean setting stored as a boolean",
    );

    accepts(
        &connection,
        A_SETTING,
        params![
            "audio_volume",
            "Integer",
            None::<i64>,
            80,
            None::<f64>,
            None::<&str>
        ],
        "an integer setting stored as an integer",
    );

    accepts(
        &connection,
        A_SETTING,
        params![
            "video_scale",
            "Float",
            None::<i64>,
            None::<i64>,
            2.5,
            None::<&str>
        ],
        "a float setting stored as a float",
    );

    accepts(
        &connection,
        A_SETTING,
        params![
            "aspect_ratio",
            "Choice",
            None::<i64>,
            None::<i64>,
            None::<f64>,
            "4:3"
        ],
        "a choice setting stored as text",
    );

    accepts(
        &connection,
        A_SETTING,
        params![
            "custom_path",
            "Text",
            None::<i64>,
            None::<i64>,
            None::<f64>,
            "/tmp/roms"
        ],
        "a text setting stored as text",
    );

    refuses(
        &connection,
        A_SETTING,
        params![
            "video_vsync",
            "Bool",
            None::<i64>,
            1,
            None::<f64>,
            None::<&str>
        ],
        "a boolean stored as an integer",
    );

    refuses(
        &connection,
        A_SETTING,
        params![
            "aspect_ratio",
            "Choice",
            1,
            None::<i64>,
            None::<f64>,
            None::<&str>
        ],
        "a choice stored as a boolean",
    );

    refuses(
        &connection,
        A_SETTING,
        params![
            "video_scale",
            "Float",
            None::<i64>,
            None::<i64>,
            None::<f64>,
            "2.5"
        ],
        "a float stored as text",
    );

    refuses(
        &connection,
        A_SETTING,
        params![
            "video_vsync",
            "Bool",
            2,
            None::<i64>,
            None::<f64>,
            None::<&str>
        ],
        "a boolean that is neither 0 nor 1",
    );

    refuses(
        &connection,
        A_SETTING,
        params![
            "video_vsync",
            "Boolean",
            1,
            None::<i64>,
            None::<f64>,
            None::<&str>
        ],
        "a value type schema v1 does not have",
    );

    drop(database);
}

/// One RetroArch setting per scope and key.
#[test]
fn one_retroarch_setting_per_scope_and_key() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    const A_SETTING: &str = "INSERT INTO retroarch_setting_overrides \
         (scope_kind, scope_system_id, scope_game_id, setting_key, value_type, \
          value_bool, value_integer, value_float, value_text, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 'Bool', ?5, NULL, NULL, NULL, 0, 0)";

    insert(
        &connection,
        A_SETTING,
        params!["Global", None::<&[u8]>, None::<&[u8]>, "video_vsync", 1],
    );

    refuses(
        &connection,
        A_SETTING,
        params!["Global", None::<&[u8]>, None::<&[u8]>, "video_vsync", 0],
        "a second global setting under one key",
    );

    // The same key under a different scope is a different setting: that is what
    // the inheritance chain is made of (§13.1).
    accepts(
        &connection,
        A_SETTING,
        params!["System", &fixture.system, None::<&[u8]>, "video_vsync", 0],
        "the same key at system scope",
    );

    accepts(
        &connection,
        A_SETTING,
        params!["Game", None::<&[u8]>, &fixture.game, "video_vsync", 0],
        "the same key at game scope",
    );

    refuses(
        &connection,
        A_SETTING,
        params!["System", &fixture.system, None::<&[u8]>, "video_vsync", 1],
        "a second system setting under one key",
    );

    drop(database);
}

/// A core option override names exactly the scope column its kind requires.
///
/// `DATA_MODEL.md` §14.1: the hierarchy is `Core Defaults → System → Game`, and it
/// is a separate configuration domain from the RetroArch settings — which is why
/// its anchor is a core *version* and its scopes are named differently.
#[test]
fn a_core_option_override_names_exactly_its_scope_column() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let core = id(0x0e);
    let version = id(0x10);

    insert(
        &connection,
        "INSERT INTO cores (core_id, component_key, created_at) VALUES (?1, 'mesen', 0)",
        [&core],
    );
    insert(
        &connection,
        "INSERT INTO core_versions \
         (core_version_id, core_id, component_key, platform, build_id, revision, \
          artifact_digest, first_seen_at, last_seen_at) \
         VALUES (?1, ?2, 'mesen', 'macos-arm64', 'build-1', NULL, NULL, 0, 0)",
        params![&version, &core],
    );

    const AN_OPTION: &str = "INSERT INTO core_option_overrides \
         (core_version_id, scope_kind, scope_system_id, scope_game_id, option_key, \
          value_text, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'enabled', 0, 0)";

    accepts(
        &connection,
        AN_OPTION,
        params![
            &version,
            "CoreDefaults",
            None::<&[u8]>,
            None::<&[u8]>,
            "mesen_ntsc"
        ],
        "a core default",
    );

    accepts(
        &connection,
        AN_OPTION,
        params![
            &version,
            "System",
            &fixture.system,
            None::<&[u8]>,
            "mesen_ntsc"
        ],
        "a system core option",
    );

    accepts(
        &connection,
        AN_OPTION,
        params![&version, "Game", None::<&[u8]>, &fixture.game, "mesen_ntsc"],
        "a game core option",
    );

    refuses(
        &connection,
        AN_OPTION,
        params![
            &version,
            "CoreDefaults",
            &fixture.system,
            None::<&[u8]>,
            "mesen_ntsc"
        ],
        "a core default that names a system",
    );

    refuses(
        &connection,
        AN_OPTION,
        params![
            &version,
            "Release",
            None::<&[u8]>,
            None::<&[u8]>,
            "mesen_ntsc"
        ],
        "a release-scoped core option, which §13.4 deliberately does not have",
    );

    refuses(
        &connection,
        AN_OPTION,
        params![&version, "Game", None::<&[u8]>, &fixture.game, "mesen_ntsc"],
        "a second game core option under one key",
    );

    refuses(
        &connection,
        AN_OPTION,
        params![
            &version,
            "CoreDefaults",
            None::<&[u8]>,
            None::<&[u8]>,
            "mesen_ntsc"
        ],
        "a second core default under one key",
    );

    drop(database);
}

/// A firmware entry's nullability follows its read state.
///
/// `DATA_MODEL.md` §11.1: an unreadable file is a first-class state and not a
/// `NULL` digest with no explanation, and a missing file has nothing to report.
/// The rule is what keeps "I could not read it" distinguishable from "it is not
/// there" and from "I never looked".
#[test]
fn a_firmware_entry_matches_its_read_state() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    seed(&connection);

    const AN_ENTRY: &str = "INSERT INTO firmware_entries \
         (relative_path, filename, byte_size, mtime, digest, digest_computed_at, \
          read_state, first_seen_at, last_seen_at) \
         VALUES (?1, 'disksys.rom', ?2, ?3, ?4, ?5, ?6, 0, 0)";

    accepts(
        &connection,
        AN_ENTRY,
        params!["Readable/disksys.rom", 8192, 0, digest(0x21), 0, "Readable"],
        "a readable firmware file with its size and digest",
    );

    refuses(
        &connection,
        AN_ENTRY,
        params![
            "Readable/no digest.rom",
            8192,
            0,
            None::<&[u8]>,
            0,
            "Readable"
        ],
        "a readable firmware file with no digest",
    );

    accepts(
        &connection,
        AN_ENTRY,
        params![
            "Unreadable/disksys.rom",
            None::<i64>,
            None::<i64>,
            None::<&[u8]>,
            None::<i64>,
            "Unreadable"
        ],
        "an unreadable firmware file, which reports no digest",
    );

    refuses(
        &connection,
        AN_ENTRY,
        params![
            "Unreadable/with digest.rom",
            8192,
            0,
            digest(0x22),
            0,
            "Unreadable"
        ],
        "an unreadable firmware file that claims a digest",
    );

    accepts(
        &connection,
        AN_ENTRY,
        params![
            "Missing/disksys.rom",
            None::<i64>,
            None::<i64>,
            None::<&[u8]>,
            None::<i64>,
            "Missing"
        ],
        "a missing firmware file, which reports nothing",
    );

    refuses(
        &connection,
        AN_ENTRY,
        params![
            "Missing/with size.rom",
            8192,
            None::<i64>,
            None::<&[u8]>,
            None::<i64>,
            "Missing"
        ],
        "a missing firmware file that claims a size",
    );

    refuses(
        &connection,
        AN_ENTRY,
        params!["Never looked.rom", 8192, 0, digest(0x23), 0, "Present"],
        "a read state schema v1 does not have",
    );

    drop(database);
}

/// The two index singletons are singletons.
///
/// `DATA_MODEL.md` §11.2 and §12.7: one row that separates "never scanned" from
/// "the folder is empty". The key is what makes a second row impossible, and the
/// check is what makes the key's single value the only one.
#[test]
fn the_index_states_are_singletons() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    seed(&connection);

    insert(
        &connection,
        "INSERT INTO firmware_index_state \
         (singleton, last_scan_at, root_location, entry_count, scan_status) \
         VALUES (1, NULL, NULL, 0, 'Never')",
        [],
    );

    refuses(
        &connection,
        "INSERT INTO firmware_index_state \
         (singleton, last_scan_at, root_location, entry_count, scan_status) \
         VALUES (1, 0, '/firmware', 3, 'Complete')",
        [],
        "a second firmware index state row",
    );

    refuses(
        &connection,
        "INSERT INTO firmware_index_state \
         (singleton, last_scan_at, root_location, entry_count, scan_status) \
         VALUES (2, 0, '/firmware', 3, 'Complete')",
        [],
        "a firmware index state row that is not the singleton",
    );

    insert(
        &connection,
        "INSERT INTO component_index_state \
         (singleton, last_scan_at, last_scan_status, indexed_entry_count) \
         VALUES (1, NULL, 'Never', 0)",
        [],
    );

    refuses(
        &connection,
        "INSERT INTO component_index_state \
         (singleton, last_scan_at, last_scan_status, indexed_entry_count) \
         VALUES (2, 0, 'Complete', 3)",
        [],
        "a component index state row that is not the singleton",
    );

    drop(database);
}

// ---------------------------------------------------------------------------
// What a key refuses
// ---------------------------------------------------------------------------

/// A library source is identified by where it points.
///
/// `DATA_MODEL.md` §5.1: two sources over the same location in the same locator
/// kind would scan the same files twice, and the same file would become two
/// observations of one content.
#[test]
fn a_library_source_is_unique_by_location_and_kind() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    seed(&connection);

    const A_SOURCE: &str = "INSERT INTO library_sources \
         (id, display_name, location, platform_locator_kind, fixed_system_id, \
          availability, availability_checked_at, created_at, removed_at) \
         VALUES (?1, ?2, '/roms/snes', 'Path', NULL, 'Available', 0, 0, NULL)";

    accepts(
        &connection,
        A_SOURCE,
        params![id(0x11), "SNES library"],
        "a second source over another location",
    );

    refuses(
        &connection,
        A_SOURCE,
        params![id(0x12), "SNES library again"],
        "a second source over the same location in the same locator kind",
    );

    drop(database);
}

/// A release is unique within its game.
///
/// `DATA_MODEL.md` §6.2: the release key is the normalized fachliche identity, and
/// one game has one release per identity. The same key under a *different* game is
/// a different release, which is why the key is composite and not global.
#[test]
fn a_release_is_unique_within_its_game() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let other_game = id(0x13);

    insert(
        &connection,
        "INSERT INTO games (id, first_seen_at, default_release_id, is_favorite, is_hidden, \
          is_ignored) \
         VALUES (?1, 0, NULL, 0, 0, 0)",
        [&other_game],
    );

    const A_RELEASE: &str = "INSERT INTO releases \
         (id, game_id, system_id, release_title, release_date, revision, release_type, \
          release_key, archive_kind, created_at) \
         VALUES (?1, ?2, ?3, 'Fixture Game (Japan)', '1990', NULL, 'Official', ?4, NULL, 0)";

    accepts(
        &connection,
        A_RELEASE,
        params![id(0x14), &fixture.game, &fixture.system, digest(0x31)],
        "another release of the same game",
    );

    refuses(
        &connection,
        A_RELEASE,
        params![id(0x15), &fixture.game, &fixture.system, digest(0x31)],
        "a second release of one game with the same release key",
    );

    accepts(
        &connection,
        A_RELEASE,
        params![id(0x16), &other_game, &fixture.system, digest(0x31)],
        "the same release key under another game",
    );

    drop(database);
}

/// A content's release belongs to the content's game.
///
/// `DATA_MODEL.md` §6.3 and §3.9 rule 4: `game_id` is redundant by design, and the
/// composite foreign key is what makes the redundancy a fact rather than a
/// promise. A content whose release belongs to another game is unrepresentable.
#[test]
fn a_content_belongs_to_its_release_and_to_that_release_s_game() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let other_game = id(0x13);
    let other_release = id(0x17);

    insert(
        &connection,
        "INSERT INTO games (id, first_seen_at, default_release_id, is_favorite, is_hidden, \
          is_ignored) \
         VALUES (?1, 0, NULL, 0, 0, 0)",
        [&other_game],
    );
    insert(
        &connection,
        "INSERT INTO releases \
         (id, game_id, system_id, release_title, release_date, revision, release_type, \
          release_key, archive_kind, created_at) \
         VALUES (?1, ?2, ?3, 'Other Game', NULL, NULL, 'Official', ?4, NULL, 0)",
        params![&other_release, &other_game, &fixture.system, digest(0x32)],
    );

    refuses(
        &connection,
        A_CONTENT,
        params![id(0x18), &other_release, &fixture.game],
        "a content whose game is not its release's game",
    );

    accepts(
        &connection,
        A_CONTENT,
        params![id(0x19), &other_release, &other_game],
        "a content of the release's own game",
    );

    drop(database);
}

/// One location row per physical file.
///
/// `DATA_MODEL.md` §5.2: a file belongs to one content, so recognizing the same
/// file twice is a write to the row that is there. It is what makes a rescan
/// idempotent instead of accumulating observations.
#[test]
fn a_file_belongs_to_one_content() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let other_content = id(0x08);

    insert(
        &connection,
        A_CONTENT,
        params![&other_content, &fixture.release, &fixture.game],
    );

    insert(
        &connection,
        A_FILE_LOCATION,
        params![&fixture.content, &fixture.source, "Game (Europe).nes"],
    );

    refuses(
        &connection,
        A_FILE_LOCATION,
        params![&other_content, &fixture.source, "Game (Europe).nes"],
        "the same file in the same source claimed by a second content",
    );

    accepts(
        &connection,
        A_FILE_LOCATION,
        params![&fixture.content, &fixture.source, "Game (Europe) [b].nes"],
        "another file in the same source",
    );

    drop(database);
}

/// One canonical fingerprint per content, and one recognition per digest.
///
/// `DATA_MODEL.md` §7.1: "the canonical payload fingerprint" is a database fact
/// rather than a convention, a payload digest names one content, and one
/// `EntryList` row is written per archive entry.
#[test]
fn fingerprints_are_unique_in_the_ways_the_model_needs() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let other_content = id(0x08);

    insert(
        &connection,
        A_CONTENT,
        params![&other_content, &fixture.release, &fixture.game],
    );

    insert(
        &connection,
        A_FINGERPRINT,
        params![&fixture.content, digest(0x41)],
    );

    accepts(
        &connection,
        "INSERT INTO content_fingerprints \
         (content_id, fingerprint_kind, algorithm, digest, entry_path, byte_size, \
          computed_at, source_scan_run_id) \
         VALUES (?1, 'Container', 'Sha256', ?2, NULL, 4096, 0, NULL)",
        params![&fixture.content, digest(0x42)],
        "a container fingerprint beside the payload one",
    );

    refuses(
        &connection,
        A_FINGERPRINT,
        params![&fixture.content, digest(0x43)],
        "a second payload fingerprint for one content",
    );

    refuses(
        &connection,
        A_FINGERPRINT,
        params![&other_content, digest(0x41)],
        "a payload digest that already belongs to another content",
    );

    accepts(
        &connection,
        "INSERT INTO content_fingerprints \
         (content_id, fingerprint_kind, algorithm, digest, entry_path, byte_size, \
          computed_at, source_scan_run_id) \
         VALUES (?1, 'EntryList', 'Sha256', ?2, 'Game (Europe).nes', 4096, 0, NULL)",
        params![&other_content, digest(0x44)],
        "an entry-list fingerprint naming its entry",
    );

    refuses(
        &connection,
        "INSERT INTO content_fingerprints \
         (content_id, fingerprint_kind, algorithm, digest, entry_path, byte_size, \
          computed_at, source_scan_run_id) \
         VALUES (?1, 'EntryList', 'Sha256', ?2, 'Game (Europe).nes', 4096, 0, NULL)",
        params![&other_content, digest(0x45)],
        "a second entry-list fingerprint for one entry of one content",
    );

    accepts(
        &connection,
        "INSERT INTO content_fingerprints \
         (content_id, fingerprint_kind, algorithm, digest, entry_path, byte_size, \
          computed_at, source_scan_run_id) \
         VALUES (?1, 'EntryList', 'Sha256', ?2, 'Second (Europe).nes', 4096, 0, NULL)",
        params![&other_content, digest(0x46)],
        "an entry-list fingerprint for another entry",
    );

    refuses(
        &connection,
        "INSERT INTO content_fingerprints \
         (content_id, fingerprint_kind, algorithm, digest, entry_path, byte_size, \
          computed_at, source_scan_run_id) \
         VALUES (?1, 'Payload', 'Sha256', ?2, 'Game (Europe).nes', 4096, 0, NULL)",
        params![&other_content, digest(0x47)],
        "a payload fingerprint that names an entry",
    );

    refuses(
        &connection,
        "INSERT INTO content_fingerprints \
         (content_id, fingerprint_kind, algorithm, digest, entry_path, byte_size, \
          computed_at, source_scan_run_id) \
         VALUES (?1, 'EntryList', 'Sha256', ?2, NULL, 4096, 0, NULL)",
        params![&other_content, digest(0x48)],
        "an entry-list fingerprint with no entry",
    );

    drop(database);
}

/// A managed playlist has at least two ordered members, and its order is decided
/// once.
///
/// `DATA_MODEL.md` §6.6: a playlist is generated for a multi-disc release, its
/// members are ordered, and the order must not depend on the order rows happened
/// to be inserted in.
#[test]
fn a_managed_playlist_has_ordered_members() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let playlist = id(0x1a);
    let first = id(0x1b);
    let second = id(0x1c);

    for content in [&playlist, &first, &second] {
        insert(
            &connection,
            A_CONTENT,
            params![content, &fixture.release, &fixture.game],
        );
    }

    const A_DERIVATION: &str = "INSERT INTO content_derivations \
         (content_id, kind, relative_path, member_count, created_at) \
         VALUES (?1, 'ManagedPlaylist', 'Fixture Game.m3u', ?2, 0)";

    accepts(
        &connection,
        A_DERIVATION,
        params![&playlist, 2],
        "a playlist of two members",
    );

    refuses(
        &connection,
        A_DERIVATION,
        params![&first, 1],
        "a playlist of one member, which is not a playlist",
    );

    const A_MEMBER: &str = "INSERT INTO content_derivation_members \
         (content_id, kind, source_content_id, member_index) \
         VALUES (?1, 'ManagedPlaylist', ?2, ?3)";

    accepts(
        &connection,
        A_MEMBER,
        params![&playlist, &first, 1],
        "the first member",
    );

    accepts(
        &connection,
        A_MEMBER,
        params![&playlist, &second, 2],
        "the second member",
    );

    refuses(
        &connection,
        A_MEMBER,
        params![&playlist, &second, 1],
        "two members claiming one position",
    );

    refuses(
        &connection,
        A_MEMBER,
        params![&playlist, &first, 3],
        "the same content listed twice",
    );

    refuses(
        &connection,
        A_MEMBER,
        params![&playlist, &first, 0],
        "a member position that is not one-based",
    );

    refuses(
        &connection,
        "INSERT INTO content_derivation_members \
         (content_id, kind, source_content_id, member_index) \
         VALUES (?1, 'ManagedPlaylist', ?2, 1)",
        params![&second, &first],
        "a member of a derivation that does not exist",
    );

    // A derivation's members are part of the derivation, so they go when it goes
    // (§6.6, §3.7) — the artifact BitArchive owns is a unit. The two members
    // accepted above are the ones the deletion has to take with it.
    insert(
        &connection,
        "DELETE FROM contents WHERE id = ?1",
        [&playlist],
    );

    assert!(
        !exists(&connection, "SELECT 1 FROM content_derivation_members", []),
        "a derivation's members must go with it"
    );

    drop(database);
}

/// A provider value and a manual override are unique per subject, field, locale,
/// and position.
///
/// `DATA_MODEL.md` §3.5, §9.3, §9.4: the two predicates are exhaustive — every row
/// has a `NULL` locale or a tag and never both — so every row is covered by
/// exactly one rule, and `COLLATE NOCASE` on the index makes `en-US` and `en-us`
/// one locale even though the stored value stays canonical.
#[test]
fn a_locale_is_part_of_the_uniqueness_of_a_value() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    insert(
        &connection,
        "INSERT INTO metadata_fields \
         (field_key, value_kind, is_multi_valued, is_localizable) \
         VALUES ('title', 'Text', 1, 1)",
        [],
    );

    const A_VALUE: &str = "INSERT INTO provider_values \
         (provider_id, subject_kind, subject_id, field_key, locale, value_index, \
          value_text, value_number, provider_item_id, fetched_at, scrape_run_id) \
         VALUES ('screenscraper', 'Game', ?1, 'title', ?2, ?3, ?4, NULL, NULL, 0, NULL)";

    accepts(
        &connection,
        A_VALUE,
        params![&fixture.game, None::<&str>, 0, "Fixture Game"],
        "a language-neutral value",
    );

    refuses(
        &connection,
        A_VALUE,
        params![&fixture.game, None::<&str>, 0, "Fixture Game again"],
        "a second language-neutral value in one position",
    );

    accepts(
        &connection,
        A_VALUE,
        params![&fixture.game, "en-US", 0, "Fixture Game"],
        "a localised value beside the neutral one",
    );

    refuses(
        &connection,
        A_VALUE,
        params![&fixture.game, "en-us", 0, "Fixture Game, lower case tag"],
        "the same locale in a different case",
    );

    accepts(
        &connection,
        A_VALUE,
        params![&fixture.game, "en-US", 1, "Fixture Game, second value"],
        "a second value in the same locale",
    );

    const AN_OVERRIDE: &str = "INSERT INTO manual_overrides \
         (subject_kind, subject_id, field_key, locale, value_text, value_number, \
          created_at, updated_at) \
         VALUES ('Game', ?1, 'title', ?2, ?3, NULL, 0, 0)";

    accepts(
        &connection,
        AN_OVERRIDE,
        params![&fixture.game, None::<&str>, "The user's title"],
        "a language-neutral manual override",
    );

    refuses(
        &connection,
        AN_OVERRIDE,
        params![&fixture.game, None::<&str>, "The user's title again"],
        "a second language-neutral manual override for one field",
    );

    accepts(
        &connection,
        AN_OVERRIDE,
        params![&fixture.game, "de", "Der Titel des Nutzers"],
        "a localised manual override beside the neutral one",
    );

    refuses(
        &connection,
        AN_OVERRIDE,
        params![&fixture.game, "DE", "Der Titel, other case"],
        "the same override locale in a different case",
    );

    // An override and a provider value live in different tables, so they never
    // collide: what the provider said stays on record beside what the user set
    // (§9.4).
    drop(database);
}

/// One media asset per subject and slot.
///
/// `DATA_MODEL.md` §10.2: replacing a cover is a replace, not an append — the
/// previous asset row is not silently orphaned by a new one taking its place.
#[test]
fn one_media_asset_reference_per_slot() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let asset = id(0x1d);
    let replacement = id(0x1e);

    for id_byte in [&asset, &replacement] {
        insert(
            &connection,
            "INSERT INTO media_assets \
             (id, media_type, locale, provider_id, source_provider_item_id, \
              managed_relative_path, content_digest, byte_size, pixel_width, \
              pixel_height, state, created_at, last_verified_at) \
             VALUES (?1, 'Cover', NULL, NULL, NULL, 'media/cover.png', NULL, 1024, 512, 512, \
                     'Available', 0, NULL)",
            [id_byte],
        );
    }

    const A_REFERENCE: &str = "INSERT INTO media_asset_references \
         (media_asset_id, subject_kind, subject_id, logical_key) \
         VALUES (?1, 'Game', ?2, 'cover-front')";

    insert(&connection, A_REFERENCE, params![&asset, &fixture.game]);

    refuses(
        &connection,
        A_REFERENCE,
        params![&replacement, &fixture.game],
        "a second asset in one subject's slot",
    );

    accepts(
        &connection,
        "INSERT INTO media_asset_references \
         (media_asset_id, subject_kind, subject_id, logical_key) \
         VALUES (?1, 'Game', ?2, 'cover-back')",
        params![&replacement, &fixture.game],
        "another asset in another slot of the same subject",
    );

    drop(database);
}

/// One row per save-state file.
///
/// `DATA_MODEL.md` §15.1: the row is BitArchive's management layer over a file
/// RetroArch owns, and the file path is what makes a rediscovery idempotent
/// instead of a second row.
#[test]
fn a_save_state_row_is_one_file() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let core = id(0x0e);

    insert(
        &connection,
        "INSERT INTO cores (core_id, component_key, created_at) VALUES (?1, 'mesen', 0)",
        [&core],
    );

    const A_STATE: &str = "INSERT INTO save_states \
         (id, release_id, core_id, core_version_id, core_version_label, slot, created_at, \
          file_relative_path, file_size, file_mtime, display_name, \
          thumbnail_media_asset_id, state, last_seen_at) \
         VALUES (?1, ?2, ?3, NULL, NULL, 0, 0, ?4, 1024, 0, NULL, NULL, 'Present', 0)";

    accepts(
        &connection,
        A_STATE,
        params![id(0x1f), &fixture.release, &core, "states/nes/Game.state"],
        "a save state for one file",
    );

    refuses(
        &connection,
        A_STATE,
        params![id(0x20), &fixture.release, &core, "states/nes/Game.state"],
        "a second row for one save-state file",
    );

    accepts(
        &connection,
        A_STATE,
        params![id(0x21), &fixture.release, &core, "states/nes/Game.state1"],
        "a save state for another slot's file",
    );

    drop(database);
}

/// Only one session is active at a time.
///
/// The single-active-session rule of the MVP, expressed where it cannot be
/// forgotten: a second active session is refused by SQLite, and an active session
/// that has already ended is refused too, because that row would say two
/// contradictory things at once (§16.1).
#[test]
fn only_one_session_is_active() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    const A_SESSION: &str = "INSERT INTO sessions \
         (id, game_id, release_id, content_id, content_location_kind, core_id, \
          core_version_id, core_version_label, runtime_version_id, runtime_version, \
          started_at, ended_at, state, duration_ms, active_duration_ms, process_id, \
          process_started_at, process_executable, exit_kind, exit_code, exit_signal, \
          recovery_note, session_directory, created_at) \
         VALUES (?1, ?2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 0, ?3, ?4, ?5, \
                 NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 0)";

    accepts(
        &connection,
        A_SESSION,
        params![
            id(0x22),
            &fixture.game,
            Option::<i64>::None,
            "Active",
            Option::<i64>::None
        ],
        "a session that is still running",
    );

    refuses(
        &connection,
        A_SESSION,
        params![
            id(0x23),
            &fixture.game,
            Option::<i64>::None,
            "Active",
            Option::<i64>::None
        ],
        "a second active session",
    );

    refuses(
        &connection,
        A_SESSION,
        params![id(0x24), &fixture.game, 100, "Active", 100],
        "a session that is active and already ended",
    );

    accepts(
        &connection,
        A_SESSION,
        params![id(0x25), &fixture.game, 100, "Finalized", 100],
        "a finalized session beside the active one",
    );

    drop(database);
}

// ---------------------------------------------------------------------------
// What a row requires to exist
// ---------------------------------------------------------------------------

/// A reference must name a row that exists.
///
/// The representative cases below are the references a library cannot be written
/// without. SQLite checks them because the connection enables foreign keys — the
/// foundation does it for the connection it owns, and the fixing of the test
/// connection is what makes these assertions about the schema rather than about a
/// setting.
#[test]
fn a_reference_must_name_a_row_that_exists() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let unknown = id(0x7f);

    refuses(
        &connection,
        "INSERT INTO releases \
         (id, game_id, system_id, release_title, release_date, revision, release_type, \
          release_key, archive_kind, created_at) \
         VALUES (?1, ?2, ?3, NULL, NULL, NULL, 'Official', ?4, NULL, 0)",
        params![id(0x26), &unknown, &fixture.system, digest(0x51)],
        "a release of a game that does not exist",
    );

    refuses(
        &connection,
        A_CONTENT,
        params![id(0x27), &unknown, &fixture.game],
        "a content of a release that does not exist",
    );

    refuses(
        &connection,
        A_FILE_LOCATION,
        params![&unknown, &fixture.source, "Orphan.nes"],
        "a location of a content that does not exist",
    );

    refuses(
        &connection,
        "INSERT INTO content_derivations \
         (content_id, kind, relative_path, member_count, created_at) \
         VALUES (?1, 'ManagedPlaylist', 'Orphan.m3u', 2, 0)",
        [&unknown],
        "a derivation of a content that does not exist",
    );

    refuses(
        &connection,
        "INSERT INTO sessions (id, game_id, started_at, state, created_at) \
         VALUES (?1, ?2, 0, 'Active', 0)",
        params![id(0x28), &unknown],
        "a session of a game that does not exist",
    );

    drop(database);
}

/// A metadata field that is in use cannot be pruned.
///
/// `DATA_MODEL.md` §9.2 and invariant 19: an invalid or retired stored value is
/// preserved for diagnosis rather than silently deleted. The reference is what
/// makes removing a field from the curated set a deliberate migration instead of a
/// side effect.
#[test]
fn a_metadata_field_that_is_in_use_cannot_be_removed() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    insert(
        &connection,
        "INSERT INTO metadata_fields \
         (field_key, value_kind, is_multi_valued, is_localizable) \
         VALUES ('title', 'Text', 1, 1)",
        [],
    );

    insert(
        &connection,
        "INSERT INTO provider_values \
         (provider_id, subject_kind, subject_id, field_key, locale, value_index, \
          value_text, value_number, provider_item_id, fetched_at, scrape_run_id) \
         VALUES ('screenscraper', 'Game', ?1, 'title', NULL, 0, 'Fixture Game', NULL, NULL, \
                 0, NULL)",
        [&fixture.game],
    );

    refuses(
        &connection,
        "DELETE FROM metadata_fields WHERE field_key = 'title'",
        [],
        "removing a metadata field that a provider value still uses",
    );

    refuses(
        &connection,
        "UPDATE metadata_fields SET field_key = 'name' WHERE field_key = 'title'",
        [],
        "renaming a metadata field that a provider value still uses",
    );

    drop(database);
}

/// A required column is required.
///
/// The column contract is asserted in `structure`, but a `NOT NULL` that is
/// declared and not in force is worth catching where it would show up: a row that
/// omits the value a reader depends on.
#[test]
fn a_required_column_is_required() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    refuses(
        &connection,
        "INSERT INTO games (first_seen_at, is_favorite, is_hidden, is_ignored) \
         VALUES (0, 0, 0, 0)",
        [],
        "a game with no identity",
    );

    // The release key and the content kind have no default, so a row that omits
    // them is a row that has none. `release_type` is deliberately not tested this
    // way: it has a default, which is the model's way of saying that a release
    // whose type is unknown is an `Official` one (§6.2).
    refuses(
        &connection,
        "INSERT INTO releases \
         (id, game_id, system_id, release_type, created_at) \
         VALUES (?1, ?2, ?3, 'Official', 0)",
        params![id(0x2a), &fixture.game, &fixture.system],
        "a release with no release key",
    );

    refuses(
        &connection,
        "INSERT INTO contents \
         (id, release_id, game_id, validation_state, created_at) \
         VALUES (?1, ?2, ?3, 'Valid', 0)",
        params![id(0x2b), &fixture.release, &fixture.game],
        "a content with no content kind",
    );

    // The other half of nullability: a column the model allows to be empty can be
    // left empty, which is what keeps an unknown release date from being stored as
    // an empty string or a sentinel (§3.5).
    accepts(
        &connection,
        "INSERT INTO releases \
         (id, game_id, system_id, release_title, release_date, revision, release_type, \
          release_key, archive_kind, created_at) \
         VALUES (?1, ?2, ?3, NULL, NULL, NULL, 'Homebrew', ?4, NULL, 0)",
        params![id(0x2c), &fixture.game, &fixture.system, digest(0x53)],
        "a release whose date and revision are unknown",
    );

    drop(database);
}

// ---------------------------------------------------------------------------
// What happens when a row goes away
// ---------------------------------------------------------------------------

/// A run that is deleted takes its issues with it and leaves the library intact.
///
/// `DATA_MODEL.md` §19.7 and §8.2: a scan run's issues are part of the run, while
/// an observation and a fingerprint outlive the run that produced them — the run
/// reference is provenance and is cleared, and the row itself stays. A `CASCADE`
/// where the model says `SET NULL` would quietly erase the library's history of
/// where a file was seen.
#[test]
fn deleting_a_run_takes_its_issues_and_clears_its_references() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    insert(
        &connection,
        "INSERT INTO scan_run_issues \
         (scan_run_id, issue_index, kind, source_id, relative_path, detail, byte_size, \
          prevents_reconciliation) \
         VALUES (?1, 0, 'Unreadable', ?2, 'Broken.nes', 'permission denied', NULL, 1)",
        params![&fixture.scan_run, &fixture.source],
    );

    insert(
        &connection,
        A_FILE_LOCATION,
        params![&fixture.content, &fixture.source, "Game (Europe).nes"],
    );

    insert(
        &connection,
        "UPDATE content_locations SET last_seen_scan_run_id = ?1",
        [&fixture.scan_run],
    );

    insert(
        &connection,
        A_FINGERPRINT,
        params![&fixture.content, digest(0x61)],
    );

    insert(
        &connection,
        "UPDATE content_fingerprints SET source_scan_run_id = ?1",
        [&fixture.scan_run],
    );

    insert(
        &connection,
        "DELETE FROM scan_runs WHERE id = ?1",
        [&fixture.scan_run],
    );

    assert!(
        !exists(&connection, "SELECT 1 FROM scan_run_issues", []),
        "a run's issues are the run's own record"
    );

    assert!(
        exists(&connection, "SELECT 1 FROM content_locations", []),
        "the observation outlives the run that made it"
    );

    assert!(
        is_null(
            &connection,
            "SELECT last_seen_scan_run_id FROM content_locations",
            []
        ),
        "the reference to the deleted run must be cleared, not the row"
    );

    assert!(
        exists(&connection, "SELECT 1 FROM content_fingerprints", []),
        "the fingerprint outlives the run that computed it"
    );

    assert!(
        is_null(
            &connection,
            "SELECT source_scan_run_id FROM content_fingerprints",
            []
        ),
        "the reference to the deleted run must be cleared, not the row"
    );

    drop(database);
}

/// A scrape run that is deleted takes its per-game records and clears the run
/// reference of the values it fetched.
///
/// `DATA_MODEL.md` §9.3, §9.6, §19.7: what a provider said stays on record with
/// its provenance; the run it came from is a reference that may be cleared. The
/// values are the metadata, and the run is the event.
#[test]
fn deleting_a_scrape_run_takes_its_items_and_clears_its_values() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let run = id(0x2d);

    insert(
        &connection,
        "INSERT INTO metadata_providers (provider_id, display_name, is_enabled) \
         VALUES ('screenscraper', 'ScreenScraper', 1)",
        [],
    );

    insert(
        &connection,
        "INSERT INTO metadata_fields \
         (field_key, value_kind, is_multi_valued, is_localizable) \
         VALUES ('title', 'Text', 1, 1)",
        [],
    );

    insert(
        &connection,
        "INSERT INTO scrape_runs \
         (id, provider_id, mode, scope_kind, scope_system_id, scope_game_id, only_missing, \
          fields_missing, status, started_at, finished_at, updated_count, no_match_count, \
          multiple_match_count, error_count, skipped_count, override_protected_count) \
         VALUES (?1, 'screenscraper', 'Automatic', 'Library', NULL, NULL, 0, NULL, \
                 'Completed', 0, 1, 0, 0, 0, 0, 0, 0)",
        [&run],
    );

    insert(
        &connection,
        "INSERT INTO scrape_run_items \
         (scrape_run_id, game_id, status, candidate_count, accepted_provider_item_id, \
          error_detail, updated_at) \
         VALUES (?1, ?2, 'Updated', 1, '1234', NULL, 0)",
        params![&run, &fixture.game],
    );

    insert(
        &connection,
        "INSERT INTO provider_values \
         (provider_id, subject_kind, subject_id, field_key, locale, value_index, \
          value_text, value_number, provider_item_id, fetched_at, scrape_run_id) \
         VALUES ('screenscraper', 'Game', ?1, 'title', NULL, 0, 'Fixture Game', NULL, \
                 '1234', 0, ?2)",
        params![&fixture.game, &run],
    );

    insert(&connection, "DELETE FROM scrape_runs WHERE id = ?1", [&run]);

    assert!(
        !exists(&connection, "SELECT 1 FROM scrape_run_items", []),
        "a run's per-game records are the run's own"
    );

    assert!(
        exists(&connection, "SELECT 1 FROM provider_values", []),
        "a provider value is metadata, and it outlives the run"
    );

    assert!(
        is_null(&connection, "SELECT scrape_run_id FROM provider_values", []),
        "the run reference must be cleared, not the value"
    );

    drop(database);
}

/// A release takes its own tags with it, and refuses to go while it has content.
///
/// `DATA_MODEL.md` §6.4 and §6.3: a region or a language tag has no meaning
/// without its release, while a content is a content unit of the library and is
/// not deleted as a side effect of deleting the release it was grouped under.
#[test]
fn deleting_a_release_takes_its_tags_and_is_refused_while_it_has_content() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let tagged = id(0x2e);

    insert(
        &connection,
        "INSERT INTO releases \
         (id, game_id, system_id, release_title, release_date, revision, release_type, \
          release_key, archive_kind, created_at) \
         VALUES (?1, ?2, ?3, 'Fixture Game (Japan)', '1990', NULL, 'Official', ?4, NULL, 0)",
        params![&tagged, &fixture.game, &fixture.system, digest(0x62)],
    );

    insert(
        &connection,
        "INSERT INTO release_regions (release_id, region) VALUES (?1, 'Japan')",
        [&tagged],
    );

    insert(
        &connection,
        "INSERT INTO release_languages (release_id, language) VALUES (?1, 'ja')",
        [&tagged],
    );

    insert(&connection, "DELETE FROM releases WHERE id = ?1", [&tagged]);

    assert!(
        !exists(&connection, "SELECT 1 FROM release_regions", []),
        "a release's region tags belong to the release"
    );

    assert!(
        !exists(&connection, "SELECT 1 FROM release_languages", []),
        "a release's language tags belong to the release"
    );

    // The seeded release still has a content, and a content is not the release's
    // to take with it.
    refuses(
        &connection,
        "DELETE FROM releases WHERE id = ?1",
        [&fixture.release],
        "deleting a release that still has content",
    );

    drop(database);
}

/// A core version takes its option schema with it, and refuses to go while an
/// override names it.
///
/// `DATA_MODEL.md` §14.1, §14.2: what a core version's introspection produced is
/// about that version and goes with it; an override the user set is a setting and
/// outlives the version it was written against, so the reference refuses the
/// delete rather than clearing itself.
#[test]
fn deleting_a_core_version_takes_its_option_schema_and_is_refused_by_overrides() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    seed(&connection);

    let core = id(0x0e);
    let introspected = id(0x2f);
    let overridden = id(0x30);

    insert(
        &connection,
        "INSERT INTO cores (core_id, component_key, created_at) VALUES (?1, 'mesen', 0)",
        [&core],
    );

    for version in [&introspected, &overridden] {
        insert(
            &connection,
            "INSERT INTO core_versions \
             (core_version_id, core_id, component_key, platform, build_id, revision, \
              artifact_digest, first_seen_at, last_seen_at) \
             VALUES (?1, ?2, 'mesen', 'macos-arm64', ?3, NULL, NULL, 0, 0)",
            params![version, &core, format!("build-{}", version[0])],
        );
    }

    insert(
        &connection,
        "INSERT INTO core_option_schemas \
         (core_version_id, introspected_at, definition_count, introspection_status) \
         VALUES (?1, 0, 1, 'Complete')",
        [&introspected],
    );

    insert(
        &connection,
        "INSERT INTO core_option_definitions \
         (core_version_id, option_key, value_type, default_value, allowed_values, \
          display_name, definition_index) \
         VALUES (?1, 'mesen_ntsc', 'Bool', 'enabled', NULL, 'NTSC filter', 0)",
        [&introspected],
    );

    insert(
        &connection,
        "DELETE FROM core_versions WHERE core_version_id = ?1",
        [&introspected],
    );

    assert!(
        !exists(&connection, "SELECT 1 FROM core_option_schemas", []),
        "an introspection result is about the version it was taken from"
    );

    assert!(
        !exists(&connection, "SELECT 1 FROM core_option_definitions", []),
        "the definitions belong to the schema, and the schema belongs to the version"
    );

    insert(
        &connection,
        "INSERT INTO core_option_overrides \
         (core_version_id, scope_kind, scope_system_id, scope_game_id, option_key, \
          value_text, created_at, updated_at) \
         VALUES (?1, 'CoreDefaults', NULL, NULL, 'mesen_ntsc', 'enabled', 0, 0)",
        [&overridden],
    );

    refuses(
        &connection,
        "DELETE FROM core_versions WHERE core_version_id = ?1",
        [&overridden],
        "deleting a core version an override still names",
    );

    drop(database);
}

/// A game takes its scoped overrides with it.
///
/// `DATA_MODEL.md` §12.3, §13.1, §14.1: a game-scoped core selection and a
/// game-scoped setting are about that game and have no meaning without it. The
/// game itself is not deleted as a side effect of anything — this is the case
/// where the user really asked for the deletion.
#[test]
fn deleting_a_game_takes_its_scoped_overrides() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    seed(&connection);

    let game = id(0x31);
    let core = id(0x0e);

    insert(
        &connection,
        "INSERT INTO games (id, first_seen_at, default_release_id, is_favorite, is_hidden, \
          is_ignored) \
         VALUES (?1, 0, NULL, 0, 0, 0)",
        [&game],
    );

    insert(
        &connection,
        "INSERT INTO cores (core_id, component_key, created_at) VALUES (?1, 'mesen', 0)",
        [&core],
    );

    insert(
        &connection,
        "INSERT INTO core_selection_overrides \
         (scope_kind, scope_system_id, scope_game_id, scope_release_id, core_id, created_at) \
         VALUES ('Game', NULL, ?1, NULL, ?2, 0)",
        params![&game, &core],
    );

    insert(
        &connection,
        "INSERT INTO retroarch_setting_overrides \
         (scope_kind, scope_system_id, scope_game_id, setting_key, value_type, value_bool, \
          value_integer, value_float, value_text, created_at, updated_at) \
         VALUES ('Game', NULL, ?1, 'video_vsync', 'Bool', 1, NULL, NULL, NULL, 0, 0)",
        [&game],
    );

    insert(&connection, "DELETE FROM games WHERE id = ?1", [&game]);

    assert!(
        !exists(&connection, "SELECT 1 FROM core_selection_overrides", []),
        "a game-scoped core selection belongs to the game"
    );

    assert!(
        !exists(&connection, "SELECT 1 FROM retroarch_setting_overrides", []),
        "a game-scoped setting belongs to the game"
    );

    drop(database);
}

/// A reference that carries meaning refuses the delete instead of clearing itself
/// or cascading.
///
/// This is the other half of the delete rules, and the one that protects user
/// data. `DATA_MODEL.md` §5.1, §6.3, §10.2, §12.6: a source that a scan names, a
/// content that is located, a media asset a subject references, and a component
/// index that names a core version are all rows whose disappearance would leave a
/// question no longer answerable. An invented `CASCADE` here would delete them
/// without a word.
#[test]
fn a_reference_that_carries_meaning_refuses_the_delete() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let asset = id(0x1d);

    insert(
        &connection,
        A_FILE_LOCATION,
        params![&fixture.content, &fixture.source, "Game (Europe).nes"],
    );

    insert(
        &connection,
        "INSERT INTO media_assets \
         (id, media_type, locale, provider_id, source_provider_item_id, \
          managed_relative_path, content_digest, byte_size, pixel_width, pixel_height, \
          state, created_at, last_verified_at) \
         VALUES (?1, 'Cover', NULL, NULL, NULL, 'media/cover.png', NULL, 1024, 512, 512, \
                 'Available', 0, NULL)",
        [&asset],
    );

    insert(
        &connection,
        "INSERT INTO media_asset_references \
         (media_asset_id, subject_kind, subject_id, logical_key) \
         VALUES (?1, 'Game', ?2, 'cover-front')",
        params![&asset, &fixture.game],
    );

    refuses(
        &connection,
        "DELETE FROM systems WHERE system_id = ?1",
        [&fixture.system],
        "deleting a system that releases belong to",
    );

    refuses(
        &connection,
        "DELETE FROM library_sources WHERE id = ?1",
        [&fixture.source],
        "deleting a source a scan run names",
    );

    refuses(
        &connection,
        "DELETE FROM contents WHERE id = ?1",
        [&fixture.content],
        "deleting a content that is still located",
    );

    refuses(
        &connection,
        "DELETE FROM media_assets WHERE id = ?1",
        [&asset],
        "deleting a media asset a subject still references",
    );

    refuses(
        &connection,
        "DELETE FROM games WHERE id = ?1",
        [&fixture.game],
        "deleting a game that still has releases",
    );

    drop(database);
}

/// A thumbnail reference is cleared instead of refusing, and the state file
/// stays.
///
/// `DATA_MODEL.md` §15.1: the save state is the file's row, and the thumbnail is
/// a convenience pointing at a media asset. Deleting the asset clears the pointer
/// rather than deleting a save state, and never refuses the delete of something
/// the user asked to remove — the contrast with the reference above is the whole
/// point of declaring each one explicitly.
#[test]
fn a_save_state_thumbnail_is_cleared_when_its_asset_goes() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let core = id(0x0e);
    let asset = id(0x1d);

    insert(
        &connection,
        "INSERT INTO cores (core_id, component_key, created_at) VALUES (?1, 'mesen', 0)",
        [&core],
    );

    insert(
        &connection,
        "INSERT INTO media_assets \
         (id, media_type, locale, provider_id, source_provider_item_id, \
          managed_relative_path, content_digest, byte_size, pixel_width, pixel_height, \
          state, created_at, last_verified_at) \
         VALUES (?1, 'SaveStateThumbnail', NULL, NULL, NULL, 'media/state.png', NULL, 1024, \
                 256, 224, 'Available', 0, NULL)",
        [&asset],
    );

    insert(
        &connection,
        "INSERT INTO save_states \
         (id, release_id, core_id, core_version_id, core_version_label, slot, created_at, \
          file_relative_path, file_size, file_mtime, display_name, \
          thumbnail_media_asset_id, state, last_seen_at) \
         VALUES (?1, ?2, ?3, NULL, NULL, 0, 0, 'states/nes/Game.state', 1024, 0, NULL, ?4, \
                 'Present', 0)",
        params![id(0x32), &fixture.release, &core, &asset],
    );

    insert(
        &connection,
        "DELETE FROM media_assets WHERE id = ?1",
        [&asset],
    );

    assert!(
        exists(&connection, "SELECT 1 FROM save_states", []),
        "the save state is the file's row, and the file is still there"
    );

    assert!(
        is_null(
            &connection,
            "SELECT thumbnail_media_asset_id FROM save_states",
            []
        ),
        "the pointer to the deleted thumbnail must be cleared"
    );

    drop(database);
}

/// A content that a derivation lists cannot be deleted out from under it.
///
/// `DATA_MODEL.md` §6.6: the playlist BitArchive generated refers to the member
/// contents, and the reference is what keeps the generated artifact from pointing
/// at rows that are gone.
#[test]
fn a_content_a_derivation_lists_cannot_be_deleted() {
    let (temporary, database) = migrated();
    let connection = inspect(&temporary);
    let fixture = seed(&connection);

    let playlist = id(0x1a);
    let member = id(0x1b);

    for content in [&playlist, &member] {
        insert(
            &connection,
            A_CONTENT,
            params![content, &fixture.release, &fixture.game],
        );
    }

    insert(
        &connection,
        "INSERT INTO content_derivations \
         (content_id, kind, relative_path, member_count, created_at) \
         VALUES (?1, 'ManagedPlaylist', 'Fixture Game.m3u', 2, 0)",
        [&playlist],
    );

    insert(
        &connection,
        "INSERT INTO content_derivation_members \
         (content_id, kind, source_content_id, member_index) \
         VALUES (?1, 'ManagedPlaylist', ?2, 1)",
        params![&playlist, &member],
    );

    refuses(
        &connection,
        "DELETE FROM contents WHERE id = ?1",
        [&member],
        "deleting a content a generated playlist still lists",
    );

    drop(database);
}

/// The library survives being written to and read back in a second connection.
///
/// The rules above are asserted on one connection, and the schema is a property of
/// the file. This is the small end-to-end proof: rows written through one handle
/// are the rows another handle of the same database sees, and the schema version
/// does not change because data was written.
#[test]
fn the_schema_holds_the_library_across_connections() {
    let temporary = TempDatabase::new();
    let database = temporary.open().expect("a new database must open");

    let fixture = {
        let connection = inspect(&temporary);
        let fixture = seed(&connection);

        insert(
            &connection,
            A_FILE_LOCATION,
            params![&fixture.content, &fixture.source, "Game (Europe).nes"],
        );

        fixture
    };

    drop(database);

    let reopened = temporary.open().expect("the database must reopen");

    assert_eq!(
        reopened.schema_version().expect("the version must be read"),
        crate::database::SchemaVersion::new(1),
        "writing data does not change the schema version"
    );

    let connection = inspect(&temporary);

    assert_eq!(
        connection
            .query_row(
                "SELECT observed_filename FROM content_locations WHERE content_id = ?1",
                [&fixture.content],
                |row| row.get::<_, String>(0),
            )
            .expect("the location must be there"),
        "Game (Europe).nes"
    );

    drop(reopened);
}
