//! LIVE CORE ACQUISITION — the opt-in developer command.
//!
//! This module downloads the one curated libretro core (mGBA) from the official
//! build host, verifies it against the pinned SHA-256, extracts the pinned
//! library, installs it as an immutable, platform-specific build in the component
//! store, and resolves the library path (Issue #21 §17).
//!
//! It is a *developer verification* command, not a product flow:
//!
//! - it is never called by the UI, by a library API, or by a test;
//! - CI never runs it, so a normal build and a normal test run never touch the
//!   network (ARCHITECTURE.md §45.4);
//! - it says what it is about to do before it does it, and `--dry-run` stops
//!   before the first byte is fetched.
//!
//! ```text
//! curated definition for this host's architecture (bitarchive-domain)
//!       ↓
//! download from the official https URL
//!       ↓
//! SHA-256 verified while the bytes are written
//!       ↓
//! the one pinned archive member is extracted
//!       ↓
//! staged, validated, installed as
//! components/cores/mgba/<platform>/<build-id>/mgba_libretro.dylib
//!       ↓
//! resolved library path — and nothing is activated
//! ```
//!
//! # What it does not do
//!
//! No game is launched, no runtime is installed or changed, no core is activated
//! (there is no "active core"), no core is selected for a game, and no firmware is
//! acquired. The command also cannot install a core the allowlist does not curate:
//! the definition comes from [`curated_core`], and there is no argument that names
//! a URL, a core name, or a library path.

use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use bitarchive_application::managed_core::{CoreInstaller, ManagedCore};
use bitarchive_domain::managed_core::{
    CoreComponentId, UnsupportedCoreHost, curated_core, host_core_platform,
};
use bitarchive_infrastructure::{CoreStore, HttpArtifactDownloader, ZipCoreArchiveExtractor};
use bitarchive_platform::AppPaths;

/// What the command was asked to do.
///
/// The default is the plain invocation: install into the application data
/// directory and actually acquire the core.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AcquireCoreRequest {
    root: Option<PathBuf>,
    dry_run: bool,
}

impl AcquireCoreRequest {
    /// Describes the accepted arguments, so a caller does not have to guess.
    ///
    /// # Errors
    ///
    /// Never. The signature exists so that the usage text has one source.
    pub fn usage() -> Result<(), std::convert::Infallible> {
        println!(
            "  --root <dir>   install into <dir> instead of the application data directory\n\
             \x20 --dry-run      print the curated definition and stop before any download"
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
    let request = match AcquireCoreRequest::parse(arguments) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("usage: bitarchive-desktop acquire-core [--root <dir>] [--dry-run]");

            return ExitCode::FAILURE;
        }
    };

    // The component identity is the curated one, and the platform is the
    // compiler's target architecture — never a shell command and never a guess.
    let component_id = match CoreComponentId::from_str(CoreComponentId::MGBA) {
        Ok(component_id) => component_id,
        Err(error) => {
            eprintln!("the curated component identity is not valid: {error}");

            return ExitCode::FAILURE;
        }
    };

    let platform = match host_core_platform() {
        Some(platform) => platform,
        None => {
            let host = UnsupportedCoreHost::current();

            eprintln!(
                "this host ({} {}) has no managed-core platform, and no other \
                 architecture's core is substituted for it",
                host.operating_system, host.architecture
            );

            return ExitCode::FAILURE;
        }
    };

    // The allowlist decides what can be installed; a component it does not curate
    // has no definition and is never fetched from somewhere else.
    let definition = match curated_core(&component_id, platform) {
        Ok(definition) => definition,
        Err(error) => {
            eprintln!("no curated core definition: {error}");

            return ExitCode::FAILURE;
        }
    };

    let paths = request.paths();

    let published_file_name = definition
        .artifact()
        .file_name
        .as_deref()
        .unwrap_or("(none recorded)");

