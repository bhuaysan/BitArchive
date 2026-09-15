//! BitArchive presentation layer.
//!
//! This crate owns the Slint UI. Slint stays presentation-only: it never
//! performs database access, filesystem scanning, HTTP requests, RetroArch
//! process management, or platform integration (ARCHITECTURE.md §37).
//!
//! The desktop composition root reaches this crate through [`run`] only, so
//! Slint component details stay behind this boundary.

/// Generated bindings for the Slint sources under `ui/`.
///
/// Kept private on purpose: the crate exposes [`run`] instead of raw Slint
/// component types.
mod app {
    slint::include_modules!();
}

use slint::ComponentHandle as _;

/// Starts the Slint presentation layer and runs its event loop until the root
/// window is closed.
///
/// # Errors
///
/// Returns a [`slint::PlatformError`] if the platform backend or the root
/// window cannot be created.
pub fn run() -> Result<(), slint::PlatformError> {
    let window = app::AppWindow::new()?;
    window.run()
}
