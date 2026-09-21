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
//! What exists here is the launch path up to the point where a launch is decided and
//! prepared, plus the runtime acquisition contract:
//!
//! - [`LaunchRequest`] — the user's launch wish, expressed as a [`GameId`] and a
//!   [`LaunchAction`]
//! - [`LaunchReadiness`] and [`ReadinessIssue`] — the structured vocabulary of a
//!   readiness result
//! - [`PreparedLaunch`] — the backend-neutral description of a fully resolved
//!   process launch
//! - [`resolve_core`] — re-exported domain policy for deterministic core
//!   selection
//! - the launch-readiness and launch-preparation use case in `game_launch`:
//!   [`GameLaunchRequest`], [`prepare_game_launch`], [`LaunchBlocker`],
//!   [`LaunchPreparation`], and [`PreparedGameLaunch`]
//! - [`launch_state`] — the state that use case reads, through the ports
//!   [`ManagedEmulationState`] and [`FirmwareChecker`], and the neutral values they
//!   answer with
//! - [`managed_runtime`] — the ports and result types of managed runtime
//!   acquisition: [`ArtifactDownloader`], [`ArtifactExtractor`],
//!   [`DiskImageExtractor`], [`RuntimeInstaller`], and [`InstalledRuntime`]
//! - [`managed_core`] — the ports and result types of curated core acquisition:
//!   [`CoreArchiveExtractor`], [`CoreInstaller`], and [`ManagedCore`]
//!
//! The launch preparation ends at *ready and prepared*: it starts no process. Release
//! resolution, content resolution, core options, generated configuration files,
//! process spawning, and session management are not implemented yet
//! (ARCHITECTURE.md §20, §21, §22, §23). This crate introduces no ports for them in
//! advance, and a readiness result never claims to have checked what those steps
//! would check.
//!
//! # One download contract, two component classes
//!
//! Runtime and core artifacts are fetched the same way, so they share one contract
//! instead of having one implementation each:
//!
//! ```text
//! ArtifactRequest              ← component id + pinned source + pinned digest
//!       ↓
//! ArtifactDownloader           ← exactly one implementation, in infrastructure
//!       ↑                    ↑
//! RuntimeInstaller      CoreInstaller
//! ```
//!
//! What the two classes do *not* share is anything that would erase what makes them
//! different. A runtime has an activation record and a core must never have one, so
//! the install contracts stay separate (Issue #21 §5, §12, §16).
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
//! [`managed_runtime`] and [`managed_core`] follow the same rule from the other
//! side. They declare the *shapes* of downloading, unpacking, and installing
//! because the application layer owns the use cases, and they carry the resulting
//! paths back to their caller, but every operation is a trait method implemented by
//! an outer layer. Declaring a port is not performing I/O.

//! # No "active core"
//!
//! [`managed_core`] has no activation concept at all. `ARCHITECTURE.md` §23.1
//! records an active runtime; core selection is the policy in
//! [`resolve_core`], which follows `Release > Game > System` with no global
//! default (invariant 21). A stored "currently active core" would contradict that
//! policy, so no type here can express one.
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

mod game_launch;
mod launch;
mod launch_readiness;
pub mod launch_state;
pub mod managed_core;
pub mod managed_runtime;
mod prepared_launch;

pub use game_launch::{
    GameLaunchContext, GameLaunchPlan, GameLaunchRequest, LaunchBlocker, LaunchPreparation,
    PreparedGameLaunch, prepare_game_launch,
};
pub use launch::{LaunchAction, LaunchRequest};
pub use launch_readiness::{LaunchReadiness, ReadinessIssue};
pub use launch_state::{
    CoreAvailability, CoreUnusableReason, FirmwareChecker, FirmwareOutcome, InstalledCore,
    LaunchRuntime, LaunchRuntimeResolution, ManagedEmulationState, SystemCoreState,
};
pub use managed_core::{
    CoreArchiveExtractor, CoreInstaller, CoreStoreError, ManagedCore, artifact_request,
};
pub use managed_runtime::{
    Artifact, ArtifactDownloader, ArtifactExtractor, ArtifactRequest, ArtifactSourceKind,
    DiskImageExtractor, DownloadTimeout, InstalledRuntime, RuntimeInstaller, RuntimeStoreError,
};
pub use prepared_launch::PreparedLaunch;

pub use bitarchive_domain::core::{CoreSelectionSource, ResolvedCore, resolve_core};
