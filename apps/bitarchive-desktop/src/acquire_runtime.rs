//! LIVE RUNTIME ACQUISITION — the opt-in developer command.
//!
//! This module downloads the pinned RetroArch runtime from the official build
//! host, verifies it against the pinned SHA-256, installs it into a component
//! store, activates it, and resolves the executable (Issue #19 §17).
//!
//! It is a *developer verification* command, not a product flow:
//!
//! - it is never called by the UI, by a library API, or by a test;
//! - CI never runs it, so a normal build and a normal test run never touch the
//!   network (ARCHITECTURE.md §45.4);
//! - it says what it is about to do before it does it, because it is the only
//!   place in BitArchive that performs a large download.
//!
//! ```text
//! pinned definition
//!       ↓
//! download from the official https URL
//!       ↓
//! SHA-256 verified while the bytes are written
//!       ↓
//! mounted read-only, bundle copied out
//!       ↓
//! staged, validated, installed as components/runtime/retroarch/macos-universal/1.22.2
//!       ↓
//! activated, executable resolved
//! ```
//!
//! # What it does not do
//!
//! No game is launched, no core is downloaded, and no session is created. The
//! command ends by printing the resolved executable path, which a later step (B7)
//! is the first to use.

use std::path::PathBuf;
use std::process::ExitCode;

use bitarchive_application::managed_runtime::{InstalledRuntime, RuntimeInstaller};
use bitarchive_emulation::retroarch_runtime;
use bitarchive_emulation::{
    PINNED_RETROARCH_ARTIFACT_FILE, PINNED_RETROARCH_ARTIFACT_SHA256, PINNED_RETROARCH_LICENSE,
    PINNED_RETROARCH_UPSTREAM_URL, PINNED_RETROARCH_VERSION, pinned_retroarch_runtime,
};
use bitarchive_infrastructure::{AppleDiskImageExtractor, ComponentStore, HttpArtifactDownloader};
use bitarchive_platform::{AppPaths, DmgBundleExtractor};

/// What the command was asked to do.
///
/// The default is the plain invocation: install into the application data
/// directory and actually acquire the runtime.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AcquireRequest {
    root: Option<PathBuf>,
    dry_run: bool,
}

impl AcquireRequest {
    /// Describes the accepted arguments, so a caller does not have to guess.
    ///
    /// # Errors
    ///
    /// Never. The signature exists so that the usage text has one source.
    pub fn usage() -> Result<(), std::convert::Infallible> {
        println!(
            "  --root <dir>   install into <dir> instead of the application data directory\n\
             \x20 --dry-run      print the pinned definition and stop before any download"
        );

        Ok(())
    }

    /// Parses the command's arguments.
    ///
    /// # Errors
    ///
    /// Returns a message when an argument is unknown or `--root` has no value.
    pub fn parse(arguments: &[String]) -> Result<Self, String> {
        let mut request = Self::default();
        let mut remaining = arguments.iter();

        while let Some(argument) = remaining.next() {
            match argument.as_str() {
                "--root" => {
                    let value = remaining
                        .next()
                        .ok_or_else(|| String::from("--root needs a directory"))?;

                    request.root = Some(PathBuf::from(value));
                }
                "--dry-run" => request.dry_run = true,
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        Ok(request)
    }

    /// Returns the root the run installs into, or the application default.
    fn paths(&self) -> AppPaths {
        match &self.root {
            Some(root) => AppPaths::below(root),
            None => AppPaths::default(),
        }
    }
}

/// Runs the developer acquisition command.
pub fn run(arguments: &[String]) -> ExitCode {
    let request = match AcquireRequest::parse(arguments) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("{message}");
            eprintln!(
                "usage: bitarchive-desktop acquire-retroarch-runtime [--root <dir>] [--dry-run]"
            );

            return ExitCode::FAILURE;
        }
    };

    let definition = pinned_retroarch_runtime();
    let paths = request.paths();

