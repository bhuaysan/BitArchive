//! The application startup sequence (ARCHITECTURE.md §6.1).
//!
//! The composition root is the one place that knows every concrete
//! implementation, so it is also the place that decides *when* each of them comes
//! up. This module holds the part of that sequence which exists today: step 1,
//! the application paths, and steps 4 and 5, opening SQLite and applying the
//! migrations.
//!
//! ```text
//! 1. AppPaths bestimmen          ← AppPaths::default()
//! 2. Logging initialisieren      ← not implemented yet
//! 3. Curated Catalog laden       ← not implemented yet
//! 4. SQLite öffnen          ─┐
//! 5. Migrationen ausführen  ─┴─► open_database()
//! 6. Settings laden              ← not implemented yet
//! ...
//! 11. Presentation Layer         ← bitarchive_ui
//! 12. Slint UI starten           ← bitarchive_ui
//! ```
//!
//! The steps that are not implemented yet are named rather than silently skipped,
//! because the order matters: the UI is started only after the database is open
//! and migrated, so a presentation layer that is handed a repository later can
//! never observe a schema that is still being brought up (§6.1: "Die normale UI
//! erhält erst Zugriff auf Repositories und Services, wenn Bootstrap, Migration
//! und Recovery erfolgreich abgeschlossen sind").
//!
//! # One path, one database
//!
//! The database directory comes from [`AppPaths::database`] and is not assembled
//! here. `AppPaths` is the single authority on where BitArchive's files live
//! (ARCHITECTURE.md §35), so a second place that spelled out
//! `…/BitArchive/database` would be a second answer to the same question — and
//! the two would drift.

use bitarchive_infrastructure::{Database, DatabaseError};
use bitarchive_platform::AppPaths;

/// Opens the BitArchive database and brings it up to the current schema version.
///
/// The database lives below [`AppPaths::database`], is created there if it does
/// not exist, and is migrated to the version this binary was built with. It is a
/// blocking call — the file is opened, configured, and migrated before it returns
/// — so it belongs in startup and not on a UI thread callback.
///
/// The returned handle must be kept for as long as the database is needed: it owns
/// the thread that holds the connection, and dropping it closes the database. A
/// later step of the sequence passes it to the repositories it constructs.
///
/// # Errors
///
/// Returns [`DatabaseError::OpenFailed`] when the database cannot be created or
/// configured, and [`DatabaseError::SchemaTooNew`] when the file holds a schema
/// from a newer BitArchive version. The second is a refusal, not a repair: the
/// file is left untouched so that the newer version can still use it.
pub fn open_database(paths: &AppPaths) -> Result<Database, DatabaseError> {
    Database::open(&paths.database())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use bitarchive_infrastructure::SchemaVersion;

    use super::*;

    /// A temporary application-data root that removes itself.
    ///
    /// The desktop binary normally works in the user's real application data
    /// directory; a test must not, so it builds the same [`AppPaths`] layout below
    /// a directory of its own (ARCHITECTURE.md §45.2).
    struct TempRoot {
        root: PathBuf,
    }

    impl TempRoot {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);

            let root = std::env::temp_dir().join(format!(
                "bitarchive-bootstrap-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));

            let _ = fs::remove_dir_all(&root);

            Self { root }
        }

        fn paths(&self) -> AppPaths {
            AppPaths::below(&self.root)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// Returns the names of the entries in `directory`, sorted.
    fn entries(directory: &Path) -> Vec<String> {
        let mut names = fs::read_dir(directory)
            .expect("the directory must be readable")
            .map(|entry| {
                entry
                    .expect("every entry must be readable")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();

        names.sort();

        names
    }

    /// Returns the database files in `directory`.
    ///
    /// WAL mode keeps `<database>-wal` and `<database>-shm` beside the database
    /// while a connection is open, so those are left out: a test here asks how
    /// many *databases* were created, and a sidecar is not a second one.
    fn database_files(directory: &Path) -> Vec<String> {
        entries(directory)
            .into_iter()
            .filter(|name| !name.ends_with("-wal") && !name.ends_with("-shm"))
            .collect()
    }

    /// The database is opened below the application-data directory `AppPaths`
    /// names, and nowhere else.
    ///
    /// This is the check that the composition root goes through the path service
    /// rather than assembling a directory of its own: the only thing the root
    /// gains is the directory `AppPaths::database()` returns.
    #[test]
    fn the_database_is_opened_below_the_application_paths() {
        let temporary = TempRoot::new();
        let paths = temporary.paths();

        let database = open_database(&paths).expect("the database must open");

        assert!(
            paths.database().is_dir(),
            "the database directory must have been created"
        );
        assert_eq!(
            database_files(&paths.database()).len(),
            1,
            "the database directory must hold the database file: {:?}",
            entries(&paths.database())
        );
        assert_eq!(
            entries(paths.application_support()),
            vec!["database".to_owned()],
            "opening the database must create nothing else in the application data \
             directory"
        );

        drop(database);
    }

    /// A fresh application starts at a schema version with no migration applied.
    ///
    /// Schema v1 is Issue #109, so the foundation on its own records version 0 and
    /// creates only its own ledger.
    #[test]
    fn a_fresh_application_starts_at_the_foundation_version() {
        let temporary = TempRoot::new();

        let database = open_database(&temporary.paths()).expect("the database must open");

        assert_eq!(
            database
                .schema_version()
                .expect("the version must be readable"),
            SchemaVersion::NONE
        );

        drop(database);
    }

    /// Starting the application again reuses the same database rather than
    /// creating a second one.
    #[test]
    fn starting_up_again_reuses_the_database() {
        let temporary = TempRoot::new();
        let paths = temporary.paths();

        let first = open_database(&paths).expect("the database must open");

        let version = first
            .schema_version()
            .expect("the version must be readable");

        drop(first);

        let second = open_database(&paths).expect("the database must open again");

        assert_eq!(
            second
                .schema_version()
                .expect("the version must be readable"),
            version,
            "a restart must not change the schema version"
        );
        assert_eq!(
            database_files(&paths.database()).len(),
            1,
            "a restart must not create a second database file"
        );

        drop(second);
    }
}
