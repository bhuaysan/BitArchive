//! FIRST REAL MANAGED LAUNCH — the opt-in developer command (Issue #23).
//!
//! This module composes the paths that B3–B6 built and never joined before. It is
//! the first execution in BitArchive that resolves its *own* managed components
//! and starts a real RetroArch process with them:
//!
//! ```text
//! ManagedRuntime::executable_path()          ← active managed RetroArch runtime
//! ManagedCore::library_path()                ← resolved curated mGBA build
//! developer supplied local content
//!         ↓
//! RetroArchLaunchInput
//!         ↓
//! RetroArchBackend::prepare_launch()         ← the only place that spells -L
//!         ↓
//! PreparedLaunch
//!         ↓
//! ProcessController::spawn()                 ← no shell, separate arguments
//!         ↓
//! SpawnedProcess::wait()                     ← the developer ends RetroArch
//!         ↓
//! exit status
//! ```
//!
//! It is a *developer vertical slice*, not a product flow:
//!
//! - it is never called by the UI, by a library API, or by a test;
//! - it persists nothing, registers no session, and captures no playtime;
//! - it starts nothing until both managed components have resolved;
//! - the only launch input a developer supplies is the content path.
//!
//! # Why the developer supplies no runtime and no core
//!
//! Issue #17 (closed as *not planned*) proposed a smoke launcher that took a
//! RetroArch executable and a core library as arguments. That is no longer the
//! architecture: after B5 and B6 both are BitArchive-managed components with a
//! pinned identity, an immutable installation, and one deterministic resolution
//! step each. Accepting a developer path would make "which RetroArch ran?" and
//! "which core ran?" unanswerable, and it would reintroduce exactly the
//! `PATH`/`/Applications`/local-file fallbacks the managed component design
//! removed.
//!
//! So the runtime comes from [`retroarch_runtime::resolve`] and the core from the
//! core store's own `resolve`, and nothing else is accepted:
//!
//! ```text
//! #17   <retroarch-executable> <core-library> <content>   ← discarded
//! B7    <content> [--root <dir>]                          ← this command
//! ```
//!
//! # Why a missing component is not acquired here
//!
//! Acquisition is an explicit, deliberate step in B5 and B6: it is the only
//! network activity in BitArchive, it is opt-in, and CI never runs it. A launch
//! that downloaded a runtime or a core would make what the user is running depend
//! on network timing, and it would create a second acquisition flow beside the
//! reviewed one.
//!
//! This command therefore reports the failure and, when installing the component
//! would actually resolve it, names the existing command that does so. The advice
//! repeats the `--root` the launch was started with, so it repairs the store the
//! launch resolved *from* rather than the application data directory:
//!
//! ```text
//! launch content.gba                    → cargo run -p bitarchive-desktop -- acquire-core
//! launch content.gba --root /tmp/store  → cargo run -p bitarchive-desktop -- acquire-core --root /tmp/store
//! ```
//!
//! Only a component that is *not installed* is answered that way. A build
//! directory that exists without its library is reported as the broken
//! installation it is: the store refuses to install a build that is already
//! installed, so the acquisition command could not repair it and suggesting it
//! would be misleading.
//!
//! # Exit status
//!
//! The command waits for the process and exits with the code the process ended
//! with, so `RetroArch` reporting `3` is reported as `3`. A code that [`ExitCode`]
//! cannot carry, and a process that reported no code at all (a signal termination,
//! for instance), become a generic failure rather than a truncated or invented
//! number.
//!
//! # What this module deliberately does not do
//!
//! - **No second launch pipeline.** The RetroArch command line stays the sole
//!   responsibility of [`RetroArchBackend`]; this module assembles
//!   [`RetroArchLaunchInput`] and nothing else.
//! - **No shell.** The executable and every argument reach the process API as
//!   separate values (ARCHITECTURE.md §20.3).
//! - **No content management.** The content path is checked for existence and
//!   passed on. It is not copied, moved, renamed, hashed, imported, or scanned,
//!   and no content is ever acquired.
//! - **No session management.** Nothing is persisted, no session is registered,
//!   no playtime is measured, and the process is not signalled or terminated:
//!   the developer ends RetroArch normally and this command waits for it
//!   (ARCHITECTURE.md §22 is a later step).
//! - **No readiness engine.** Firmware readiness, core compatibility, and
//!   content validation belong to the product launch flow and are not faked
//!   here.

