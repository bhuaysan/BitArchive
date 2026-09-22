//! The SQLite foundation, exercised against isolated temporary databases.
//!
//! Every test here works inside its own [`TempDatabase`], a temporary directory
//! that removes itself. Nothing opens `AppPaths::database()` of the real user, so
//! a test run cannot read, migrate, or remove a developer's library
//! (ARCHITECTURE.md §45.2), and the tests stay parallel-safe because no two of
//! them share a file. The helper itself is general infrastructure and lives in
//! [`crate::tests::support::database`], where any test module can reach it; what
//! is specific to *these* tests — the fixture migrations and the assertions built
//! on them — stays here.
//!
//! The migration runner is driven with **fixture migrations** that live in this
//! file. They create small dummy tables so that ordering, idempotence, failure,
//! and the newer-database refusal can be observed; none of them is a BitArchive
//! table, and the shipped catalogue is asserted to be empty so the two cannot be
//! confused with schema v1 (Issue #109).
//!
//! The tests assert *behaviour*: that a re-run leaves the data as it was, that a
//! failed migration left no table behind, that a refused database still holds the
//! schema it had, and that foreign keys and WAL are in effect when queried —
//! rather than that some pragma was requested.

use std::fs;

use rusqlite::Connection;

use super::support::database::TempDatabase;
use crate::database::{
    BITARCHIVE_MIGRATIONS, DATABASE_FILE_NAME, DatabaseError, Migration, MigrationRunner,
    SchemaVersion,
};

/// The first fixture migration: one table, and one row so that a re-run of a
/// later migration is observable.
const FIXTURE_ONE: Migration = Migration {
    version: 1,
    description: "fixture: a parent table with one row",
    sql: "CREATE TABLE fixture_parents (id INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL);\n\
          INSERT INTO fixture_parents (id, name) VALUES (1, 'first');\n",
};

/// The second fixture migration: a child table and a change that is *not*
/// idempotent, so a second run of it would be visible.
const FIXTURE_TWO: Migration = Migration {
    version: 2,
    description: "fixture: a child table, and a change that must happen once",
    sql: "CREATE TABLE fixture_children (\n\
          \x20   id INTEGER PRIMARY KEY NOT NULL,\n\
          \x20   parent_id INTEGER NOT NULL REFERENCES fixture_parents(id)\n\
          );\n\
          UPDATE fixture_parents SET name = name || '!';\n",
};

/// A third fixture migration that fails after doing something: the table it
/// creates must not survive, which is what the transaction is for.
const FIXTURE_THREE_BROKEN: Migration = Migration {
    version: 3,
    description: "fixture: a migration that fails halfway",
    sql: "CREATE TABLE fixture_orphans (id INTEGER PRIMARY KEY NOT NULL);\n\
          INSERT INTO fixture_no_such_table (id) VALUES (1);\n",
};

/// A catalogue of three migrations, the first two of which work.
const FIXTURES: &[Migration] = &[FIXTURE_ONE, FIXTURE_TWO, FIXTURE_THREE_BROKEN];

/// A catalogue of two migrations, both of which work.
const FIXTURES_WITHOUT_THE_BROKEN_ONE: &[Migration] = &[FIXTURE_ONE, FIXTURE_TWO];

/// A catalogue with a single migration, standing in for an older build.
const FIXTURES_ONE_ONLY: &[Migration] = &[FIXTURE_ONE];

fn runner(migrations: &'static [Migration]) -> MigrationRunner {
    MigrationRunner::new(migrations)
}

/// Returns the names of the tables in `connection`, sorted.
fn table_names(connection: &Connection) -> Vec<String> {
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("sqlite_master must be queryable");

    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("sqlite_master must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable")
}

/// Returns every ledger row as `(version, description, checksum, applied_at,
/// bitarchive_version)`, ordered by version.
fn ledger_rows(connection: &Connection) -> Vec<(i64, String, String, i64, String)> {
    let mut statement = connection
        .prepare(
            "SELECT version, description, checksum, applied_at, bitarchive_version \
             FROM schema_migrations ORDER BY version",
        )
        .expect("the ledger must be queryable");

    statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .expect("the ledger must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable")
}

