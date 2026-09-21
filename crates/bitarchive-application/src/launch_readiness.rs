//! The structured result of a launch readiness check.

/// A structured reason why a launch cannot proceed.
///
/// The categories mirror the readiness model in ARCHITECTURE.md §21, extended by the
/// one category that model does not name and a launch decision genuinely needs:
/// [`InvalidConfiguration`](ReadinessIssue::InvalidConfiguration). They are data, not
/// presentation: no user-facing text, no localization, and no recovery action. A UI
/// layer decides how to phrase an issue and which action to offer.
///
/// Checking readiness belongs to the steps that produce these issues. This contract
/// only names them.
///
/// # Categories are not interchangeable
///
/// Every variant names one condition, and a condition is never reported as a
/// neighbouring one: an unusable stored configuration key is not invalid content, and
/// a system BitArchive curates no core for is not a broken installation. A category is
/// only interchangeable with another when the *repair* is the same, which is what makes
/// a readiness result usable by a UI.
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
    /// A firmware file exists, but its content or hash is wrong or was not
    /// recognized.
    FirmwareWrongContent,
    /// A firmware file with recognized, correct content exists, but its file
    /// name does not match the name the core expects.
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

    /// A stored configuration override cannot be used.
    ///
    /// The stored value is preserved rather than deleted (ARCHITECTURE.md invariant
    /// 19); this category only reports that it could not take part in the effective
    /// launch configuration. It is deliberately **not**
    /// [`InvalidContent`](Self::InvalidContent): the content of the launch is not what
    /// is wrong, and a UI would offer the wrong action for it.
    ///
    /// ARCHITECTURE.md §21 does not name this category. It is added here because the
    /// configuration hierarchy is part of the launch path and a stored override can
    /// genuinely be unusable, which the model has to be able to say.
    InvalidConfiguration,

    /// Another emulation session is already active.
    ///
    /// BitArchive allows exactly one active session (ARCHITECTURE.md §22).
    SessionActive,
}

/// At least one [`ReadinessIssue`].
///
/// The field is private and there is no public constructor, so a value of this
/// type always holds at least one issue. Only this module can create one, and
/// only from a non-empty collection, which is what makes the blocked state
/// below enforceable instead of merely documented.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct BlockingIssues(Vec<ReadinessIssue>);

impl BlockingIssues {
    /// Wraps `issues`, or returns [`None`] when nothing would be blocked.
    ///
    /// The returned value is the only evidence that a launch is blocked, so an
    /// empty issue list can never turn into a blocked result.
    #[must_use]
    fn from_issues(issues: Vec<ReadinessIssue>) -> Option<Self> {
        if issues.is_empty() {
            None
        } else {
            Some(Self(issues))
        }
    }

    /// Returns the issues as a slice.
    fn as_slice(&self) -> &[ReadinessIssue] {
        &self.0
    }
}

/// Whether a launch may proceed.
///
/// The two states are unambiguous: a result is either ready — nothing blocks
/// the launch — or blocked, carrying every issue that does.
///
/// The state itself is private on purpose. A blocked result always names at
/// least one issue, because it can only be produced from a non-empty
/// collection, so "blocked for no reason" has no constructor and is not
/// expressible through this API:
///
/// ```text
/// Ready
/// Blocked [CoreMissing, SourceOffline]
/// ```
///
/// Create a result with [`LaunchReadiness::ready`] or
/// [`LaunchReadiness::from_issues`], and inspect it with
/// [`LaunchReadiness::is_ready`] and [`LaunchReadiness::blocking_issues`].
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct LaunchReadiness(Readiness);

/// The private readiness state.
///
/// Kept separate from [`LaunchReadiness`] so that the
/// [`Blocked`](Readiness::Blocked) variant can hold evidence that only this
/// module can produce, and no caller can build an empty one.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Readiness {
    /// The launch may proceed.
    Ready,
    /// The launch is blocked by at least one issue.
    Blocked(BlockingIssues),
}

impl LaunchReadiness {
    /// Returns a ready result.
    #[must_use]
    pub const fn ready() -> Self {
        Self(Readiness::Ready)
    }

    /// Returns a result for the given blocking `issues`.
    ///
    /// An empty list means nothing blocks the launch, so the result is ready. A
    /// non-empty list always produces a blocked result.
    #[must_use]
    pub fn from_issues(issues: impl IntoIterator<Item = ReadinessIssue>) -> Self {
        let issues: Vec<ReadinessIssue> = issues.into_iter().collect();

        match BlockingIssues::from_issues(issues) {
            Some(blocking) => Self(Readiness::Blocked(blocking)),
            None => Self(Readiness::Ready),
        }
    }

    /// Returns whether the launch may proceed.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self.0, Readiness::Ready)
    }

    /// Returns every issue that blocks the launch, or an empty slice when the
    /// launch is ready.
    #[must_use]
    pub fn blocking_issues(&self) -> &[ReadinessIssue] {
        match &self.0 {
            Readiness::Ready => &[],
            Readiness::Blocked(issues) => issues.as_slice(),
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

    /// A blocked result always names at least one issue: the blocked state is
    /// private, holds owner-only evidence, and is only ever built from a
    /// non-empty collection, so "blocked for no reason" has no public
    /// constructor at all — not even `LaunchReadiness::Blocked(Vec::new())`.
    ///
    /// The type system enforces that half of the invariant and this test pins
    /// the observable half for every reachable issue count.
    #[test]
    fn a_blocked_result_always_names_at_least_one_issue() {
        let issue_counts = [1, 2, 13];
        let all_issues = [
            ReadinessIssue::CoreMissing,
            ReadinessIssue::CoreIntegrityFailure,
            ReadinessIssue::RuntimeUnavailable,
            ReadinessIssue::FirmwareMissing,
            ReadinessIssue::FirmwareWrongContent,
            ReadinessIssue::FirmwareWrongFilename,
            ReadinessIssue::ContentUnavailable,
            ReadinessIssue::SourceOffline,
            ReadinessIssue::SourcePermissionDenied,
            ReadinessIssue::UnsupportedFormat,
            ReadinessIssue::InvalidContent,
            ReadinessIssue::InvalidConfiguration,
            ReadinessIssue::SessionActive,
        ];

        for count in issue_counts {
            let blocked = LaunchReadiness::from_issues(all_issues.into_iter().take(count));

            assert!(!blocked.is_ready(), "{count} issues must block the launch");
            assert_eq!(
                blocked.blocking_issues().len(),
                count,
                "every blocking issue must be reported"
            );
            assert!(
                !blocked.blocking_issues().is_empty(),
                "a blocked result must name at least one issue"
            );
        }
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