use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;

use bitarchive_application::PreparedLaunch;
use bitarchive_application::managed_core::{CoreInstaller, CoreStoreError, ManagedCore};
use bitarchive_domain::component::ComponentIdError;
use bitarchive_domain::managed_core::{
    CoreComponentId, CorePlatform, CuratedCoreError, UnsupportedCoreHost, curated_core,
    host_core_platform,
};
use bitarchive_emulation::{
    ManagedRuntime, RetroArchBackend, RetroArchLaunchInput, RetroArchRuntimeError,
    pinned_retroarch_runtime, retroarch_runtime,
};
use bitarchive_infrastructure::{
    AppleDiskImageExtractor, ComponentStore, CoreStore, HttpArtifactDownloader,
    ZipCoreArchiveExtractor,
};
use bitarchive_platform::{AppPaths, DmgBundleExtractor, ProcessController, ProcessExitStatus};

/// How every developer command of this composition root is started.
///
/// The recovery advice quotes a command a developer can copy, so the prefix lives
/// in one place and cannot drift from the dispatch in `main`.
const COMMAND_PREFIX: &str = "cargo run -p bitarchive-desktop --";

/// The command that installs the pinned RetroArch runtime (B5 / Issue #19).
const ACQUIRE_RUNTIME_COMMAND: &str = "acquire-retroarch-runtime";

/// The command that installs the curated mGBA core (B6 / Issue #21).
const ACQUIRE_CORE_COMMAND: &str = "acquire-core";

/// What the command was asked to launch.
///
/// The content path is required and the store root is optional: without `--root`
/// the run resolves the managed components from the application data directory.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LaunchRequest {
    content: PathBuf,
    root: Option<PathBuf>,
}

impl LaunchRequest {
    /// Describes the accepted arguments, so a caller does not have to guess.
    ///
    /// # Errors
    ///
    /// Never. The signature exists so that the usage text has one source.
    pub fn usage() -> Result<(), std::convert::Infallible> {
        println!(
            "  <content>      path of the local content to launch — the only launch input\n\
             \x20                the developer supplies (required, exactly one)\n\
             \x20 --root <dir>   resolve the managed runtime and the curated core below <dir>\n\
             \x20                instead of the application data directory"
        );

        Ok(())
    }

    /// Parses the command's arguments.
    ///
    /// Exactly one positional argument is accepted and it is the content path.
    /// Everything else is either `--root` or an error, so no argument can name a
    /// runtime, a core, a version, a URL, or a library.
    ///
    /// # Errors
    ///
    /// Returns a message when the content path is missing or given more than
    /// once, when `--root` has no value, or when an argument is unknown.
    pub fn parse(arguments: &[String]) -> Result<Self, String> {
        let mut content = None;
        let mut root = None;
        let mut remaining = arguments.iter();

        while let Some(argument) = remaining.next() {
            match argument.as_str() {
                "--root" => {
                    let value = remaining
                        .next()
                        .ok_or_else(|| String::from("--root needs a directory"))?;

                    root = Some(PathBuf::from(value));
                }
                other if other.starts_with('-') => {
                    return Err(format!(
                        "unknown argument: {other} (the runtime and the core are managed \
                         components and cannot be named here)"
                    ));
                }
                other => {
                    if content.is_some() {
                        return Err(format!(
                            "only one content path can be launched, but another one was given: \
                             {other}"
                        ));
                    }

                    content = Some(PathBuf::from(other));
                }
            }
        }

        let content =
            content.ok_or_else(|| String::from("the path of the content to launch is missing"))?;

        Ok(Self { content, root })
    }

    /// Returns the content path the developer supplied.
    fn content(&self) -> &Path {
        &self.content
    }