/// Returns the `CREATE` statements of everything the schema contains, sorted.
///
/// Comparing this before and after an operation is how a test shows that nothing
/// in the schema changed.
fn schema_objects(connection: &Connection) -> Vec<String> {
    let mut statement = connection
        .prepare("SELECT COALESCE(sql, '') FROM sqlite_master ORDER BY type, name")
        .expect("sqlite_master must be queryable");

    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("sqlite_master must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable")
}

/// Returns the names of every parent-facing foreign key on `table`.
fn referenced_parents(connection: &Connection, table: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA foreign_key_list({table})"))
        .expect("pragma must be preparable");

    statement
        .query_map([], |row| row.get::<_, String>(2))
        .expect("pragma must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable")
}

/// A fresh database carries the ledger and nothing else.
///
/// This is the boundary of Issue #34 stated as a test: the foundation creates the
/// bookkeeping table, and schema v1's tables arrive with Issue #109.
#[test]
fn a_new_database_carries_only_the_migration_ledger() {
    let temporary = TempDatabase::new();
    let database = temporary.open().expect("a new database must open");

    assert_eq!(
        database
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::NONE,
        "a database with no applied migration is at version 0"
    );

    let connection = temporary.inspect();

    assert_eq!(table_names(&connection), vec!["schema_migrations"]);

    drop(database);
}

/// The ledger has the columns `DATA_MODEL.md` §18.1 fixes.
#[test]
fn the_ledger_has_the_columns_the_data_model_fixes() {
    let temporary = TempDatabase::new();
    let database = temporary.open().expect("a new database must open");

    let connection = temporary.inspect();
    let mut statement = connection
        .prepare("PRAGMA table_info(schema_migrations)")
        .expect("pragma must be preparable");

    let columns = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .expect("pragma must be queryable")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row must be readable");

    assert_eq!(
        columns,
        vec![
            ("version".to_owned(), "INTEGER".to_owned(), 1, 1),
            ("description".to_owned(), "TEXT".to_owned(), 1, 0),
            ("checksum".to_owned(), "TEXT".to_owned(), 1, 0),
            ("applied_at".to_owned(), "INTEGER".to_owned(), 1, 0),
            ("bitarchive_version".to_owned(), "TEXT".to_owned(), 1, 0),
        ]
    );

    drop(database);
}

/// The shipped catalogue knows no migration, because schema v1 is Issue #109.
#[test]
fn the_shipped_catalogue_carries_no_migration_yet() {
    assert!(
        BITARCHIVE_MIGRATIONS.is_empty(),
        "schema v1 belongs to Issue #109; the foundation must not pre-empt it"
    );

    assert_eq!(
        runner(BITARCHIVE_MIGRATIONS).supported_schema_version(),
        SchemaVersion::NONE
    );
}

/// Migrations run in ascending order, and a later one can rely on an earlier one.
#[test]
fn migrations_are_applied_in_ascending_order() {
    let temporary = TempDatabase::new();
    let database = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect("the fixture migrations must apply");

    assert_eq!(
        database
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::new(2)
    );

    let connection = temporary.inspect();

    // Migration 2 references the table migration 1 created, so it can only have
    // succeeded after it.
    assert_eq!(
        referenced_parents(&connection, "fixture_children"),
        vec!["fixture_parents"]
    );

    let versions: Vec<i64> = ledger_rows(&connection)
        .into_iter()
        .map(|(version, ..)| version)
        .collect();

    assert_eq!(versions, vec![1, 2]);

    drop(database);
}

