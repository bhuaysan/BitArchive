//! BitArchive infrastructure layer.
//!
//! Infrastructure holds the concrete adapters for the network, the file system,
//! and the database. It sits outside the inner layers and implements the ports the
//! application layer declares (ARCHITECTURE.md §2.4, §5.3):
//!
//! ```text
//! UI
//!  ↓
//! Application   ← declares ArtifactDownloader, ArtifactExtractor,
//!  ↓               CoreArchiveExtractor, RuntimeInstaller, CoreInstaller,
//! Domain          and the repository ports
//!
//! Infrastructure ← this crate: SQLite, HTTP, download, staging, installation
//! Platform       ← the OS-specific bridges those adapters need
//! ```
//!
//! # The database
//!
//! [`Database`] is the SQLite foundation (Issue #34): it opens the one local
//! database below `AppPaths::database()`, configures it — WAL, foreign keys, a
//! busy timeout — and applies the forward-only migrations this binary was built
//! with. It is an opaque handle over a connection that one owned thread uses, so
//! no caller performs SQLite work on its own thread, and no rusqlite type appears
//! outside this crate (ARCHITECTURE.md §8).
//!
//! It carries no BitArchive table yet: schema v1 is Issue #109 and arrives as the
//! first migration. See [`Database`] and the module documentation for the
//! boundary and the versioning rules.
//!
//! # Managed component acquisition
//!
//! What exists beyond the database is the infrastructure of managed component
//! acquisition, for both component classes:
//!
//! - [`HttpArtifactDownloader`] — fetches a pinned artifact over HTTPS and verifies
//!   its SHA-256 while it arrives. One implementation serves a runtime image and a
//!   core archive alike, because the contract it implements names no component
//!   class and carries no version, platform, or install layout
//! - [`AppleDiskImageExtractor`] — unpacks a runtime disk image by handing it to
//!   the platform layer
//! - [`ZipCoreArchiveExtractor`] — writes the one pinned library out of a verified
//!   core archive, and never unpacks an archive wholesale
//! - [`ComponentStore`] — stages, installs, and activates a runtime version
//! - [`CoreStore`] — stages, installs, and resolves a core build, and activates
//!   nothing, because there is no active core
//!
//! ```text
//! RuntimeDefinition → ComponentStore::install → InstalledRuntime → executable_path()
//! CoreDefinition    → CoreStore::install      → ManagedCore      → library_path()
//! ```
//!
//! The two flows share the download, the staging area, and the artifact cache, and
//! nothing else. A runtime has an activation record; a core must never have one
//! (ARCHITECTURE.md §23.1, invariant 21). A core is installed as an opaque file:
//! nothing here opens, loads, or interprets a `.dylib`.
//!
//! # Boundary
//!
//! Dependencies point inwards (ARCHITECTURE.md §2.4). This crate depends on
//! [`bitarchive_application`], [`bitarchive_domain`], and the crates its transport,
//! hashing, archive reading, and database access need — `ureq`, `sha2`, `zip`,
//! and `rusqlite`. SQLite belongs *here* and is used nowhere else: `rusqlite` is
//! not a dependency of any other crate, and no rusqlite type appears in this
//! crate's public API. The crate stays free of:
//!
//! - Slint
//! - Tokio and async runtimes
//! - [`bitarchive_emulation`], and therefore of `RetroArchBackend`,
//!   `RetroArchLaunchInput`, core semantics, and content
//! - process creation, and therefore of any shell
//!
//! The last two matter: installing a runtime must not require knowing what
//! RetroArch is beyond the [`RuntimeDefinition`](bitarchive_domain::runtime::RuntimeDefinition)
//! it is handed, installing a core must not require knowing what libretro is
//! beyond the [`CoreDefinition`](bitarchive_domain::managed_core::CoreDefinition)
//! it is handed, and a download must never be a shell command.
//!
//! # What is not implemented yet
//!
//! - no retries, resume, progress reporting, or cancellation; those belong to the
//!   job layer and the shared network layer ARCHITECTURE.md §31 and §30 describe
//! - no signature verification. ARCHITECTURE.md §24 describes signed distribution
//!   manifests and that step is not implemented yet: the pinned SHA-256 is the
//!   trust anchor until it is, and nothing is weakened in the meantime
//! - no rollback. The store keeps the layout that makes a one-step rollback
//!   possible (ARCHITECTURE.md §23.3) but does not implement the operation
//! - no version pruning. Which older versions are kept is an update-policy
//!   decision for the Issue that implements updates
//! - no core catalog. Exactly one core is curated, and this crate can install
//!   nothing that has no reviewed definition (Issue #21)
//!
//! [`bitarchive_emulation`]: https://docs.rs/bitarchive-emulation

// The crate uses no unsafe code. A disk image is mounted through a platform
// bridge that starts a process, rather than through an OS API called from here.
#![forbid(unsafe_code)]

mod artifact_extract;
mod component_store;
mod core_archive;
mod core_store;
mod database;
mod firmware_inventory;
mod http_download;
mod store_layout;

pub use artifact_extract::AppleDiskImageExtractor;
pub use component_store::ComponentStore;
pub use core_archive::ZipCoreArchiveExtractor;
pub use core_store::CoreStore;
pub use database::{Database, DatabaseError, SchemaVersion};
pub use firmware_inventory::FilesystemFirmwareChecker;
pub use http_download::{HttpArtifactDownloader, user_agent};

#[cfg(test)]
mod tests;
