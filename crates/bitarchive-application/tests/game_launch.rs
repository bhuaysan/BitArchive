//! Launch readiness and game launch preparation, exercised offline.
//!
//! Everything in this file is deterministic and needs no network, no RetroArch, no
//! ROM, no BIOS dump, and no GUI. The managed state a negotiation reads arrives
//! through the
//! [`ManagedEmulationState`](bitarchive_application::ManagedEmulationState) and
//! [`FirmwareChecker`](bitarchive_application::FirmwareChecker) ports, and both are
//! answered by the small fakes below, so a test states exactly the state it wants to
//! exercise.
//!
//! ```text
//! ready path          missing runtime       missing core        missing content
//! unsupported system  optional firmware     required firmware   config precedence
//! ```
//!
//! The prepared launch is asserted on the values the *real* launch path consumes —
//! the managed executable, the managed core library, the exact content, and the
//! effective configuration — so a later play flow cannot be handed a different
//! answer than the one this file pins.

use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use bitarchive_application::launch_state::{
    CoreAvailability, CoreUnusableReason, InstalledCore, LaunchRuntime, LaunchRuntimeResolution,
    RuntimeUnavailableReason,
};
use bitarchive_application::{
    FirmwareChecker, GameLaunchContext, GameLaunchPlan, GameLaunchRequest, LaunchBlocker,
    ManagedEmulationState, PreparedGameLaunch, SystemCoreState, UnsupportedSystemOrCoreReason,
    prepare_game_launch,
};
use bitarchive_domain::component::{
    ArtifactReference, ArtifactSource, ComponentArtifactSource, ComponentAttribution,
    LicenseIdentifier, RelativePath, Sha256Digest,
};
use bitarchive_domain::config::{ConfigKey, ConfigScope, ConfigValue, EffectiveLaunchConfig};
use bitarchive_domain::managed_core::{CoreArtifactKind, CoreBuildId, CorePlatform};
use bitarchive_domain::system::{
    EmulatedSystemKey, EmulatedSystemKeyError, SystemCore, curated_systems, resolve_system_core,
};
use bitarchive_domain::{
    CoreComponentId, CoreDefinition, CoreParts, CoreProvenance, FirmwareRequirement,
    FirmwareRequirementLevel, GameId, ReleaseId,
};

/// The curated Game Boy Advance key used by most tests.
const GBA: &str = "gba";

/// The content most tests launch.
const GBA_CONTENT: &str = "/library/advance-wars.gba";

/// The managed runtime executable a resolved runtime reports.
const MANAGED_EXECUTABLE: &str =
    "/components/runtime/retroarch/macos-universal/1.22.2/RetroArch.app/Contents/MacOS/RetroArch";

/// The managed core library an installed core reports.
const MANAGED_LIBRARY: &str = "/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d/\
                               mgba_libretro.dylib";

/// A resolved managed runtime.
fn runtime() -> LaunchRuntime {
    LaunchRuntime::new("retroarch", "1.22.2", MANAGED_EXECUTABLE)
}

/// The installed mGBA build of the curated definition.
fn installed_core() -> InstalledCore {
    InstalledCore::new(
        "mgba-0.11-212-7a12d6d",
        "/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d",
        MANAGED_LIBRARY,
    )
}

/// A runtime resolution for a runtime that is not installed.
fn runtime_not_installed() -> LaunchRuntimeResolution {
    LaunchRuntimeResolution::Unavailable {
        id: String::from("retroarch"),
        reason: RuntimeUnavailableReason::NotInstalled,
    }
}

/// A system key from the curated list.
fn key(value: &str) -> EmulatedSystemKey {
    EmulatedSystemKey::from_str(value).expect("the curated system key is valid")
}

/// Returns the curated definition of Game Boy Advance's core.
fn curated_definition() -> CoreDefinition {
    let catalog = curated_systems();
    let system = catalog
        .system(&key(GBA))
        .expect("Game Boy Advance is curated");

    match resolve_system_core(system, CorePlatform::MacOsArm64) {
        SystemCore::Curated(definition) => *definition,
        SystemCore::Unavailable { reason } => panic!("the curated core must resolve: {reason}"),
    }
}