/// Running the same migrations again changes nothing.
#[test]
fn applying_migrations_twice_is_idempotent() {
    let temporary = TempDatabase::new();

    let first = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect("the fixture migrations must apply");

    let after_first_run = {
        let connection = temporary.inspect();
        let ledger = ledger_rows(&connection);
        let name: String = connection
            .query_row("SELECT name FROM fixture_parents WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("the row the first migration inserted must be readable");

        (ledger, name, schema_objects(&connection))
    };

    drop(first);

    let second = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect("re-opening an up-to-date database must succeed");

    assert_eq!(
        second
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::new(2),
        "the second run must leave the schema version where it was"
    );

    let after_second_run = {
        let connection = temporary.inspect();
        let ledger = ledger_rows(&connection);
        let name: String = connection
            .query_row("SELECT name FROM fixture_parents WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("the row must still be readable");

        (ledger, name, schema_objects(&connection))
    };

    assert_eq!(
        after_first_run, after_second_run,
        "a second run must add no ledger row, change no schema object, and re-run \
         no migration — migration 2 appends a character, so running it again would \
         be visible in the row"
    );

    assert_eq!(after_first_run.1, "first!");

    drop(second);
}

/// A database from a newer build is refused with its own error.
#[test]
fn a_newer_database_is_refused() {
    let temporary = TempDatabase::new();

    drop(
        temporary
            .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
            .expect("the fixture migrations must apply"),
    );

    // The same database, met by a build that only knows migration 1.
    let error = temporary
        .open_with(runner(FIXTURES_ONE_ONLY))
        .expect_err("a database from a newer build must not open");

    match error {
        DatabaseError::SchemaTooNew {
            database,
            supported,
        } => {
            assert_eq!(database, SchemaVersion::new(2));
            assert_eq!(supported, SchemaVersion::new(1));
        }
        other => panic!("expected the newer-schema refusal, got {other:?}"),
    }
}

/// A refused database is left exactly as it was found.
#[test]
fn a_refused_database_is_not_modified() {
    let temporary = TempDatabase::new();

    drop(
        temporary
            .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
            .expect("the fixture migrations must apply"),
    );

    let before = {
        let connection = temporary.inspect();

        (schema_objects(&connection), ledger_rows(&connection))
    };

    let _ = temporary.open_with(runner(FIXTURES_ONE_ONLY));
    let _ = temporary.open_with(runner(BITARCHIVE_MIGRATIONS));

    let after = {
        let connection = temporary.inspect();

        (schema_objects(&connection), ledger_rows(&connection))
    };

    assert_eq!(
        before, after,
        "the refusal must not write a ledger row, drop a schema object, or change \
         one"
    );

    assert_eq!(
        before.1.len(),
        2,
        "the ledger must still record exactly the two migrations that were applied"
    );
}

/// Refusing a newer database leaves its journal mode — and everything else —
/// exactly as it was.
///
/// This is the regression test for the order of an open. WAL is a property of the
/// *file*, not of a connection, so a build that configures its connection before
/// asking whether it may use the database rewrites the journal mode of a database
/// from a newer BitArchive version on the way to rejecting it. Comparing
/// `sqlite_master` and the ledger cannot see that, because the journal mode is
/// stored in the database header.
///
/// The database here is left in `DELETE` mode on purpose, standing in for one
/// whose own version chose a different journal mode. It must still be in `DELETE`
/// mode after being refused.
#[test]
fn a_refused_database_keeps_its_journal_mode() {
    let temporary = TempDatabase::new();

    drop(
        temporary
            .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
            .expect("the fixture migrations must apply"),
    );

    let before = {
        let connection = temporary.inspect();

        // Switching out of WAL is itself persistent, and it can only be done while
        // this is the only connection on the database — which is why the handle
        // above was dropped first.
        let mode: String = connection
            .query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))
            .expect("the journal mode must be changeable");

        assert_eq!(
            mode.to_ascii_lowercase(),
            "delete",
            "the test must start from a journal mode that is not WAL, otherwise it \
             could not tell a rewrite from a no-op"
        );

        (mode, schema_objects(&connection), ledger_rows(&connection))
    };

    // The same file, met by a build that only knows migration 1.
    let error = temporary
        .open_with(runner(FIXTURES_ONE_ONLY))
        .expect_err("a database from a newer build must not open");

    assert!(
        matches!(error, DatabaseError::SchemaTooNew { .. }),
        "expected the newer-schema refusal, got {error:?}"
    );

    let after = {
        let connection = temporary.inspect();

        let mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("the pragma must be queryable");

        (mode, schema_objects(&connection), ledger_rows(&connection))
    };

    assert_eq!(
        after, before,
        "a refused database must be left exactly as it was found — its journal mode \
         included, because that one is written to the file and outlives the process"
    );
}

