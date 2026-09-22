//! Schema v1 — the initial SQLite structure — exercised against isolated
//! temporary databases.
//!
//! Every test here opens its own [`TempDatabase`], so nothing touches a
//! developer's database and no two tests share a file (ARCHITECTURE.md §45.2).
//! The scaffolding is the crate's shared support; the fixture migrations of
//! [`crate::tests::database`] are deliberately not used, and the schema is
//! reached the way the application reaches it: [`TempDatabase::open`], which
//! applies the shipped catalogue.
//!
//! # What is asserted, and how
//!
//! The subject is the *schema*, not any code above it — there is no repository,
//! no query, and no domain type in this module, because schema v1 does not
//! contain one. Two complementary kinds of test carry that:
//!
//! - **Structure**, in [`structure`]: which objects exist, which columns they
//!   have, how those columns are typed and whether they may be NULL, what the
//!   primary and foreign keys are, which indexes exist, and what the FTS5
//!   projection looks like. These read `sqlite_master` and `PRAGMA table_info`,
//!   `foreign_key_list`, `index_list`, and `index_info`.
//! - **Behaviour**, in [`rules`]: the constraints in force. Each one is asserted
//!   twice where that is meaningful — the row the model allows is accepted, and
//!   the row it forbids is refused *by SQLite* — because a constraint that exists
//!   in the text and not in effect protects nothing.
//!
//! The structure tests compare against the expected schema declared as data, so a
//! missing column, an extra index, or a changed delete rule is reported as the
//! difference it is. `DATA_MODEL.md` is the source those expectations come from;
//! where a test looks like it invents a rule, the doc comment names the section
//! that fixes it.
//!
//! # SQLite's own objects
//!
//! The FTS5 projection brings five shadow tables with it. They are SQLite's
//! implementation of a virtual table that schema v1 merely declares, so they are
//! separated from the objects under test rather than counted as undeclared
//! tables — see [`FTS_SHADOW_TABLES`].

use rusqlite::{Connection, Params};

use super::support::database::TempDatabase;
use crate::database::Database;

mod rules;
mod structure;

/// The migration ledger, which the foundation owns (`DATA_MODEL.md` §18.1).
///
/// It is not part of schema v1: the runner creates it before any numbered
/// migration, and migration 1 neither creates nor changes it.
const LEDGER_TABLE: &str = "schema_migrations";

/// The FTS5 projection (`DATA_MODEL.md` §17, §20.1 row 37).
const SEARCH_INDEX: &str = "search_index";

/// The shadow tables SQLite creates for [`SEARCH_INDEX`] and owns.
///
/// FTS5 implements a virtual table as several real tables. They are the storage
/// of the projection, not objects schema v1 declares, and a test that compared
/// `sqlite_master` against the model without separating them would report them as
/// undeclared tables. Their presence is asserted by
/// `structure::the_search_projection_is_an_fts5_table_with_the_declared_columns`.
const FTS_SHADOW_TABLES: &[&str] = &[
    "search_index_data",
    "search_index_idx",
    "search_index_content",
    "search_index_docsize",
    "search_index_config",
];

/// Opens a temporary database with the shipped catalogue applied.
///
/// The subject of this module is what that catalogue produces, so every test
/// starts here. A test that needs a different starting point — the foundation's
/// own state — opens it explicitly and says so.
fn migrated() -> (TempDatabase, Database) {
    let temporary = TempDatabase::new();

    let database = temporary
        .open()
        .expect("the shipped catalogue must apply to a new database");

    (temporary, database)
}

/// Opens a connection for introspection and for behavioral tests.
///
/// Foreign keys are a property of a *connection*, not of the file, and the
/// foundation enables them on the connection it owns — not on this one. Enabling
/// them here is what makes a test that inserts a row violating a reference fail
/// for the reason the test is about, rather than passing unnoticed.
fn inspect(temporary: &TempDatabase) -> Connection {
    let connection = temporary.inspect();

    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("an inspecting connection must be able to enforce foreign keys");

    connection
}

