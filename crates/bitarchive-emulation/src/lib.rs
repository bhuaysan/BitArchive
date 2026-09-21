//! BitArchive emulation layer.
//!
//! This crate owns the concrete emulation backend. RetroArch is the MVP
//! backend, so RetroArch-specific launch knowledge lives here and not in the
//! application or domain layers (ARCHITECTURE.md §5.4, §19).
//!
//! What exists here is deliberately only the beginning of the backend:
//!
//! - [`RetroArchLaunchInput`] — already fully resolved technical launch inputs
//! - [`RetroArchBackend`] — the concrete preparation implementation
//! - [`pinned_retroarch_runtime`] — the one RetroArch version, artifact, and
//!   digest BitArchive pins
//! - [`ManagedRuntime`] and [`RetroArchRuntimeError`] — resolving the executable
//!   of the installed, active runtime
//!
//! ```text
//! pinned_retroarch_runtime()      ← what BitArchive manages (definition only)
//!       ↓
//! RetroArchRuntime::resolve(...)  ← reads the component store through a port
//!       ↓
//! ManagedRuntime::executable_path()
//!       ↓
//! RetroArchLaunchInput::new(...)
//!       ↓
//! deterministic argument construction
//!       ↓
//! PreparedLaunch
//! ```
//!
//! Acquiring, staging, installing, and activating the runtime are *not* done
//! here: they are infrastructure work driven by the definition this crate pins.
//! Starting the prepared process, observing it, and owning the session are the
//! steps after this one, and none of them is part of this crate yet.
//!
//! # Boundary
//!
//! Dependencies point inwards (ARCHITECTURE.md §2.4). This crate depends on
//! [`bitarchive_application`], [`bitarchive_domain`], and [`std`] only, and must
//! stay free of:
//!
//! - Slint
//! - SQLite / rusqlite
//! - Tokio
//! - HTTP clients
//! - OS APIs
//! - filesystem *writes*
//! - process creation, including [`std::process::Command`]
//! - dynamic loading, including `libloading`
//!
//! Preparation is pure: it resolves nothing, validates nothing, and reads
//! nothing. Resolving the managed runtime's executable is the one place this crate
//! looks at the filesystem, and it only *checks* that the activated installation
//! still contains its executable. Discovering, downloading, installing, and
//! activating runtimes and cores, checking core integrity, and deciding whether
//! firmware is present all belong to component management and readiness, not to
//! launch preparation.
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

mod launch_state;
mod managed_runtime;
mod retroarch;
mod runtime;

pub use launch_state::ManagedEmulation;
pub use managed_runtime::{ManagedRuntime, RetroArchRuntimeError};
pub use retroarch::{RetroArchBackend, RetroArchLaunchInput};
pub use runtime::{
    PINNED_RETROARCH_ARTIFACT_FILE, PINNED_RETROARCH_ARTIFACT_SHA256,
    PINNED_RETROARCH_ARTIFACT_URL, PINNED_RETROARCH_BUNDLE, PINNED_RETROARCH_EXECUTABLE,
    PINNED_RETROARCH_LICENSE, PINNED_RETROARCH_UPSTREAM_PROJECT, PINNED_RETROARCH_UPSTREAM_URL,
    PINNED_RETROARCH_VERSION, pinned_retroarch_runtime,
};

/// Resolving the executable of the managed RetroArch runtime.
///
/// A small facade so that the two answers a launch path needs — "which runtime
/// should be started?" and "why can none be started?" — are reachable without
/// naming the module path.
pub mod retroarch_runtime {
    pub use crate::managed_runtime::{ManagedRuntime, RetroArchRuntimeError, resolve};
}