/// A migration that fails is not recorded, and nothing it did survives.
#[test]
fn a_failing_migration_is_not_recorded() {
    let temporary = TempDatabase::new();

    let error = temporary
        .open_with(runner(FIXTURES))
        .expect_err("a failing migration must fail the open");

    match error {
        DatabaseError::MigrationFailed { version, cause, .. } => {
            assert_eq!(version, SchemaVersion::new(3));
            assert!(
                cause.contains("fixture_no_such_table"),
                "the failure must name what SQLite reported: {cause}"
            );
        }
        other => panic!("expected a migration failure, got {other:?}"),
    }

    let connection = temporary.inspect();

    assert_eq!(
        table_names(&connection),
        vec!["fixture_children", "fixture_parents", "schema_migrations"],
        "the table the failed migration created before it failed must have been \
         rolled back with it"
    );

    let versions: Vec<i64> = ledger_rows(&connection)
        .into_iter()
        .map(|(version, ..)| version)
        .collect();

    assert_eq!(
        versions,
        vec![1, 2],
        "the failed migration must not be recorded, so the schema version must not \
         claim it succeeded"
    );

    drop(connection);

    // The same build, run again: the two applied migrations are not re-run and the
    // failure is reported again rather than reported as success.
    let error = temporary
        .open_with(runner(FIXTURES))
        .expect_err("the migration must fail again");

    assert!(matches!(
        error,
        DatabaseError::MigrationFailed {
            version,
            ..
        } if version == SchemaVersion::new(3)
    ));

    let connection = temporary.inspect();

    assert_eq!(
        ledger_rows(&connection).len(),
        2,
        "the second run must not have recorded the failed migration either"
    );

    // A build that does not carry the broken migration opens the same file at the
    // version the successful migrations reached.
    drop(connection);

    let database = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect("the database must be usable at the version that succeeded");

    assert_eq!(
        database
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::new(2)
    );
}

/// A ledger that is not the sequence this runner produces is refused.
#[test]
fn a_ledger_that_is_not_a_sequence_is_refused() {
    let temporary = TempDatabase::new();

    drop(
        temporary
            .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
            .expect("the fixture migrations must apply"),
    );

    {
        let connection = temporary.inspect();

        connection
            .execute("DELETE FROM schema_migrations WHERE version = 1", [])
            .expect("the ledger row must be deletable");
    }

    let error = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect_err("a ledger with a gap must be refused");

    assert!(
        matches!(
            error,
            DatabaseError::LedgerOutOfSequence {
                highest,
                recorded: 1,
            } if highest == SchemaVersion::new(2)
        ),
        "expected the sequence refusal, got {error:?}"
    );
}

/// An edited migration is detected by the digest the ledger stores.
#[test]
fn an_edited_migration_is_detected() {
    /// The same version and description as [`FIXTURE_ONE`], with different SQL.
    const EDITED: Migration = Migration {
        version: 1,
        description: "fixture: a parent table with one row",
        sql: "CREATE TABLE fixture_parents (id INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL);\n\
              INSERT INTO fixture_parents (id, name) VALUES (1, 'edited');\n",
    };

    const EDITED_CATALOGUE: &[Migration] = &[EDITED, FIXTURE_TWO];

    let temporary = TempDatabase::new();

    drop(
        temporary
            .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
            .expect("the fixture migrations must apply"),
    );

    let error = temporary
        .open_with(runner(EDITED_CATALOGUE))
        .expect_err("an edited migration must be detected");

    assert!(
        matches!(
            error,
            DatabaseError::MigrationDefinitionChanged { version }
                if version == SchemaVersion::new(1)
        ),
        "expected the changed-definition refusal, got {error:?}"
    );
}

