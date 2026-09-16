//! The platform-owned result of a finished process.
//!
//! [`ProcessExitStatus`] is the small value type a caller outside
//! [`crate::process`] sees when a process has ended. It deliberately does not
//! leak [`std::process::ExitStatus`]: that type carries a platform wait status
//! whose interpretation differs per operating system, while BitArchive needs
//! the two facts a session later records (PRODUCT.md §26):
//!
//! ```text
//! success: bool
//! code:    Option<i32>
//! ```
//!
//! Signal numbers, core-dump flags, and stopped/continued states are
//! deliberately not represented. Graceful termination and forced termination
//! belong to the later session lifecycle, not to the adapter that starts and
//! observes a process.

/// How a process ended.
///
/// A value is created by the process adapter from the status the operating
/// system reported, and is read-only from the outside. Both facts are derived
/// from the same status, so they never contradict each other: a success is a
/// zero exit code, and a successful process always has `code == Some(0)`.
///
/// A [`ProcessController`](crate::ProcessController) produces the value when it
/// waits for a process:
///
/// ```no_run
/// use bitarchive_application::PreparedLaunch;
/// use bitarchive_platform::ProcessController;
///
/// let launch = PreparedLaunch::new("/runtime/RetroArch", Vec::new());
/// let mut process = ProcessController::new().spawn(&launch)?;
///
/// let status = process.wait()?;
///
/// assert!(status.is_success() || status.code().is_some());
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProcessExitStatus {
    success: bool,
    code: Option<i32>,
}

impl ProcessExitStatus {
    /// Creates a status from the facts the operating system reported.
    ///
    /// Only the process adapter inside this crate constructs a
    /// [`ProcessExitStatus`], so the value always describes a real process and
    /// cannot be invented by a caller.
    pub(crate) const fn new(success: bool, code: Option<i32>) -> Self {
        Self { success, code }
    }

    /// Returns whether the process terminated successfully.
    ///
    /// Success means a zero exit code. A process that was terminated by a
    /// signal is never a success.
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.success
    }

    /// Returns the exit code the process ended with, if it ended with one.
    ///
    /// [`None`] means the operating system did not report an exit code, for
    /// example because the process was terminated by a signal instead of
    /// calling `exit`.
    #[must_use]
    pub const fn code(self) -> Option<i32> {
        self.code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A successful process reports success and the zero exit code, so the two
    /// facts a caller reads agree with each other.
    #[test]
    fn a_successful_status_reports_success_and_code_zero() {
        let status = ProcessExitStatus::new(true, Some(0));

        assert!(status.is_success());
        assert_eq!(status.code(), Some(0));
    }

    /// A process that ended with a non-zero code reports failure together with
    /// the code that caused it.
    #[test]
    fn a_failing_status_reports_the_failing_code() {
        let status = ProcessExitStatus::new(false, Some(3));

        assert!(!status.is_success());
        assert_eq!(status.code(), Some(3));
    }

    /// A process terminated by the operating system instead of exiting has no
    /// exit code, and the absence of a code is representable without inventing
    /// one.
    #[test]
    fn a_status_without_an_exit_code_is_representable() {
        let status = ProcessExitStatus::new(false, None);

        assert!(!status.is_success());
        assert_eq!(status.code(), None, "no exit code is not an exit code");
    }

    /// The status is a value: it can be copied, compared, and used as a key,
    /// which keeps the adapter's result cheap to pass on.
    #[test]
    fn the_status_is_a_comparable_value() {
        let status = ProcessExitStatus::new(true, Some(0));

        assert_eq!(status, status);
        assert_ne!(status, ProcessExitStatus::new(false, Some(1)));
    }
}
