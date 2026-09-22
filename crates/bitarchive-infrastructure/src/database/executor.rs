//! The database handle and the one thread that owns its connection.
//!
//! SQLite connections are not shared. A single connection cannot be used from two
//! threads at once, and a free `Arc<Mutex<Connection>>` passed through the
//! application would put the lock, the transaction boundaries, and the decision
//! about which thread blocks into every caller's hands (ARCHITECTURE.md §8.3).
//!
//! Instead, one thread owns the connection for the lifetime of the process and
//! every operation is handed to it:
//!
//! ```text
//! caller                     database thread
//!   │                              │
//!   ├── open(path) ───────────────►│ open, configure, migrate
//!   │◄── ready, or the error ──────┤
//!   │                              │
//!   ├── read(work) ───────────────►│ work(&connection)
//!   │◄── the value ────────────────┤
//!   │                              │
//!   ├── write(work) ──────────────►│ BEGIN, work(&transaction), COMMIT
//!   │◄── the value ────────────────┤
//! ```
//!
//! Three consequences are the point of the design:
//!
//! - **No SQLite type crosses the crate boundary.** [`Database`] is an opaque
//!   handle; the connection never leaves the thread that opened it.
//! - **Blocking database work is never on the caller's thread.** A caller can be
//!   the Slint UI thread, and it only waits for a result it asked for; the
//!   statement runs somewhere else (invariant 17, ARCHITECTURE.md §48).
//! - **Transactions have one owner.** `write` opens the transaction, runs the
//!   work, and commits — so a repository cannot forget the commit, and a
//!   migration cannot be recorded without its schema change (Issue #34 §11).
//!
//! The executor is deliberately one thread and not a pool: BitArchive has a
//! single local database and no measured need for concurrent readers, and a pool
//! would reintroduce exactly the shared-state question this design answers. When
//! a later Issue needs concurrent reads, it adds them here, behind this boundary.

use std::fmt;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rusqlite::{Connection, Transaction};

use super::error::DatabaseError;
use super::migration::{
    MigrationRunner, SchemaVersion, applied_schema_version, check_compatibility,
};
use super::migrations::BITARCHIVE_MIGRATIONS;

/// The name of the SQLite database file below the database directory.
///
/// The directory is `AppPaths::database()`; the file name below it is decided
/// here and nowhere else, so the database lives in exactly one place no matter
/// which component opens it.
pub(crate) const DATABASE_FILE_NAME: &str = "bitarchive.sqlite3";

/// How long a statement waits for a lock before it gives up.
///
/// The database is local and BitArchive is its only writer, so a wait means a
/// second process is reading or a checkpoint is running. Five seconds is long
/// enough for both and short enough that a genuinely stuck lock is reported
/// instead of hanging a startup.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// One unit of work, run on the database thread.
type Task = Box<dyn FnOnce(&mut Connection) + Send + 'static>;

/// What the database thread is asked to do next.
enum Request {
    /// Run one unit of work.
    Run(Task),
    /// Close the connection and end the thread.
    Stop,
}

/// The BitArchive database.
///
/// A handle to one SQLite database in the application data directory, owned by
/// one thread. It is cheap to pass around — repositories will hold it — and it is
/// `Send` and `Sync`, because every operation is submitted to the owning thread
/// rather than performed by the caller.
///
/// Opening it applies the migrations this build was compiled with, so a
/// repository can never observe a partially migrated database
/// (DATA_MODEL.md §18.2 rule 4).
///
/// The handle stays usable only while its thread lives. An operation that runs
/// on a stopped thread reports [`DatabaseError::ExecutorStopped`] instead of
/// blocking, which can only happen after an earlier operation panicked.
pub struct Database {
    requests: Sender<Request>,
    worker: Option<JoinHandle<()>>,
}

