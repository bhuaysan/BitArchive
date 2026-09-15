//! Starting and observing a process on this platform.
//!
//! This module owns the one place in BitArchive that reaches for
//! [`std::process`]. It maps an already prepared
//! [`PreparedLaunch`](bitarchive_application::PreparedLaunch) onto
//! [`std::process::Command`] and hands back a
//! [`SpawnedProcess`] that can be observed.
//!
//! ```text
//! PreparedLaunch
//!       ↓   command_for(...)
//! std::process::Command
//!       ↓   spawn()
//! SpawnedProcess
//! ```
//!
//! # Why a `Command` and not a command line
//!
//! [`Command`] is a process builder, not a string. The executable is the
//! program it starts, and every argument is passed separately, so a path
//! containing spaces stays one argument. No shell is created and no shell runs
//! in between, which is why the official documentation can state that shell
//! syntax such as quotes, escaped characters, word splitting, glob patterns,
//! and variable substitution have no effect on an argument
//! (ARCHITECTURE.md §20.3).
//!
//! # What this module deliberately does not do
//!
//! - **No shell.** Neither `sh -c`, nor `bash`, nor any other interpreter, and
//!   nothing is joined into a command string.
//! - **No filesystem preflight.** The executable, the working directory, and
//!   everything the arguments name are handed to the process API exactly as
//!   prepared. Nothing checks whether a path exists, is readable, or is
//!   executable first: such a check would be a time-of-check/time-of-use race
//!   and would duplicate the readiness work that belongs to component
//!   management, not to this adapter. A launch that cannot start fails in
//!   [`Command::spawn`], which reports the technical reason.
//! - **No validation.** A [`PreparedLaunch`](bitarchive_application::PreparedLaunch)
//!   is already process-ready, so this adapter interprets nothing, rewrites
//!   nothing, and normalizes nothing.
//! - **No error framework.** Failures are the [`std::io::Error`] the standard
//!   library returns, so the caller sees the real cause rather than a
//!   reclassified summary.
//! - **No stdio redirection.** [`Command::spawn`] inherits the parent's
//!   standard streams. Session-specific log artifacts and pipe readers are a
//!   later concern and are not invented here.
//! - **No process control.** This module starts and observes. It does not
//!   signal, terminate, or kill a process; ordered shutdown and the forced
//!   shutdown that may follow it belong to the session lifecycle
//!   (ARCHITECTURE.md §22.3).
//!
//! [`Command`]: std::process::Command

use std::process::{Child, Command};

use bitarchive_application::PreparedLaunch;

use crate::ProcessExitStatus;

/// Builds the process command for `launch`.
///
/// The mapping is deliberately literal and complete:
///
/// - [`Command::new`] receives the prepared executable, so it is the program to
///   start and never `argv[0]` of the argument list;
/// - [`Command::args`] receives the prepared arguments unchanged and in order,
///   as separate values;
/// - [`Command::envs`] receives the prepared overrides unchanged;
/// - [`Command::current_dir`] is called only when the prepared launch names a
///   working directory.
///
/// # Environment
///
/// [`Command::new`] inherits the current process's environment, and
/// [`Command::envs`] adds explicit mappings that take precedence over inherited
/// ones. That combination is exactly the meaning of
/// [`PreparedLaunch::environment`](bitarchive_application::PreparedLaunch::environment):
/// additional or explicit overrides, not a replacement environment.
/// [`Command::env_clear`] is therefore never called and the parent environment
/// is never rebuilt from
/// [`std::env::vars`] by hand.
///
/// # Working directory
///
/// [`None`] means "decide nothing", so no override is set and the child
/// inherits the current working directory, which is what
/// [`Command::current_dir`] returning [`None`] expresses. A supplied path is
/// passed through as it is: it is not canonicalized, not checked for existence,
/// and not created.
///
/// # Inspection
///
/// The function is private, and the example that exercises it lives in this
/// module's test suite, where the resulting [`Command`] is inspected through
/// [`Command::get_program`], [`Command::get_args`], [`Command::get_envs`], and
/// [`Command::get_current_dir`]. From outside the crate, the mapping is reached
/// through [`ProcessController::spawn`](crate::ProcessController::spawn).
///
/// [`PreparedLaunch::environment`]: bitarchive_application::PreparedLaunch::environment
/// [`PreparedLaunch::working_directory`]: bitarchive_application::PreparedLaunch::working_directory
pub(crate) fn command_for(launch: &PreparedLaunch) -> Command {
    let mut command = Command::new(launch.executable());

    command.args(launch.arguments());

    // `envs` takes the mappings by value, so the prepared slice is cloned into
    // the command. Every mapping is copied unchanged; nothing is merged,
    // filtered, or validated here. The order of the resulting mappings belongs
    // to the standard library, which resolves a repeated key on its own terms.
    command.envs(launch.environment().iter().cloned());

    if let Some(directory) = launch.working_directory() {
        command.current_dir(directory);
    }

    command
}

