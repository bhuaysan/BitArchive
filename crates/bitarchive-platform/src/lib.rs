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
//! What exists here is deliberately only the first platform capability:
//!
//! - [`ProcessController`] — the concrete service that starts a prepared launch
//! - [`SpawnedProcess`] — the handle that keeps the started process observable
//! - [`ProcessExitStatus`] — the platform-neutral result of a finished process
//!
//! ```text
//! PreparedLaunch
//!       ↓
//! ProcessController::spawn
//!       ↓
//! SpawnedProcess   →  id() / try_wait() / wait()
//! ```
//!
//! # Boundary
//!
//! Dependencies point inwards (ARCHITECTURE.md §2.4). This crate depends on
//! [`bitarchive_application`] and [`std`] only, and must stay free of:
//!
//! - Slint
//! - SQLite / rusqlite
//! - Tokio and asynchronous process handling
//! - HTTP clients
//! - third-party platform or process crates (`nix`, `libc`, `async-trait`, …)
//! - [`bitarchive_emulation`], and therefore of `RetroArchBackend`,
//!   `RetroArchLaunchInput`, cores, content, and every other RetroArch-specific
//!   type
//!
//! The input is [`PreparedLaunch`](bitarchive_application::PreparedLaunch) and
//! nothing else, so the launch path keeps
//! the direction `Emulation → PreparedLaunch → Platform` and never acquires
//! `Platform → RetroArch` knowledge.
//!
//! # What is not implemented yet
//!
//! This crate starts and observes a process. It does not own one:
//!
//! - no session management, session exclusivity, playtime, or recovery
//!   (ARCHITECTURE.md §22)
//! - no process identity recovery from start time or executable identity
//!   (ARCHITECTURE.md §22.1)
//! - no ordered shutdown, no termination, and no forced termination
//!   (ARCHITECTURE.md §22.3)
//! - no stdout/stderr capture and no session log artifacts
//!   (ARCHITECTURE.md §28, §33.2)
//! - no readiness, runtime, core, firmware, or configuration work
//!   (ARCHITECTURE.md §21, §23, §25, §26)
//! - no `ProcessController` port in the application layer, because nothing
//!   there consumes one yet
//!
//! File system access, the secret store, app paths, notifications, controller
//! input, and window activation are listed in ARCHITECTURE.md §5.5 as further
//! platform responsibilities, and none of them is added before its consumer
//! exists.
//!
//! [`bitarchive_emulation`]: https://docs.rs/bitarchive-emulation
//! [`std`]: std

// The crate uses no unsafe code and needs none: [`std::process`] already
// exposes the process API this layer is built on, so there is no reason to
// reach below it.
#![forbid(unsafe_code)]

mod process;
mod process_controller;
mod process_exit_status;

pub use process::SpawnedProcess;
pub use process_controller::ProcessController;
pub use process_exit_status::ProcessExitStatus;

#[cfg(test)]
mod tests {
    /// The crate's public surface is exactly the three types a caller needs to
    /// start a process and observe it. Anything else — including the command
    /// builder and the exit-status conversion — stays internal, so the mapping
    /// cannot be bypassed from outside.
    #[test]
    fn the_public_surface_is_the_process_adapter_only() {
        use crate::{ProcessController, ProcessExitStatus, SpawnedProcess};

        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ProcessController>();
        assert_send_sync::<SpawnedProcess>();
        assert_send_sync::<ProcessExitStatus>();
    }
}