impl Database {
    /// Opens the database below `directory`, creating it if it does not exist,
    /// and brings it up to the schema version this build expects.
    ///
    /// `directory` is the database directory — `AppPaths::database()` — not the
    /// file: which file inside it holds the database is decided here.
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseError::OpenFailed`] when the directory or the file
    /// cannot be created or configured, and [`DatabaseError::SchemaTooNew`] when
    /// the file holds a schema from a newer BitArchive version. The latter leaves
    /// the file unchanged.
    pub fn open(directory: &Path) -> Result<Self, DatabaseError> {
        Self::open_with(directory, MigrationRunner::new(BITARCHIVE_MIGRATIONS))
    }

    /// Opens the database below `directory` with a specific migration catalogue.
    ///
    /// The shipped catalogue is the one a built application applies; this exists
    /// so a test can drive the runner with its own migrations.
    pub(crate) fn open_with(
        directory: &Path,
        runner: MigrationRunner,
    ) -> Result<Self, DatabaseError> {
        let database = Self::start(directory, runner)?;

        // The migration runs on the database thread, before the handle is handed
        // out, so no repository can observe a partially migrated database
        // (DATA_MODEL.md §18.2 rule 4). A failure drops the handle, which stops the
        // thread and closes the connection.
        runner.apply(&database)?;

        Ok(database)
    }

    /// Starts the database thread on an opened, checked, and configured
    /// connection.
    ///
    /// The runner goes with it because the thread needs the supported schema
    /// version to decide whether the database may be used *before* the connection
    /// changes anything (see [`open_connection`]).
    fn start(directory: &Path, runner: MigrationRunner) -> Result<Self, DatabaseError> {
        let directory = directory.to_path_buf();
        let (ready, readiness) = mpsc::channel();
        let (requests, incoming) = mpsc::channel();

        let worker = thread::Builder::new()
            .name("bitarchive-database".to_owned())
            .spawn(move || serve(&directory, runner, &ready, &incoming))
            .map_err(|cause| DatabaseError::OpenFailed {
                cause: cause.to_string(),
            })?;

        match readiness.recv() {
            Ok(Ok(())) => Ok(Self {
                requests,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();

                Err(error)
            }
            // The thread ended before it could report; `serve` sends on every
            // path that reaches a connection, so this is a panic during startup.
            Err(_) => {
                let _ = worker.join();

                Err(DatabaseError::ExecutorStopped)
            }
        }
    }

    /// Returns the schema version recorded in the database.
    ///
    /// Reading it does not apply anything: this is what the ledger says, which is
    /// [`SchemaVersion::NONE`] for a database that carries only the foundation.
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseError::QueryFailed`] if the ledger cannot be read, and
    /// [`DatabaseError::ExecutorStopped`] if the database is no longer available.
    pub fn schema_version(&self) -> Result<SchemaVersion, DatabaseError> {
        self.read(applied_schema_version)
    }

    /// Runs `work` on the database thread and returns its value.
    ///
    /// Use this for statements that only read. It opens no transaction, so a
    /// single statement is as atomic as SQLite makes it and nothing is held open
    /// afterwards.
    pub(crate) fn read<T, F>(&self, work: F) -> Result<T, DatabaseError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, DatabaseError> + Send + 'static,
    {
        self.submit(move |connection| work(connection))
    }

    /// Runs `work` inside one write transaction on the database thread and
    /// returns its value once that transaction has committed.
    ///
    /// The transaction is opened and committed here, so the work either happened
    /// entirely or not at all. Returning `Ok` from `work` and then failing to
    /// commit is reported as a failure; the value is only returned if the commit
    /// succeeded.
    pub(crate) fn write<T, F>(&self, work: F) -> Result<T, DatabaseError>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> Result<T, DatabaseError> + Send + 'static,
    {
        self.submit(move |connection| {
            let transaction = connection.transaction().map_err(DatabaseError::query)?;

            let value = work(&transaction)?;

            transaction.commit().map_err(DatabaseError::query)?;

            Ok(value)
        })
    }

    /// Hands one unit of work to the database thread and waits for its answer.
    fn submit<T>(
        &self,
        task: impl FnOnce(&mut Connection) -> Result<T, DatabaseError> + Send + 'static,
    ) -> Result<T, DatabaseError>
    where
        T: Send + 'static,
    {
        let (answers, answer) = mpsc::channel();

        let task: Task = Box::new(move |connection| {
            // A send failure means the caller stopped waiting, which happens when
            // it panicked; there is nothing left to report to.
            let _ = answers.send(task(connection));
        });

        self.requests
            .send(Request::Run(task))
            .map_err(|_| DatabaseError::ExecutorStopped)?;

        answer.recv().unwrap_or(Err(DatabaseError::ExecutorStopped))
    }
}

impl fmt::Debug for Database {
    /// Names the handle and nothing about what it holds.
    ///
    /// A `Database` is reached from error paths that want to print the value they
    /// did not get, so it is `Debug`; the connection, the file, and the directory
    /// it was opened with are all deliberately left out, because a printed handle
    /// must not carry a user's path into a log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Drop for Database {
    /// Stops the database thread and waits for it.
    ///
    /// Joining is what makes the shutdown deterministic: the connection is closed
    /// on the thread that opened it, and an operation already running finishes
    /// before it does. The connection is never closed by a caller.
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Stop);

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Opens, checks, and configures the database, then serves requests on it.
///
/// A failure here is reported before anything is served, so a caller either
/// receives a database that is open, configured, and compatible, or an error and
/// no handle at all.
fn serve(
    directory: &Path,
    runner: MigrationRunner,
    ready: &Sender<Result<(), DatabaseError>>,
    incoming: &Receiver<Request>,
) {
    let mut connection = match open_connection(directory, runner) {
        Ok(connection) => connection,
        Err(error) => {
            let _ = ready.send(Err(error));

            return;
        }
    };

    if ready.send(Ok(())).is_err() {
        // The handle that asked for this database is gone, so nothing can be
        // served and no work can arrive.
        return;
    }

    while let Ok(request) = incoming.recv() {
        match request {
            Request::Run(task) => task(&mut connection),
            Request::Stop => break,
        }
    }
}