/// A running or already finished process started by BitArchive.
///
/// The handle wraps [`std::process::Child`] so the rest of the architecture does
/// not build on a standard library type whose shape is tied to this platform.
/// It offers exactly the observation the next steps need: the process id, a
/// non-blocking check, and a blocking wait.
///
/// ```text
/// id()              → the process id the operating system assigned
/// try_wait()        → non-blocking: has it exited yet?
/// wait()            → blocking: wait until it has exited
/// ```
///
/// # No implicit process control
///
/// There is deliberately **no** `Drop` implementation. Dropping this handle
/// does not kill the process, does not force it to exit, and does not block on
/// it. [`std::process::Child`] has no `Drop` either, so a process keeps running
/// after its handle goes out of scope, and that is the behaviour the product
/// requires: when the user closes BitArchive while a game runs, the game may
/// continue (PRODUCT.md §27.5). A handle that killed or waited on drop would
/// silently remove that choice.
///
/// # Reaping
///
/// Because nothing happens implicitly, the process must be observed through
/// [`try_wait`](Self::try_wait) or [`wait`](Self::wait) so the operating system
/// can release the process. A process that has ended but has not been waited on
/// remains a zombie on Unix, and leaving too many of them can exhaust global
/// resources. Recording the status, deciding whether a session has ended, and
/// reacting to it belong to session management
/// (ARCHITECTURE.md §22); this handle only makes the observation possible.
#[derive(Debug)]
pub struct SpawnedProcess {
    child: Child,
}

impl SpawnedProcess {
    /// Wraps a started child process.
    ///
    /// Only [`ProcessController::spawn`](crate::ProcessController::spawn)
    /// constructs this type, so a handle always belongs to a process this
    /// platform layer started.
    pub(crate) const fn new(child: Child) -> Self {
        Self { child }
    }

    /// Returns the process id the operating system assigned.
    ///
    /// The id identifies the process for as long as it runs. It is deliberately
    /// not a domain identity and not persisted here: process start time and
    /// executable identity are part of the later recovery model
    /// (ARCHITECTURE.md §22.1), not of this adapter.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Collects the exit status if the process has already exited.
    ///
    /// This does not block: it returns [`None`] while the process is still
    /// running and [`Some`] once it has ended. Repeating the call after the
    /// process has exited returns the same status instead of failing, so a
    /// later [`wait`](Self::wait) is safe and reports the same result.
    ///
    /// # Errors
    ///
    /// Returns the [`std::io::Error`] the operating system reported while
    /// checking the process.
    pub fn try_wait(&mut self) -> std::io::Result<Option<ProcessExitStatus>> {
        self.child
            .try_wait()
            .map(|status| status.map(to_exit_status))
    }

    /// Waits until the process has exited and returns its exit status.
    ///
    /// The standard input handle to the process, if one was captured, is closed
    /// before waiting. Nothing here is captured, so the child keeps the standard
    /// streams it was started with.
    ///
    /// # Errors
    ///
    /// Returns the [`std::io::Error`] the operating system reported while
    /// waiting for the process.
    pub fn wait(&mut self) -> std::io::Result<ProcessExitStatus> {
        self.child.wait().map(to_exit_status)
    }
}