/// A curated definition with `firmware` requirements, for the firmware cases.
///
/// The rest of the pin is a synthetic but complete definition, so the firmware rules
/// can be exercised without pretending that a real core requires a real BIOS.
fn definition_with_firmware(firmware: Vec<FirmwareRequirement>) -> CoreDefinition {
    CoreDefinition::new(CoreParts {
        identity: (
            CoreComponentId::from_str(CoreComponentId::MGBA).expect("a valid component identity"),
            String::from("mGBA"),
            String::from("mgba_libretro"),
            CoreBuildId::from_str("mgba-0.0-synthetic-0000000").expect("a valid build identity"),
            CorePlatform::MacOsArm64,
        ),
        artifact: ArtifactReference::with_file_name(
            ComponentArtifactSource::official(
                ArtifactSource::new(
                    "https://buildbot.libretro.com/nightly/apple/osx/arm64/latest/\
                     mgba_libretro.dylib.zip",
                )
                .expect("a valid https URL"),
            ),
            Sha256Digest::from_str(
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .expect("a valid digest"),
            "mgba_libretro.dylib.zip",
        )
        .expect("a plain file name"),
        layout: (
            CoreArtifactKind::LibretroCoreArchive {
                member: String::from("mgba_libretro.dylib"),
            },
            RelativePath::from_str("mgba_libretro.dylib").expect("a canonical relative path"),
        ),
        firmware,
        provenance: CoreProvenance {
            version: String::from("0.0-synthetic-0000000"),
            revision: String::from("0000000000000000000000000000000000000000"),
            revision_short: String::from("0000000"),
            channel_date: String::from("1970-01-01"),
            channel_crc32: String::from("00000000"),
        },
        attribution: ComponentAttribution {
            component: String::from("mGBA"),
            upstream_project: String::from("mgba-emu/mgba"),
            upstream_url: String::from("https://github.com/mgba-emu/mgba"),
            license: LicenseIdentifier::from_str(LicenseIdentifier::MPL_2_0)
                .expect("a valid license identifier"),
        },
    })
}

/// The managed state a test wants a negotiation to read.
///
/// [`Clone`] so that one fully resolved state can be varied several ways in one test,
/// which is how the runtime reasons are compared against each other.
#[derive(Clone)]
struct FakeState {
    runtime: LaunchRuntimeResolution,
    core: Option<SystemCoreState>,
}

impl FakeState {
    /// A state in which everything resolves, using the given core definition.
    fn ready(definition: CoreDefinition) -> Self {
        Self {
            runtime: LaunchRuntimeResolution::Resolved(runtime()),
            core: Some(SystemCoreState::Curated {
                availability: CoreAvailability::Installed(installed_core()),
                core: SystemCore::Curated(Box::new(definition)),
            }),
        }
    }

    /// A state in which nothing is installed.
    fn nothing_installed(definition: CoreDefinition) -> Self {
        Self {
            runtime: runtime_not_installed(),
            core: Some(SystemCoreState::Curated {
                availability: CoreAvailability::Unusable {
                    reason: CoreUnusableReason::NotInstalled,
                },
                core: SystemCore::Curated(Box::new(definition)),
            }),
        }
    }

    /// The same state without a runtime.
    fn without_runtime(mut self) -> Self {
        self.runtime = runtime_not_installed();

        self
    }

    /// The same state with a runtime that is installed but broken.
    fn with_broken_runtime(mut self) -> Self {
        self.runtime = LaunchRuntimeResolution::Unavailable {
            id: String::from("retroarch"),
            reason: RuntimeUnavailableReason::ExecutableMissing {
                expected: PathBuf::from(MANAGED_EXECUTABLE),
                version: String::from("1.22.2"),
            },
        };

        self
    }

    /// The same state with a component store that cannot be read.
    fn with_unreadable_runtime_store(mut self) -> Self {
        self.runtime = LaunchRuntimeResolution::Unavailable {
            id: String::from("retroarch"),
            reason: RuntimeUnavailableReason::StoreUnreadable,
        };

        self
    }

    /// The same state without an installed core.
    fn without_core(mut self) -> Self {
        self.core = Some(SystemCoreState::Curated {
            availability: CoreAvailability::Unusable {
                reason: CoreUnusableReason::NotInstalled,
            },
            core: SystemCore::Curated(Box::new(curated_definition())),
        });

        self
    }
}

impl ManagedEmulationState for FakeState {
    fn runtime(&self) -> LaunchRuntimeResolution {
        self.runtime.clone()
    }

    fn core_for_system(&self, _system: &EmulatedSystemKey) -> SystemCoreState {
        self.core
            .clone()
            .expect("a test that expects a core state supplies one")
    }
}

/// A firmware checker that reports exactly the names a test declares.
struct FakeFirmware {
    available: Vec<String>,
    unreadable: bool,
}

impl FakeFirmware {
    /// A location holding `names`.
    fn holding(names: &[&str]) -> Self {
        Self {
            available: names.iter().map(|name| String::from(*name)).collect(),
            unreadable: false,
        }
    }

    /// A location that holds nothing.
    fn empty() -> Self {
        Self::holding(&[])
    }

    /// A location that cannot be listed at all.
    fn unreadable() -> Self {
        Self {
            available: Vec::new(),
            unreadable: true,
        }
    }
}

impl FirmwareChecker for FakeFirmware {
    fn available(&self, expected: &[String]) -> io::Result<Vec<String>> {
        if self.unreadable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the firmware folder cannot be read",
            ));
        }

        Ok(expected
            .iter()
            .filter(|name| self.available.contains(name))
            .cloned()
            .collect())
    }
}

/// A scope of typed configuration values.
fn scoped(scope: ConfigScope, entries: &[(&str, &str)]) -> bitarchive_domain::ScopedConfig {
    let values = entries.iter().map(|(key, value)| {
        (
            ConfigKey::from_str(key).expect("the test keys are well formed"),
            ConfigValue::Text(String::from(*value)),
        )
    });

    bitarchive_domain::ScopedConfig::new(scope, values)
}

/// Builds a context with no configuration scopes.
fn context<'a>(
    systems: &'a bitarchive_domain::system::EmulatedSystemCatalog,
    state: &FakeState,
) -> GameLaunchContext<'a> {
    let _ = state;

    GameLaunchContext {
        systems,
        platform: CorePlatform::MacOsArm64,
        release_id: ReleaseId::new(),
        content_available: true,
        runtime: state.runtime(),
        core: state.core.clone(),
        config: Vec::new(),
    }
}

