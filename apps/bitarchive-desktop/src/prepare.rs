//! GAME LAUNCH PREPARATION — the opt-in developer command (Issue #29 / B8).
//!
//! This module is the composition root for the product decision that stands in front
//! of a play action. It joins the curated system list, the two managed component
//! stores, the firmware inventory, and the launch configuration hierarchy into one
//! answer:
//!
//! ```text
//! GameLaunchRequest
//!         ↓
//! EmulatedSystemCatalog::system_for_content
//!         ↓
//! ManagedEmulation (runtime activation + curated core build, through the stores)
//!         ↓
//! FilesystemFirmwareChecker (the one firmware folder)
//!         ↓
//! ScopedConfig (Global → System → Game)
//!         ↓
//! prepare_game_launch
//!         ↓
//! PreparedGameLaunch  ──→  RetroArchLaunchInput
//!         ↓                        ↓
//!   (ready + prepared)   RetroArchBackend::prepare_launch
//!                                  ↓
//!                            PreparedLaunch
//!                                  ↓
//!                          ── STOP. No process. ──
//! ```
//!
//! It is a *developer vertical slice*, not a product flow:
//!
//! - it is never called by the UI, by a library API, or by a test;
//! - it starts nothing: the whole point of B8 is that readiness and preparation are
//!   decided *before* anything is started, and B7 (Issue #23) already proved the
//!   process start end to end;
//! - it persists nothing, registers no session, generates no configuration file, and
//!   measures no playtime;
//! - the only inputs a developer supplies are the content, an optional store root, and
//!   optional configuration overrides. No argument can name a runtime, a core, a
//!   version, a URL, or a library.
//!
//! # Why the command still builds the launch arguments
//!
//! Readiness is only useful if the result can actually be handed to the launch path, so
//! the command composes the resolved values into a
//! [`PreparedLaunch`](bitarchive_application::PreparedLaunch) and prints it —
//! and starts nothing. That is the seam the later product play flow goes through:
//!
//! ```text
//! PreparedGameLaunch.runtime().executable()      ┐
//! PreparedGameLaunch.core_installation().library()├→ RetroArchLaunchInput
//! PreparedGameLaunch.content()                   │
//! PreparedGameLaunch.config()                    ┘
//! ```
//!
//! The arguments themselves stay the sole responsibility of [`RetroArchBackend`], so
//! this module cannot drift from the documented RetroArch CLI form.
//!
//! # Why a missing component is not acquired here
//!
//! Acquisition is an explicit, deliberate step (ADR 0001, ADR 0002): it is the only
//! network activity in BitArchive, it is opt-in, and CI never runs it. A preparation
//! that downloaded a runtime or a core would make the answer depend on network timing
//! and would create a second acquisition flow beside the reviewed one, so this command
//! reports what is missing and, when installing the component would actually resolve
//! it, names the existing command that does so.
//!
//! # Exit status
//!
//! `0` when the launch is ready and prepared, `1` when it is blocked or when the
//! arguments are wrong. Nothing is started in either case.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;

use bitarchive_application::{
    CoreUnusableReason, GameLaunchContext, GameLaunchPlan, GameLaunchRequest, LaunchBlocker,
    LaunchPreparation, ManagedEmulationState, PreparedGameLaunch, prepare_game_launch,
};
use bitarchive_domain::config::ConfigScope;
use bitarchive_domain::managed_core::{UnsupportedCoreHost, host_core_platform};
use bitarchive_domain::system::EmulatedSystemKey;
use bitarchive_domain::{GameId, ReleaseId, ScopedConfig};
use bitarchive_emulation::{
    ManagedEmulation, RetroArchBackend, RetroArchLaunchInput, pinned_retroarch_runtime,
};
use bitarchive_infrastructure::{
    AppleDiskImageExtractor, ComponentStore, CoreStore, FilesystemFirmwareChecker,
    HttpArtifactDownloader, ZipCoreArchiveExtractor,
};
use bitarchive_platform::{AppPaths, DmgBundleExtractor};

/// How every developer command of this composition root is started.
const COMMAND_PREFIX: &str = "cargo run -p bitarchive-desktop --";

/// The command that installs the pinned RetroArch runtime (B5 / Issue #19).
const ACQUIRE_RUNTIME_COMMAND: &str = "acquire-retroarch-runtime";

