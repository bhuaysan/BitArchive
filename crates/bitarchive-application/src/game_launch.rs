//! Game launch readiness and launch preparation.
//!
//! This module answers the product question that stands in front of every play
//! action:
//!
//! > Can this concrete game be started with the currently configured and installed
//! > BitArchive state, and which concrete launch inputs follow from that?
//!
//! ```text
//! GameLaunchRequest
//!         ↓
//! system of the content            (curated system list)
//!         ↓
//! curated core + its installation  (managed core store, through a port)
//!         ↓
//! managed runtime                  (managed RetroArch runtime, through a port)
//!         ↓
//! firmware readiness               (core requirements vs. available files)
//!         ↓
//! effective launch configuration   (Global → System → Game)
//!         ↓
//! PreparedGameLaunch  or  LaunchBlocker[]
//! ```
//!
//! # It decides and prepares, and it starts nothing
//!
//! The end of this module is [`PreparedGameLaunch`]: a description of a launch
//! whose every input is resolved. It is deliberately *not* a process launch.
//! Starting the prepared process is the step after this one (Issue #23 proved that
//! step end to end), and a later product play flow joins the two. Nothing here
//! reaches for [`std::process`], opens a file, performs network I/O, or generates a
//! RetroArch configuration file.
//!
//! # Readiness is a value
//!
//! A blocked launch is not an error and not an exception: it is a
//! [`LaunchPreparation::Blocked`] with at least one [`LaunchBlocker`]. "Blocked for
//! no reason" is not representable either — a blocked result only exists when the
//! negotiation found a reason — and a caller can therefore render an explanation
//! without a fallback for the impossible case.
//!
//! A [`LaunchBlocker`] is data. It carries no user-facing text, no localization, and
//! no recovery action; deciding how to phrase it and which action to offer belongs
//! to the presentation layer (ARCHITECTURE.md §21).
//!
//! # Every blocker is determined from state that really exists
//!
//! The set of blockers is a strict subset of the readiness categories
//! [`ReadinessIssue`](crate::ReadinessIssue) names, because a negotiation may only
//! report what it can actually observe. Source availability, permission problems,
//! content format validation, and save-state compatibility need a library index, a
//! filesystem source model, and a save-state resolver, none of which exists yet;
//! this module does not claim to have checked them.
//!
//! # Nothing is acquired or repaired
//!
//! A missing runtime, a missing core, and missing firmware are reported, never
//! resolved. Acquisition is an explicit step (ADR 0001, ADR 0002), and BitArchive
//! never obtains firmware on its own (ARCHITECTURE.md §25.3).

use std::path::{Path, PathBuf};

use bitarchive_domain::config::{EffectiveLaunchConfig, ScopedConfig, resolve_launch_config};
use bitarchive_domain::system::{EmulatedSystem, EmulatedSystemCatalog, SystemCore};
use bitarchive_domain::{
    CoreDefinition, FirmwareRequirement, FirmwareRequirementLevel, GameId, ReleaseId,
};

use crate::launch_readiness::LaunchReadiness;
use crate::launch_state::{
    CoreUnusableReason, FirmwareChecker, FirmwareOutcome, InstalledCore, LaunchRuntime,
    LaunchRuntimeResolution, SystemCoreState,
};

/// A request to prepare a launch of one concrete game.
///
/// The request names the game and the content to start. It carries no core, no
/// runtime, no configuration, and no firmware: those are the results of the
/// resolution steps that run *after* a request exists (ARCHITECTURE.md §20.1).
///
/// # Why the content path is part of the request
///
/// A game is not a file: the same game can have several releases, and a release can
/// have several contents (PRODUCT.md §9). Which content a play action starts is
/// therefore a decision that has to be made *before* this module can prepare
/// anything, and the resolver that makes it — index, default release, multi-disc
/// handling — needs the library, which does not exist yet.
///
/// Until it does, the caller supplies the content it wants launched, and the request
/// is the honest shape of that: an identified game plus the content to start. When
/// content resolution exists, it produces this value instead of a caller.
///
/// # Why there is no release
///
/// [`ReleaseId`] identifies the release a prepared launch belongs to, and a prepared
/// launch carries one. Nothing can *derive* it yet: a release is an indexed entity,
/// and no store exists that maps a game to its releases. The application contract
/// therefore takes the release as an input ([`GameLaunchContext::release_id`])
/// rather than inventing one, because a fabricated identity would be worse than an
/// absent one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GameLaunchRequest {
    game_id: GameId,
    content: PathBuf,
}

