//! The forward-only migration runner and the ledger it keeps.
//!
//! A migration is a numbered, forward-only step from one schema version to the
//! next (ARCHITECTURE.md §9). The runner applies the steps this build knows, in
//! ascending order, and records each one in `schema_migrations` — the ledger
//! `DATA_MODEL.md` §18.1 defines — inside the same transaction as the step
//! itself. There is deliberately **no down-migration**: a released migration is
//! never undone, and an application that meets a database from a newer build
//! refuses it instead of guessing (DATA_MODEL.md §18.2, §18.3).
//!
//! ```text
//! read the ledger            ← the recorded schema version
//!       ↓
//! newer than this build?     ← refuse, and change nothing
//!       ↓
//! create the ledger if absent
//!       ↓
//! for each newer migration:  BEGIN → apply → record → COMMIT
//!       ↓
//! read the ledger back       ← what was actually applied
//! ```
//!
//! # The ledger is bookkeeping, not a migration
//!
//! `schema_migrations` is created by the runner itself, before any numbered
//! migration runs, and it is not itself recorded in the ledger. That keeps one
//! unambiguous meaning for a version number: **schema version 1 is the first
//! domain migration**, which is Issue #109, and a database this foundation
//! created on its own is at version [`SchemaVersion::NONE`]. A single version row
//! would answer "what version is this?" just as well, but the ledger also answers
//! "was migration 7 applied with a definition that has since been edited?", and
//! the runner checks exactly that on every open.
//!
//! # One transaction per migration
//!
//! Each migration is applied inside its own transaction together with the ledger
//! row that records it. A failure therefore leaves a *definite* state: the
//! migration is not recorded, nothing it created survives, and the database stays
//! at the highest version that did succeed. A later run retries precisely the
//! failed migration. Applied migrations before it are not re-run, because they are
//! already recorded — a re-run is idempotent without any conditional SQL.

use std::fmt;

use rusqlite::{Connection, Transaction, params};
use sha2::{Digest as _, Sha256};

use super::Database;
use super::error::DatabaseError;

/// The table that records which migrations have been applied.
///
/// The definition is the one `DATA_MODEL.md` §18.1 fixes: a sequential storage
/// primary key, the migration's description, the digest of its own definition,
/// when it ran, and which application version ran it.
pub(crate) const LEDGER_TABLE: &str = "schema_migrations";

/// Creates the ledger if it does not exist yet.
///
/// Idempotent by construction, so it is the first write of every run rather than
/// a one-time setup step that a restored or copied database could have missed.
const CREATE_LEDGER: &str = "\
CREATE TABLE IF NOT EXISTS schema_migrations (
    version            INTEGER PRIMARY KEY NOT NULL,
    description        TEXT    NOT NULL,
    checksum           TEXT    NOT NULL,
    applied_at         INTEGER NOT NULL,
    bitarchive_version TEXT    NOT NULL
)";

/// The application version recorded for the migrations this build applies.
const APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A schema version.
///
/// [`SchemaVersion::NONE`] is version 0: the ledger exists and records no applied
/// migration. Migrations are numbered from 1, so a database at `NONE` has only
/// the ledger and whatever the foundation itself created.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SchemaVersion(u32);

impl SchemaVersion {
    /// The version of a database that has no applied migration.
    pub const NONE: Self = Self(0);

    /// Creates a version from a migration number.
    #[must_use]
    pub const fn new(version: u32) -> Self {
        Self(version)
    }

    /// Returns the version number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One forward-only schema step.
///
/// The definition is compiled into the binary, so a built application applies
/// exactly the migrations it was built with and nothing else. `sql` is a
/// `&'static str`, which means a later Issue can either inline the statements or
/// pull them out of a file with `include_str!` — `ARCHITECTURE.md` §5 sketches a
/// `migrations/` directory, and that choice is left to the Issue that adds the
/// first real migration.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Migration {
    /// The version this migration brings the schema to.
    pub(crate) version: u32,
    /// The human-readable purpose, as the ledger records it.
    pub(crate) description: &'static str,
    /// The statements that carry the migration out.
    pub(crate) sql: &'static str,
}

impl Migration {
    /// Returns the version this migration brings the schema to.
    pub(crate) const fn version(&self) -> SchemaVersion {
        SchemaVersion::new(self.version)
    }