/// The command that installs the curated mGBA core (B6 / Issue #21).
const ACQUIRE_CORE_COMMAND: &str = "acquire-core";

/// What the command was asked to prepare.
///
/// The content is required. The store root and the configuration overrides are
/// optional, and every override names its scope explicitly.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PrepareRequest {
    content: PathBuf,
    root: Option<PathBuf>,
    global: Vec<(String, String)>,
    system: Vec<(String, String, String)>,
    game: Vec<(String, String)>,
}

impl PrepareRequest {
    /// Describes the accepted arguments, so a caller does not have to guess.
    ///
    /// # Errors
    ///
    /// Never. The signature exists so that the usage text has one source, matching the
    /// other developer commands.
    pub fn usage() -> Result<(), std::convert::Infallible> {
        println!(
            "  <content>              path of the local content to prepare — the only launch\n\
             \x20                        input BitArchive does not own (required, exactly one)\n\
             \x20 --root <dir>         resolve the managed components below <dir> instead of the\n\
             \x20                        application data directory\n\
             \x20 --global <key>=<value>   a global configuration override (repeatable)\n\
             \x20 --system <key> <key>=<value>\n\
             \x20                        a configuration override for one system, for example\n\
             \x20                        `--system gba video_vsync=false` (repeatable)\n\
             \x20 --game <key>=<value>     a configuration override for this game (repeatable)\n\
             \n\
             \x20 This command decides and prepares. It starts no process."
        );

        Ok(())
    }

    /// Parses the command's arguments.
    ///
    /// Exactly one positional argument is accepted and it is the content path.
    /// Everything else is `--root`, a configuration override, or an error, so no
    /// argument can name a runtime, a core, a version, a URL, or a library.
    ///
    /// # Errors
    ///
    /// Returns a message when the content path is missing or given more than once, when
    /// an option has no value, when an override is not a `key=value` pair, or when an
    /// argument is unknown.
    pub fn parse(arguments: &[String]) -> Result<Self, String> {
        let mut content = None;
        let mut root = None;
        let mut global = Vec::new();
        let mut system = Vec::new();
        let mut game = Vec::new();
        let mut remaining = arguments.iter();

        while let Some(argument) = remaining.next() {
            match argument.as_str() {
                "--root" => {
                    let value = remaining
                        .next()
                        .ok_or_else(|| String::from("--root needs a directory"))?;

                    root = Some(PathBuf::from(value));
                }
                "--global" => {
                    global.push(parse_override(
                        remaining
                            .next()
                            .ok_or_else(|| String::from("--global needs <key>=<value>"))?,
                        "--global",
                    )?);
                }
                "--game" => {
                    game.push(parse_override(
                        remaining
                            .next()
                            .ok_or_else(|| String::from("--game needs <key>=<value>"))?,
                        "--game",
                    )?);
                }
                "--system" => {
                    let system_key = remaining
                        .next()
                        .ok_or_else(|| String::from("--system needs a system key"))?;

                    EmulatedSystemKey::from_str(system_key).map_err(|error| {
                        format!("--system {system_key} is not a usable system key: {error}")
                    })?;

                    let (key, value) = parse_override(
                        remaining.next().ok_or_else(|| {
                            String::from("--system needs <key>=<value> after the system")
                        })?,
                        "--system",
                    )?;

                    system.push((system_key.clone(), key, value));
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
                            "only one content path can be prepared, but another one was given: \
                             {other}"
                        ));
                    }

                    content = Some(PathBuf::from(other));
                }
            }
        }

        let content =
            content.ok_or_else(|| String::from("the path of the content to prepare is missing"))?;

        Ok(Self {
            content,
            root,
            global,
            system,
            game,
        })
    }

    /// Returns the content path the developer supplied.
    fn content(&self) -> &Path {
        &self.content
    }

    /// Returns the store root the run resolves components from.
    fn paths(&self) -> AppPaths {
        match &self.root {
            Some(root) => AppPaths::below(root),
            None => AppPaths::default(),
        }
    }

    /// Returns the configuration sources for `system`, from the least to the most
    /// specific scope.
    ///
    /// A `--system` override only takes part when its system key is the system the
    /// content resolved to, so an override for another system never leaks into this
    /// launch.
    fn config_for(&self, system: &EmulatedSystemKey) -> Vec<ScopedConfig> {
        let system_values = self
            .system
            .iter()
            .filter(|(key, _, _)| key == system.as_str())
            .map(|(_, key, value)| (key.clone(), value.clone()));

        vec![
            ScopedConfig::stored(ConfigScope::Global, self.global.iter().cloned()),
            ScopedConfig::stored(ConfigScope::System, system_values),
            ScopedConfig::stored(ConfigScope::Game, self.game.iter().cloned()),
        ]
    }
}