impl GameLaunchRequest {
    /// Creates a request to prepare a launch of `content` for `game_id`.
    #[must_use]
    pub fn new(game_id: GameId, content: impl Into<PathBuf>) -> Self {
        Self {
            game_id,
            content: content.into(),
        }
    }

    /// Returns the game this request is about.
    #[must_use]
    pub const fn game_id(&self) -> GameId {
        self.game_id
    }

    /// Returns the content to start.
    #[must_use]
    pub fn content(&self) -> &Path {
        &self.content
    }
}

/// The state a launch negotiation reads, gathered by the caller.
///
/// Every field is a fact an outer layer observed. Nothing here is derived by this
/// module, and nothing here can be filled in by guessing: a caller that cannot
/// answer a question passes the state that says so — `content_available: false`, a
/// [`LaunchRuntimeResolution::Missing`], a [`SystemCoreState`] without an
/// installation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GameLaunchContext<'a> {
    /// The curated system list this build ships.
    pub systems: &'a EmulatedSystemCatalog,
    /// The platform the curated core definition is resolved for.
    pub platform: bitarchive_domain::CorePlatform,
    /// The release the launch belongs to.
    ///
    /// Supplied by the caller; see [`GameLaunchRequest`] for why nothing derives it
    /// yet.
    pub release_id: ReleaseId,
    /// Whether the content to start is available.
    pub content_available: bool,
    /// The runtime resolution.
    pub runtime: LaunchRuntimeResolution,
    /// The state of the system's curated core; [`None`] when no system could be
    /// resolved for the content, so no core was asked for.
    ///
    /// Ignored when no system was resolved: without a system there is nothing to
    /// resolve a core *for*.
    pub core: Option<SystemCoreState>,
    /// The applicable configurations, from the least to the most specific scope.
    ///
    /// The caller passes the scopes that apply to this concrete launch — the global
    /// values, the values of the game's *system*, and the values of this game — in
    /// the order `Global`, `System`, `Game`, because that is the order the
    /// precedence is applied in.
    pub config: Vec<ScopedConfig>,
}

/// Why a launch cannot proceed.
///
/// Each variant names a condition a negotiation observed, and each is repairable in
/// a different way: install the runtime, install the core, provide content, use a
/// system BitArchive has a core for, provide the firmware, or fix the stored
/// configuration.
///
/// The variants carry what was observed — an identity, a platform, a list of file
/// names — so a caller can name the component or the file instead of reporting a
/// generic failure. They carry no severity, no ordering, and no advice: reporting
/// *what* is wrong is this layer's job, and deciding what to do about it is not.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LaunchBlocker {
    /// No managed runtime is installed and activated.
    ///
    /// Repairing this means acquiring the pinned runtime; no `PATH` lookup, no
    /// `/Applications` scan, and no system RetroArch is consulted instead
    /// (PRODUCT.md §16.1, invariant 10).
    RuntimeMissing {
        /// The runtime identity that is not installed.
        id: String,
    },
    /// The content's system has no curated core definition on this platform.
    ///
    /// BitArchive runs only cores it curates, so this is not repairable by
    /// installing something: either the content targets a system BitArchive does not
    /// curate, or this platform has no reviewed artifact for it.
    UnsupportedSystemOrCore {
        /// The system of the content, when a system could be resolved at all.
        system: Option<String>,
        /// Why no curated definition was available.
        reason: String,
    },
    /// A curated core definition exists, but no usable build is installed.
    ///
    /// [`NotInstalled`](CoreUnusableReason::NotInstalled) is repaired by installing
    /// the core; [`Unusable`](CoreUnusableReason::Unusable) is a broken installation
    /// that needs diagnosis, and installing the same build again is not the answer,
    /// because the store refuses a build it already has.
    CoreUnusable {
        /// The curated component identity.
        component_id: String,
        /// The reviewed build identity the definition pins.
        build_id: String,
        /// Why the installation is not usable.
        reason: CoreUnusableReason,
    },
    /// The content to start is not available.
    ContentMissing,
    /// A firmware file the core requires is missing.
    ///
    /// Only a [`Required`](FirmwareRequirementLevel::Required) requirement can
    /// produce this blocker. An absent
    /// [optional](FirmwareRequirementLevel::Optional) firmware file is recorded in
    /// the preparation result and never blocks a launch.
    FirmwareMissing {
        /// The file names the core requires and that are not available.
        filenames: Vec<String>,
    },
    /// A stored configuration override is not usable.
    ///
    /// The offending value is preserved rather than deleted (invariant 19); this
    /// blocker only reports that it cannot take part in the effective
    /// configuration.
    InvalidConfiguration {
        /// The stored key that could not be used.
        key: String,
    },
}