/// Returns every object SQLite records in the schema, as `(type, name)`, sorted.
fn schema_objects(connection: &Connection) -> Vec<(String, String)> {
    let mut statement = connection
        .prepare("SELECT type, name FROM sqlite_master ORDER BY type, name")
        .expect("sqlite_master must be queryable");

    statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("sqlite_master must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable")
}

/// Returns the names of every table in the schema except the FTS5 shadow tables,
/// sorted.
fn table_names(connection: &Connection) -> Vec<String> {
    schema_objects(connection)
        .into_iter()
        .filter(|(kind, name)| kind == "table" && !FTS_SHADOW_TABLES.contains(&name.as_str()))
        .map(|(_, name)| name)
        .collect()
}

/// One column of a table, as SQLite records it.
#[derive(Debug, PartialEq, Eq)]
struct Column {
    /// The column name.
    name: String,
    /// The declared type, which is the type schema v1 wrote.
    declared_type: String,
    /// Whether the column is `NOT NULL`.
    not_null: bool,
}

/// Returns `table`'s columns in declared order.
fn columns(connection: &Connection, table: &str) -> Vec<Column> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("the pragma must be preparable");

    statement
        .query_map([], |row| {
            Ok(Column {
                name: row.get(1)?,
                declared_type: row.get(2)?,
                not_null: row.get::<_, i64>(3)? != 0,
            })
        })
        .expect("the pragma must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable")
}

/// Returns `table`'s primary key as its columns in key order.
///
/// `PRAGMA table_info` reports a 1-based ordinal for every key column, so
/// ordering by it returns the key in the order it was declared — which for a
/// composite key is part of the key.
fn primary_key(connection: &Connection, table: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("the pragma must be preparable");

    let mut keyed = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(5)?, row.get::<_, String>(1)?))
        })
        .expect("the pragma must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable")
        .into_iter()
        .filter(|(ordinal, _)| *ordinal > 0)
        .collect::<Vec<_>>();

    keyed.sort();

    keyed.into_iter().map(|(_, name)| name).collect()
}

/// One foreign key of a table, as SQLite records it.
#[derive(Debug, PartialEq, Eq)]
struct ForeignKey {
    /// The referencing columns, in order.
    columns: Vec<String>,
    /// The referenced table.
    parent: String,
    /// The referenced columns, in the same order.
    parent_columns: Vec<String>,
    /// The `ON DELETE` action.
    on_delete: String,
    /// The `ON UPDATE` action.
    on_update: String,
}

/// One key column as [`foreign_keys`] reads it: its position in the key, the
/// referenced table, the referencing column, the referenced column, and the two
/// actions joined into one string so that the column sorts alongside the rest.
type KeyColumn = (i64, String, String, String, String);

/// Returns every foreign key `table` declares, in a stable order.
///
/// SQLite reports one row per key *column*, sharing an `id` per key and a `seq`
/// for the position inside it, so the rows are grouped back into keys. The result
/// is sorted, because the order SQLite lists keys in is not the declaration
/// order.
fn foreign_keys(connection: &Connection, table: &str) -> Vec<ForeignKey> {
    let mut statement = connection
        .prepare(&format!("PRAGMA foreign_key_list({table})"))
        .expect("the pragma must be preparable");

    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .expect("the pragma must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable");

    let mut keys: Vec<(i64, Vec<KeyColumn>)> = Vec::new();

    for (id, seq, parent, from, to, on_update, on_delete) in rows {
        let parts = match keys.iter_mut().find(|(key, _)| *key == id) {
            Some((_, parts)) => parts,
            None => {
                keys.push((id, Vec::new()));

                &mut keys.last_mut().expect("just pushed").1
            }
        };

        parts.push((seq, parent, from, to, format!("{on_update}\0{on_delete}")));
    }

    let mut keys = keys
        .into_iter()
        .map(|(_, mut parts)| {
            parts.sort();

            let parent = parts[0].1.clone();
            let action = parts[0].4.clone();
            let (on_update, on_delete) = action
                .split_once('\0')
                .expect("the action is stored as a pair");

            ForeignKey {
                columns: parts.iter().map(|part| part.2.clone()).collect(),
                parent,
                parent_columns: parts.iter().map(|part| part.3.clone()).collect(),
                on_delete: on_delete.to_owned(),
                on_update: on_update.to_owned(),
            }
        })
        .collect::<Vec<_>>();

    keys.sort_by(|left, right| (&left.parent, &left.columns).cmp(&(&right.parent, &right.columns)));

    keys
}

