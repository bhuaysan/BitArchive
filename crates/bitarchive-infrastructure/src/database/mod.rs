//! The SQLite foundation: the database, its connection settings, and the
//! forward-only migration runner.
//!
//! BitArchive keeps its persistent state in one local SQLite database in its
//! application data directory (ARCHITECTURE.md §8.1, §35). This module is the
//! only place in the workspace that knows how that database is opened, how it is
//! configured, and how it is upgraded:
//!
//! ```text
//! AppPaths::database()                  ← the directory, decided by the platform layer
//!       ↓
//! Database::open()                      ← this module; the file name is decided here
//!       ↓
//! configure the connection              ← foreign_keys = ON and the busy timeout
//!       ↓
//! check the schema version              ← read-only; a newer database stops here
//!       ↓
//! enable WAL                            ← the first statement that changes the file
//!       ↓
//! MigrationRunner::apply                ← the migrations this binary was built with
//!       ↓
//! Database                              ← an opaque handle, used by repositories
//! ```
//!
//! The check sits between the connection settings and WAL on purpose: the settings
//! are held in memory and die with the connection, while the journal mode is
//! written into the database file and outlives the process. A database from a
//! newer BitArchive version is therefore refused while it is still exactly as its
//! own version left it (see `executor::open_connection`).
//!
//! # What this is not
//!
//! There is no BitArchive table yet. Schema v1 — `games`, `releases`, `contents`,
//! the library sources, the fingerprints, the search projection — is Issue #109
//! and arrives as the first migration of [`migrations`]. Establishing the
//! foundation without it is what keeps the meaning of a schema version
//! unambiguous: an empty database is at version 0, and version 1 is schema v1.
//!
//! There are no repositories either. Later Issues add them on top of
//! [`Database`], which is why its execution methods take a closure over the
//! connection rather than exposing one: a repository is written inside this
//! crate, where SQLite is allowed to appear, and the layers above it are not.
//!
//! # The boundary
//!
//! `rusqlite` is a dependency of this crate alone, and no rusqlite type appears
//! in a public signature here. The UI never sees SQLite (invariant 5), the domain
//! never depends on it (invariant 6), and the application layer talks to
//! repositories rather than to SQL. A caller outside this crate can only open the
//! database, read its schema version, and hold the handle.
//!
//! # Threading
//!
//! [`Database`] owns one thread, and every operation runs on it, so calling a
//! database method never runs SQLite on the caller's thread. That matters most
//! for the Slint UI thread, which must never block on I/O (invariant 17,
//! ARCHITECTURE.md §8.2, §48). Opening the database — including the migrations —
//! happens on that thread too, so startup work of unbounded duration is not on
//! the UI thread either.
//!
//! # Failure
//!
//! Failures are typed by category ([`DatabaseError`]) and none of them carries
//! credentials, and none carries a user path (ARCHITECTURE.md §33.1). A database
//! from a newer BitArchive version is refused rather than half-read
//! (DATA_MODEL.md §18.3), and a migration that fails is rolled back rather than
//! recorded (Issue #34 §11).

mod error;
mod executor;
mod migration;
mod migrations;

pub use error::DatabaseError;
pub use executor::Database;
pub use migration::SchemaVersion;

/// The migration machinery and the database file name, for the crate's own tests.
///
/// A test drives the runner with its own fixture migrations and inspects the file
/// those produced, so it needs the pieces the application itself never names: the
/// runner, the migration definition, the shipped catalogue, and the file name
/// below the database directory. All four are internal to the crate; the tests
/// that use them live in `crate::tests::database`.
#[cfg(test)]
pub(crate) use executor::DATABASE_FILE_NAME;
#[cfg(test)]
pub(crate) use migration::{Migration, MigrationRunner};
#[cfg(test)]
pub(crate) use migrations::BITARCHIVE_MIGRATIONS;
