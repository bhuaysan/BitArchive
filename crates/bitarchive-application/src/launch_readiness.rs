//! The structured result of a launch readiness check.

/// A structured reason why a launch cannot proceed.
///
/// The categories mirror the readiness model in ARCHITECTURE.md §21. They are
/// data, not presentation: no user-facing text, no localization, and no
/// recovery action. A UI layer decides how to phrase an issue and which action
/// to offer.
///
/// Checking readiness belongs to the steps that produce these issues. This
/// contract only names them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ReadinessIssue {
    /// The launch needs a core, but none is configured for any scope.
    CoreMissing,
    /// A core is configured, but its integrity check failed.
    CoreIntegrityFailure,
    /// No usable emulation runtime is available.
    RuntimeUnavailable,

    /// A firmware file the core requires is missing.
    FirmwareMissing,
    /// A firmware file exists but has unexpected content.
    FirmwareWrongContent,
    /// A firmware file exists with unexpected content, but the core selects
    /// firmware by file name.
    FirmwareWrongFilename,

    /// The content to launch is not available.
    ContentUnavailable,
    /// The library source holding the content is offline.
    SourceOffline,
    /// The library source holding the content cannot be read due to missing
    /// permission.
    SourcePermissionDenied,

    /// The content is in a format that cannot be launched.
    UnsupportedFormat,
    /// The content is present but not valid.
    InvalidContent,

    /// Another emulation session is already active.
    ///
    /// BitArchive allows exactly one active session (ARCHITECTURE.md §22).
    SessionActive,
}

/// Whether a launch may proceed.
///
/// The two states are unambiguous: [`Ready`](Self::Ready) means nothing blocks
/// the launch, [`Blocked`](Self::Blocked) carries every issue that does. A
/// blocked result always names at least one issue, so "blocked for no reason"
/// cannot be expressed.
///
/// ```text
/// Ready
/// Blocked [CoreMissing, SourceOffline]
/// ```
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum LaunchReadiness {
    /// The launch may proceed.
    Ready,
    /// The launch is blocked by at least one issue.
    Blocked(Vec<ReadinessIssue>),
}

impl LaunchReadiness {
    /// Returns a ready result.
    #[must_use]
    pub const fn ready() -> Self {
        Self::Ready
    }

    /// Returns a result for the given blocking `issues`.
    ///
    /// An empty list means nothing blocks the launch, so the result is
    /// [`Ready`](Self::Ready).
    #[must_use]
    pub fn from_issues(issues: impl IntoIterator<Item = ReadinessIssue>) -> Self {
        let issues: Vec<ReadinessIssue> = issues.into_iter().collect();

        if issues.is_empty() {
            Self::Ready
        } else {
            Self::Blocked(issues)
        }
    }

    /// Returns whether the launch may proceed.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Returns every issue that blocks the launch.
    ///
    /// Empty when the launch is ready.
    #[must_use]
    pub fn blocking_issues(&self) -> &[ReadinessIssue] {
        match self {
            Self::Ready => &[],
            Self::Blocked(issues) => issues,
        }
    }
}

impl From<Vec<ReadinessIssue>> for LaunchReadiness {
    /// Builds a readiness result from collected issues, matching
    /// [`LaunchReadiness::from_issues`].
    fn from(issues: Vec<ReadinessIssue>) -> Self {
        Self::from_issues(issues)
    }
}

impl FromIterator<ReadinessIssue> for LaunchReadiness {
    /// Collects blocking issues into a readiness result, matching
    /// [`LaunchReadiness::from_issues`].
    fn from_iter<I: IntoIterator<Item = ReadinessIssue>>(issues: I) -> Self {
        Self::from_issues(issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ready is representable on its own and reports that the launch may
    /// proceed.
    #[test]
    fn ready_means_the_launch_may_proceed() {
        let ready = LaunchReadiness::ready();

        assert!(ready.is_ready());
        assert!(
            ready.blocking_issues().is_empty(),
            "a ready launch has no blocking issues"
        );
    }

    /// A single blocking issue is represented structurally, including which
    /// issue blocks the launch.
    #[test]
    fn a_single_blocking_issue_is_representable() {
        let blocked = LaunchReadiness::from_issues([ReadinessIssue::CoreMissing]);

        assert!(!blocked.is_ready());
        assert_eq!(blocked.blocking_issues(), [ReadinessIssue::CoreMissing]);
    }

    /// Several blocking issues are all represented, so a readiness check can
    /// report everything that is wrong instead of only the first problem.
    #[test]
    fn several_blocking_issues_are_representable() {
        let issues = [
            ReadinessIssue::SessionActive,
            ReadinessIssue::SourceOffline,
            ReadinessIssue::FirmwareWrongFilename,
        ];

        let blocked = LaunchReadiness::from_issues(issues);

        assert!(!blocked.is_ready());
        assert_eq!(blocked.blocking_issues(), issues);
    }

    /// No collected issues blocks nothing, so the launch is ready rather than
    /// blocked without a reason.
    #[test]
    fn collecting_no_issues_is_ready() {
        let nothing_blocks: [ReadinessIssue; 0] = [];

        let readiness = LaunchReadiness::from_issues(nothing_blocks);

        assert!(readiness.is_ready());
        assert!(readiness.blocking_issues().is_empty());
    }

    /// Issues distinguish the categories they name and never collapse into one
    /// generic failure.
    #[test]
    fn distinct_categories_stay_distinguishable() {
        let missing = LaunchReadiness::from_issues([ReadinessIssue::CoreMissing]);
        let integrity = LaunchReadiness::from_issues([ReadinessIssue::CoreIntegrityFailure]);

        assert_ne!(missing, integrity);
        assert_ne!(
            LaunchReadiness::ready(),
            LaunchReadiness::from_issues([ReadinessIssue::CoreMissing]),
            "ready and blocked are different states"
        );
    }
}