/// Reduces a platform wait status to the facts BitArchive records.
fn to_exit_status(status: std::process::ExitStatus) -> ProcessExitStatus {
    ProcessExitStatus::new(status.success(), status.code())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The executable used by the command-mapping tests.
    const EXECUTABLE: &str = "/runtime/RetroArch";

    /// Builds a launch with a config-like command line of two values.
    fn launch() -> PreparedLaunch {
        PreparedLaunch::new(
            EXECUTABLE,
            vec![
                std::ffi::OsString::from("-L"),
                std::ffi::OsString::from("/cores/test_libretro.dylib"),
            ],
        )
    }

    /// The prepared executable becomes the program to start, exactly as it was
    /// prepared and without being resolved or rewritten.
    #[test]
    fn the_executable_becomes_the_program() {
        let command = command_for(&launch());

        assert_eq!(
            command.get_program(),
            std::ffi::OsStr::new(EXECUTABLE),
            "the prepared executable is the program and is not altered"
        );
    }

    /// The arguments reach the process API unchanged, in order and as separate
    /// values: nothing is joined into a command line and nothing is appended.
    #[test]
    fn the_arguments_keep_their_order_and_are_not_joined() {
        let command = command_for(&launch());

        let arguments: Vec<&std::ffi::OsStr> = command.get_args().collect();

        assert_eq!(
            arguments,
            [
                std::ffi::OsStr::new("-L"),
                std::ffi::OsStr::new("/cores/test_libretro.dylib")
            ],
            "the argument vector is exactly the prepared one"
        );
    }

    /// An argument containing spaces stays one argument. This is the property a
    /// shell would destroy, which is why no shell is involved.
    #[test]
    fn an_argument_with_spaces_stays_one_argument() {
        let core = "/Users/Test/Retro Cores/test_libretro.dylib";
        let content = "/Users/Test/My Games/game.rom";
        let launch = PreparedLaunch::new(
            EXECUTABLE,
            vec![
                std::ffi::OsString::from("-L"),
                std::ffi::OsString::from(core),
                std::ffi::OsString::from(content),
            ],
        );

        let command = command_for(&launch);

        let arguments: Vec<&std::ffi::OsStr> = command.get_args().collect();

        assert_eq!(
            arguments,
            [
                std::ffi::OsStr::new("-L"),
                std::ffi::OsStr::new(core),
                std::ffi::OsStr::new(content),
            ],
            "a path containing spaces is not split and not quoted"
        );
        assert_eq!(arguments.len(), 3, "three arguments stay three arguments");
    }

    /// The prepared environment overrides become the command's explicit
    /// environment mappings, with both keys and both values unchanged.
    ///
    /// [`Command::get_envs`] reports the mappings in an unspecified order, so
    /// this asserts membership rather than position. Positions are not
    /// meaningful here: an environment is a mapping, and duplicate keys are
    /// resolved by the standard library, not by adapter-side ordering.
    #[test]
    fn the_environment_overrides_are_mapped_as_explicit_mappings() {
        let environment = vec![
            (
                std::ffi::OsString::from("LIBRETRO_SYSTEM_DIRECTORY"),
                std::ffi::OsString::from("/Users/Test/My Firmware"),
            ),
            (
                std::ffi::OsString::from("LIBRETRO_DIRECTORY"),
                std::ffi::OsString::from("/Users/Test/Retro Cores"),
            ),
        ];
        let launch = launch().with_environment(environment);

        let command = command_for(&launch);

        let mappings: Vec<(&std::ffi::OsStr, Option<&std::ffi::OsStr>)> =
            command.get_envs().collect();

        assert_eq!(mappings.len(), 2, "both overrides are mapped");
        assert!(
            mappings.contains(&(
                std::ffi::OsStr::new("LIBRETRO_SYSTEM_DIRECTORY"),
                Some(std::ffi::OsStr::new("/Users/Test/My Firmware")),
            )),
            "the first override keeps its value, including the space in it"
        );
        assert!(
            mappings.contains(&(
                std::ffi::OsStr::new("LIBRETRO_DIRECTORY"),
                Some(std::ffi::OsStr::new("/Users/Test/Retro Cores")),
            )),
            "the second override keeps its value"
        );
        assert!(
            mappings.iter().all(|(_, value)| value.is_some()),
            "the overrides are additions to the environment, never removals"
        );
    }

    /// A launch without overrides adds no mapping at all, and no mapping is a
    /// removal: the child keeps the inherited environment.
    #[test]
    fn a_launch_without_overrides_adds_no_environment_mapping() {
        let command = command_for(&launch());

        assert_eq!(
            command.get_envs().count(),
            0,
            "nothing is set explicitly, so the environment is inherited untouched"
        );
        assert!(
            !command.get_envs().any(|(_, value)| value.is_none()),
            "no environment variable is explicitly removed"
        );
    }

    /// A prepared working directory becomes the child's working directory,
    /// passed through exactly as prepared.
    #[test]
    fn a_prepared_working_directory_becomes_the_child_directory() {
        let working_directory = "/Users/Test/My Sessions/123";
        let launch = launch().with_working_directory(Some(working_directory.into()));

        let command = command_for(&launch);

        assert_eq!(
            command.get_current_dir(),
            Some(std::path::Path::new(working_directory)),
            "the directory is used verbatim, not canonicalized"
        );
    }

    /// Without a prepared working directory no override is set, so the child
    /// inherits the current working directory.
    #[test]
    fn without_a_prepared_working_directory_no_override_is_set() {
        let command = command_for(&launch());

        assert_eq!(
            command.get_current_dir(),
            None,
            "no directory is imposed when the launch names none"
        );
    }

    /// Building the command leaves the standard streams alone, so
    /// [`Command::spawn`] inherits them instead of creating pipes: the started
    /// child has no captured handles, and BitArchive therefore owes it no
    /// reader.
    ///
    /// The child is a second instance of the test binary, which is harmless and
    /// needs no shell, no external tool, and no emulator; it is waited on, so no
    /// process is left behind.
    #[test]
    fn no_standard_stream_is_redirected() {
        let mut command = command_for(&PreparedLaunch::new(
            std::env::current_exe().expect("the test binary can be located"),
            vec![std::ffi::OsString::from("--list")],
        ));

        let mut child = command.spawn().expect("the test binary can be started");

        assert!(
            child.stdin.is_none() && child.stdout.is_none() && child.stderr.is_none(),
            "no stream is captured, so all three are inherited"
        );

        let status = child.wait().expect("the child process can be waited on");

        assert!(status.success(), "the child test run exited successfully");
    }
}