/// The outcome of preparing a game launch.
///
/// Exactly one of the two variants holds, so "the launch is blocked" and "the launch
/// is prepared" cannot contradict each other. A [`Blocked`](Self::Blocked) always
/// names at least one reason, because the negotiation only produces one when it
/// found a reason.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LaunchPreparation {
    /// The launch is blocked by at least one reason.
    Blocked {
        /// Everything that blocks the launch, in a deterministic order.
        blockers: Vec<LaunchBlocker>,
    },
    /// The launch is ready and every input is resolved.
    Prepared(Box<PreparedGameLaunch>),
}

impl LaunchPreparation {
    /// Returns the prepared launch, if the launch is ready.
    #[must_use]
    pub fn prepared(&self) -> Option<&PreparedGameLaunch> {
        match self {
            Self::Prepared(prepared) => Some(prepared),
            Self::Blocked { .. } => None,
        }
    }

    /// Returns what blocks the launch, or an empty slice when it is ready.
    #[must_use]
    pub fn blockers(&self) -> &[LaunchBlocker] {
        match self {
            Self::Prepared(_) => &[],
            Self::Blocked { blockers } => blockers,
        }
    }

    /// Returns whether the launch may proceed.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Prepared(_))
    }

    /// Returns the readiness result of this preparation.
    ///
    /// The result is derived from the preparation itself rather than stored next to
    /// it, so a readiness that disagrees with the preparation is not representable.
    #[must_use]
    pub fn readiness(&self) -> LaunchReadiness {
        LaunchReadiness::from_issues(self.blockers().iter().map(LaunchBlocker::issue))
    }
}

impl LaunchBlocker {
    /// Returns the readiness category this blocker belongs to.
    ///
    /// The mapping is total, and several blockers may share one category: a
    /// readiness category is a coarse answer for a UI, while the blocker keeps the
    /// detail a caller needs to act.
    #[must_use]
    pub fn issue(&self) -> crate::launch_readiness::ReadinessIssue {
        use crate::launch_readiness::ReadinessIssue;

        match self {
            Self::RuntimeMissing { .. } => ReadinessIssue::RuntimeUnavailable,
            // A system or a core combination BitArchive cannot run is a format
            // question from the product's point of view: this content cannot be
            // launched here, however intact it is.
            Self::UnsupportedSystemOrCore { .. } => ReadinessIssue::UnsupportedFormat,
            Self::CoreUnusable { .. } => ReadinessIssue::CoreMissing,
            Self::ContentMissing => ReadinessIssue::ContentUnavailable,
            Self::FirmwareMissing { .. } => ReadinessIssue::FirmwareMissing,
            Self::InvalidConfiguration { .. } => ReadinessIssue::InvalidContent,
        }
    }
}