    /// Returns the store root the developer supplied, if any.
    ///
    /// The recovery advice uses this to point at the store the launch actually
    /// resolved from; it is the `--root` value and not the resolved [`AppPaths`],
    /// so a run without `--root` keeps the unadorned default advice.
    fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Returns the store root the run resolves components from.
    fn paths(&self) -> AppPaths {
        match &self.root {
            Some(root) => AppPaths::below(root),
            None => AppPaths::default(),
        }
    }
}

/// Why the developer launch could not be started.
///
/// A value carries the technical cause and nothing else: the developer-facing
/// recovery advice is derived from it separately by [`recovery_advice`], because
/// what can repair the failure depends on the store the launch resolved from and
/// not on the failure alone.
#[derive(Debug)]
enum LaunchError {
    /// The curated component identity in the code is not valid.
    ///
    /// The identity is a reviewed constant, so this is a build defect and not a
    /// condition a developer can fix from the outside.
    ComponentIdentity(ComponentIdError),
    /// The allowlist has no definition for the curated component on `platform`.
    CuratedCore(CuratedCoreError),
    /// The managed RetroArch runtime could not be resolved.
    Runtime(RetroArchRuntimeError),
    /// The curated core build could not be resolved.
    Core(CoreStoreError),
}

impl LaunchError {
    /// Returns the acquisition command that installs the component this failure
    /// is about, when installing that component resolves the failure.
    ///
    /// Only a component that is **not installed** has an answer: the acquisition
    /// command installs it and the launch then resolves it. A build directory
    /// that exists without its library does *not* have one — `CoreStore::install`
    /// refuses a build that is already installed, so the acquisition command
    /// could not repair that state and pointing at it would send the developer to
    /// a command that fails for a different reason. Every other failure — an
    /// unreadable store, a digest or platform problem, an invalid definition —
    /// needs diagnosis rather than a download.
    fn acquisition_command(&self) -> Option<AcquisitionCommand> {
        match self {
            Self::Runtime(RetroArchRuntimeError::NotInstalled { .. }) => {
                Some(AcquisitionCommand::Runtime)
            }
            Self::Core(CoreStoreError::NotInstalled { .. }) => Some(AcquisitionCommand::Core),
            Self::ComponentIdentity(_)
            | Self::CuratedCore(_)
            | Self::Runtime(_)
            | Self::Core(_) => None,
        }
    }
}

/// The developer command that installs one managed component class.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AcquisitionCommand {
    /// Installs the pinned RetroArch runtime (B5 / Issue #19).
    Runtime,
    /// Installs the curated mGBA core (B6 / Issue #21).
    Core,
}

impl AcquisitionCommand {
    /// Returns the command name, as the composition root dispatches it.
    const fn name(self) -> &'static str {
        match self {
            Self::Runtime => ACQUIRE_RUNTIME_COMMAND,
            Self::Core => ACQUIRE_CORE_COMMAND,
        }
    }

    /// Renders the command, aimed at `store_root` when the launch used one.
    ///
    /// The advice has to repair the store the launch resolved *from*, so a run
    /// started with `--root <dir>` is answered with the acquisition command for
    /// that same `<dir>`; without `--root` the application data directory is the
    /// store the launch used and the unadorned command is the right answer.
    fn render(self, store_root: Option<&Path>) -> String {
        match store_root {
            Some(root) => format!("{COMMAND_PREFIX} {} --root {}", self.name(), root.display()),
            None => format!("{COMMAND_PREFIX} {}", self.name()),
        }
    }
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ComponentIdentity(error) => {
                write!(f, "the curated component identity is not valid: {error}")
            }
            Self::CuratedCore(error) => write!(f, "no curated core definition: {error}"),
            Self::Runtime(error) => write!(f, "{error}"),
            Self::Core(error) => write!(f, "{error}"),
        }
    }
}

/// Returns the developer-facing advice for `error`, when a command can resolve it.
///
/// `store_root` is the `--root` the launch was started with, if any, so the
/// advice names the store the failure came from instead of the default one.
fn recovery_advice(error: &LaunchError, store_root: Option<&Path>) -> Option<String> {
    let command = error.acquisition_command()?;

    Some(format!("run this first:\n  {}", command.render(store_root)))
}

