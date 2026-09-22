//! Everything that can go wrong while opening, migrating, or using the database.
//!
//! The variants are failure *categories*, not messages (ARCHITECTURE.md §33.1):
//! a caller tells "this build does not know this migration" from "the database
//! was written by a newer build" from "a migration failed and was rolled back".
//!
//! Two properties are deliberate:
//!
//! - **No variant carries a filesystem path.** The database lives below the
//!   user's home directory, and these messages reach the user, so the path is
//!   never part of one — a diagnostic says *what* is wrong, never *where* the
//!   user's application data happens to be (Issue #34).
//! - **No variant carries a `rusqlite` type.** The layer above this crate
//!   compiles against these variants, and SQLite stays inside this crate
//!   (ARCHITECTURE.md §8.3). A cause that needs to survive is carried as its
//!   own text.

use std::fmt;

use super::migration::SchemaVersion;

/// Everything that can go wrong while opening, migrating, or using the database.
#[derive(Debug)]
pub enum DatabaseError {
    /// The database file could not be created, opened, or configured.
    ///
    /// The cause is the operating system's or SQLite's own message, which names
    /// no path.
    OpenFailed {
        /// What the operating system or SQLite reported.
        cause: String,
    },
    /// The connection did not end up in WAL mode.
    ///
    /// WAL is a persistent property of the database file (ARCHITECTURE.md §8.1),
    /// so this is reported rather than assumed: the mode is read back after it is
    /// requested, and a build that silently ran without it would differ in
    /// concurrency and durability from every other one.
    JournalModeNotWal {
        /// The mode SQLite reported after WAL was requested.
        mode: String,
    },
    /// Foreign-key enforcement could not be switched on.
    ///
    /// `PRAGMA foreign_keys` is per connection and off by default, so a
    /// connection on which it did not take effect would accept rows that violate
    /// the schema's references (ARCHITECTURE.md §8.1).
    ForeignKeysNotEnforced,
    /// The applied migrations are not the gapless sequence version 1 upwards.
    ///
    /// Version `n + 1` may only be applied after version `n` (DATA_MODEL.md
    /// §18.2), so a ledger that records fewer migrations than its highest version
    /// cannot be the history this runner produced.
    LedgerOutOfSequence {
        /// The highest version the ledger records.
        highest: SchemaVersion,
        /// How many migrations the ledger actually records.
        recorded: u64,
    },
    /// The ledger records a migration this build does not know.
    ///
    /// The database was written by a build with a different migration
    /// catalogue — a fork, or a build whose catalogue was edited — and applying
    /// this build's migrations on top of it could produce a schema neither build
    /// expects.
    AppliedMigrationUnknown {
        /// The version this build does not know.
        version: SchemaVersion,
    },
    /// A migration's recorded definition differs from this build's.
    ///
    /// A released migration must not be edited (DATA_MODEL.md §18.2 rule 7);
    /// this is what the ledger's `checksum` column detects.
    MigrationDefinitionChanged {
        /// The version whose definition changed.
        version: SchemaVersion,
    },
    /// The database schema is newer than this application supports.
    ///
    /// This is the refusal of DATA_MODEL.md §18.3 and ARCHITECTURE.md §9. It is
    /// decided before the first statement that would change the database — a
    /// refused file keeps its journal mode as well as its rows, because the journal
    /// mode is written into the file and is not undone by closing the connection.
    SchemaTooNew {
        /// The version recorded in the database.
        database: SchemaVersion,
        /// The highest version this build knows.
        supported: SchemaVersion,
    },
    /// A migration failed.
    ///
    /// It ran inside one transaction, so nothing it did survives and it was not
    /// recorded as applied: the database is at `version` — the highest version
    /// that did succeed — and a later run retries this one.
    MigrationFailed {
        /// The version that failed.
        version: SchemaVersion,
        /// The migration's own description.
        description: &'static str,
        /// What SQLite reported.
        cause: String,
    },
    /// A statement or query failed outside a migration.
    QueryFailed {
        /// What SQLite reported.
        cause: String,
    },
    /// The database's execution thread is no longer running.
    ///
    /// A `Database` runs every operation on one owned thread; this is reported
    /// when that thread has stopped, which happens after an earlier operation
    /// panicked. The connection is not shared, so there is no poisoned state to
    /// continue from and no reason to pretend the handle still works.
    ExecutorStopped,
}

impl DatabaseError {
    /// Wraps a failed statement or query.
    ///
    /// Used where the failure is not a migration's, so a migration failure can
    /// never be reported as a generic query failure by accident.
    pub(crate) fn query(cause: rusqlite::Error) -> Self {
        Self::QueryFailed {
            cause: cause.to_string(),
        }
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenFailed { cause } => {
                write!(f, "the database could not be opened: {cause}")
            }
            Self::JournalModeNotWal { mode } => write!(
                f,
                "the database is in journal mode {mode} instead of WAL, so it was not opened"
            ),
            Self::ForeignKeysNotEnforced => write!(
                f,
                "the database connection could not enforce foreign keys, so it was not opened"
            ),
            Self::LedgerOutOfSequence { highest, recorded } => write!(
                f,
                "the migration ledger records {recorded} migrations but its highest version is \
                 {highest}, which is not a gapless sequence"
            ),
            Self::AppliedMigrationUnknown { version } => write!(
                f,
                "the migration ledger records migration {version}, which this version of \
                 BitArchive does not know"
            ),
            Self::MigrationDefinitionChanged { version } => write!(
                f,
                "migration {version} was applied with a different definition than this version of \
                 BitArchive carries"
            ),
            Self::SchemaTooNew {
                database,
                supported,
            } => write!(
                f,
                "this library was created by a newer BitArchive version: its schema is at version \
                 {database}, and this version supports {supported}"
            ),
            Self::MigrationFailed {
                version,
                description,
                cause,
            } => write!(
                f,
                "migration {version} ({description}) failed and was rolled back: {cause}"
            ),
            Self::QueryFailed { cause } => write!(f, "a database operation failed: {cause}"),
            Self::ExecutorStopped => write!(
                f,
                "the database is no longer available, because its execution thread has stopped"
            ),
        }
    }
}

impl std::error::Error for DatabaseError {}