/// Everything a later play flow needs to start this launch through the existing
/// launch path.
///
/// ```text
/// PreparedGameLaunch
///         ↓
/// RetroArchLaunchInput      (runtime executable, core library, content, config)
///         ↓
/// RetroArchBackend::prepare_launch
///         ↓
/// PreparedLaunch
///         ↓
/// ProcessController::spawn  ← the step after this one, not part of it
/// ```
///
/// A value exists only for a launch that is ready: it is produced after content,
/// runtime, core, firmware, and configuration all resolved, so holding one is
/// evidence that every readiness question was answered. It contains no
/// RetroArch-specific type and no process, and it starts nothing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PreparedGameLaunch {
    game_id: GameId,
    release_id: ReleaseId,
    system: EmulatedSystem,
    content: PathBuf,
    runtime: LaunchRuntime,
    core: CoreDefinition,
    core_installation: InstalledCore,
    config: EffectiveLaunchConfig,
    firmware: Vec<FirmwareOutcome>,
}

impl PreparedGameLaunch {
    /// Returns the game this launch belongs to.
    #[must_use]
    pub const fn game_id(&self) -> GameId {
        self.game_id
    }

    /// Returns the release this launch belongs to.
    #[must_use]
    pub const fn release_id(&self) -> ReleaseId {
        self.release_id
    }

    /// Returns the system the content is launched as.
    #[must_use]
    pub const fn system(&self) -> &EmulatedSystem {
        &self.system
    }

    /// Returns the content to launch, exactly as it was resolved.
    ///
    /// The path is a location, not an identity (ARCHITECTURE.md §2.3). It is never
    /// copied, renamed, hashed, or rewritten: content is a user-owned external file.
    #[must_use]
    pub fn content(&self) -> &Path {
        &self.content
    }

    /// Returns the runtime whose executable should be started.
    #[must_use]
    pub const fn runtime(&self) -> &LaunchRuntime {
        &self.runtime
    }

    /// Returns the curated definition of the core to load.
    #[must_use]
    pub const fn core(&self) -> &CoreDefinition {
        &self.core
    }

    /// Returns the installed build of that core.
    #[must_use]
    pub const fn core_installation(&self) -> &InstalledCore {
        &self.core_installation
    }

    /// Returns the effective launch configuration.
    #[must_use]
    pub const fn config(&self) -> &EffectiveLaunchConfig {
        &self.config
    }

    /// Returns the firmware outcomes of this launch, in requirement order.
    ///
    /// An outcome whose missing list is non-empty is an *optional* firmware file
    /// that is not available; a missing required file blocks the launch and never
    /// reaches a prepared launch.
    #[must_use]
    pub fn firmware(&self) -> &[FirmwareOutcome] {
        &self.firmware
    }
}

/// The result of a negotiation: everything that was observed, and the outcome.
///
/// This is the diagnostic shape of a preparation. It carries the observed facts as
/// well as the outcome, so a caller can explain *why* a launch is blocked — which
/// system was resolved, which runtime was missing, which firmware was checked —
/// without re-running the resolution or guessing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GameLaunchPlan {
    request: GameLaunchRequest,
    system: Option<EmulatedSystem>,
    core: Option<SystemCoreState>,
    preparation: LaunchPreparation,
}

impl GameLaunchPlan {
    /// Returns the request this plan answers.
    #[must_use]
    pub const fn request(&self) -> &GameLaunchRequest {
        &self.request
    }

    /// Returns the system that was resolved for the content, if any.
    #[must_use]
    pub const fn system(&self) -> Option<&EmulatedSystem> {
        self.system.as_ref()
    }

    /// Returns the state of the system's curated core, if one was resolved.
    #[must_use]
    pub const fn core_state(&self) -> Option<&SystemCoreState> {
        self.core.as_ref()
    }

