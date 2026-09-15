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
//! What exists here is deliberately only the beginning of the launch path:
//!
//! - [`LaunchRequest`] — the user's launch wish, expressed as a [`GameId`] and a
//!   [`LaunchAction`]
//! - [`LaunchReadiness`] and [`ReadinessIssue`] — the structured result of a
//!   readiness check
//! - [`resolve_core`] — re-exported domain policy for deterministic core
//!   selection
//!
//! Release resolution, content resolution, firmware readiness, configuration
//! resolution, core options, prepared launches, process spawning, and session
//! management are not implemented yet (ARCHITECTURE.md §20, §21, §22). This
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
//! - filesystem APIs
//! - RetroArch process, executable, argument, or core path details
//!
//! There are consequently no ROM paths, core library paths, RetroArch
//! executable paths, launch arguments, or `PathBuf` fields in any type here.
//! A launch request names the game it wants to launch and the action to
//! perform — nothing that only a later resolution step can know.
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

pub use launch::{LaunchAction, LaunchRequest};
pub use launch_readiness::{LaunchReadiness, ReadinessIssue};

pub use bitarchive_domain::core::{CoreSelectionSource, ResolvedCore, resolve_core};
