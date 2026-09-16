//! BitArchive infrastructure layer.
//!
//! Infrastructure holds the concrete adapters for the network and the file
//! system. It sits outside the inner layers and implements the ports the
//! application layer declares (ARCHITECTURE.md §2.4, §5.3):
//!
//! ```text
//! UI
//!  ↓
//! Application   ← declares ArtifactDownloader, ArtifactExtractor, RuntimeInstaller
//!  ↓
//! Domain        ← owns the pinned RuntimeDefinition
//!
//! Infrastructure ← this crate: HTTP, download, staging, installation
//! Platform       ← the OS-specific bridges those adapters need
//! ```
//!
//! What exists here is deliberately only the first infrastructure capability, the
//! one managed runtime acquisition needs (Issue #19):
//!
//! - [`HttpArtifactDownloader`] — fetches the pinned artifact over HTTPS and
//!   verifies its SHA-256 while it arrives
//! - [`AppleDiskImageExtractor`] — dispatches unpacking by artifact kind and hands
//!   a disk image to the platform layer
//! - [`ComponentStore`] — stages, installs into the versioned component store, and
//!   activates a runtime version
//!
//! ```text
//! RuntimeDefinition
//!       ↓
//! ComponentStore::install
//!       ├── HttpArtifactDownloader      download + verify
//!       ├── AppleDiskImageExtractor     unpack (delegating to the platform layer)
//!       ├── validate                    the pinned executable exists
//!       ├── move into place             one rename
//!       └── activate                    one atomic record write
//!       ↓
//! InstalledRuntime   →  executable_path()
//! ```
//!
//! # Boundary
//!
//! Dependencies point inwards (ARCHITECTURE.md §2.4). This crate depends on
//! [`bitarchive_application`], [`bitarchive_domain`], and the two crates its
//! transport and hashing need — `ureq` and `sha2`. It stays free of:
//!
//! - Slint
//! - SQLite / rusqlite
//! - Tokio and async runtimes
//! - [`bitarchive_emulation`], and therefore of `RetroArchBackend`,
//!   `RetroArchLaunchInput`, cores, and content
//! - process creation, and therefore of any shell
//!
//! The last two matter: installing a runtime must not require knowing what
//! RetroArch is beyond the [`RuntimeDefinition`](bitarchive_domain::runtime::RuntimeDefinition)
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
//! - no core management of any kind. Cores are a separate component class with
//!   their own identity and licensing (ARCHITECTURE.md §23.4, AGENTS.md §11), and
//!   nothing here downloads, bundles, or installs one
//!
//! [`bitarchive_emulation`]: https://docs.rs/bitarchive-emulation

// The crate uses no unsafe code. A disk image is mounted through a platform
// bridge that starts a process, rather than through an OS API called from here.
#![forbid(unsafe_code)]

mod artifact_extract;
mod component_store;
mod http_download;

pub use artifact_extract::AppleDiskImageExtractor;
pub use component_store::ComponentStore;
pub use http_download::{HttpArtifactDownloader, user_agent};

#[cfg(test)]
mod tests;