    /// Returns the preparation outcome.
    #[must_use]
    pub const fn preparation(&self) -> &LaunchPreparation {
        &self.preparation
    }

    /// Returns whether the launch may proceed.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.preparation.is_ready()
    }

    /// Returns the prepared launch, if the launch is ready.
    #[must_use]
    pub fn prepared(&self) -> Option<&PreparedGameLaunch> {
        self.preparation.prepared()
    }

    /// Returns what blocks the launch, or an empty slice when it is ready.
    #[must_use]
    pub fn blockers(&self) -> &[LaunchBlocker] {
        self.preparation.blockers()
    }

    /// Returns the readiness result of this plan.
    #[must_use]
    pub fn readiness(&self) -> LaunchReadiness {
        self.preparation.readiness()
    }
}

/// Prepares a game launch and evaluates its readiness.
///
/// This is the whole product decision in one call: resolve the content's system from
/// the curated list, ask the caller-supplied state for the runtime and the curated
/// core, check the core's firmware requirements against the available firmware
/// files, resolve the effective launch configuration, and answer with either a
/// prepared launch or the reasons that block it.
///
/// Every observation is independent of the others, so a blocked result reports
/// *everything* that is wrong instead of only the first problem found: a missing
/// runtime and missing firmware appear in one answer.
///
/// The call performs no I/O of its own. Everything it reads arrives through
/// `context` and through the [`FirmwareChecker`] port, so a caller can exercise the
/// whole decision with fakes and without a filesystem.
///
/// # A firmware location that cannot be read
///
/// Listing the firmware location can fail for a reason that has nothing to do with
/// firmware being absent — a folder that exists but cannot be read, for instance.
/// Such a failure is answered conservatively: the files that could not be looked up
/// count as not available, so a required firmware file is reported as missing and a
/// launch never claims to be ready on the strength of a check that did not happen.
/// An optional requirement stays non-blocking, as it always is.
#[must_use]
pub fn prepare_game_launch(
    request: GameLaunchRequest,
    context: &GameLaunchContext<'_>,
    firmware: &dyn FirmwareChecker,
) -> GameLaunchPlan {
    let mut blockers = Vec::new();

    if !context.content_available {
        blockers.push(LaunchBlocker::ContentMissing);
    }

    let system = context
        .systems
        .system_for_content(request.content())
        .cloned();

    // Without a system nothing else can be resolved: there is no core to ask for, no
    // firmware requirement to check, and no system scope to apply.
    let Some(system) = system else {
        blockers.push(LaunchBlocker::UnsupportedSystemOrCore {
            system: None,
            reason: String::from(
                "no curated system accepts the content format of this file, so BitArchive has no \
                 core it could run it with",
            ),
        });

        return blocked(request, None, None, blockers);
    };

    let core_state = context.core.clone();

    if let Some(state) = &core_state {
        report_core_state(&system, state, &mut blockers);
    }

    // The curated definition is the same value in every `SystemCoreState` that has
    // one; only the installation state differs.
    let definition = core_state.as_ref().and_then(curated_definition).cloned();

    let (firmware_outcomes, firmware_blockers) =
        gather_firmware(&system, definition.as_ref(), firmware);
    blockers.extend(firmware_blockers);

    let (config, config_blockers) = resolve_config(&context.config);
    blockers.extend(config_blockers);

    let installed = core_state.as_ref().and_then(SystemCoreState::installed);

    if context.runtime.runtime().is_none() {
        blockers.push(LaunchBlocker::RuntimeMissing {
            id: context.runtime.id().to_owned(),
        });
    }

    // A prepared launch needs the runtime, the definition, and the installation. When
    // one of them is missing, `blockers` already says which one.
    let (Some(core), Some(installation), Some(runtime)) = (
        definition.clone(),
        installed.cloned(),
        context.runtime.runtime(),
    ) else {
        return blocked(request, Some(system), core_state, blockers);
    };

    let launch = PreparedGameLaunch {
        game_id: request.game_id(),
        release_id: context.release_id,
        system: system.clone(),
        content: request.content().to_path_buf(),
        runtime: runtime.clone(),
        core,
        core_installation: installation,
        config,
        firmware: firmware_outcomes,
    };

    prepared(request, system, core_state, blockers, launch)
}