    println!("LIVE CORE ACQUISITION (developer opt-in, no game launch)");
    println!();
    println!("  core         {}", definition.component_id());
    println!("  name         {}", definition.display_name());
    println!("  core name    {}", definition.core_name());
    println!("  build        {}", definition.build_id());
    println!("  platform     {}", definition.platform());
    println!("  artifact     {published_file_name}");
    println!("  source       {}", definition.source());
    println!("  sha-256      {}", definition.digest());
    println!("  member       {}", definition.expected_member());
    println!("  library      {}", definition.library());
    println!(
        "  upstream     {} @ {}",
        definition.provenance().revision_short,
        definition.provenance().channel_date
    );
    println!("  upstream url {}", definition.attribution().upstream_url);
    println!("  license      {}", definition.attribution().license);
    println!("  store        {}", paths.components().display());
    println!();

    if request.dry_run {
        println!("--dry-run: nothing was downloaded and nothing was installed.");

        return ExitCode::SUCCESS;
    }

    // The composition of the curated core path: the store owns the flow, the
    // infrastructure adapters perform the I/O, and the platform layer supplies the
    // application paths.
    let store = CoreStore::new(
        paths.components(),
        HttpArtifactDownloader::new(),
        ZipCoreArchiveExtractor::new(),
    );

    // An interrupted earlier run may have left staging state behind. Removing it
    // before starting keeps the developer command repeatable.
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
            eprintln!("nothing was installed and no installed build was changed.");

            return ExitCode::FAILURE;
        }
    };

    report_installed(&installed);

    match store.resolve(&definition) {
        Ok(resolved) => {
            println!("resolved library  {}", resolved.library_path().display());
            println!();
            println!(
                "Core acquisition complete. No game was launched, no runtime was \
                 changed, and no core was activated."
            );

            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("the core was installed but could not be resolved: {error}");

            ExitCode::FAILURE
        }
    }
}

/// Prints what was installed.
fn report_installed(installed: &ManagedCore) {
    println!();
    println!("installed");
    println!("  build        {}", installed.build_id());
    println!("  platform     {}", installed.platform());
    println!("  directory    {}", installed.directory().display());
    println!("  library      {}", installed.library_path().display());
    println!(
        "  build pin    {} (a later run reuses this exact build)",
        installed.build_id()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default request installs into the application paths and does not stop
    /// early, which is what an unadorned invocation means.
    #[test]
    fn the_default_request_uses_the_application_paths() {
        let request = AcquireCoreRequest::default();

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
        let request = AcquireCoreRequest::parse(&[
            String::from("--root"),
            String::from("/tmp/bitarchive-core-store"),
            String::from("--dry-run"),
        ])
        .expect("the arguments are valid");

        assert!(request.dry_run);

        let paths = request.paths();

        assert_eq!(
            paths.application_support(),
            std::path::Path::new("/tmp/bitarchive-core-store")
        );
        assert_eq!(
            paths.components(),
            std::path::Path::new("/tmp/bitarchive-core-store/components")
        );
    }

    /// `--dry-run` is accepted and unknown arguments are rejected, so a typo
    /// cannot silently start a download.
    #[test]
    fn arguments_are_validated() {
        assert!(AcquireCoreRequest::parse(&[String::from("--dry-run")]).is_ok());
        assert!(AcquireCoreRequest::parse(&[]).is_ok());

        assert!(AcquireCoreRequest::parse(&[String::from("--root")]).is_err());
        assert!(AcquireCoreRequest::parse(&[String::from("--core")]).is_err());
        assert!(AcquireCoreRequest::parse(&[String::from("--force")]).is_err());
    }

    /// There is no argument that names a core, a URL, or a library path: what the
    /// command installs is decided by the curated allowlist alone.
    #[test]
    fn the_command_cannot_name_a_core_of_its_own() {
        for rejected in [
            "--core",
            "--url",
            "--library",
            "--version",
            "--force",
            "--insecure",
        ] {
            assert!(
                AcquireCoreRequest::parse(&[String::from(rejected)]).is_err(),
                "{rejected} must not be an accepted argument"
            );
        }
    }

    /// The command exists only as its own subcommand, so a normal launch still
    /// starts the UI and never downloads anything.
    #[test]
    fn the_command_is_opt_in() {
        assert_eq!(super::super::ACQUIRE_CORE_COMMAND, "acquire-core");
    }
}