/// Prints a resolution failure, its recovery advice when there is one, and
/// returns the failure exit code.
fn report_failure(error: &LaunchError, store_root: Option<&Path>) -> ExitCode {
    eprintln!("{error}");

    if let Some(advice) = recovery_advice(error, store_root) {
        eprintln!("{advice}");
    }

    ExitCode::FAILURE
}

/// Runs the developer launch command.
pub fn run(arguments: &[String]) -> ExitCode {
    let request = match LaunchRequest::parse(arguments) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("usage: bitarchive-desktop launch <content> [--root <dir>]");

            return ExitCode::FAILURE;
        }
    };

    let paths = request.paths();
    let content = request.content();

    println!("MANAGED RETROARCH LAUNCH (developer opt-in, real process start)");
    println!();
    println!("  content      {}", content.display());
    println!("  store        {}", paths.components().display());
    println!();

    // The one input BitArchive does not own. It is checked because a launch that
    // cannot find its content would otherwise reach RetroArch as a runtime
    // failure of the emulator instead of a clear answer to the developer.
    if !content.is_file() {
        eprintln!("the content to launch is not a file: {}", content.display());
        eprintln!(
            "BitArchive does not acquire content; pass the path of content that already \
             exists locally."
        );

        return ExitCode::FAILURE;
    }

    // The host architecture decides which curated build is asked for. No other
    // architecture's core is substituted for it (Issue #21).
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

    let runtime = match resolve_runtime(&paths) {
        Ok(runtime) => runtime,
        Err(error) => return report_failure(&error, request.root()),
    };

    let core = match resolve_core(&paths, platform) {
        Ok(core) => core,
        Err(error) => return report_failure(&error, request.root()),
    };

    let prepared = prepare_managed_launch(&runtime, &core, content);

    report_plan(&paths, &runtime, &core, &prepared);

    let started = |id: u32| {
        println!("RetroArch started (PID {id}). Waiting for RetroArch to exit…");
    };

    match start_and_wait(&prepared, started) {
        Ok(status) => report_exit(status),
        Err(error) => {
            eprintln!("RetroArch could not be started and waited on: {error}");

            ExitCode::FAILURE
        }
    }
}

/// Resolves the executable of the active managed RetroArch runtime (B5 / #19).
///
/// The pinned definition is answered from `paths` alone, through the component
/// store's activation. Nothing is downloaded, installed, activated, or searched
/// for anywhere else.
///
/// # Errors
///
/// Returns [`LaunchError::Runtime`] when no active managed runtime resolves.
fn resolve_runtime(paths: &AppPaths) -> Result<ManagedRuntime, LaunchError> {
    let definition = pinned_retroarch_runtime();
    let store = ComponentStore::new(
        paths.components(),
        HttpArtifactDownloader::new(),
        AppleDiskImageExtractor::new(DmgBundleExtractor::new()),
    );

    retroarch_runtime::resolve(&definition, &store).map_err(LaunchError::Runtime)
}

/// Resolves the library of the curated core build for `platform` (B6 / #21).
///
/// The definition comes from the allowlist for the host architecture, and the
/// build is answered from `paths` alone, through the core store's build
/// resolution. The downloader and the extractor are passed because the store owns
/// both flows; resolving performs neither.
///
/// # Errors
///
/// Returns [`LaunchError::ComponentIdentity`] or [`LaunchError::CuratedCore`] when
/// the reviewed constants cannot be resolved to a definition, and
/// [`LaunchError::Core`] when the curated build does not resolve.
fn resolve_core(paths: &AppPaths, platform: CorePlatform) -> Result<ManagedCore, LaunchError> {
    let component_id =
        CoreComponentId::from_str(CoreComponentId::MGBA).map_err(LaunchError::ComponentIdentity)?;
    let definition = curated_core(&component_id, platform).map_err(LaunchError::CuratedCore)?;

    let store = CoreStore::new(
        paths.components(),
        HttpArtifactDownloader::new(),
        ZipCoreArchiveExtractor::new(),
    );

    store.resolve(&definition).map_err(LaunchError::Core)
}

