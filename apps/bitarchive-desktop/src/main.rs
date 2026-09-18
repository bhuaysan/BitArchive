//! BitArchive desktop entry point.
//!
//! This crate is the desktop composition root (ARCHITECTURE.md §5.7). It owns
//! process-level concerns: decide what to run, wire the concrete implementations
//! together, and translate the result into a process exit code.
//!
//! ```text
//! argv
//!  ├── (nothing)                        → the Slint desktop UI
//!  ├── acquire-retroarch-runtime [opts] → the opt-in developer runtime acquisition
//!  └── acquire-core [opts]              → the opt-in developer core acquisition
//! ```
//!
//! # LIVE COMPONENT ACQUISITION
//!
//! `acquire-retroarch-runtime` and `acquire-core` are the two commands that
//! perform network I/O, and they exist for developer verification of the managed
//! component paths (Issue #19 §17, Issue #21 §17). Both are deliberately
//! **opt-in**: nothing in the product flow calls them, no test calls them, and CI
//! never runs them. The automated test suite of the workspace uses fake downloaders
//! and loopback servers instead, so a normal `cargo test` never downloads a
//! RetroArch artifact or a libretro core.
//!
//! What the runtime command does:
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
//! and what the core command does:
//!
//! ```text
//! curated definition for this architecture (bitarchive-domain)
//!       ↓
//! HttpArtifactDownloader        the same downloader, the same verification
//!       ↓
//! ZipCoreArchiveExtractor       exactly one pinned member, no shell
//!       ↓
//! CoreStore::install            stage → validate → install (and nothing activated)
//!       ↓
//! ManagedCore::library_path()
//! ```
//!
//! They install into the real application component store, so they must be invoked
//! deliberately. `--root <dir>` points a run at a throwaway directory instead,
//! which is how a command is exercised without touching the user's application
//! data.

mod acquire_core;
mod acquire_runtime;

use std::process::ExitCode;

use acquire_core::AcquireCoreRequest;
use acquire_runtime::AcquireRequest;

/// The command that acquires the pinned RetroArch runtime.
const ACQUIRE_COMMAND: &str = "acquire-retroarch-runtime";

/// The command that acquires the curated libretro core.
const ACQUIRE_CORE_COMMAND: &str = "acquire-core";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    match arguments.split_first() {
        Some((command, rest)) if command == ACQUIRE_COMMAND => acquire_runtime::run(rest),
        Some((command, rest)) if command == ACQUIRE_CORE_COMMAND => acquire_core::run(rest),
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
         \x20 bitarchive-desktop {ACQUIRE_CORE_COMMAND} [--root <dir>] [--dry-run]\n\
         \x20                                              acquire the curated mGBA core\n\
         \x20                                              (LIVE CORE ACQUISITION, developer opt-in)\n\
         \x20 bitarchive-desktop --help\n\
         \n\
         Options for {ACQUIRE_COMMAND}:"
    );

    let _ = AcquireRequest::usage();

    println!("\nOptions for {ACQUIRE_CORE_COMMAND}:");

    let _ = AcquireCoreRequest::usage();
}
