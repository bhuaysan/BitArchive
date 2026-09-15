//! BitArchive desktop entry point.
//!
//! This crate is the desktop composition root (ARCHITECTURE.md §5.7). It owns
//! process-level concerns only: start the presentation layer and translate the
//! result into a process exit code.
//!
//! Domain, application, infrastructure, emulation, and platform wiring is
//! added here once those layers exist. Nothing is anticipated in advance.

use std::process::ExitCode;

fn main() -> ExitCode {
    match bitarchive_ui::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("BitArchive could not start the desktop UI: {error}");
            ExitCode::FAILURE
        }
    }
}