/// Creates the database directory, opens the file below it, checks that this
/// build may use it, and only then starts changing it.
///
/// # The order is the guarantee
///
/// These three steps are separate functions *because* they have to run in this
/// order, and the order is what Issue #34's refusal requires:
///
/// ```text
/// create the directory, open the file   ← creates an empty file if none exists
///       ↓
/// configure_connection                  ← per connection, nothing persists
///       ↓
/// check the schema version              ← read-only: refuse or continue
///       ↓
/// enable_wal                            ← the first change to the file itself
/// ```
///
/// Opening a database that does not exist creates an empty file, and an empty
/// database cannot be too new. Everything between that and [`enable_wal`] leaves
/// the file alone: `foreign_keys` and the busy timeout are properties of *this
/// connection*, held in memory and gone when it closes, and the compatibility
/// check only reads. The journal mode is the exception — WAL is written into the
/// database file and outlives the process — so it is the first thing that may
/// change it, and it waits until the database is known to be one this build
/// understands.
///
/// Doing this the other way round — configuring first and checking afterwards —
/// would silently rewrite the journal mode of a database from a newer BitArchive
/// version on the way to refusing it. Refusing to open something is not licence to
/// modify it.
///
/// # One connection, not two
///
/// The check and the change run on the *same* connection, so there is no window
/// between them for another writer to swap the database underneath. A separate
/// short-lived preflight connection would open exactly that window for no gain.
fn open_connection(directory: &Path, runner: MigrationRunner) -> Result<Connection, DatabaseError> {
    // The path accessors create nothing (ARCHITECTURE.md §35): the component that
    // is about to write makes the directory, and this is that component.
    std::fs::create_dir_all(directory).map_err(|cause| DatabaseError::OpenFailed {
        cause: cause.to_string(),
    })?;

    let connection = Connection::open(directory.join(DATABASE_FILE_NAME)).map_err(|cause| {
        DatabaseError::OpenFailed {
            cause: cause.to_string(),
        }
    })?;

    configure_connection(&connection)?;

    // Refuse before the first persistent change. A database this build does not
    // understand is left exactly as it was found (DATA_MODEL.md §18.3).
    check_compatibility(&connection, runner.supported_schema_version())?;

    enable_wal(&connection)?;

    Ok(connection)
}

/// Applies the connection settings every connection needs that do not touch the
/// database file.
///
/// There is one place where a BitArchive connection is configured, and this is
/// it: a second connection added later inherits these settings by being opened
/// through `open_connection`, and cannot accidentally run without them
/// (ARCHITECTURE.md §8.1).
///
/// Both settings are per connection. `foreign_keys` is a connection flag that
/// defaults to off and is never stored in the file; the busy timeout is a
/// connection setting too. Neither survives the connection, which is why this may
/// run before the compatibility check — see [`open_connection`].
///
/// Each setting is read back rather than assumed. A setting that silently did not
/// take effect would be invisible until it caused a bug somewhere else — a
/// connection without foreign keys accepts rows that violate the schema — so the
/// open fails here instead.
fn configure_connection(connection: &Connection) -> Result<(), DatabaseError> {
    let open_failure = |cause: rusqlite::Error| DatabaseError::OpenFailed {
        cause: cause.to_string(),
    };

    // Foreign keys are enforced per connection and are off by default; without
    // this, the references the schema declares would not be checked at all.
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(open_failure)?;

    let enforced: bool = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(open_failure)?;

    if !enforced {
        return Err(DatabaseError::ForeignKeysNotEnforced);
    }

    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(open_failure)?;

    Ok(())
}

/// Puts the database file into WAL mode.
///
/// This is the first statement of an open that changes the database itself: WAL
/// is a persistent property of the file, written to it the first time and
/// confirmed on every later open. It is why the database can be read while a
/// write is in progress, which the scan and scrape pipelines rely on
/// (ARCHITECTURE.md §8.1).
///
/// It is only ever called once the database has been accepted as one this build
/// understands, so a database from a newer version keeps the journal mode its own
/// version chose.
fn enable_wal(connection: &Connection) -> Result<(), DatabaseError> {
    let open_failure = |cause: rusqlite::Error| DatabaseError::OpenFailed {
        cause: cause.to_string(),
    };

    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .map_err(open_failure)?;

    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(DatabaseError::JournalModeNotWal { mode: journal_mode });
    }

    Ok(())
}
