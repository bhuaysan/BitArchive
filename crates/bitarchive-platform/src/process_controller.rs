//! The concrete platform service that starts a prepared process.
//!
//! [`ProcessController`] is the step that turns a prepared launch into a running
//! process:
//!
//! ```text
//! PreparedLaunch        (bitarchive-application)
//!       ↓
//! ProcessController     (this module)
//!       ↓
//! SpawnedProcess        (bitarchive-platform)
//! ```
//!
//! It is a concrete platform service, not an application port. The application
//! layer defines no process trait yet, because nothing in the application layer
//! consumes one: orchestration and session management are later steps, and a
//! port defined before its consumer would only guess at the boundary. When that
//! consumer exists, the port belongs to the application layer and this
//! controller implements it without changing its behaviour.
//!
//! The controller is stateless. It holds no registry, no process table, and no
//! global state: it starts one process, hands back its handle, and owns nothing
//! afterwards. Session ownership, exclusivity, and persistence belong to the
//! later session lifecycle (ARCHITECTURE.md §22).
//!
//! # RetroArch-agnostic
//!
//! Nothing in this crate knows about RetroArch, emulation, cores, or content.
//! The only input is [`PreparedLaunch`], so the dependency direction stays
//!
//! ```text
//! Emulation  →  PreparedLaunch  →  Platform
//! ```
//!
//! and never becomes `Platform → RetroArch`.

use bitarchive_application::PreparedLaunch;

use crate::process::{SpawnedProcess, command_for};

/// Starts a prepared launch as a process on this platform.
///
/// ```
/// use bitarchive_application::PreparedLaunch;
/// use bitarchive_platform::ProcessController;
///
/// let launch = PreparedLaunch::new("/runtime/RetroArch", Vec::new());
///
/// // Starting the process is the only thing this call does. A launch that
/// // cannot start reports the technical failure of the attempt:
/// let result = ProcessController::new().spawn(&launch);
/// assert!(result.is_err(), "/runtime/RetroArch does not exist here");
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProcessController;

