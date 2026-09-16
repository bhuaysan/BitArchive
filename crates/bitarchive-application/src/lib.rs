//! BitArchive application layer.
//!
//! This crate owns the contracts that describe a launch and the vocabulary used
//! to report whether a launch may proceed. It sits between the presentation
//! layer and the domain:
//!
//! ```text
//! UI
//!  ↓
//! Application   ← this crate
//!  ↓
//! Domain
//! ```
//!
//! What exists here is deliberately only the beginning of the launch path plus
//! the runtime acquisition contract:
//!
//! - [`LaunchRequest`] — the user's launch wish, expressed as a [`GameId`] and a
//!   [`LaunchAction`]
//! - [`LaunchReadiness`] and [`ReadinessIssue`] — the structured result of a
//!   readiness check
//! - [`PreparedLaunch`] — the backend-neutral description of a fully resolved
//!   process launch
//! - [`resolve_core`] — re-exported domain policy for deterministic core
//!   selection
//! - [`managed_runtime`] — the ports and result types of managed runtime
//!   acquisition: [`ArtifactDownloader`], [`ArtifactExtractor`],
//!   [`DiskImageExtractor`], [`RuntimeInstaller`], and [`InstalledRuntime`]
//!
//! Release resolution, content resolution, firmware readiness, configuration
//! resolution, core options, process spawning, session management, and core
//! management are not implemented yet (ARCHITECTURE.md §20, §21, §22, §23). This
//! crate introduces no ports for them in advance.
//!
//! # Boundary
//!
//! Dependencies point inwards (ARCHITECTURE.md §2.4). This crate depends on
//! [`bitarchive_domain`] and [`std`] only, and must stay free of:
//!
//! - Slint
//! - SQLite / rusqlite
//! - Tokio
//! - HTTP clients such as reqwest
//! - OS APIs
//! - filesystem access
//! - process creation, including [`std::process::Command`]
//! - RetroArch process, executable, argument, or core path details
//!
//! Paths are a deliberate exception, and only at one end of the launch path. A
//! [`LaunchRequest`] carries no release, content, core, runtime, configuration,
//! or path at all: it names the game to launch and the action to perform,
//! nothing that only a later resolution step can know. A [`PreparedLaunch`] is
//! the opposite end of the same path — the later, concretely resolved process
//! contract — so it carries an executable, separate arguments, environment
//! overrides, and an optional working directory. Using [`PathBuf`] and
//! [`OsString`] there is exactly what keeps a launch free of shell quoting, and
//! it does not turn this crate into an adapter: no type here reads, writes, or
//! starts anything.
//!
//! [`managed_runtime`] follows the same rule from the other side. It declares the
//! *shapes* of downloading, unpacking, and installing because it owns the use
//! case, and it carries the resulting paths back to its caller, but every
//! operation is a trait method implemented by an outer layer. Declaring a port
//! is not performing I/O.
//!
//! [`PathBuf`]: std::path::PathBuf
//! [`OsString`]: std::ffi::OsString
//! [`std::process::Command`]: std::process::Command
//!
//! # Readiness
//!
//! [`ReadinessIssue`] carries no user-facing text, no localization, and no
//! recovery actions. A readiness check reports *what* is wrong in structured
//! form; the UI decides how to present it and which recovery action to offer
//! (ARCHITECTURE.md §21).
//!
//! A [`LaunchReadiness`] is either ready or blocked by at least one issue. The
//! blocked state is private and can only be produced from a non-empty issue
//! collection, so a blocked result always names a reason:
//!
//! ```text
//! Ready
//! Blocked [CoreMissing, SourceOffline]
//! ```
//!
//! [`GameId`]: bitarchive_domain::GameId

mod launch;
mod launch_readiness;
pub mod managed_runtime;
mod prepared_launch;

pub use launch::{LaunchAction, LaunchRequest};
pub use launch_readiness::{LaunchReadiness, ReadinessIssue};
pub use managed_runtime::{
    Artifact, ArtifactDownloader, ArtifactExtractor, ArtifactSourceKind, DiskImageExtractor,
    DownloadTimeout, InstalledRuntime, RuntimeInstaller, RuntimeStoreError,
};
pub use prepared_launch::PreparedLaunch;

pub use bitarchive_domain::core::{CoreSelectionSource, ResolvedCore, resolve_core};