/// Composes the resolved components and the content into the prepared launch.
///
/// The runtime supplies the program, the core supplies the `-L` library, and the
/// content is the positional argument. The arguments themselves are built by
/// [`RetroArchBackend`] alone, so this function cannot drift from the documented
/// RetroArch CLI form.
fn prepare_managed_launch(
    runtime: &ManagedRuntime,
    core: &ManagedCore,
    content: &Path,
) -> PreparedLaunch {
    let input = RetroArchLaunchInput::new(runtime.executable_path(), core.library_path(), content);

    RetroArchBackend::new().prepare_launch(&input)
}

/// Prints what was resolved and what is about to be started.
///
/// The program and its arguments are printed as separate lines on purpose: they
/// are separate values, and joining them into one string would suggest a command
/// line that no shell ever sees.
fn report_plan(
    paths: &AppPaths,
    runtime: &ManagedRuntime,
    core: &ManagedCore,
    prepared: &PreparedLaunch,
) {
    println!("resolved");
    println!("  runtime      {runtime}");
    println!("  core         {} {}", core.component_id(), core.build_id());
    println!("  platform     {}", core.platform());
    println!("  library      {}", core.library_path().display());
    println!("  store        {}", paths.components().display());
    println!();
    println!("prepared launch");
    println!("  program      {}", prepared.executable().display());

    for argument in prepared.arguments() {
        println!("  argument     {}", argument.to_string_lossy());
    }

    println!();
}

/// Starts `prepared` and waits for the process to exit.
///
/// `started` is called once with the process id, so the caller can report it
/// while the process runs; this function itself prints nothing and returns as
/// soon as the process has ended. Nothing else is done with the process: there
/// is no polling loop, no timeout, no signal, and no registry.
///
/// # Errors
///
/// Returns the [`std::io::Error`] of the failed start or the failed wait.
fn start_and_wait(
    prepared: &PreparedLaunch,
    started: impl FnOnce(u32),
) -> io::Result<ProcessExitStatus> {
    let mut process = ProcessController::new().spawn(prepared)?;

    started(process.id());

    process.wait()
}

/// Reports how the process ended and returns the command's exit code.
fn report_exit(status: ProcessExitStatus) -> ExitCode {
    let code = status.code();

    match code {
        Some(0) => println!("RetroArch exited successfully."),
        Some(code) => {
            println!("RetroArch exited with code {code}; reporting the same exit code.");
        }
        None => println!("RetroArch ended without an exit code."),
    }

    exit_code_for(code)
}