/// Runs a negotiation against the curated system catalogue.
fn negotiate(
    content: &str,
    state: &FakeState,
    firmware: &FakeFirmware,
    config: Vec<bitarchive_domain::ScopedConfig>,
    content_available: bool,
) -> GameLaunchPlan {
    let systems = curated_systems();
    let mut context = context(&systems, state);

    context.content_available = content_available;
    context.config = config;

    prepare_game_launch(
        GameLaunchRequest::new(GameId::new(), PathBuf::from(content)),
        &context,
        firmware,
    )
}

/// Returns the prepared launch of a plan, or panics with the blockers.
fn prepared(plan: &GameLaunchPlan) -> &PreparedGameLaunch {
    plan.prepared().unwrap_or_else(|| {
        panic!(
            "the launch must be ready, but was blocked: {:?}",
            plan.blockers()
        )
    })
}

/// Returns the effective value of `key` in `config`.
fn config_value<'a>(config: &'a EffectiveLaunchConfig, key: &str) -> Option<&'a ConfigValue> {
    config.get(&ConfigKey::from_str(key).expect("a well formed key"))
}

/// The ready path: a supported system, an installed runtime, an installed curated
/// core, and no firmware requirement produce a prepared launch that names exactly the
/// managed components and the exact content.
#[test]
fn a_fully_installed_state_prepares_a_launch() {
    let state = FakeState::ready(curated_definition());

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert!(
        plan.is_ready(),
        "the launch must be ready: {:?}",
        plan.blockers()
    );
    assert!(plan.blockers().is_empty());
    assert!(plan.readiness().is_ready());

    let launch = prepared(&plan);

    assert_eq!(
        launch.runtime().executable(),
        Path::new(MANAGED_EXECUTABLE),
        "the prepared launch uses the managed runtime executable"
    );
    assert_eq!(
        launch.core_installation().library(),
        Path::new(MANAGED_LIBRARY),
        "the prepared launch uses the managed core library"
    );
    assert_eq!(
        launch.core().component_id().as_str(),
        CoreComponentId::MGBA,
        "the prepared launch uses the curated core"
    );
    assert_eq!(
        launch.content(),
        Path::new(GBA_CONTENT),
        "the prepared launch uses the exact content path"
    );
    assert_eq!(launch.system().key(), &key(GBA));
    assert_eq!(plan.system().map(|system| system.key()), Some(&key(GBA)));
}

/// A missing managed runtime blocks the launch structurally, and the blocker names
/// the runtime that is missing.
#[test]
fn a_missing_runtime_blocks_the_launch() {
    let state = FakeState::ready(curated_definition()).without_runtime();

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert!(!plan.is_ready());
    assert!(plan.prepared().is_none());
    assert_eq!(
        plan.blockers(),
        [LaunchBlocker::RuntimeUnavailable {
            id: String::from("retroarch"),
            reason: RuntimeUnavailableReason::NotInstalled,
        }]
    );
    assert_eq!(
        plan.readiness().blocking_issues(),
        [bitarchive_application::ReadinessIssue::RuntimeUnavailable]
    );
}

/// A curated core that is not installed blocks the launch, and the blocker is
/// distinguishable from "no curated core exists": it names the component, the build,
/// and the reason.
#[test]
fn a_missing_core_blocks_the_launch() {
    let state = FakeState::ready(curated_definition()).without_core();

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert!(!plan.is_ready());

    match plan.blockers() {
        [
            LaunchBlocker::CoreUnusable {
                component_id,
                build_id,
                reason,
            },
        ] => {
            assert_eq!(component_id, CoreComponentId::MGBA);
            assert_eq!(build_id, "mgba-0.11-212-7a12d6d");
            assert_eq!(reason, &CoreUnusableReason::NotInstalled);
        }
        blockers => panic!("the core must be reported as unusable: {blockers:?}"),
    }
}

/// An installed core whose installation is broken is reported as unusable rather than
/// as missing, because installing the same build again could not repair it.
#[test]
fn a_broken_core_installation_is_not_reported_as_missing() {
    let mut state = FakeState::ready(curated_definition());
    state.core = Some(SystemCoreState::Curated {
        availability: CoreAvailability::Unusable {
            reason: CoreUnusableReason::Unusable,
        },
        core: SystemCore::Curated(Box::new(curated_definition())),
    });

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    match plan.blockers() {
        [LaunchBlocker::CoreUnusable { reason, .. }] => {
            assert_eq!(reason, &CoreUnusableReason::Unusable);
        }
        blockers => panic!("a broken installation must be reported as unusable: {blockers:?}"),
    }
}