/// Wraps `request` and everything that was observed into a blocked plan.
fn blocked(
    request: GameLaunchRequest,
    system: Option<EmulatedSystem>,
    core: Option<SystemCoreState>,
    blockers: Vec<LaunchBlocker>,
) -> GameLaunchPlan {
    LaunchGameLaunchPlan::new(request, system, core).finish(LaunchPreparation::Blocked { blockers })
}

/// Wraps a fully prepared launch, or reports the blockers that appeared anyway.
///
/// A prepared launch is only built when nothing blocks the launch, so `blockers` is
/// empty here by construction. It is still checked rather than assumed: a
/// preparation that carried both a prepared launch and a blocker would be an
/// internal contradiction, and this is the one place that could create one.
fn prepared(
    request: GameLaunchRequest,
    system: EmulatedSystem,
    core: Option<SystemCoreState>,
    blockers: Vec<LaunchBlocker>,
    launch: PreparedGameLaunch,
) -> GameLaunchPlan {
    let preparation = if blockers.is_empty() {
        LaunchPreparation::Prepared(Box::new(launch))
    } else {
        LaunchPreparation::Blocked { blockers }
    };

    LaunchGameLaunchPlan::new(request, Some(system), core).finish(preparation)
}

/// Builds a [`GameLaunchPlan`] step by step, keeping its fields private.
struct LaunchGameLaunchPlan {
    request: GameLaunchRequest,
    system: Option<EmulatedSystem>,
    core: Option<SystemCoreState>,
}

impl LaunchGameLaunchPlan {
    /// Starts a plan for `request`.
    const fn new(
        request: GameLaunchRequest,
        system: Option<EmulatedSystem>,
        core: Option<SystemCoreState>,
    ) -> Self {
        Self {
            request,
            system,
            core,
        }
    }

    /// Finishes the plan with `preparation`.
    fn finish(self, preparation: LaunchPreparation) -> GameLaunchPlan {
        GameLaunchPlan {
            request: self.request,
            system: self.system,
            core: self.core,
            preparation,
        }
    }
}

/// Adds the blocker a system's core state produces, if it produces one.
///
/// The two cases are different repairs and stay distinguishable: a system BitArchive
/// curates no core for cannot be fixed by installing anything, while a curated
/// definition that is not installed can.
fn report_core_state(
    system: &EmulatedSystem,
    state: &SystemCoreState,
    blockers: &mut Vec<LaunchBlocker>,
) {
    let Some(core) = state.definition() else {
        // The caller asked the port for a system that the curated catalogue does not
        // contain, so there is no definition and no component to install. The
        // negotiation only asks about systems it resolved itself, which makes this
        // unreachable from the launch path.
        blockers.push(LaunchBlocker::UnsupportedSystemOrCore {
            system: Some(system.key().as_str().to_owned()),
            reason: String::from(
                "no curated core definition was resolved for this system, and no uncurated core is \
                 ever used",
            ),
        });

        return;
    };

    match core {
        SystemCore::Curated(definition) => {
            let Some(reason) = state.unusable_reason() else {
                // The curated build is installed, so nothing blocks the launch here.
                return;
            };

            blockers.push(LaunchBlocker::CoreUnusable {
                component_id: definition.component_id().as_str().to_owned(),
                build_id: definition.build_id().as_str().to_owned(),
                reason: reason.clone(),
            });
        }
        SystemCore::Unavailable { reason } => {
            // The system or the platform has no curated core, so the reason names what
            // is missing instead of reporting a generic failure.
            blockers.push(LaunchBlocker::UnsupportedSystemOrCore {
                system: Some(system.key().as_str().to_owned()),
                reason: reason.to_string(),
            });
        }
    }
}

