//! BitArchive platform layer.
//!
//! This crate owns the operating-system integration that the inner layers must
//! not perform themselves (ARCHITECTURE.md §2.5, §5.5). It is the outermost
//! layer on the launch path:
//!
//! ```text
//! UI
//!  ↓
//! Application   ← PreparedLaunch, the contract this crate consumes
//!  ↓
//! Domain
//!
//! Emulation     ← RetroArchBackend produces PreparedLaunch
//!  ↓
//! Platform      ← this crate starts it
//! ```
//!
//! What exists here is deliberately only the platform capabilities whose
//! consumers exist:
//!
//! - [`ProcessController`] — the concrete service that starts a prepared launch
//! - [`SpawnedProcess`] — the handle that keeps the started process observable
//! - [`ProcessExitStatus`] — the platform-neutral result of a finished process
//! - [`AppPaths`] — the one authority on where BitArchive keeps its files
//! - [`DmgBundleExtractor`] — reads an application bundle out of an Apple disk
//!   image, the packaging of the official macOS runtime artifact
//!
//! ```text
//! PreparedLaunch
//!       ↓
//! ProcessController::spawn
//!       ↓
//! SpawnedProcess   →  id() / try_wait() / wait()
//!
//! AppPaths::default()
//!       ↓
//! components/ · downloads/ · staging/ · …
//!
//! verified .dmg  →  DmgBundleExtractor::extract_bundle  →  RetroArch.app
//! ```
//!
//! # Boundary
//!
//! Dependencies point inwards (ARCHITECTURE.md §2.4). This crate depends on
//! [`bitarchive_application`], [`bitarchive_domain`], and [`std`] only, and must
//! stay free of:
//!
//! - Slint
//! - SQLite / rusqlite
//! - Tokio and asynchronous process handling
//! - HTTP clients
//! - third-party platform or process crates (`nix`, `libc`, `async-trait`, …)
//! - `bitarchive-emulation`, and therefore of `RetroArchBackend`,
//!   `RetroArchLaunchInput`, cores, content, and every other RetroArch-specific
//!   type
//!
//! The launch input is
//! [`PreparedLaunch`](bitarchive_application::PreparedLaunch) and nothing else, so
//! the launch path keeps the direction
//! `Emulation → PreparedLaunch → Platform` and never acquires
//! `Platform → RetroArch` knowledge.
//!
//! [`DmgBundleExtractor`] implements
//! [`DiskImageExtractor`](bitarchive_application::DiskImageExtractor), a port the
//! application layer declares. It therefore reads the artifact kind out of a
//! [`bitarchive_domain::runtime::RuntimeDefinition`] without knowing which runtime
//! it describes, and it is the *installer* in `bitarchive-infrastructure` that
//! decides what an installation is.
//!
//! # Starting a tool rather than calling an OS API
//!
//! Reading a disk image means starting the platform's own `/usr/bin/hdiutil`,
//! always through [`std::process::Command`] with an absolute program path and one
//! argument per value. No shell runs in between and no command line is ever
//! joined, which is why a path containing spaces needs no quoting
//! (ARCHITECTURE.md §20.3). The alternative — calling a disk-image API through a
//! third-party crate — would add an `unsafe` boundary for a job the operating
//! system already exposes as a tool.
//!
//! # What is not implemented yet
//!
//! This crate starts and observes a process and resolves paths. It does not own
//! one:
//!
//! - no session management, session exclusivity, playtime, or recovery
//!   (ARCHITECTURE.md §22)
//! - no process identity recovery from start time or executable identity
//!   (ARCHITECTURE.md §22.1)
//! - no ordered shutdown, no termination, and no forced termination
//!   (ARCHITECTURE.md §22.3)
//! - no stdout/stderr capture and no session log artifacts
//!   (ARCHITECTURE.md §28, §33.2)
//! - no readiness, core, firmware, or configuration work
//!   (ARCHITECTURE.md §21, §23, §25, §26)
//! - no startup cleanup. [`AppPaths`] only says where things live; removing
//!   abandoned state belongs to the recovery step of bootstrap
//!   (ARCHITECTURE.md §6.1, §35.1)
//! - no `ProcessController` port in the application layer, because nothing there
//!   consumes one yet
//!
//! The controller service, the secret store, notifications, window activation,
//! volume monitoring, and app installation are listed in ARCHITECTURE.md §5.5 as
//! further platform responsibilities, and none of them is added before its
//! consumer exists.
//!
//! [`std::process::Command`]: std::process::Command

// The crate uses no unsafe code and needs none: [`std::process`] already
// exposes the process API this layer is built on, so there is no reason to
// reach below it.
#![forbid(unsafe_code)]

mod app_paths;
mod dmg;
mod process;
mod process_controller;
mod process_exit_status;

pub use app_paths::AppPaths;
pub use dmg::DmgBundleExtractor;
pub use process::SpawnedProcess;
pub use process_controller::ProcessController;
pub use process_exit_status::ProcessExitStatus;

#[cfg(test)]
mod tests {
    /// The crate's public surface is exactly the capabilities its consumers need:
    /// start a process, observe it, resolve application paths, and read a bundle
    /// out of a disk image. Everything else — including the command builder, the
    /// exit-status conversion, and the directory walk — stays internal, so the
    /// mapping cannot be bypassed from outside.
    #[test]
    fn the_public_surface_is_the_process_adapter_and_platform_paths_only() {
        use crate::{
            AppPaths, DmgBundleExtractor, ProcessController, ProcessExitStatus, SpawnedProcess,
        };

        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ProcessController>();
        assert_send_sync::<SpawnedProcess>();
        assert_send_sync::<ProcessExitStatus>();
        assert_send_sync::<AppPaths>();
        assert_send_sync::<DmgBundleExtractor>();
    }
}