/// Missing content blocks the launch, and it is reported together with everything
/// else that is wrong instead of replacing it.
#[test]
fn missing_content_blocks_the_launch_and_is_reported_with_the_rest() {
    let state = FakeState::ready(curated_definition()).without_runtime();

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        false,
    );

    assert!(!plan.is_ready());
    assert!(
        plan.blockers().contains(&LaunchBlocker::ContentMissing),
        "missing content is a blocker of its own: {:?}",
        plan.blockers()
    );
    assert!(
        plan.blockers()
            .iter()
            .any(|blocker| matches!(blocker, LaunchBlocker::RuntimeUnavailable { .. })),
        "the missing runtime is still reported: {:?}",
        plan.blockers()
    );
    assert_eq!(
        plan.readiness().blocking_issues(),
        [
            bitarchive_application::ReadinessIssue::ContentUnavailable,
            bitarchive_application::ReadinessIssue::RuntimeUnavailable
        ],
        "every blocker contributes its readiness category, in order"
    );
}

/// A content format that no curated system accepts blocks the launch, and the blocker
/// says that no system could be resolved rather than naming a fabricated one.
#[test]
fn an_unsupported_content_format_blocks_the_launch() {
    let state = FakeState::ready(curated_definition());

    let plan = negotiate(
        "/library/game.nes",
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert!(!plan.is_ready());
    assert_eq!(plan.system(), None, "no system is invented for the content");

    match plan.blockers() {
        [LaunchBlocker::UnsupportedSystemOrCore { system, reason }] => {
            assert_eq!(system, &None);
            assert_eq!(
                reason,
                &UnsupportedSystemOrCoreReason::UnsupportedContentFormat
            );
        }
        blockers => panic!("the format must be reported as unsupported: {blockers:?}"),
    }

    assert_eq!(
        plan.readiness().blocking_issues(),
        [bitarchive_application::ReadinessIssue::UnsupportedFormat]
    );
}

/// A system whose core is not curated for the platform blocks the launch without
/// asking for a runtime or a core, because there is no component to run.
#[test]
fn a_system_without_a_curated_core_blocks_the_launch() {
    let state = FakeState {
        runtime: LaunchRuntimeResolution::Resolved(runtime()),
        core: Some(SystemCoreState::NotCurated(SystemCore::Unavailable {
            reason: bitarchive_domain::CoreUnavailableReason::CoreNotCuratedForPlatform {
                component_id: CoreComponentId::from_str(CoreComponentId::MGBA)
                    .expect("a valid component identity"),
                platform: CorePlatform::MacOsArm64,
            },
        })),
    };

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert!(!plan.is_ready());

    match plan.blockers() {
        [LaunchBlocker::UnsupportedSystemOrCore { system, reason }] => {
            assert_eq!(system.as_deref(), Some(GBA));
            assert!(
                matches!(
                    reason,
                    UnsupportedSystemOrCoreReason::CoreNotCuratedForPlatform { .. }
                ),
                "the platform is the uncurated part: {reason:?}"
            );
        }
        blockers => panic!("the combination must be reported as unsupported: {blockers:?}"),
    }
}

/// The curated mGBA core's Game Boy Advance BIOS is optional, so a missing
/// `gba_bios.bin` must not block a launch — and the prepared launch still records that
/// the file is absent.
#[test]
fn an_absent_optional_firmware_file_does_not_block_the_launch() {
    let state = FakeState::ready(curated_definition());

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert!(
        plan.is_ready(),
        "an optional firmware file must never block a launch: {:?}",
        plan.blockers()
    );

    let launch = prepared(&plan);

    assert_eq!(
        launch.firmware().len(),
        1,
        "the one firmware entry of the curated core was checked"
    );
    assert_eq!(
        launch.firmware()[0].level,
        FirmwareRequirementLevel::Optional
    );
    assert_eq!(
        launch.firmware()[0].missing,
        [String::from(CoreComponentId::MGBA_GBA_BIOS_FILE_NAME)]
    );
    assert!(!launch.firmware()[0].is_satisfied());
}

/// A present optional firmware file is recorded as present, and the launch is ready
/// either way.
#[test]
fn a_present_optional_firmware_file_is_recorded_as_present() {
    let state = FakeState::ready(curated_definition());
    let firmware = FakeFirmware::holding(&[CoreComponentId::MGBA_GBA_BIOS_FILE_NAME]);

    let plan = negotiate(GBA_CONTENT, &state, &firmware, Vec::new(), true);

    let launch = prepared(&plan);

    assert_eq!(
        launch.firmware()[0].present,
        [String::from(CoreComponentId::MGBA_GBA_BIOS_FILE_NAME)]
    );
    assert!(launch.firmware()[0].missing.is_empty());
    assert!(launch.firmware()[0].is_satisfied());
}

/// Required firmware blocks the launch when it is missing — modelled through a
/// synthetic definition, because no curated core requires external firmware today.
#[test]
fn a_missing_required_firmware_file_blocks_the_launch() {
    let gba = key(GBA);
    let definition = definition_with_firmware(vec![FirmwareRequirement::required(
        vec![String::from("required_bios.bin")],
        vec![gba],
    )]);
    let state = FakeState::ready(definition);

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert!(!plan.is_ready());
    assert_eq!(
        plan.blockers(),
        [LaunchBlocker::FirmwareMissing {
            filenames: vec![String::from("required_bios.bin")]
        }]
    );
    assert_eq!(
        plan.readiness().blocking_issues(),
        [bitarchive_application::ReadinessIssue::FirmwareMissing]
    );
}

/// The same required firmware, when present, does not block the launch.
#[test]
fn a_present_required_firmware_file_does_not_block_the_launch() {
    let gba = key(GBA);
    let definition = definition_with_firmware(vec![FirmwareRequirement::required(
        vec![String::from("required_bios.bin")],
        vec![gba],
    )]);
    let state = FakeState::ready(definition);

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::holding(&["required_bios.bin"]),
        Vec::new(),
        true,
    );

    assert!(
        plan.is_ready(),
        "a present required file must not block: {:?}",
        plan.blockers()
    );
}