/// Returns the exit code this command reports for a process that ended with `code`.
///
/// `code` is what the operating system reported
/// ([`ProcessExitStatus::code`]), or [`None`] when the process did not exit
/// normally. A normal exit code becomes this command's exit code, so a script or
/// a developer sees the number the launched process ended with — `3` stays `3` —
/// instead of a generic `1`.
///
/// [`std::process::ExitCode`] carries exactly one byte ([`ExitCode::from`] takes a
/// [`u8`]), which is the whole range a Unix exit status can hold. A code outside
/// that range — possible where exit codes are wider, such as Windows — and a
/// process that reported no code at all (a signal termination, for example) are
/// both answered with the generic [`ExitCode::FAILURE`]: a wrapped or invented
/// number would be a wrong answer instead of a merely less precise one.
fn exit_code_for(code: Option<i32>) -> ExitCode {
    match code.and_then(|code| u8::try_from(code).ok()) {
        Some(code) => ExitCode::from(code),
        None => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::ffi::OsString;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use bitarchive_application::managed_runtime::InstalledRuntime;
    use bitarchive_domain::managed_core::mgba_bootstrap;
    use bitarchive_emulation::PINNED_RETROARCH_EXECUTABLE;

    use super::*;

    /// The executable path the pinned runtime definition names below its
    /// installation directory.
    const RUNTIME_EXECUTABLE: &str = PINNED_RETROARCH_EXECUTABLE;

    /// The argument that makes the test binary start, list its tests, and exit
    /// successfully. It is inert: no test body runs, so the controlled child
    /// process asserts nothing and touches nothing.
    const LIST_TESTS: &str = "--list";

    /// A unique directory below the system temporary directory.
    ///
    /// Process id plus wall clock keeps two concurrent runs of this test binary,
    /// and two sequential runs of the same test, from sharing a directory.
    struct TempDirectory {
        path: PathBuf,
    }

    impl TempDirectory {
        /// Creates a uniquely named directory that removes itself afterwards.
        fn create(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos());
            let path = env::temp_dir().join(format!(
                "bitarchive-launch-{label}-{}-{nanos}",
                std::process::id()
            ));

            fs::create_dir_all(&path).expect("the temporary directory can be created");

            Self { path }
        }

        /// Returns the directory path.
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// The default request resolves the managed components from the application
    /// data directory, so an unadorned invocation uses the store the acquisition
    /// commands install into.
    #[test]
    fn the_default_request_uses_the_application_data_directory() {
        let request =
            LaunchRequest::parse(&[String::from("/content/test.gba")]).expect("the content path");

        assert_eq!(request.content(), Path::new("/content/test.gba"));
        assert!(
            request
                .paths()
                .components()
                .ends_with("Library/Application Support/BitArchive/components"),
            "the default run resolves from the real application data directory"
        );
    }

    /// `--root` redirects the whole run, so a developer can verify the launch
    /// path against a throwaway store instead of the application data
    /// directory — the same option the acquisition commands accept.
    #[test]
    fn a_root_argument_redirects_the_component_store() {
        let request = LaunchRequest::parse(&[
            String::from("--root"),
            String::from("/tmp/bitarchive-launch-store"),
            String::from("/content/test.gba"),
        ])
        .expect("the arguments are valid");

        let paths = request.paths();

        assert_eq!(
            paths.application_support(),
            Path::new("/tmp/bitarchive-launch-store")
        );
        assert_eq!(
            paths.components(),
            Path::new("/tmp/bitarchive-launch-store/components")
        );
        assert_eq!(request.content(), Path::new("/content/test.gba"));
    }

    /// The content path is the one required input, it is positional, and a
    /// second one is refused instead of silently ignored.
    #[test]
    fn the_content_path_is_required_and_given_once() {
        assert!(LaunchRequest::parse(&[]).is_err());
        assert!(
            LaunchRequest::parse(&[String::from("--root"), String::from("/tmp/store")]).is_err(),
            "a run without content has nothing to launch"
        );
        assert!(
            LaunchRequest::parse(&[
                String::from("/content/first.gba"),
                String::from("/content/second.gba"),
            ])
            .is_err(),
            "two content paths do not describe one launch"
        );
        assert!(LaunchRequest::parse(&[String::from("--root")]).is_err());
    }

    /// No argument can name a runtime, a core, a version, a URL, or a library.
    /// The rejected #17 accepted three such paths; B7 accepts none of them.
    #[test]
    fn the_command_cannot_name_a_runtime_or_a_core() {
        for rejected in [
            "--retroarch",
            "--runtime",
            "--core",
            "--core-library",
            "--core-path",
            "--core-url",
            "--runtime-url",
            "--runtime-version",
            "--core-version",
            "--library",
            "--executable",
            "--url",
            "--version",
            "--config",
        ] {
            assert!(
                LaunchRequest::parse(&[
                    String::from(rejected),
                    String::from("/somewhere"),
                    String::from("/content/test.gba"),
                ])
                .is_err(),
                "{rejected} must not be an accepted argument"
            );
        }
    }

    /// The composition uses the managed executable, the managed library, and the
    /// developer's content — and nothing else. The content path stays one
    /// argument, so a path containing spaces needs no quoting.
    #[test]
    fn the_composition_uses_the_managed_executable_library_and_content() {
        let runtime_directory = Path::new("/store/components/runtime/retroarch/1.22.2");
        let definition = pinned_retroarch_runtime();
        let installed = InstalledRuntime::new(
            definition.id().clone(),
            definition.version().clone(),
            runtime_directory,
            definition.executable().clone(),
        );
        let runtime = ManagedRuntime::new(definition, &installed);

        let core_directory = Path::new("/store/components/cores/mgba/macos-arm64/mgba-build");
        let core = ManagedCore::new(mgba_bootstrap(CorePlatform::MacOsArm64), core_directory);

        let content = Path::new("/content/local test content.gba");

        let prepared = prepare_managed_launch(&runtime, &core, content);

        assert_eq!(
            prepared.executable(),
            runtime_directory.join(RUNTIME_EXECUTABLE),
            "the program is the executable of the managed runtime"
        );
        assert_eq!(
            prepared.arguments(),
            [
                OsString::from("-L"),
                core.library_path().into_os_string(),
                content.as_os_str().to_owned(),
            ],
            "the core is the managed library and the content is the positional argument"
        );
        assert!(
            core.library_path().starts_with(core_directory),
            "the library path is derived from the resolved core build directory"
        );
        assert!(prepared.environment().is_empty());
        assert_eq!(prepared.working_directory(), None);
    }

    /// The process composition starts a real, harmless child process and waits
    /// for it: a second instance of the test binary asked to list its tests. No
    /// RetroArch, no shell, and no network take part.
    #[test]
    fn a_controlled_child_process_is_started_and_waited_on() {
        let prepared = PreparedLaunch::new(
            env::current_exe().expect("the test binary can be located"),
            vec![OsString::from(LIST_TESTS)],
        );

        let mut observed = None;
        let status = start_and_wait(&prepared, |id| observed = Some(id))
            .expect("the controlled child process can be started and waited on");

        let id = observed.expect("the started process reports its id before the wait");
        assert!(id > 0, "the operating system assigned a valid process id");
        assert!(
            status.is_success(),
            "the child test run must succeed, but it reported {status:?}"
        );
        assert_eq!(status.code(), Some(0));
        assert_eq!(
            exit_code_for(status.code()),
            ExitCode::SUCCESS,
            "the status of a real child goes through the conversion the command reports with"
        );
    }

    /// The exit code a process ended with becomes the exit code of this command,
    /// so a script or a developer sees the number the launched process reported
    /// instead of a generic failure — `3` stays `3`.
    #[test]
    fn the_child_exit_code_becomes_the_command_exit_code() {
        assert_eq!(exit_code_for(Some(0)), ExitCode::SUCCESS);

        for code in [1u8, 3, 42, 255] {
            assert_eq!(
                exit_code_for(Some(i32::from(code))),
                ExitCode::from(code),
                "exit code {code} must be reported unchanged"
            );
        }
    }

    /// A code [`ExitCode`] cannot carry, and a process that reported no code at
    /// all, are a generic failure: truncating or inventing a number would be a
    /// wrong answer rather than a less precise one.
    #[test]
    fn a_code_outside_the_exit_code_range_is_a_generic_failure() {
        for code in [None, Some(-1), Some(256), Some(i32::MAX)] {
            assert_eq!(
                exit_code_for(code),
                ExitCode::FAILURE,
                "{code:?} cannot be carried by ExitCode and must not be invented"
            );
        }
    }

    /// A store with nothing installed resolves no runtime, and the failure names
    /// the command that installs one. Nothing is acquired while resolving.
    #[test]
    fn a_missing_runtime_is_reported_with_the_command_that_installs_it() {
        let root = TempDirectory::create("missing-runtime");
        let paths = AppPaths::below(root.path());

        let error = resolve_runtime(&paths).expect_err("a fresh store holds no installed runtime");

        assert!(
            matches!(
                error,
                LaunchError::Runtime(RetroArchRuntimeError::NotInstalled { .. })
            ),
            "a store without an activation record is reported as not installed: {error}"
        );

        let advice = recovery_advice(&error, None)
            .expect("an uninstalled runtime has an acquisition command");

        assert!(
            advice.contains(ACQUIRE_RUNTIME_COMMAND),
            "the failure must name the acquisition command: {advice}"
        );
        assert!(
            !advice.contains("--root"),
            "a run without `--root` must keep the default advice: {advice}"
        );
        assert!(
            !paths.components().exists(),
            "resolving a component must not create or download anything"
        );
    }

    /// The recovery advice points at the store the launch resolved from: a run
    /// started with `--root <dir>` is answered with the acquisition command for
    /// that same `<dir>`, not for the application data directory.
    #[test]
    fn the_acquisition_advice_follows_the_store_root_of_the_run() {
        let root = TempDirectory::create("missing-runtime-rooted");
        let paths = AppPaths::below(root.path());

        let error = resolve_runtime(&paths).expect_err("a fresh store holds no installed runtime");

        let advice = recovery_advice(&error, Some(root.path()))
            .expect("an uninstalled runtime has an acquisition command");

        assert!(
            advice.contains(&format!(
                "{ACQUIRE_RUNTIME_COMMAND} --root {}",
                root.path().display()
            )),
            "the advice must repair the store the launch resolved from: {advice}"
        );
        assert!(
            !paths.components().exists(),
            "resolving a component must not create or download anything"
        );
    }

    /// A missing core is answered with the core acquisition command for the same
    /// store the launch tried to resolve from.
    #[test]
    fn a_missing_core_is_reported_with_the_core_command_for_the_same_store() {
        let root = TempDirectory::create("missing-core");
        let paths = AppPaths::below(root.path());

        let error = resolve_core(&paths, CorePlatform::MacOsArm64)
            .expect_err("a fresh store holds no installed core build");

        assert!(
            matches!(
                error,
                LaunchError::Core(CoreStoreError::NotInstalled { .. })
            ),
            "a store without the curated build is reported as not installed: {error}"
        );

        let advice = recovery_advice(&error, Some(root.path()))
            .expect("an uninstalled build has an acquisition command");

        assert!(
            advice.contains(&format!(
                "{ACQUIRE_CORE_COMMAND} --root {}",
                root.path().display()
            )),
            "the advice must repair the store the launch resolved from: {advice}"
        );
        assert!(
            !paths.components().exists(),
            "resolving a component must not create or download anything"
        );
    }

    /// A build directory that exists without its library is reported as exactly
    /// that, and it gets **no** acquisition advice: the store refuses to install
    /// a build that is already installed, so `acquire-core` could not repair this
    /// state and suggesting it would send the developer to a command that fails
    /// for a different reason.
    #[test]
    fn a_broken_installation_is_reported_without_a_misleading_acquisition_hint() {
        let root = TempDirectory::create("broken-core");
        let paths = AppPaths::below(root.path());
        let definition = mgba_bootstrap(CorePlatform::MacOsArm64);

        // The installation directory of the curated build exists, the pinned
        // library inside it does not: the state `CoreStore::resolve` reports as
        // `LibraryMissing`. The path comes from the store itself, so this test
        // does not spell out the store layout.
        let store = CoreStore::new(
            paths.components(),
            HttpArtifactDownloader::new(),
            ZipCoreArchiveExtractor::new(),
        );
        let directory = store.installation_directory(
            definition.component_id(),
            definition.platform(),
            definition.build_id(),
        );
        fs::create_dir_all(&directory).expect("the build directory can be created");

        let error = resolve_core(&paths, CorePlatform::MacOsArm64)
            .expect_err("the library is missing, so the build does not resolve");

        assert!(
            matches!(
                error,
                LaunchError::Core(CoreStoreError::LibraryMissing { .. })
            ),
            "a present build directory without its library must be reported as missing: {error}"
        );
        assert_eq!(
            error.acquisition_command(),
            None,
            "installing an already installed build is refused, so there is no acquisition advice"
        );
        assert_eq!(recovery_advice(&error, None), None);
        assert!(
            !error.to_string().contains(ACQUIRE_CORE_COMMAND),
            "the failure must not suggest a command that cannot repair it: {error}"
        );
    }

    /// The command exists only as its own subcommand, so a normal launch still
    /// starts the UI and never starts RetroArch.
    #[test]
    fn the_command_is_opt_in() {
        assert_eq!(super::super::LAUNCH_COMMAND, "launch");
    }
}
