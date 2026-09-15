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
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    /// The test the child instance runs when the probe activates it.
    ///
    /// This is the test harness name of the test, including its module path,
    /// which is the spelling `--exact` matches.
    const ENVIRONMENT_PROBE_TEST: &str =
        "process_controller::tests::environment_probe_runs_in_the_child_process";

    /// The directory name prefix for one probe's request and report files.
    const PROBE_DIRECTORY_PREFIX: &str = "bitarchive-platform-environment-probe";

    /// The argument that makes the test binary start, list its tests, and exit
    /// successfully. It is inert: no test body runs, so nothing is asserted and
    /// no probe is consumed.
    const LIST_TESTS: &str = "--list";

    /// The environment variable that marks a process as the probe child.
    ///
    /// It is set only on the child's prepared launch, so the parent instance of
    /// the probe test can never mistake itself for the child and consume a
    /// request that belongs to it.
    const PROBE_ACTIVATION_KEY: &str = "BITARCHIVE_PLATFORM_TEST_PROBE_ACTIVE";

    /// The environment variable carrying the request file the child must read.
    const PROBE_REQUEST_KEY: &str = "BITARCHIVE_PLATFORM_TEST_REQUEST_PATH";

    /// The environment variable carrying the report file the child must write.
    const PROBE_REPORT_KEY: &str = "BITARCHIVE_PLATFORM_TEST_REPORT_PATH";

    /// The environment variable the parent prepares as an explicit override and
    /// the child looks for. This is the variable under test: it must reach the
    /// child through the adapter, not through the parent's own environment.
    const PROBE_KEY: &str = "BITARCHIVE_PLATFORM_TEST_OVERRIDE";

    /// The environment variable the child uses to prove that the adapter adds
    /// overrides instead of replacing the environment. `PATH` is not touched by
    /// any override in these tests, so the child can only have inherited it.
    const INHERITED_KEY: &str = "PATH";

    /// Returns a value that differs between two probes.
    ///
    /// Process id plus wall clock is enough to keep two concurrent runs of this
    /// test binary, and two sequential runs of the same test, from sharing a
    /// probe directory.
    fn probe_nonce() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());

        format!("{}-{nanos}", std::process::id())
    }

    /// The directory holding one probe's request and report files.
    ///
    /// The nonce makes the directory unique per probe run, so a parallel test or
    /// a parallel `cargo test` invocation cannot read or overwrite this probe's
    /// files. The directory is created when the request is written.
    fn probe_directory(nonce: &str) -> PathBuf {
        env::temp_dir().join(format!("{PROBE_DIRECTORY_PREFIX}-{nonce}"))
    }

    /// Removes a probe's directory and everything in it.
    ///
    /// Cleanup is best effort: the directory is uniquely owned by this probe, so
    /// a failure here can only leave a temporary file behind, never affect the
    /// test result.
    fn clear_probe_directory(directory: &Path) {
        let _ = fs::remove_dir_all(directory);
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
    /// no shell or external tool, and exits successfully without touching any
    /// probe file. Every caller waits for the handle.
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
    /// The parent never sets an environment variable in its own process: every
    /// probe variable is prepared on the child's launch only. The child is
    /// activated explicitly through [`PROBE_ACTIVATION_KEY`], so this test's own
    /// instance in the parent harness stays inert and cannot race the child for
    /// the request file.
    #[test]
    fn spawned_processes_inherit_the_parent_environment() {
        let nonce = probe_nonce();
        let directory = probe_directory(&nonce);
        let request_path = directory.join("request");
        let report_path = directory.join("report");

        // A nonce in the value under test as well: the child compares the value
        // it received against this one, so no earlier run can satisfy it.
        let expected_override = format!("override-{nonce}");
        let expected_inherited = env::var_os(INHERITED_KEY);
        let activation = format!("probe-{nonce}");

        let environment = vec![
            // The variable under test: it must reach the child through the
            // adapter's environment mapping, not through the parent's own
            // environment.
            (
                OsString::from(PROBE_KEY),
                OsString::from(expected_override.as_str()),
            ),
            // The probe handshake. Only the child sees these, and only the
            // child acts on them.
            (
                OsString::from(PROBE_ACTIVATION_KEY),
                OsString::from(activation.as_str()),
            ),
            (
                OsString::from(PROBE_REQUEST_KEY),
                request_path.as_os_str().to_owned(),
            ),
            (
                OsString::from(PROBE_REPORT_KEY),
                report_path.as_os_str().to_owned(),
            ),
        ];

        // The request carries the activation nonce on its first line and the
        // value the override must deliver on its second, so the child can prove
        // it was activated for this run and that nothing rewrote the value.
        let request = format!("{activation}\n{expected_override}");

        clear_probe_directory(&directory);
        fs::create_dir_all(&directory).expect("the probe directory can be created");
        fs::write(&request_path, request.as_bytes()).expect("the probe request can be written");
        assert!(
            request_path.exists() && !report_path.exists(),
            "this run starts from a clean request and no report, so neither a stale request \
             nor a stale report can be mistaken for this probe's evidence"
        );

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
            !request_path.exists(),
            "the child consumes the request, so a request that survived means the child \
             never ran and its environment assertions were never evaluated"
        );

        // The report is the second piece of evidence, and the stronger one: it
        // carries the nonce of this probe run, so it can only have been written
        // by a child that ran the activated test during this run. A filter that
        // matched nothing would leave it missing.
        let report = fs::read(&report_path)
            .expect("the child writes a report; a missing report means the child probe never ran");
        clear_probe_directory(&directory);
        assert!(
            !directory.exists(),
            "the probe artifacts are cleaned up by the run that created them"
        );

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

    /// Runs only in the process the parent explicitly activated, and reports
    /// from inside that child what the adapter actually delivered: the prepared
    /// override, and the variable that can only have been inherited.
    ///
    /// The activation variable is set on the child's launch and nowhere else, so
    /// this test returns immediately in every other process, including this
    /// test's own instance in the parent harness. That is what makes the probe
    /// child-only: no other instance can read the request file and consume it
    /// while the child is starting.
    ///
    /// The child does not assert about its own environment, because its output
    /// is captured by its own harness. It records what it observed and lets the
    /// parent assert; the request file it must read first is what keeps that
    /// from being circular, since the parent wrote the expected value before
    /// starting it.
    #[test]
    fn environment_probe_runs_in_the_child_process() {
        if env::var_os(PROBE_ACTIVATION_KEY).is_none() {
            // Not the activated child: this test is inert here.
            return;
        }

        let request_path = PathBuf::from(
            env::var_os(PROBE_REQUEST_KEY).expect("the activated child names its request file"),
        );
        let report_path = PathBuf::from(
            env::var_os(PROBE_REPORT_KEY).expect("the activated child names its report file"),
        );

        let request =
            fs::read_to_string(&request_path).expect("the activated child finds its request file");
        let mut lines = request.lines();
        let expected_activation = lines.next().unwrap_or_default();
        let expected_override = lines.next().unwrap_or_default().to_owned();

        // The activation variable must carry this run's nonce, so this child
        // cannot be confused with an instance activated by an older run.
        assert_eq!(
            env::var_os(PROBE_ACTIVATION_KEY).unwrap_or_default(),
            OsString::from(expected_activation),
            "the child must be activated for exactly this probe run"
        );

        // Consume the request so its absence is unambiguous afterwards: the
        // child ran, whatever the parent concludes from the report.
        let _ = fs::remove_file(&request_path);

        let observed_override =
            env::var_os(PROBE_KEY).expect("the prepared override must reach the child");
        let observed_inherited = env::var_os(INHERITED_KEY).unwrap_or_default();

        let mut report = observed_override.into_encoded_bytes();
        report.extend_from_slice(observed_inherited.as_encoded_bytes());

        // The report starts with the value the parent prepared before starting
        // this process, so the parent can read it back byte for byte; the
        // inherited value follows it.
        assert!(
            report.starts_with(expected_override.as_bytes()),
            "the child must observe exactly the override the parent prepared"
        );

        fs::write(&report_path, &report).expect("the probe report can be written");
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