/// Foreign keys are enforced on the connection the database uses.
#[test]
fn foreign_keys_are_enforced() {
    let temporary = TempDatabase::new();

    let database = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect("the fixture migrations must apply");

    let enforced = database
        .read(|connection| {
            connection
                .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, bool>(0))
                .map_err(DatabaseError::query)
        })
        .expect("the pragma must be queryable");

    assert!(
        enforced,
        "the connection must report foreign keys as enabled"
    );

    // The pragma is not the behaviour: a row that violates the reference the
    // fixture schema declares must be rejected.
    let rejected = database.write(|transaction| {
        transaction
            .execute(
                "INSERT INTO fixture_children (id, parent_id) VALUES (1, 999)",
                [],
            )
            .map_err(DatabaseError::query)
    });

    assert!(
        rejected.is_err(),
        "a child whose parent does not exist must not be insertable"
    );

    let accepted = database.write(|transaction| {
        transaction
            .execute(
                "INSERT INTO fixture_children (id, parent_id) VALUES (1, 1)",
                [],
            )
            .map_err(DatabaseError::query)
    });

    assert!(
        accepted.is_ok(),
        "a child whose parent exists must be insertable"
    );

    drop(database);
}

/// The database file is in WAL mode, as a property of the file.
#[test]
fn the_database_file_is_in_wal_mode() {
    let temporary = TempDatabase::new();

    let database = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect("the fixture migrations must apply");

    let mode = database
        .read(|connection| {
            connection
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .map_err(DatabaseError::query)
        })
        .expect("the pragma must be queryable");

    assert!(mode.eq_ignore_ascii_case("wal"), "journal mode is {mode}");

    drop(database);

    // WAL is stored in the file, so a connection that never asked for it reports
    // it too. This is what tells a persistent setting apart from a per-connection
    // one.
    let connection = temporary.inspect();
    let mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("the pragma must be queryable");

    assert!(mode.eq_ignore_ascii_case("wal"), "journal mode is {mode}");
}

/// The connection waits for a lock instead of failing immediately.
#[test]
fn the_connection_waits_for_a_lock() {
    let temporary = TempDatabase::new();

    let database = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect("the fixture migrations must apply");

    let timeout: i64 = database
        .read(|connection| {
            connection
                .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
                .map_err(DatabaseError::query)
        })
        .expect("the pragma must be queryable");

    assert!(
        timeout >= 1000,
        "a busy timeout must be configured; it is {timeout} ms"
    );

    drop(database);
}