    /// Returns the digest of this migration's own definition.
    ///
    /// The digest covers the version, the description, and the statements, which
    /// together are everything that decides what the migration does. It is stored
    /// in the ledger so that an edited migration is detectable, and it is
    /// compared on every open.
    ///
    /// The fields are separated by a NUL, which the description and the SQL
    /// cannot contain, so two different definitions can never produce the same
    /// input text.
    pub(crate) fn checksum(&self) -> String {
        let mut digest = Sha256::new();

        digest.update(self.version.to_string().as_bytes());
        digest.update([0]);
        digest.update(self.description.as_bytes());
        digest.update([0]);
        digest.update(self.sql.as_bytes());

        format!("{:x}", digest.finalize())
    }

    /// Applies this migration inside `transaction`.
    ///
    /// The caller owns the transaction; this only writes what the migration and
    /// its ledger row consist of, so the two can never be separated.
    fn apply(&self, transaction: &Transaction<'_>) -> Result<(), DatabaseError> {
        let failure = |cause: rusqlite::Error| DatabaseError::MigrationFailed {
            version: self.version(),
            description: self.description,
            cause: cause.to_string(),
        };

        transaction.execute_batch(self.sql).map_err(failure)?;

        transaction
            .execute(
                "INSERT INTO schema_migrations \
                 (version, description, checksum, applied_at, bitarchive_version) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    self.version,
                    self.description,
                    self.checksum(),
                    applied_at(),
                    APPLICATION_VERSION,
                ],
            )
            .map_err(failure)?;

        Ok(())
    }
}

/// Applies the migrations of one build, in order.
///
/// A runner carries a fixed catalogue for its whole life: it is constructed from
/// the migrations the binary was built with, so the set of steps that can run is
/// decided at build time and not by anything on disk.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MigrationRunner {
    migrations: &'static [Migration],
}

impl MigrationRunner {
    /// Creates a runner over a migration catalogue.
    ///
    /// # Panics
    ///
    /// Never in a release build. A catalogue whose versions are not exactly
    /// `1..=n` in ascending order is rejected in debug builds only, because it is
    /// a programming error in this crate: the catalogue is a constant compiled
    /// into the binary.
    pub(crate) const fn new(migrations: &'static [Migration]) -> Self {
        let mut expected = 1;
        let mut index = 0;

        while index < migrations.len() {
            debug_assert!(
                migrations[index].version == expected,
                "the migration catalogue must be numbered 1..=n in ascending order"
            );

            expected += 1;
            index += 1;
        }

        Self { migrations }
    }

    /// Returns the highest version this build knows.
    pub(crate) fn supported_schema_version(&self) -> SchemaVersion {
        self.migrations
            .last()
            .map_or(SchemaVersion::NONE, |migration| migration.version())
    }

