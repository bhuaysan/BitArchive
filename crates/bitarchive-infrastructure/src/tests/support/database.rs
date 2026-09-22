//! An isolated SQLite database for one test.
//!
//! Every test that needs a database works inside its own temporary directory, so
//! no test opens `AppPaths::database()` of the real user and a test run cannot
//! read, migrate, or remove a developer's library (ARCHITECTURE.md §45.2). The
//! temporary root carries the process id and a counter, so two tests running at
//! the same time never see each other's files and the tests stay parallel-safe.
//!
//! This is the scaffolding only. Which migrations a test applies, and what it
//! asserts about them, stays in that test: a test that drives the runner with its
//! own migrations calls [`TempDatabase::open_with`], and one that wants the
//! shipped catalogue — a repository test, say — calls [`TempDatabase::open`].
//!
//! ```ignore
//! let temporary = TempDatabase::new();
//! let database = temporary.open()?;
//!
//! assert_eq!(database.schema_version()?, SchemaVersion::NONE);
//! ```
//!
//! Test-only: this module is compiled under `cfg(test)` and is `pub(crate)`, so
//! none of it reaches the library's public API.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;

use crate::database::{DATABASE_FILE_NAME, Database, DatabaseError, MigrationRunner};

/// A temporary database directory that removes itself, and the database in it.
///
/// The directory is created lazily: [`TempDatabase::new`] reserves a path but
/// makes nothing, so a test can tell the difference between a directory that
/// exists and one the foundation created. `AppPaths` creates nothing either
/// (ARCHITECTURE.md §35), and this mirrors it.
pub(crate) struct TempDatabase {
    root: PathBuf,
}

impl TempDatabase {
    /// Reserves a temporary root that no database has been opened in yet.
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);

        let root = std::env::temp_dir().join(format!(
            "bitarchive-database-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));

        // A leftover with this name can only come from a run that was killed
        // before its `Drop`; clearing it keeps a rerun from reading stale state.
        let _ = fs::remove_dir_all(&root);

        Self { root }
    }

    /// Returns the database directory, which is what `AppPaths::database()` is.
    pub(crate) fn directory(&self) -> &Path {
        &self.root
    }

    /// Returns the path of the database file inside the database directory.
    pub(crate) fn file(&self) -> PathBuf {
        self.root.join(DATABASE_FILE_NAME)
    }

    /// Opens the database with the shipped migration catalogue.
    ///
    /// This is what the application itself opens, so a test that is about stored
    /// data rather than about migrations — a repository test — starts here.
    pub(crate) fn open(&self) -> Result<Database, DatabaseError> {
        Database::open(self.directory())
    }

    /// Opens the database with a test's own migration catalogue.
    pub(crate) fn open_with(&self, runner: MigrationRunner) -> Result<Database, DatabaseError> {
        Database::open_with(self.directory(), runner)
    }

    /// Opens the file directly, without the foundation, to inspect what is stored.
    ///
    /// A refused or failed database cannot be queried through [`Database`], and a
    /// test has to look at what is actually on disk to prove that nothing changed.
    /// It is also the only way to read a property that is not in `sqlite_master`,
    /// such as the journal mode.
    ///
    /// The connection is the test's own: opening one while a [`Database`] handle is
    /// alive gives two connections to the same file, which WAL tolerates. A test
    /// that needs to change a property of the file itself — the journal mode, for
    /// instance — has to drop the handle first, because SQLite only allows that
    /// while nothing else holds the database.
    pub(crate) fn inspect(&self) -> Connection {
        Connection::open(self.file()).expect("the database file must be readable")
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