/// Parses one `key=value` override.
///
/// The key is not validated here: an unusable key is data the readiness result has to
/// report, not an argument error, because a stored override is preserved for diagnosis
/// rather than rejected at the door (invariant 19). Only the shape is checked, so a
/// missing `=` cannot be mistaken for a key.
fn parse_override(value: &str, option: &str) -> Result<(String, String), String> {
    match value.split_once('=') {
        Some((key, value)) => Ok((key.to_owned(), value.to_owned())),
        None => Err(format!(
            "{option} needs <key>=<value>, but this value has no '=': {value}"
        )),
    }
}

/// Runs the developer preparation command.
pub fn run(arguments: &[String]) -> ExitCode {
    let request = match PrepareRequest::parse(arguments) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("{message}");
            eprintln!(
                "usage: bitarchive-desktop prepare <content> [--root <dir>] \
                 [--global <k>=<v>] [--system <key> <k>=<v>] [--game <k>=<v>]"
            );

            return ExitCode::FAILURE;
        }
    };

    let paths = request.paths();
    let content = request.content();

    println!("GAME LAUNCH PREPARATION (developer opt-in, no process is started)");
    println!();
    println!("  content      {}", content.display());
    println!("  store        {}", paths.components().display());
    println!("  firmware     {}", paths.firmware().display());
    println!();

    // The host architecture decides which curated build is asked for, exactly as in
    // the runtime and core acquisition commands. No other architecture's core is
    // substituted for it.
    let platform = match host_core_platform() {
        Some(platform) => platform,
        None => {
            let host = UnsupportedCoreHost::current();

            eprintln!(
                "this host ({} {}) has no managed-core platform, and no other architecture's \
                 core is substituted for it",
                host.operating_system, host.architecture
            );

            return ExitCode::FAILURE;
        }
    };

    let runtime_definition = pinned_retroarch_runtime();
    let runtime_store = runtime_store(&paths);
    let core_store = core_store(&paths);

    let state = ManagedEmulation::new(&runtime_definition, platform, &runtime_store, &core_store);

    // The one input BitArchive does not own. A launch that could not find its content
    // would otherwise reach RetroArch as a runtime failure of the emulator instead of a
    // clear answer to the developer.
    let content_available = content.is_file();
    let systems = bitarchive_domain::curated_systems();

    // The system is resolved first, because the system scope of the configuration
    // belongs to it. The readiness result resolves it again from the same catalogue, so
    // the two cannot disagree.
    let resolved_system = systems
        .system_for_content(content)
        .map(|system| system.key().clone());

    let config = match &resolved_system {
        Some(system) => request.config_for(system),
        None => vec![ScopedConfig::stored(
            ConfigScope::Global,
            request.global.iter().cloned(),
        )],
    };

    let context = GameLaunchContext {
        systems: &systems,
        platform,
        // A release identity belongs to the library, which does not exist yet; see the
        // application contract. The command supplies a fresh one so that a prepared
        // launch carries an identity rather than a placeholder.
        release_id: ReleaseId::new(),
        content_available,
        runtime: state.runtime(),
        core: resolved_system
            .as_ref()
            .map(|system| state.core_for_system(system)),
        config,
    };

    let firmware = FilesystemFirmwareChecker::new(paths.firmware());

    let plan = prepare_game_launch(
        GameLaunchRequest::new(GameId::new(), content),
        &context,
        &firmware,
    );

    report_plan(&plan);

    if plan.is_ready() {
        report_prepared_launch(&plan);

        ExitCode::SUCCESS
    } else {
        report_blockers(&plan, request.root.as_deref());

        ExitCode::FAILURE
    }
}