    /// Brings `database` up to the version this build expects.
    ///
    /// Returns the version that is recorded once the run has finished, which is
    /// the highest migration this build knows — read back from the ledger rather
    /// than assumed, so the answer is what is actually stored.
    ///
    /// Every statement runs on the database's own thread: this function decides
    /// *which* migration runs next, and the database handle decides *where* it
    /// runs (ARCHITECTURE.md §8.2, §48).
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseError::SchemaTooNew`] without writing anything if the
    /// database is already ahead of this build, and
    /// [`DatabaseError::MigrationFailed`] if a migration fails, in which case the
    /// failed migration and its ledger row are both rolled back.
    pub(crate) fn apply(&self, database: &Database) -> Result<SchemaVersion, DatabaseError> {
        let supported = self.supported_schema_version();

        // The refusal comes before the first write of this run, and the caller
        // has already run the same check before allowing the connection to change
        // the database at all (see `executor::open_connection`). Repeating it here
        // is what makes `apply` safe to call on its own — a restored or copied
        // database goes through this path without an open sequence in front of it
        // (DATA_MODEL.md §18.2 rule 5) — and it costs one read.
        let applied =
            database.read(move |connection| check_compatibility(connection, supported))?;

        database.write(|transaction| {
            transaction
                .execute_batch(CREATE_LEDGER)
                .map_err(DatabaseError::query)
        })?;

        self.verify_ledger(database.read(read_ledger)?)?;

        for migration in self.migrations {
            if migration.version() <= applied {
                continue;
            }

            database.write(move |transaction| migration.apply(transaction))?;
        }

        database.read(applied_schema_version)
    }

    /// Checks the recorded migrations against this build's catalogue.
    ///
    /// The ledger must be the gapless sequence `1..=n` that this runner produces,
    /// every recorded migration must exist in the catalogue, and its recorded
    /// digest must match the definition compiled into the binary.
    fn verify_ledger(&self, recorded: Vec<RecordedMigration>) -> Result<(), DatabaseError> {
        let highest = recorded
            .last()
            .map_or(SchemaVersion::NONE, |migration| migration.version);

        // `recorded` is ordered by version, so its length is the number of
        // versions a gapless sequence starting at 1 would end at.
        if recorded.len() as u64 != u64::from(highest.get()) {
            return Err(DatabaseError::LedgerOutOfSequence {
                highest,
                recorded: recorded.len() as u64,
            });
        }

        for entry in &recorded {
            let Some(migration) = self
                .migrations
                .iter()
                .find(|migration| migration.version() == entry.version)
            else {
                return Err(DatabaseError::AppliedMigrationUnknown {
                    version: entry.version,
                });
            };

            if migration.checksum() != entry.checksum {
                return Err(DatabaseError::MigrationDefinitionChanged {
                    version: entry.version,
                });
            }
        }

        Ok(())
    }
}

/// Returns the schema version recorded in `connection`, refusing a database this
/// build is too old for.
///
/// **This function performs no write.** It reads the ledger — or finds none —
/// and compares the result with `supported`; nothing it does outlives the
/// connection it was handed. That is the property the whole refusal depends on:
/// it is called *before* the first statement that changes the database file, so a
/// database from a newer BitArchive version is refused while still exactly as its
/// own version left it (DATA_MODEL.md §18.3, Issue #34).
///
/// The version is returned rather than only compared, because both callers need
/// it anyway: the open sequence refuses and moves on, and [`MigrationRunner::apply`]
/// refuses and then decides which migrations are still missing.
///
/// A database with no ledger has no applied migration, which is
/// [`SchemaVersion::NONE`] and not an error: that is what a fresh database looks
/// like before the runner creates the ledger, and a database that does not exist
/// cannot be too new.
///
/// # Errors
///
/// Returns [`DatabaseError::SchemaTooNew`] when the database is ahead of this
/// build, and [`DatabaseError::QueryFailed`] when the ledger cannot be read.
pub(crate) fn check_compatibility(
    connection: &Connection,
    supported: SchemaVersion,
) -> Result<SchemaVersion, DatabaseError> {
    let applied = applied_schema_version(connection)?;

    if applied > supported {
        return Err(DatabaseError::SchemaTooNew {
            database: applied,
            supported,
        });
    }

    Ok(applied)
}

/// Returns the schema version recorded in `connection`'s ledger.
///
/// A database with no ledger has no applied migration, which is version
/// [`SchemaVersion::NONE`] and not an error: that is what a fresh database looks
/// like before the runner creates the ledger.
///
/// This is the read behind [`check_compatibility`]; it deliberately does not
/// compare, so the two concerns — what is recorded, and whether this build
/// accepts it — stay separable.
pub(crate) fn applied_schema_version(
    connection: &Connection,
) -> Result<SchemaVersion, DatabaseError> {
    if !has_ledger(connection)? {
        return Ok(SchemaVersion::NONE);
    }

    let highest: Option<i64> = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .map_err(DatabaseError::query)?;

    match highest {
        None => Ok(SchemaVersion::NONE),
        Some(version) => {
            u32::try_from(version)
                .map(SchemaVersion::new)
                .map_err(|_| DatabaseError::QueryFailed {
                    cause: format!(
                        "the ledger records version {version}, which is not a valid one"
                    ),
                })
        }
    }
}

/// Returns `true` when `connection` has a migration ledger.
fn has_ledger(connection: &Connection) -> Result<bool, DatabaseError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![LEDGER_TABLE],
            |row| row.get(0),
        )
        .map_err(DatabaseError::query)?;

    Ok(count > 0)
}

/// One applied migration, as the ledger records it.
struct RecordedMigration {
    version: SchemaVersion,
    checksum: String,
}

/// Reads every migration the ledger records, in ascending version order.
fn read_ledger(connection: &Connection) -> Result<Vec<RecordedMigration>, DatabaseError> {
    let mut statement = connection
        .prepare("SELECT version, checksum FROM schema_migrations ORDER BY version")
        .map_err(DatabaseError::query)?;

    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(DatabaseError::query)?;

    let mut recorded = Vec::new();

    for row in rows {
        let (version, checksum) = row.map_err(DatabaseError::query)?;

        let version = u32::try_from(version).map_err(|_| DatabaseError::QueryFailed {
            cause: format!("the ledger records version {version}, which is not a valid one"),
        })?;

        recorded.push(RecordedMigration {
            version: SchemaVersion::new(version),
            checksum,
        });
    }

    Ok(recorded)
}

/// Returns the current instant in the encoding the model uses: milliseconds
/// since the Unix epoch, UTC (DATA_MODEL.md §3.2).
fn applied_at() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
}