/// A required firmware file cannot be verified when the firmware location cannot be
/// read, so the launch is not reported as ready.
#[test]
fn an_unreadable_firmware_location_blocks_a_required_requirement() {
    let gba = key(GBA);
    let definition = definition_with_firmware(vec![FirmwareRequirement::required(
        vec![String::from("required_bios.bin")],
        vec![gba],
    )]);
    let state = FakeState::ready(definition);

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::unreadable(),
        Vec::new(),
        true,
    );

    assert!(
        !plan.is_ready(),
        "a launch must not claim to be ready on the strength of a check that did not happen"
    );
}

/// An unreadable firmware location does not block an optional requirement, exactly
/// like an absent optional file does not.
#[test]
fn an_unreadable_firmware_location_does_not_block_an_optional_requirement() {
    let state = FakeState::ready(curated_definition());

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::unreadable(),
        Vec::new(),
        true,
    );

    assert!(
        plan.is_ready(),
        "an optional requirement never blocks a launch: {:?}",
        plan.blockers()
    );
}

/// A firmware requirement that belongs to another system is not checked for this one,
/// so a Game Boy Advance requirement never appears in a Game Boy Color launch.
#[test]
fn a_requirement_of_another_system_is_not_checked() {
    let definition = definition_with_firmware(vec![FirmwareRequirement::required(
        vec![String::from("required_bios.bin")],
        vec![key("gba")],
    )]);
    let state = FakeState::ready(definition);

    let plan = negotiate(
        "/library/links-awakening.gbc",
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert!(
        plan.is_ready(),
        "the requirement belongs to Game Boy Advance, not Game Boy Color: {:?}",
        plan.blockers()
    );
    assert!(
        prepared(&plan).firmware().is_empty(),
        "a requirement of another system is not reported as checked"
    );
}

/// The configuration hierarchy resolves `game > system > global`, and the prepared
/// launch carries the resolved value together with the scope it came from.
#[test]
fn the_configuration_resolves_game_over_system_over_global() {
    let state = FakeState::ready(curated_definition());

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        vec![
            scoped(
                ConfigScope::Global,
                &[("video_vsync", "global"), ("audio_volume", "global")],
            ),
            scoped(
                ConfigScope::System,
                &[("video_vsync", "system"), ("audio_mute", "system")],
            ),
            scoped(ConfigScope::Game, &[("video_vsync", "game")]),
        ],
        true,
    );

    let launch = prepared(&plan);

    assert_eq!(
        config_value(launch.config(), "video_vsync"),
        Some(&ConfigValue::Text(String::from("game"))),
        "the game scope is the most specific one and wins"
    );
    assert_eq!(
        config_value(launch.config(), "audio_volume"),
        Some(&ConfigValue::Text(String::from("global"))),
        "a key only the global scope sets keeps its global value"
    );
    assert_eq!(
        config_value(launch.config(), "audio_mute"),
        Some(&ConfigValue::Text(String::from("system"))),
        "a key the game scope does not set keeps its system value"
    );
    assert_eq!(
        launch
            .config()
            .trace(&ConfigKey::from_str("video_vsync").expect("a well formed key"))
            .expect("a trace")
            .source,
        ConfigScope::Game,
        "the effective value names the scope it came from"
    );
}

/// An unusable stored configuration key is reported as a blocker and preserved, while
/// the usable overrides of the same scope still resolve.
#[test]
fn an_invalid_stored_configuration_key_blocks_the_launch_and_is_preserved() {
    let state = FakeState::ready(curated_definition());
    let stored = bitarchive_domain::ScopedConfig::stored(
        ConfigScope::Game,
        [("Video_Vsync", "true"), ("audio_volume", "0.5")],
    );

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        vec![stored],
        true,
    );

    assert!(!plan.is_ready());
    assert_eq!(
        plan.blockers(),
        [LaunchBlocker::InvalidConfiguration {
            key: String::from("Video_Vsync")
        }],
        "the stored key is named instead of being deleted"
    );
    assert_eq!(
        plan.readiness().blocking_issues(),
        [bitarchive_application::ReadinessIssue::InvalidConfiguration],
        "an unusable stored override is a configuration problem, not a content problem"
    );
}

/// Without any configuration the effective configuration is empty rather than filled
/// with invented defaults, and a launch is still ready.
#[test]
fn an_empty_configuration_does_not_block_a_launch() {
    let state = FakeState::ready(curated_definition());

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    let launch = prepared(&plan);

    assert!(launch.config().is_empty());
    assert_eq!(launch.config().len(), 0);
}