/// One index of a table, as SQLite records it.
#[derive(Debug, PartialEq, Eq)]
struct Index {
    /// The index name.
    name: String,
    /// Whether the index enforces uniqueness.
    unique: bool,
    /// Whether the index covers only the rows its predicate selects.
    partial: bool,
    /// The indexed columns, in order.
    columns: Vec<String>,
    /// The `WHERE` clause of a partial index, whitespace-normalised.
    predicate: Option<String>,
}

/// Returns every index SQLite holds for `table`, sorted by name.
///
/// An index SQLite created for a `PRIMARY KEY` or `UNIQUE` clause is named
/// `sqlite_autoindex_…` and has no `sql` of its own; those are still returned,
/// because they are the indexes the model's unique keys consist of.
fn indexes(connection: &Connection, table: &str) -> Vec<Index> {
    let mut statement = connection
        .prepare(&format!("PRAGMA index_list({table})"))
        .expect("the pragma must be preparable");

    let listed = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(4)? != 0,
            ))
        })
        .expect("the pragma must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable");

    let mut indexes = listed
        .into_iter()
        .map(|(name, unique, partial)| Index {
            columns: indexed_columns(connection, &name),
            predicate: index_predicate(connection, &name),
            name,
            unique,
            partial,
        })
        .collect::<Vec<_>>();

    indexes.sort_by(|left, right| left.name.cmp(&right.name));

    indexes
}

/// Returns the columns of the index named `name`, in order.
fn indexed_columns(connection: &Connection, name: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA index_info({name})"))
        .expect("the pragma must be preparable");

    let mut columns = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(2)?))
        })
        .expect("the pragma must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable");

    columns.sort();

    columns
        .into_iter()
        .map(|(_, name)| name.expect("no index in schema v1 is an expression index"))
        .collect()
}

