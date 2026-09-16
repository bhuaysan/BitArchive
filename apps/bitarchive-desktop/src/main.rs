//! BitArchive desktop entry point.
//!
//! This crate is the desktop composition root (ARCHITECTURE.md §5.7). It owns
//! process-level concerns: decide what to run, wire the concrete implementations
//! together, and translate the result into a process exit code.
//!
//! ```text
//! argv
//!  ├── (nothing)                        → the Slint desktop UI
//!  └── acquire-retroarch-runtime [opts] → the opt-in developer acquisition
//! ```
//!
//! # LIVE RUNTIME ACQUISITION
//!
//! `acquire-retroarch-runtime` is the one command that performs network I/O, and
//! it exists for developer verification of the managed runtime path (Issue #19
//! §17). It is deliberately **opt-in**: nothing in the product flow calls it, no
//! test calls it, and CI never runs it. The automated test suite of the workspace
//! uses fake downloaders and loopback servers instead, so a normal `cargo test`
//! never downloads a RetroArch artifact.
//!
//! What it does:
//!
//! ```text
//! pinned definition (bitarchive-emulation)
//!       ↓
//! HttpArtifactDownloader        official https URL, verify SHA-256 while writing
//!       ↓
//! AppleDiskImageExtractor       /usr/bin/hdiutil, no shell
//!       ↓
//! ComponentStore::install       stage → validate → install → activate
//!       ↓
//! ManagedRuntime::executable_path()
//! ```
//!
//! It installs into the real application component store, so it must be invoked
//! deliberately. `--root <dir>` points the whole run at a throwaway directory
//! instead, which is how the command is exercised without touching the user's
//! application data.

mod acquire_runtime;

use std::process::ExitCode;

use acquire_runtime::AcquireRequest;

/// The command that acquires the pinned RetroArch runtime.
const ACQUIRE_COMMAND: &str = "acquire-retroarch-runtime";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    match arguments.split_first() {
        Some((command, rest)) if command == ACQUIRE_COMMAND => acquire_runtime::run(rest),
        Some((command, _)) if command == "--help" || command == "-h" => {
            print_usage();

            ExitCode::SUCCESS
        }
        None => match bitarchive_ui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("BitArchive could not start the desktop UI: {error}");
                ExitCode::FAILURE
            }
        },
        Some((unknown, _)) => {
            eprintln!("unknown command: {unknown}");
            print_usage();

            ExitCode::FAILURE
        }
    }
}

/// Prints what the binary accepts.
fn print_usage() {
    println!(
        "BitArchive\n\
         \n\
         Usage:\n\
         \x20 bitarchive-desktop                          start the desktop UI\n\
         \x20 bitarchive-desktop {ACQUIRE_COMMAND} [--root <dir>] [--dry-run]\n\
         \x20                                              acquire the pinned RetroArch runtime\n\
         \x20                                              (LIVE RUNTIME ACQUISITION, developer opt-in)\n\
         \x20 bitarchive-desktop --help\n\
         \n\
         Options for {ACQUIRE_COMMAND}:"
    );

    let _ = AcquireRequest::usage();
}