/// Returns the curated definition of a core state, if it has one.
fn curated_definition(state: &SystemCoreState) -> Option<&CoreDefinition> {
    match state.definition() {
        Some(SystemCore::Curated(definition)) => Some(definition),
        Some(SystemCore::Unavailable { .. }) | None => None,
    }
}

/// Evaluates the firmware requirements of `definition` for `system`.
///
/// The result is a pair: what was checked (one outcome per applicable requirement,
/// whether or not it is satisfied) and the blockers a *required* requirement that is
/// not satisfied produces. An optional requirement never produces a blocker, however
/// it turns out.
fn gather_firmware(
    system: &EmulatedSystem,
    definition: Option<&CoreDefinition>,
    firmware: &dyn FirmwareChecker,
) -> (Vec<FirmwareOutcome>, Vec<LaunchBlocker>) {
    let Some(definition) = definition else {
        return (Vec::new(), Vec::new());
    };

    let requirements: Vec<&FirmwareRequirement> =
        definition.firmware_requirements_for(system.key()).collect();

    if requirements.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut expected: Vec<String> = requirements
        .iter()
        .flat_map(|requirement| requirement.expected_filenames().iter().cloned())
        .collect();
    expected.sort();
    expected.dedup();

    // A location that cannot be read is treated as a location that provides
    // nothing, so a required firmware file is reported as missing instead of the
    // launch silently claiming to be ready.
    let available = firmware.available(&expected).unwrap_or_default();

    let mut outcomes = Vec::with_capacity(requirements.len());
    let mut blockers = Vec::new();

    for requirement in requirements {
        let expected = requirement.expected_filenames();

        // The order is the requirement's own order, so the result does not depend on
        // how a checker happened to enumerate anything.
        let present: Vec<String> = expected
            .iter()
            .filter(|filename| available.contains(filename))
            .cloned()
            .collect();
        let missing: Vec<String> = expected
            .iter()
            .filter(|filename| !available.contains(filename))
            .cloned()
            .collect();

        if !missing.is_empty() && requirement.level() == FirmwareRequirementLevel::Required {
            blockers.push(LaunchBlocker::FirmwareMissing {
                filenames: missing.clone(),
            });
        }

        outcomes.push(FirmwareOutcome {
            level: requirement.level(),
            expected: expected.to_vec(),
            present,
            missing,
        });
    }

    (outcomes, blockers)
}

/// Resolves the effective launch configuration, keeping unusable overrides.
///
/// The precedence rule itself lives in the domain ([`resolve_launch_config`]). An
/// invalid stored key neither disappears nor silently takes part: it is reported as a
/// blocker (invariant 19) while the remaining overrides are still applied, so a
/// caller can show the launch state and the stored value that needs attention at the
/// same time.
fn resolve_config(sources: &[ScopedConfig]) -> (EffectiveLaunchConfig, Vec<LaunchBlocker>) {
    match resolve_launch_config(sources.iter().cloned()) {
        Ok(config) => (config, Vec::new()),
        Err(outcome) => {
            let blockers = outcome
                .errors
                .iter()
                .map(|error| LaunchBlocker::InvalidConfiguration {
                    key: error.key().to_owned(),
                })
                .collect();

            (outcome.effective, blockers)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blocked preparation always names a reason, and readiness is derived from
    /// the preparation instead of being stored next to it.
    #[test]
    fn a_blocked_preparation_always_names_a_reason() {
        use crate::launch_readiness::ReadinessIssue;

        let blocked = LaunchPreparation::Blocked {
            blockers: vec![
                LaunchBlocker::ContentMissing,
                LaunchBlocker::RuntimeMissing {
                    id: String::from("retroarch"),
                },
            ],
        };

        assert!(!blocked.is_ready());
        assert!(blocked.prepared().is_none());
        assert_eq!(
            blocked.readiness().blocking_issues(),
            [
                ReadinessIssue::ContentUnavailable,
                ReadinessIssue::RuntimeUnavailable
            ]
        );
    }
}