/// The whole negotiation is offline and deterministic: the same state, the same
/// content, and the same configuration produce the same plan twice, including the
/// same identities.
#[test]
fn the_same_inputs_produce_the_same_decision() {
    let state = FakeState::ready(curated_definition());
    let game = GameId::new();
    let release = ReleaseId::new();
    let systems = curated_systems();

    let run = || {
        let context = GameLaunchContext {
            systems: &systems,
            platform: CorePlatform::MacOsArm64,
            release_id: release,
            content_available: true,
            runtime: state.runtime(),
            core: state.core.clone(),
            config: vec![scoped(ConfigScope::Global, &[("video_vsync", "true")])],
        };

        prepare_game_launch(
            GameLaunchRequest::new(game, PathBuf::from(GBA_CONTENT)),
            &context,
            &FakeFirmware::empty(),
        )
    };

    let first = run();
    let second = run();

    assert_eq!(first, second);

    let launch = prepared(&first);

    assert_eq!(launch.game_id(), game);
    assert_eq!(launch.release_id(), release);
    assert_eq!(
        config_value(launch.config(), "video_vsync"),
        Some(&ConfigValue::Text(String::from("true")))
    );
}

/// A blocker is data, not presentation: it renders no user-facing text of its own,
/// and the readiness result it maps to is a category rather than a message.
#[test]
fn blockers_carry_structure_and_no_presentation() {
    let state = FakeState::nothing_installed(curated_definition());

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert_eq!(
        plan.blockers().len(),
        2,
        "both missing components are reported in one answer"
    );

    for blocker in plan.blockers() {
        let issue = blocker.issue();

        assert!(
            !format!("{issue:?}").is_empty(),
            "a blocker maps to a readiness category"
        );
    }
}

/// The preparation result of a ready launch is exactly the shape that the existing
/// launch path consumes: the managed executable, the managed core library, the exact
/// content, and the effective configuration.
#[test]
fn a_prepared_launch_carries_the_inputs_the_launch_path_consumes() {
    let state = FakeState::ready(curated_definition());

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::holding(&[CoreComponentId::MGBA_GBA_BIOS_FILE_NAME]),
        vec![scoped(ConfigScope::Game, &[("video_fullscreen", "true")])],
        true,
    );

    let launch = prepared(&plan);

    assert_eq!(launch.runtime().executable(), Path::new(MANAGED_EXECUTABLE));
    assert_eq!(
        launch.core().library().file_name(),
        "mgba_libretro.dylib",
        "the curated definition names the library below the installation"
    );
    assert_eq!(
        launch.core_installation().library(),
        Path::new(MANAGED_LIBRARY)
    );
    assert_eq!(launch.content(), Path::new(GBA_CONTENT));
    assert_eq!(
        config_value(launch.config(), "video_fullscreen"),
        Some(&ConfigValue::Text(String::from("true")))
    );
    assert_eq!(launch.firmware().len(), 1);
}

/// A `SystemCoreState` that reports no curated definition at all is reported as an
/// unsupported combination rather than as a missing core, because installing a core
/// could not repair it.
#[test]
fn a_missing_curated_definition_is_not_reported_as_a_missing_core() {
    let state = FakeState {
        runtime: LaunchRuntimeResolution::Resolved(runtime()),
        core: Some(SystemCoreState::NotCurated(SystemCore::Unavailable {
            reason: bitarchive_domain::CoreUnavailableReason::SystemNotCurated,
        })),
    };

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert!(
        plan.blockers()
            .iter()
            .all(|blocker| !matches!(blocker, LaunchBlocker::CoreUnusable { .. })),
        "an uncurated system is not an installable core: {:?}",
        plan.blockers()
    );
}

/// The run-time `FirmwareOutcome` of a prepared launch reports the requirement level,
/// so a caller can distinguish an absent optional file from an absent required one
/// without re-reading the core definition.
#[test]
fn a_firmware_outcome_carries_its_requirement_level() {
    let gba = key(GBA);
    let definition = definition_with_firmware(vec![
        FirmwareRequirement::optional(vec![String::from("optional.bin")], vec![gba.clone()]),
        FirmwareRequirement::required(vec![String::from("required.bin")], vec![gba]),
    ]);
    let state = FakeState::ready(definition);

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::holding(&["required.bin"]),
        Vec::new(),
        true,
    );

    let launch = prepared(&plan);

    assert_eq!(launch.firmware().len(), 2);
    assert_eq!(
        launch.firmware()[0].level,
        FirmwareRequirementLevel::Optional
    );
    assert_eq!(
        launch.firmware()[1].level,
        FirmwareRequirementLevel::Required
    );

    let optional = &launch.firmware()[0];

    assert!(!optional.is_satisfied(), "the optional file is absent");
    assert!(
        launch.firmware()[1].is_satisfied(),
        "the required file is present"
    );
}