/// Returns the `WHERE` clause of the index named `name`, if it has one.
///
/// SQLite records a partial index as an ordinary index plus a predicate, and the
/// predicate is only visible in the statement that created it. Whitespace is
/// normalised so that the assertion compares the predicate and not the layout of
/// the file it was written in.
fn index_predicate(connection: &Connection, name: &str) -> Option<String> {
    let sql: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [name],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    // Bound before the split: the predicate borrows the statement, and the
    // temporary a nested `?` would leave behind does not live long enough.
    let sql = sql?;
    let (_, predicate) = sql.split_once(" WHERE ")?;

    Some(predicate.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Asserts that `connection` refuses the row `sql` describes.
///
/// The refusal must be a constraint failure: a test that only asserted *an* error
/// would pass when the statement failed for a typo. `what` names the row, so a
/// failure reports the invariant rather than the statement.
#[track_caller]
fn refuses<P: Params>(connection: &Connection, sql: &str, parameters: P, what: &str) {
    match connection.execute(sql, parameters) {
        Ok(_) => panic!("the schema must refuse {what}"),
        Err(error) => {
            let reported = error.to_string();

            assert!(
                reported.contains("constraint failed"),
                "the refusal of {what} must be a constraint failure, but SQLite \
                 reported: {reported}"
            );
        }
    }
}

/// Asserts that `connection` accepts the row `sql` describes.
#[track_caller]
fn accepts<P: Params>(connection: &Connection, sql: &str, parameters: P, what: &str) {
    if let Err(error) = connection.execute(sql, parameters) {
        panic!("the schema must accept {what}, but SQLite reported: {error}");
    }
}

/// Returns the number of rows in `table`.
fn count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("the table must be queryable")
}

/// A 16-byte identity, as a UUIDv7 column stores one.
fn id(byte: u8) -> Vec<u8> {
    vec![byte; 16]
}

/// A 32-byte digest, as a SHA-256 column stores one.
fn digest(byte: u8) -> Vec<u8> {
    vec![byte; 32]
}

/// The identities a behavioral test hangs its rows off.
///
/// The field names are the roles, not the tables: `release` is the `releases` row
/// a content belongs to, and a test that needs a second one inserts its own.
struct Fixture {
    system: Vec<u8>,
    source: Vec<u8>,
    scan_run: Vec<u8>,
    game: Vec<u8>,
    release: Vec<u8>,
    content: Vec<u8>,
}

/// Inserts the smallest consistent library schema v1 accepts.
///
/// Most constraints are only reached *through* the library, and a test about one
/// of them should not have to build the graph again. What this inserts is a
/// plausible row of every table a content depends on — a system, a source, a
/// completed scan run, a game, a release, and a content — and nothing that is not
/// needed for that.
fn seed(connection: &Connection) -> Fixture {
    let fixture = Fixture {
        system: id(0x01),
        source: id(0x02),
        scan_run: id(0x03),
        game: id(0x04),
        release: id(0x05),
        content: id(0x06),
    };

    connection
        .execute(
            "INSERT INTO systems (system_id, catalog_key, created_at) VALUES (?1, 'nes', 0)",
            [&fixture.system],
        )
        .expect("a system must be insertable");

    connection
        .execute(
            "INSERT INTO library_sources \
             (id, display_name, location, platform_locator_kind, fixed_system_id, \
              availability, availability_checked_at, created_at, removed_at) \
             VALUES (?1, 'NES library', '/roms/nes', 'Path', ?2, 'Available', 0, 0, NULL)",
            rusqlite::params![&fixture.source, &fixture.system],
        )
        .expect("a library source must be insertable");

    connection
        .execute(
            "INSERT INTO scan_runs \
             (id, target_kind, target_source_id, status, started_at, finished_at, \
              reconciliation_eligible, discovered_count, imported_count, updated_count, \
              rediscovered_count, missing_count, unknown_count, unsupported_count, \
              problem_count) \
             VALUES (?1, 'FullLibrary', NULL, 'Completed', 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0)",
            [&fixture.scan_run],
        )
        .expect("a scan run must be insertable");

    connection
        .execute(
            "INSERT INTO games (id, first_seen_at, default_release_id, is_favorite, is_hidden, \
              is_ignored) \
             VALUES (?1, 0, NULL, 0, 0, 0)",
            [&fixture.game],
        )
        .expect("a game must be insertable");

    connection
        .execute(
            "INSERT INTO releases \
             (id, game_id, system_id, release_title, release_date, revision, release_type, \
              release_key, archive_kind, created_at) \
             VALUES (?1, ?2, ?3, 'Fixture Game (Europe)', '1990', NULL, 'Official', ?4, NULL, 0)",
            rusqlite::params![
                &fixture.release,
                &fixture.game,
                &fixture.system,
                digest(0x11)
            ],
        )
        .expect("a release must be insertable");

    connection
        .execute(
            "INSERT INTO contents \
             (id, release_id, game_id, content_kind, format, validation_state, \
              validation_detail, disc_index, created_at) \
             VALUES (?1, ?2, ?3, 'Content', 'nes', 'Valid', NULL, NULL, 0)",
            rusqlite::params![&fixture.content, &fixture.release, &fixture.game],
        )
        .expect("a content must be insertable");

    fixture
}