/// The database file is created below the directory it was opened with.
#[test]
fn the_database_file_is_created_below_the_database_directory() {
    let temporary = TempDatabase::new();

    assert!(!temporary.directory().exists(), "the root starts empty");

    let database = temporary.open().expect("a new database must open");

    assert!(
        temporary.file().is_file(),
        "the database file must exist below the database directory"
    );

    let entries = fs::read_dir(temporary.directory())
        .expect("the directory must be readable")
        .map(|entry| {
            entry
                .expect("every entry must be readable")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| !name.ends_with("-wal") && !name.ends_with("-shm"))
        .collect::<Vec<_>>();

    assert_eq!(
        entries,
        vec![DATABASE_FILE_NAME.to_owned()],
        "the database directory holds the database file and nothing else"
    );

    drop(database);
}

/// Two test databases are independent.
#[test]
fn temporary_databases_are_isolated() {
    let first = TempDatabase::new();
    let second = TempDatabase::new();

    assert_ne!(first.directory(), second.directory());

    drop(
        first
            .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
            .expect("the fixture migrations must apply"),
    );

    let second_database = second.open().expect("an empty database must open");

    assert_eq!(
        second_database
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::NONE,
        "a migration applied in one database must not be visible in another"
    );

    assert_eq!(
        table_names(&second.inspect()),
        vec!["schema_migrations"],
        "the second database must carry no table of the first"
    );

    drop(second_database);
}

/// FTS5 is available, because search uses it (ARCHITECTURE.md §36.4).
#[test]
fn fts5_is_available() {
    let temporary = TempDatabase::new();
    let database = temporary.open().expect("a new database must open");

    let matched = database
        .write(|transaction| {
            transaction
                .execute_batch(
                    "CREATE VIRTUAL TABLE fixture_search USING fts5(title);\n\
                     INSERT INTO fixture_search (title) VALUES ('bit archive');\n",
                )
                .map_err(DatabaseError::query)?;

            transaction
                .query_row(
                    "SELECT COUNT(*) FROM fixture_search WHERE fixture_search MATCH 'archive'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(DatabaseError::query)
        })
        .expect("an FTS5 table must be creatable and searchable");

    assert_eq!(matched, 1);

    drop(database);
}

/// Every operation runs on the database's own thread, never on the caller's.
#[test]
fn database_work_runs_on_the_database_thread() {
    let temporary = TempDatabase::new();
    let database = temporary.open().expect("a new database must open");

    let caller = std::thread::current().id();

    let worker = database
        .read(|_| Ok(std::thread::current().id()))
        .expect("the work must run");

    let writer = database
        .write(|_| Ok(std::thread::current().id()))
        .expect("the work must run");

    assert_ne!(
        worker, caller,
        "a read must not run on the calling thread, which may be the UI thread"
    );
    assert_ne!(writer, caller, "a write must not run on the calling thread");

    let name = database
        .read(|_| Ok(std::thread::current().name().map(str::to_owned)))
        .expect("the work must run");

    assert_eq!(name.as_deref(), Some("bitarchive-database"));

    drop(database);
}

/// A write that fails leaves nothing behind, and the database stays usable.
#[test]
fn a_failed_write_is_rolled_back() {
    let temporary = TempDatabase::new();

    let database = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect("the fixture migrations must apply");

    let failure = database.write(|transaction| {
        transaction
            .execute(
                "INSERT INTO fixture_parents (id, name) VALUES (2, 'second')",
                [],
            )
            .map_err(DatabaseError::query)?;

        transaction
            .execute("INSERT INTO fixture_no_such_table (id) VALUES (3)", [])
            .map_err(DatabaseError::query)?;

        Ok(())
    });

    assert!(failure.is_err(), "the failing statement must be reported");

    let rows: i64 = database
        .read(|connection| {
            connection
                .query_row("SELECT COUNT(*) FROM fixture_parents", [], |row| row.get(0))
                .map_err(DatabaseError::query)
        })
        .expect("the table must still be queryable");

    assert_eq!(rows, 1, "the insert before the failure must be rolled back");

    assert!(
        database.schema_version().is_ok(),
        "the database must stay usable after a failed write"
    );

    drop(database);
}

/// Dropping the handle closes the connection on the thread that opened it, and
/// what it leaves on disk opens again unchanged.
///
/// A connection that was closed from the wrong thread, or a file left with a
/// transaction open, would show up here as a database that cannot be reopened.
#[test]
fn dropping_the_handle_leaves_a_reopenable_database() {
    let temporary = TempDatabase::new();

    let database = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect("the fixture migrations must apply");

    assert_eq!(
        database
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::new(2)
    );

    let before = {
        let connection = temporary.inspect();

        (schema_objects(&connection), ledger_rows(&connection))
    };

    drop(database);

    let reopened = temporary
        .open_with(runner(FIXTURES_WITHOUT_THE_BROKEN_ONE))
        .expect("the database must open again after its handle was dropped");

    assert_eq!(
        reopened
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::new(2),
        "the reopened database must be where the closed one left it"
    );

    let after = {
        let connection = temporary.inspect();

        (schema_objects(&connection), ledger_rows(&connection))
    };

    assert_eq!(before, after, "closing must not change what is stored");
}