/// Builds the runtime store of the managed components.
fn runtime_store(
    paths: &AppPaths,
) -> ComponentStore<HttpArtifactDownloader, AppleDiskImageExtractor<DmgBundleExtractor>> {
    ComponentStore::new(
        paths.components(),
        HttpArtifactDownloader::new(),
        AppleDiskImageExtractor::new(DmgBundleExtractor::new()),
    )
}

/// Builds the core store of the curated cores.
fn core_store(paths: &AppPaths) -> CoreStore<HttpArtifactDownloader, ZipCoreArchiveExtractor> {
    CoreStore::new(
        paths.components(),
        HttpArtifactDownloader::new(),
        ZipCoreArchiveExtractor::new(),
    )
}

/// Prints what the negotiation observed.
fn report_plan(plan: &GameLaunchPlan) {
    println!("observed");

    match plan.system() {
        Some(system) => println!(
            "  system       {} ({})",
            system.key(),
            system.display_name()
        ),
        None => println!("  system       none — no curated system accepts this content format"),
    }

    match plan.core_state().and_then(|state| state.definition()) {
        Some(bitarchive_domain::system::SystemCore::Curated(definition)) => println!(
            "  core         {} {} ({})",
            definition.component_id(),
            definition.build_id(),
            definition.platform()
        ),
        Some(bitarchive_domain::system::SystemCore::Unavailable { reason }) => {
            println!("  core         none — {reason}")
        }
        None => println!("  core         not resolved"),
    }

    match plan.core_state().and_then(|state| state.installed()) {
        Some(installed) => println!("  library      {}", installed.library().display()),
        None => println!("  library      none — no usable build is installed"),
    }

    match plan.preparation() {
        LaunchPreparation::Prepared(launch) => {
            println!(
                "  runtime      {} {}",
                launch.runtime().id(),
                launch.runtime().version()
            );
            println!("  executable   {}", launch.runtime().executable().display());

            for outcome in launch.firmware() {
                println!(
                    "  firmware     {:?} {:?} present={:?} missing={:?}",
                    outcome.level, outcome.expected, outcome.present, outcome.missing
                );
            }
        }
        LaunchPreparation::Blocked { .. } => {}
    }

    println!();
}

/// Prints the reasons a launch is blocked and, when one exists, the command that
/// resolves the first one.
fn report_blockers(plan: &GameLaunchPlan, store_root: Option<&Path>) {
    println!("NOT READY");
    println!();

    for blocker in plan.blockers() {
        println!("  {}", describe(blocker));
    }

    println!();

    // Every component that is not installed has its own installing command, and a run
    // that misses both gets both. Only the blockers with an answer produce advice: a
    // broken installation, an unsupported system, missing content, and missing firmware
    // are not repaired by a download.
    let mut commands: Vec<AcquisitionCommand> = Vec::new();

    for blocker in plan.blockers() {
        if let Some(command) = acquisition_command(blocker)
            && !commands.contains(&command)
        {
            commands.push(command);
        }
    }

    if commands.is_empty() {
        println!(
            "no installing command resolves this: the input or the state itself has to change."
        );

        return;
    }

    println!("run this first:");

    for command in &commands {
        println!("  {}", command.render(store_root));
    }

    if commands.len() < plan.blockers().len() {
        println!("the remaining issues are not resolved by installing a component.");
    }
}

/// Renders one blocker for a developer.
///
/// The blocker itself carries no user-facing text (ARCHITECTURE.md §21); this is the
/// composition root's presentation of it, which is exactly where such text belongs.
fn describe(blocker: &LaunchBlocker) -> String {
    match blocker {
        LaunchBlocker::RuntimeMissing { id } => {
            format!("the managed runtime is missing: {id}")
        }
        LaunchBlocker::UnsupportedSystemOrCore { system, reason } => match system {
            Some(system) => format!("no curated core can run {system} here: {reason}"),
            None => format!("the content format is not supported: {reason}"),
        },
        LaunchBlocker::CoreUnusable {
            component_id,
            build_id,
            reason,
        } => match reason {
            CoreUnusableReason::NotInstalled => {
                format!("the curated core {component_id} {build_id} is not installed")
            }
            CoreUnusableReason::Unusable => format!(
                "the installed core {component_id} {build_id} is not usable and needs diagnosis"
            ),
        },
        LaunchBlocker::ContentMissing => {
            String::from("the content to launch is not a readable file at the given path")
        }
        LaunchBlocker::FirmwareMissing { filenames } => format!(
            "the core requires firmware that is not available: {}",
            filenames.join(", ")
        ),
        LaunchBlocker::InvalidConfiguration { key } => format!(
            "the stored configuration key {key:?} is not usable; the stored value is preserved"
        ),
    }
}