    println!("LIVE RUNTIME ACQUISITION (developer opt-in, no game launch)");
    println!();
    println!("  runtime      {}", definition.id());
    println!("  version      {}", definition.version());
    println!("  platform     {}", definition.platform());
    println!("  artifact     {}", PINNED_RETROARCH_ARTIFACT_FILE);
    println!("  source       {}", definition.source());
    println!("  sha-256      {}", PINNED_RETROARCH_ARTIFACT_SHA256);
    println!("  upstream     {PINNED_RETROARCH_UPSTREAM_URL}");
    println!("  license      {PINNED_RETROARCH_LICENSE}");
    println!("  store        {}", paths.components().display());
    println!();

    if request.dry_run {
        println!("--dry-run: nothing was downloaded and nothing was installed.");

        return ExitCode::SUCCESS;
    }

    // The composition of the managed runtime path: the store owns the flow, the
    // infrastructure adapters perform the I/O, and the platform layer supplies the
    // disk-image bridge.
    let store = ComponentStore::new(
        paths.components(),
        HttpArtifactDownloader::new(),
        AppleDiskImageExtractor::new(DmgBundleExtractor::new()),
    );

    // An interrupted earlier run may have left staging state behind. Removing it
    // before starting is what the bootstrap recovery step will do; doing it here
    // keeps the developer command repeatable.
    match store.cleanup_staging() {
        Ok(0) => {}
        Ok(removed) => println!("removed {removed} abandoned staging director(ies)"),
        Err(error) => eprintln!("warning: staging cleanup failed: {error}"),
    }

    println!("downloading and verifying…");

    let installed = match store.install(&definition) {
        Ok(installed) => installed,
        Err(error) => {
            eprintln!("acquisition failed: {error}");
            eprintln!("nothing was activated; an already active runtime, if any, is unchanged.");

            return ExitCode::FAILURE;
        }
    };

    report_installed(&installed);

    match retroarch_runtime::resolve(&definition, &store) {
        Ok(runtime) => {
            println!(
                "resolved executable  {}",
                runtime.executable_path().display()
            );
            println!();
            println!(
                "Runtime acquisition complete. No game was launched and no core was installed."
            );

            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("the runtime was installed but could not be resolved: {error}");

            ExitCode::FAILURE
        }
    }
}

/// Prints what was installed.
fn report_installed(installed: &InstalledRuntime) {
    println!();
    println!("installed");
    println!("  version      {}", installed.version());
    println!("  directory    {}", installed.directory().display());
    println!("  executable   {}", installed.executable_path().display());
    println!("  version pin  {PINNED_RETROARCH_VERSION} (a later run reuses it)");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default request installs into the application paths and does not stop
    /// early, which is what an unadorned invocation means.
    #[test]
    fn the_default_request_uses_the_application_paths() {
        let request = AcquireRequest::default();

        assert!(request.root.is_none());
        assert!(!request.dry_run);

        let paths = request.paths();

        assert!(
            paths
                .application_support()
                .ends_with("Library/Application Support/BitArchive"),
            "the default run uses the real application data directory"
        );
    }

    /// `--root` redirects the whole run, so the command can be exercised without
    /// touching a user's application data.
    #[test]
    fn a_root_argument_redirects_every_path() {
        let request = AcquireRequest::parse(&[
            String::from("--root"),
            String::from("/tmp/bitarchive-dev-store"),
            String::from("--dry-run"),
        ])
        .expect("the arguments are valid");

        assert!(request.dry_run);

        let paths = request.paths();

        assert_eq!(
            paths.application_support(),
            std::path::Path::new("/tmp/bitarchive-dev-store")
        );
        assert_eq!(
            paths.components(),
            std::path::Path::new("/tmp/bitarchive-dev-store/components")
        );
    }

    /// `--dry-run` is accepted and unknown arguments are rejected, so a typo
    /// cannot silently start a download.
    #[test]
    fn arguments_are_validated() {
        assert!(AcquireRequest::parse(&[String::from("--dry-run")]).is_ok());
        assert!(AcquireRequest::parse(&[]).is_ok());

        assert!(AcquireRequest::parse(&[String::from("--root")]).is_err());
        assert!(AcquireRequest::parse(&[String::from("--force")]).is_err());
    }

    /// The command exists only as its own subcommand, so a normal launch still
    /// starts the UI and never downloads anything.
    #[test]
    fn the_command_is_opt_in() {
        assert_eq!(super::super::ACQUIRE_COMMAND, "acquire-retroarch-runtime");
    }
}