/// A content path with a system key but an unknown format selects no system, so the
/// launch is blocked instead of the system being guessed from the file name.
#[test]
fn a_file_name_that_looks_like_a_system_is_not_a_system() {
    let state = FakeState::ready(curated_definition());

    let plan = negotiate(
        "/library/gba",
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert!(!plan.is_ready());
    assert_eq!(plan.system(), None);
}

/// Every `LaunchBlocker` variant maps to a readiness category, and the mapping is
/// stable: the categories are what a UI layer reads.
#[test]
fn every_blocker_maps_to_a_readiness_category() {
    use bitarchive_application::ReadinessIssue;

    let cases = [
        (
            LaunchBlocker::RuntimeUnavailable {
                id: String::from("retroarch"),
                reason: RuntimeUnavailableReason::NotInstalled,
            },
            ReadinessIssue::RuntimeUnavailable,
        ),
        (
            LaunchBlocker::UnsupportedSystemOrCore {
                system: None,
                reason: UnsupportedSystemOrCoreReason::UnsupportedContentFormat,
            },
            ReadinessIssue::UnsupportedFormat,
        ),
        (
            LaunchBlocker::CoreUnusable {
                component_id: String::from("mgba"),
                build_id: String::from("mgba-0.11-212-7a12d6d"),
                reason: CoreUnusableReason::NotInstalled,
            },
            ReadinessIssue::CoreMissing,
        ),
        (
            LaunchBlocker::ContentMissing,
            ReadinessIssue::ContentUnavailable,
        ),
        (
            LaunchBlocker::FirmwareMissing {
                filenames: vec![String::from("bios.bin")],
            },
            ReadinessIssue::FirmwareMissing,
        ),
        (
            LaunchBlocker::InvalidConfiguration {
                key: String::from("Video_Vsync"),
            },
            ReadinessIssue::InvalidConfiguration,
        ),
    ];

    for (blocker, expected) in cases {
        assert_eq!(blocker.issue(), expected, "{blocker:?}");
    }
}

/// The domain's emulated system key rejects anything that is not a safe path
/// component, so a stored or derived key can never escape a store path.
#[test]
fn a_system_key_is_a_safe_path_component() {
    for rejected in ["", "../escape", "GBA", "game boy", "gba/extra"] {
        let error = EmulatedSystemKey::from_str(rejected).expect_err("the key must be rejected");

        assert!(
            matches!(
                error,
                EmulatedSystemKeyError::Empty
                    | EmulatedSystemKeyError::InvalidStart(_)
                    | EmulatedSystemKeyError::InvalidCharacter(_)
            ),
            "{rejected} must be rejected with a structured error, but was {error:?}"
        );
    }
}

/// A plan for a blocked launch still reports what it observed, so a caller can explain
/// the failure without re-running the resolution.
#[test]
fn a_blocked_plan_reports_what_it_observed() {
    let state = FakeState::ready(curated_definition()).without_runtime();

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    assert_eq!(plan.request().content(), Path::new(GBA_CONTENT));
    assert_eq!(plan.system().map(|system| system.key()), Some(&key(GBA)));
    assert!(
        !plan.preparation().is_ready(),
        "the outcome is a blocked preparation"
    );
    assert!(plan.core_state().is_some(), "the core state is observable");
    assert!(
        plan.blockers()
            .iter()
            .any(|blocker| matches!(blocker, LaunchBlocker::RuntimeUnavailable { .. })),
        "the blocker names the missing runtime"
    );
}

/// A runtime that is not installed and a runtime that is installed but broken are
/// different conditions: both block the launch, and the blocker keeps them apart with a
/// typed reason.
///
/// This is the regression test for the review finding that all three store failures were
/// collapsed into one "missing" answer — which made the command line offer
/// `acquire-retroarch-runtime` for a state that command cannot repair.
#[test]
fn a_not_installed_runtime_and_a_broken_runtime_are_different_blockers() {
    let ready = FakeState::ready(curated_definition());

    let missing = negotiate(
        GBA_CONTENT,
        &ready.clone().without_runtime(),
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );
    let broken = negotiate(
        GBA_CONTENT,
        &ready.clone().with_broken_runtime(),
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );
    let unreadable = negotiate(
        GBA_CONTENT,
        &ready.with_unreadable_runtime_store(),
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    match missing.blockers() {
        [LaunchBlocker::RuntimeUnavailable { reason, .. }] => {
            assert_eq!(reason, &RuntimeUnavailableReason::NotInstalled);
            assert!(reason.is_installable());
        }
        blockers => panic!("the runtime must be reported: {blockers:?}"),
    }

    match broken.blockers() {
        [LaunchBlocker::RuntimeUnavailable { reason, .. }] => {
            assert_eq!(
                reason,
                &RuntimeUnavailableReason::ExecutableMissing {
                    expected: PathBuf::from(MANAGED_EXECUTABLE),
                    version: String::from("1.22.2"),
                },
                "a broken installation names the expected executable and the version"
            );
            assert!(
                !reason.is_installable(),
                "acquiring the runtime again cannot repair an installation the store has"
            );
        }
        blockers => panic!("the broken installation must be reported: {blockers:?}"),
    }

    match unreadable.blockers() {
        [LaunchBlocker::RuntimeUnavailable { reason, .. }] => {
            assert_eq!(reason, &RuntimeUnavailableReason::StoreUnreadable);
            assert!(!reason.is_installable());
        }
        blockers => panic!("the unreadable store must be reported: {blockers:?}"),
    }

    assert_ne!(missing.blockers(), broken.blockers());
    assert_ne!(broken.blockers(), unreadable.blockers());
    assert_eq!(
        missing.readiness().blocking_issues(),
        broken.readiness().blocking_issues(),
        "all three are a runtime readiness problem, and the blocker keeps the detail"
    );
}

/// An unusable stored configuration key reaches readiness as a configuration problem,
/// never as a content problem, through the whole negotiation.
#[test]
fn an_invalid_stored_key_is_a_configuration_problem_not_a_content_problem() {
    use bitarchive_application::ReadinessIssue;

    let state = FakeState::ready(curated_definition());
    let stored = bitarchive_domain::ScopedConfig::stored(
        ConfigScope::Game,
        [("Video_Vsync", "true"), ("audio_volume", "0.5")],
    );

    let plan = negotiate(
        GBA_CONTENT,
        &state,
        &FakeFirmware::empty(),
        vec![stored],
        true,
    );

    assert_eq!(
        plan.blockers(),
        [LaunchBlocker::InvalidConfiguration {
            key: String::from("Video_Vsync")
        }]
    );
    assert_eq!(
        plan.readiness().blocking_issues(),
        [ReadinessIssue::InvalidConfiguration]
    );
    assert_ne!(
        plan.readiness().blocking_issues(),
        [ReadinessIssue::InvalidContent],
        "the content bytes were never checked, so they are never reported as invalid"
    );
}

/// The configuration hierarchy is a property of the scope, not of the order a caller
/// supplies its scopes in: the game value wins for every input order.
#[test]
fn the_configuration_precedence_does_not_depend_on_the_caller_order() {
    let state = FakeState::ready(curated_definition());

    let orders = [
        [ConfigScope::Global, ConfigScope::System, ConfigScope::Game],
        [ConfigScope::Game, ConfigScope::System, ConfigScope::Global],
        [ConfigScope::System, ConfigScope::Global, ConfigScope::Game],
    ];

    for order in orders {
        let config = order
            .into_iter()
            .map(|scope| {
                let value = match scope {
                    ConfigScope::Global => "global",
                    ConfigScope::System => "system",
                    ConfigScope::Game => "game",
                };

                scoped(scope, &[("video_vsync", value)])
            })
            .collect::<Vec<_>>();

        let plan = negotiate(GBA_CONTENT, &state, &FakeFirmware::empty(), config, true);
        let launch = prepared(&plan);

        assert_eq!(
            config_value(launch.config(), "video_vsync"),
            Some(&ConfigValue::Text(String::from("game"))),
            "the game scope must win for input order {order:?}"
        );
        assert_eq!(
            launch
                .config()
                .trace(&ConfigKey::from_str("video_vsync").expect("a well formed key"))
                .expect("a trace")
                .overridden,
            [ConfigScope::Global, ConfigScope::System],
            "the replaced scopes are reported least specific first, for {order:?}"
        );
    }
}

/// The three unsupported cases reach the plan as distinct typed reasons, so a caller
/// can tell a format problem from an uncurated system from an uncurated platform.
#[test]
fn the_unsupported_reasons_are_typed_and_distinct() {
    let state = FakeState::ready(curated_definition());

    let format = negotiate(
        "/library/game.nes",
        &state,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    match format.blockers() {
        [LaunchBlocker::UnsupportedSystemOrCore { system, reason }] => {
            assert_eq!(system, &None);
            assert_eq!(
                reason,
                &UnsupportedSystemOrCoreReason::UnsupportedContentFormat
            );
        }
        blockers => panic!("the format must be reported as unsupported: {blockers:?}"),
    }

    let uncurated_system = FakeState {
        runtime: LaunchRuntimeResolution::Resolved(runtime()),
        core: Some(SystemCoreState::NotCurated(SystemCore::Unavailable {
            reason: bitarchive_domain::CoreUnavailableReason::SystemNotCurated,
        })),
    };
    let plan = negotiate(
        GBA_CONTENT,
        &uncurated_system,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    match plan.blockers() {
        [LaunchBlocker::UnsupportedSystemOrCore { system, reason }] => {
            assert_eq!(system.as_deref(), Some(GBA));
            assert_eq!(reason, &UnsupportedSystemOrCoreReason::SystemNotCurated);
        }
        blockers => panic!("the system must be reported as uncurated: {blockers:?}"),
    }

    let uncurated_platform = FakeState {
        runtime: LaunchRuntimeResolution::Resolved(runtime()),
        core: Some(SystemCoreState::NotCurated(SystemCore::Unavailable {
            reason: bitarchive_domain::CoreUnavailableReason::CoreNotCuratedForPlatform {
                component_id: CoreComponentId::from_str(CoreComponentId::MGBA)
                    .expect("a valid component identity"),
                platform: CorePlatform::MacOsArm64,
            },
        })),
    };
    let plan = negotiate(
        GBA_CONTENT,
        &uncurated_platform,
        &FakeFirmware::empty(),
        Vec::new(),
        true,
    );

    match plan.blockers() {
        [LaunchBlocker::UnsupportedSystemOrCore { reason, .. }] => {
            assert_eq!(
                reason,
                &UnsupportedSystemOrCoreReason::CoreNotCuratedForPlatform {
                    component_id: String::from(CoreComponentId::MGBA),
                    platform: CorePlatform::MacOsArm64,
                },
                "the platform case names the component and the platform, not a sentence"
            );
        }
        blockers => panic!("the platform must be reported as uncurated: {blockers:?}"),
    }
}