/// The controller carries no state, so the default controller is an ordinary
/// one. It is written out rather than derived so the two constructors cannot
/// drift apart.
impl Default for ProcessController {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessController {
    /// Creates the controller.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Starts `launch` and returns a handle to the running process.
    ///
    /// The prepared launch is mapped onto a process command and started
    /// directly: the executable is the program, the arguments are passed
    /// separately, the environment overrides are added to the inherited
    /// environment, and a prepared working directory is applied. No shell is
    /// involved at any point.
    ///
    /// Nothing is inspected before starting. A launch whose executable does not
    /// exist, whose working directory is missing, or whose arguments name
    /// unavailable paths is *attempted*, and the operating system reports what
    /// went wrong; readiness is decided before a launch is prepared, not here.
    ///
    /// This call returns as soon as the process has started. It does not wait
    /// for the process, and the returned handle does not end it.
    ///
    /// # Errors
    ///
    /// Returns the [`std::io::Error`] that [`Command::spawn`] reported, so a
    /// missing executable, a missing working directory, or a permission problem
    /// arrives as the real technical cause instead of a reclassified summary.
    ///
    /// [`Command::spawn`]: std::process::Command::spawn
    pub fn spawn(&self, launch: &PreparedLaunch) -> std::io::Result<SpawnedProcess> {
        command_for(launch).spawn().map(SpawnedProcess::new)
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;

    /// The marker file name prefix in the system temporary directory.
    ///
    /// The marker is how the environment test reaches a child instance of the
    /// test binary without a shell, and without setting a process-global
    /// environment variable in the parent test process. The parent writes it,
    /// the child reads it and removes it, so a finished run leaves nothing
    /// behind but a removed temporary file.
    const MARKER_PREFIX: &str = "bitarchive-platform-environment-probe";

    /// The test the child instance runs when the marker asks for the
    /// environment probe.
    ///
    /// This is the test harness name of the test, including its module path,
    /// which is the spelling `--exact` matches. The parent asks the child for
    /// exactly one test, so a child that matches nothing runs zero tests and
    /// still succeeds; the parent's marker assertion is what turns that silent
    /// no-op into a visible failure.
    const ENVIRONMENT_PROBE_TEST: &str =
        "process_controller::tests::environment_probe_runs_in_the_child_process";

    /// The argument that makes the test binary start, list its tests, and exit
    /// successfully. It is inert: no test body runs, so nothing is asserted and
    /// no marker is consumed.
    const LIST_TESTS: &str = "--list";

    /// The environment variable the parent prepares as an explicit override and
    /// the child looks for.
    const PROBE_KEY: &str = "BITARCHIVE_PLATFORM_TEST_OVERRIDE";

    /// The environment variable the child uses to prove that the adapter adds
    /// overrides instead of replacing the environment. `PATH` is not touched by
    /// any override in these tests, so the child can only have inherited it.
    const INHERITED_KEY: &str = "PATH";

    /// The path shared by the parent test process and the child instance of the
    /// test binary, tagged with `tag`.
    ///
    /// The name is derived from the test executable rather than from a process
    /// id, because the child is a different process with a different id: the
    /// test binary is the thing both processes know in common. Cargo gives each
    /// test binary a unique name, so a parallel `cargo test` run cannot collide
    /// with this one.
    fn probe_path(tag: &str) -> PathBuf {
        let name = test_binary()
            .file_name()
            .map_or_else(|| OsString::from("bitarchive-platform"), OsString::from);

        let mut path = OsString::from(MARKER_PREFIX);
        path.push("-");
        path.push(tag);
        path.push("-");
        path.push(name);

        env::temp_dir().join(path)
    }

    /// The file the parent writes the expected override into.
    fn request_path() -> PathBuf {
        probe_path("request")
    }

    /// The file the child writes what it actually observed into.
    fn report_path() -> PathBuf {
        probe_path("report")
    }

    /// Removes both probe files.
    fn clear_probe_files() {
        let _ = fs::remove_file(request_path());
        let _ = fs::remove_file(report_path());
    }

    /// The path of the test binary itself.
    ///
    /// Starting the test binary is how these tests exercise a real process
    /// without depending on a shell, an external tool, or an installed
    /// emulator.
    fn test_binary() -> PathBuf {
        env::current_exe().expect("the test binary can be located")
    }

    /// Starts a real, harmless child process through the controller and returns
    /// the started handle.
    ///
    /// The child is asked to list its tests: that starts a real process, needs
    /// no shell or external tool, and exits successfully without touching the
    /// marker the environment probe uses.
    fn spawn_harmless_child() -> SpawnedProcess {
        let launch = PreparedLaunch::new(test_binary(), vec![OsString::from(LIST_TESTS)]);

        ProcessController::new()
            .spawn(&launch)
            .expect("the test binary can be started through the controller")
    }

    /// Starting a process is the one thing the controller does, so this test
    /// starts a real, harmless child process: a second instance of the test
    /// binary. The process id is valid, the process is waited on, and it exits
    /// successfully — so the test leaves no zombie behind.
    #[test]
    fn a_real_child_process_can_be_started_waited_on_and_reaped() {
        let mut process = spawn_harmless_child();

        let id = process.id();
        assert!(id > 0, "the operating system assigned a valid process id");

        let status = process.wait().expect("the child process can be waited on");

        assert!(
            status.is_success(),
            "the child test run must succeed, but it reported {status:?}"
        );
        assert_eq!(
            status.code(),
            Some(0),
            "a successful process exits with code zero"
        );
    }

    /// The controller does not wait for the process: control returns while the
    /// child is still running. A non-blocking check is therefore possible, and
    /// the blocking wait then reports the same status a check may already have
    /// seen.
    #[test]
    fn a_started_process_can_be_checked_and_waited_on() {
        let mut process = spawn_harmless_child();

        let observed = process
            .try_wait()
            .expect("checking a running process must not fail");

        let status = process
            .wait()
            .expect("the child process can be waited on after a check");

        assert!(
            status.is_success(),
            "the child test run must succeed, but it reported {status:?}"
        );

        if let Some(observed) = observed {
            assert_eq!(
                observed, status,
                "a check that already saw the exit status agrees with the wait"
            );
        }

        assert_eq!(
            process.wait().expect("waiting again reports a status"),
            status,
            "a process that has been waited on keeps reporting its status"
        );

        assert_eq!(
            process
                .try_wait()
                .expect("checking a finished process must not fail"),
            Some(status),
            "a finished process stays observable instead of failing"
        );
    }

    /// A launch that cannot start reports the technical failure of the attempt
    /// as an [`std::io::Error`]. Nothing checked the path first — the error is
    /// the operating system's answer to the actual attempt.
    #[test]
    fn a_missing_executable_is_reported_as_an_error() {
        let executable = "/nonexistent/bitarchive/RetroArch-that-does-not-exist";
        assert!(
            !Path::new(executable).exists(),
            "this test needs a path that does not exist"
        );

        let launch = PreparedLaunch::new(executable, Vec::new());

        let result = ProcessController::new().spawn(&launch);

        assert!(
            result.is_err(),
            "starting a process without an executable must fail"
        );
    }

    /// The adapter adds environment overrides instead of replacing the
    /// environment. A real child process receives the explicit override with
    /// exactly the prepared value, and still finds a variable it can only have
    /// inherited.
    ///
    /// The parent never sets an environment variable in its own process. It
    /// writes the expected override value to the marker file, prepares the
    /// override on the child's process, and the child asserts both halves from
    /// the inside.
    #[test]
    fn spawned_processes_inherit_the_parent_environment() {
        let expected_override = format!("override-{}", std::process::id());
        let expected_inherited = env::var_os(INHERITED_KEY);
        let environment = vec![(
            OsString::from(PROBE_KEY),
            OsString::from(expected_override.as_str()),
        )];

        clear_probe_files();
        fs::write(request_path(), expected_override.as_bytes())
            .expect("the probe request can be written");

        let launch =
            PreparedLaunch::new(test_binary(), vec![OsString::from(ENVIRONMENT_PROBE_TEST)])
                .with_environment(environment);

        let mut process = ProcessController::new()
            .spawn(&launch)
            .expect("a launch with environment overrides starts");

        let status = process.wait().expect("the child process can be waited on");

        assert!(
            status.is_success(),
            "the child test run must pass, but it reported {status:?}"
        );
        assert!(
            !request_path().exists(),
            "the child consumes the request, so a request that survived means the child \
             test never ran and its environment assertions were never evaluated"
        );

        // The child reports back exactly what it observed, so the parent can
        // assert the prepared override and the inherited value byte for byte.
        let report = fs::read(report_path()).expect("the child writes a report");
        let _ = fs::remove_file(report_path());

        let half = expected_override.len();
        let (observed_override, observed_inherited) = report.split_at(half);

        assert_eq!(
            observed_override,
            expected_override.as_bytes(),
            "the child must see the prepared override unchanged"
        );
        assert_eq!(
            observed_inherited,
            expected_inherited
                .as_deref()
                .unwrap_or_default()
                .as_encoded_bytes(),
            "{INHERITED_KEY} was set in the parent and nothing overrode it, so the child \
             must have inherited it unchanged"
        );
        assert_eq!(
            expected_inherited,
            env::var_os(INHERITED_KEY),
            "the parent's own environment is untouched by the probe"
        );
    }

    /// Runs in the child instance of the test binary, and asserts from inside
    /// the child that the environment is what the adapter promised: the
    /// explicit override is present with exactly the prepared value, and the
    /// variable the parent had is still inherited.
    ///
    /// The parent instance of this test binary finds no marker and returns
    /// immediately, so this test is inert unless a probe asked for it.
    #[test]
    fn environment_probe_runs_in_the_child_process() {
        let Ok(expected_override) = fs::read(request_path()) else {
            // The parent instance of this test binary: nothing to probe.
            return;
        };

        // Consume the request first so its absence is unambiguous afterwards:
        // the child ran, whatever the assertions below decide. The child does
        // not assert about its own environment here, because the child test's
        // output is captured; it records what it observed and lets the parent
        // assert.
        let _ = fs::remove_file(request_path());

        let observed_override =
            env::var_os(PROBE_KEY).expect("the prepared override must reach the child");
        let observed_inherited = env::var_os(INHERITED_KEY).unwrap_or_default();

        let mut report = observed_override.into_encoded_bytes();
        report.extend_from_slice(observed_inherited.as_encoded_bytes());

        // The report starts with the expected value, so the parent only has to
        // read it back; the inherited value follows it.
        assert!(
            report.starts_with(&expected_override),
            "the child must report at least the expected value"
        );

        fs::write(report_path(), &report).expect("the probe report can be written");
    }

    /// Dropping the handle neither ends the process nor blocks on it.
    ///
    /// No `Drop` implementation exists, so dropping a handle releases only the
    /// handle. A process that was started keeps running, which is what the
    /// product needs when the user closes BitArchive while a game runs
    /// (PRODUCT.md §27.5).
    ///
    /// The test cannot observe the first child's status, because dropping the
    /// handle is exactly what gives up that ability. It proves the two things
    /// that matter: dropping returns without blocking, and a process can still
    /// be started, observed, and reaped afterwards, so the adapter holds no
    /// state that a dropped handle could have corrupted.
    #[test]
    fn dropping_a_handle_does_not_end_or_wait_on_the_process() {
        let process = spawn_harmless_child();

        assert!(process.id() > 0);

        drop(process);

        let mut after = ProcessController::new()
            .spawn(&PreparedLaunch::new(
                test_binary(),
                vec![OsString::from(LIST_TESTS)],
            ))
            .expect("a process can still be started after a handle was dropped");

        let status = after.wait().expect("the later child can be waited on");

        assert!(status.is_success());
    }

    /// The controller is a stateless value: it can be created in a `const`
    /// context, copied, compared, and defaulted, and it holds no registry that
    /// two controllers could disagree about.
    #[test]
    fn the_controller_is_stateless() {
        const CONTROLLER: ProcessController = ProcessController::new();

        /// Returns `T::default()`, so the assertion below exercises the
        /// [`Default`] implementation instead of constructing the unit struct
        /// directly.
        fn defaulted<T: Default>() -> T {
            T::default()
        }

        assert_eq!(CONTROLLER, defaulted::<ProcessController>());
        assert_eq!(CONTROLLER, ProcessController);
    }

    /// Repeated environment keys are not validated by the adapter: the
    /// standard library decides which one wins, and starting the process is not
    /// refused because of it.
    #[test]
    fn repeated_environment_keys_are_not_validated() {
        let environment = vec![
            (OsString::from(PROBE_KEY), OsString::from("first")),
            (OsString::from(PROBE_KEY), OsString::from("second")),
        ];

        let launch = PreparedLaunch::new(test_binary(), vec![OsString::from(LIST_TESTS)])
            .with_environment(environment);

        let mut process = ProcessController::new()
            .spawn(&launch)
            .expect("a repeated environment key is not an adapter error");

        let status = process.wait().expect("the child process can be waited on");

        assert!(status.is_success());
    }
}