/// The developer command that installs the component a blocker is about, when
/// installing it would resolve the blocker.
///
/// Only "not installed" has an answer: the acquisition command installs the component
/// and a preparation then resolves it. A build directory that exists without its
/// library is a broken installation, and the store refuses to install a build it
/// already has, so that blocker is reported for diagnosis instead.
fn acquisition_command(blocker: &LaunchBlocker) -> Option<AcquisitionCommand> {
    match blocker {
        LaunchBlocker::RuntimeMissing { .. } => Some(AcquisitionCommand::Runtime),
        LaunchBlocker::CoreUnusable {
            reason: CoreUnusableReason::NotInstalled,
            ..
        } => Some(AcquisitionCommand::Core),
        LaunchBlocker::UnsupportedSystemOrCore { .. }
        | LaunchBlocker::CoreUnusable { .. }
        | LaunchBlocker::ContentMissing
        | LaunchBlocker::FirmwareMissing { .. }
        | LaunchBlocker::InvalidConfiguration { .. } => None,
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

    /// Renders the command, aimed at `store_root` when the run used one.
    fn render(self, store_root: Option<&Path>) -> String {
        match store_root {
            Some(root) => format!("{COMMAND_PREFIX} {} --root {}", self.name(), root.display()),
            None => format!("{COMMAND_PREFIX} {}", self.name()),
        }
    }
}

/// Prints the prepared launch: the resolved values and the arguments the launch path
/// would use.
///
/// Nothing is started. The arguments are printed as separate lines on purpose: they are
/// separate values, and joining them into one string would suggest a command line that
/// no shell ever sees.
fn report_prepared_launch(plan: &GameLaunchPlan) {
    let Some(launch) = plan.prepared() else {
        return;
    };

    println!("READY — the launch is prepared and no process is started.");
    println!();

    report_effective_config(launch);

    let prepared = prepared_launch(launch);

    println!("prepared launch (not started)");
    println!("  program      {}", prepared.executable().display());

    for argument in prepared.arguments() {
        println!("  argument     {}", argument.to_string_lossy());
    }

    println!();
}

/// Prints the effective configuration and the scope each value came from.
fn report_effective_config(launch: &PreparedGameLaunch) {
    if launch.config().is_empty() {
        println!("effective configuration: empty (no override is configured)");
        println!();

        return;
    }

    println!("effective configuration (game > system > global)");

    for (key, value) in launch.config().iter() {
        let source = launch
            .config()
            .trace(key)
            .map(|trace| trace.source.to_string())
            .unwrap_or_else(|| String::from("—"));

        println!("  {key:<24} {value:?} ← {source}");
    }

    println!();
}

/// Composes the prepared product launch into the launch path's own contract.
///
/// This is the seam the later play flow uses: the resolved runtime executable, the
/// installed core library, the content, and the effective configuration become a
/// [`RetroArchLaunchInput`], and [`RetroArchBackend`] alone turns that into a
/// [`bitarchive_application::PreparedLaunch`]. The environment and the working
/// directory stay unset, because nothing in B8 decides them.
fn prepared_launch(launch: &PreparedGameLaunch) -> bitarchive_application::PreparedLaunch {
    let input = RetroArchLaunchInput::new(
        launch.runtime().executable(),
        launch.core_installation().library(),
        launch.content(),
    );

    RetroArchBackend::new().prepare_launch(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the argument vector of a test run.
    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| String::from(*value)).collect()
    }

    /// The default request resolves the managed components from the application data
    /// directory, matching the other developer commands.
    #[test]
    fn the_default_request_uses_the_application_data_directory() {
        let request =
            PrepareRequest::parse(&arguments(&["/content/test.gba"])).expect("the content path");

        assert_eq!(request.content(), Path::new("/content/test.gba"));
        assert!(
            request
                .paths()
                .components()
                .ends_with("Library/Application Support/BitArchive/components"),
            "the default run resolves from the real application data directory"
        );
    }

    /// `--root` redirects the whole run, so a developer can prepare against a
    /// throwaway store instead of the application data directory.
    #[test]
    fn a_root_argument_redirects_the_component_store() {
        let request = PrepareRequest::parse(&arguments(&[
            "--root",
            "/tmp/bitarchive-prepare-store",
            "/content/test.gba",
        ]))
        .expect("the arguments are valid");

        let paths = request.paths();

        assert_eq!(
            paths.components(),
            Path::new("/tmp/bitarchive-prepare-store/components")
        );
        assert_eq!(
            paths.firmware(),
            Path::new("/tmp/bitarchive-prepare-store/firmware")
        );
    }

    /// The content path is required, positional, and given exactly once.
    #[test]
    fn the_content_path_is_required_and_given_once() {
        assert!(PrepareRequest::parse(&[]).is_err());
        assert!(
            PrepareRequest::parse(&arguments(&["/content/first.gba", "/content/second.gba"]))
                .is_err(),
            "two content paths do not describe one launch"
        );
        assert!(PrepareRequest::parse(&arguments(&["--root"])).is_err());
    }

    /// No argument can name a runtime or a core, matching the B7 rule.
    #[test]
    fn the_command_cannot_name_a_runtime_or_a_core() {
        for rejected in [
            "--retroarch",
            "--runtime",
            "--core",
            "--core-library",
            "--core-path",
            "--core-url",
            "--core-version",
            "--runtime-url",
            "--runtime-version",
        ] {
            assert!(
                PrepareRequest::parse(&arguments(&[rejected, "/x", "/content/test.gba"])).is_err(),
                "{rejected} must be rejected"
            );
        }
    }

    /// Overrides are `key=value` pairs, and a value without `=` is refused instead of
    /// being silently treated as a key.
    #[test]
    fn an_override_needs_a_key_and_a_value() {
        assert!(
            PrepareRequest::parse(&arguments(&[
                "--global",
                "video_vsync",
                "/content/test.gba"
            ]))
            .is_err(),
            "an override without a value is refused"
        );
        assert!(
            PrepareRequest::parse(&arguments(&["--system", "gba", "/content/test.gba"])).is_err(),
            "a system override without a value is refused"
        );
        assert!(
            PrepareRequest::parse(&arguments(&[
                "--system",
                "gba",
                "video_vsync",
                "/content/test.gba"
            ]))
            .is_err(),
            "a system override without '=' is refused"
        );
        assert!(
            PrepareRequest::parse(&arguments(&[
                "--game",
                "video_vsync=true",
                "/content/test.gba"
            ]))
            .is_ok(),
            "the content is still required"
        );
    }

    /// A `--system` override belongs to one system and is only applied to that system,
    /// so an override for another system cannot leak into a launch.
    #[test]
    fn a_system_override_only_applies_to_its_own_system() {
        let request = PrepareRequest::parse(&arguments(&[
            "--global",
            "audio_volume=0.5",
            "--system",
            "gba",
            "video_vsync=false",
            "--system",
            "gb",
            "video_vsync=true",
            "--game",
            "video_vsync=game",
            "/content/test.gba",
        ]))
        .expect("the arguments are valid");

        let gba = EmulatedSystemKey::from_str("gba").expect("the curated key");
        let gb = EmulatedSystemKey::from_str("gb").expect("the curated key");

        let gba_config = request.config_for(&gba);
        let gb_config = request.config_for(&gb);

        assert_eq!(
            gba_config.len(),
            3,
            "the scopes are supplied from the least to the most specific one"
        );
        assert_eq!(gba_config[0].scope(), ConfigScope::Global);
        assert_eq!(gba_config[1].scope(), ConfigScope::System);
        assert_eq!(gba_config[2].scope(), ConfigScope::Game);

        assert_eq!(
            gba_config[1].stored_values(),
            [(String::from("video_vsync"), String::from("false"))]
        );
        assert_eq!(
            gb_config[1].stored_values(),
            [(String::from("video_vsync"), String::from("true"))],
            "the Game Boy scope holds only its own override"
        );
    }

    /// The command's presentation of a blocker names the component, and the
    /// acquisition advice is only offered for a component that is not installed.
    #[test]
    fn only_a_missing_component_has_an_install_command() {
        let missing_runtime = LaunchBlocker::RuntimeMissing {
            id: String::from("retroarch"),
        };
        let missing_core = LaunchBlocker::CoreUnusable {
            component_id: String::from("mgba"),
            build_id: String::from("mgba-0.11-212-7a12d6d"),
            reason: CoreUnusableReason::NotInstalled,
        };
        let broken_core = LaunchBlocker::CoreUnusable {
            component_id: String::from("mgba"),
            build_id: String::from("mgba-0.11-212-7a12d6d"),
            reason: CoreUnusableReason::Unusable,
        };
        let missing_firmware = LaunchBlocker::FirmwareMissing {
            filenames: vec![String::from("bios.bin")],
        };

        assert_eq!(
            acquisition_command(&missing_runtime),
            Some(AcquisitionCommand::Runtime)
        );
        assert_eq!(
            acquisition_command(&missing_core),
            Some(AcquisitionCommand::Core)
        );
        assert_eq!(
            acquisition_command(&broken_core),
            None,
            "a broken installation is not repaired by installing the same build again"
        );
        assert_eq!(
            acquisition_command(&missing_firmware),
            None,
            "BitArchive never acquires firmware"
        );
    }

    /// The advice names the store the run actually used, so it repairs that store.
    #[test]
    fn the_acquisition_advice_names_the_store_that_was_used() {
        let default = AcquisitionCommand::Runtime.render(None);
        let rooted = AcquisitionCommand::Runtime.render(Some(Path::new("/tmp/store")));

        assert!(default.ends_with("acquire-retroarch-runtime"));
        assert!(rooted.ends_with("acquire-retroarch-runtime --root /tmp/store"));
    }

    /// The deduplication of the advice keeps one line per component class, so a run
    /// that misses both components is not told to install the core twice.
    #[test]
    fn the_acquisition_advice_lists_each_component_class_once() {
        let blockers = [
            LaunchBlocker::CoreUnusable {
                component_id: String::from("mgba"),
                build_id: String::from("mgba-0.11-212-7a12d6d"),
                reason: CoreUnusableReason::NotInstalled,
            },
            LaunchBlocker::RuntimeMissing {
                id: String::from("retroarch"),
            },
            LaunchBlocker::CoreUnusable {
                component_id: String::from("mgba"),
                build_id: String::from("mgba-0.11-212-7a12d6d"),
                reason: CoreUnusableReason::NotInstalled,
            },
        ];

        let mut commands: Vec<AcquisitionCommand> = Vec::new();

        for blocker in &blockers {
            if let Some(command) = acquisition_command(blocker)
                && !commands.contains(&command)
            {
                commands.push(command);
            }
        }

        assert_eq!(
            commands,
            [AcquisitionCommand::Core, AcquisitionCommand::Runtime],
            "each component class is advised once, in blocker order"
        );
        assert_eq!(
            AcquisitionCommand::Core.name(),
            ACQUIRE_CORE_COMMAND,
            "the advice names the existing command"
        );
    }

    /// Every blocker renders a description, so a developer always learns what is
    /// wrong.
    #[test]
    fn every_blocker_renders_a_description() {
        let blockers = [
            LaunchBlocker::RuntimeMissing {
                id: String::from("retroarch"),
            },
            LaunchBlocker::UnsupportedSystemOrCore {
                system: Some(String::from("gba")),
                reason: String::from("no curated definition"),
            },
            LaunchBlocker::UnsupportedSystemOrCore {
                system: None,
                reason: String::from("no curated system"),
            },
            LaunchBlocker::CoreUnusable {
                component_id: String::from("mgba"),
                build_id: String::from("mgba-0.11-212-7a12d6d"),
                reason: CoreUnusableReason::NotInstalled,
            },
            LaunchBlocker::ContentMissing,
            LaunchBlocker::FirmwareMissing {
                filenames: vec![String::from("bios.bin")],
            },
            LaunchBlocker::InvalidConfiguration {
                key: String::from("Video_Vsync"),
            },
        ];

        for blocker in blockers {
            assert!(
                !describe(&blocker).is_empty(),
                "{blocker:?} must be explainable to a developer"
            );
        }
    }
}
