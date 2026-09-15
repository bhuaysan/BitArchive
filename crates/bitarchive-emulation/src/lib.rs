//! BitArchive emulation layer.
//!
//! This crate owns the concrete emulation backend. RetroArch is the MVP
//! backend, so RetroArch-specific launch knowledge lives here and not in the
//! application or domain layers (ARCHITECTURE.md §5.4, §19).
//!
//! What exists here is deliberately only the first capability of the backend:
//!
//! - [`RetroArchLaunchInput`] — already fully resolved technical launch inputs
//! - [`RetroArchBackend`] — the concrete preparation implementation
//!
//! ```text
//! concrete RetroArch launch inputs
//!       ↓
//! deterministic argument construction
//!       ↓
//! PreparedLaunch
//! ```
//!
//! Starting the prepared process, observing it, and owning the session are the
//! steps after this one, and none of them is part of this crate yet.
//!
//! # Boundary
//!
//! Dependencies point inwards (ARCHITECTURE.md §2.4). This crate depends on
//! [`bitarchive_application`] and [`std`] only, and must stay free of:
//!
//! - Slint
//! - SQLite / rusqlite
//! - Tokio
//! - HTTP clients
//! - OS APIs
//! - filesystem access
//! - process creation, including [`std::process::Command`]
//! - dynamic loading, including `libloading`
//!
//! Preparation is pure: it resolves nothing, validates nothing, and reads
//! nothing. Discovering or installing runtimes and cores, checking core
//! integrity, and deciding whether firmware is present all belong to component
//! management and readiness, not to launch preparation.
//!
//! # No port yet
//!
//! An `EmulationBackend` port is deliberately not defined. The long-term port
//! also covers readiness checking, process launch, session handles, and an
//! error framework, and none of those types or responsibilities exist yet.
//! Defining the full trait today would force all of them in advance, which the
//! project rule against empty or speculative ports forbids.
//! [`RetroArchBackend`] is therefore a concrete preparation implementation; the
//! port arrives together with the application orchestration that consumes it
//! (ARCHITECTURE.md §19).
//!
//! [`std::process::Command`]: std::process::Command

mod retroarch;

pub use retroarch::{RetroArchBackend, RetroArchLaunchInput};
