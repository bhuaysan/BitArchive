//! RetroArch launch preparation.
//!
//! This module turns already resolved RetroArch launch inputs into the
//! backend-neutral [`PreparedLaunch`] contract. It is the only place that knows
//! how a RetroArch command line is spelled.
//!
//! # The CLI contract
//!
//! The argument construction follows the official RetroArch documentation:
//!
//! - <https://docs.libretro.com/guides/cli-intro/> —
//!   `retroarch -L /path/to/libretro/core.so game.rom` and
//!   `retroarch --config customconfig.cfg`
//! - <https://man.archlinux.org/man/retroarch.6.en> — `-L PATH, --libretro
//!   PATH`, `--config PATH, -c PATH`, and the positional `[rom file(s)]`
//!
//! The resulting forms are therefore:
//!
//! ```text
//! -L <core> <content>
//! --config <config> -L <core> <content>
//! ```
//!
//! Every value is its own argument. `-L` is the documented short form of
//! `--libretro`.
//!
//! # What this module does not do
//!
//! Preparation is deterministic and free of side effects:
//!
//! - no process is started, and nothing here reaches for [`std::process`];
//! - no path is inspected, so a prepared launch never depends on what happens
//!   to be on disk;
//! - no runtime or core is discovered or installed;
//! - no RetroArch configuration is generated and no core options are resolved;
//! - no flag is added that the caller did not provide. Fullscreen, verbose
//!   logging, menu mode, shaders, save states, recording, and netplay are
//!   settings of the configured RetroArch configuration, not hidden defaults
//!   of this builder.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bitarchive_application::PreparedLaunch;

/// The flag that names the libretro core to load.
///
/// The documented short form of `--libretro`.
const CORE_FLAG: &str = "-L";

/// The flag that names the configuration file to load.
const CONFIG_FLAG: &str = "--config";

/// Already fully resolved technical inputs for one RetroArch launch.
///
/// Every field is a technical path or value. None of them is a domain identity:
/// a path is a location, while a `CoreId` or `ContentId` identifies something
/// else entirely (ARCHITECTURE.md §2.3). Turning identities into these paths is
/// the job of the resolution steps that run before this one.
///
/// Nothing here is inspected. The input does not claim whether a file exists,
/// whether it was installed by BitArchive, whether a hash matches, or whether
/// the firmware a core needs is present. Those are readiness and component
/// management questions, and launch preparation deliberately does not answer
/// them.
///
/// ```
/// use bitarchive_emulation::RetroArchLaunchInput;
///
/// let input = RetroArchLaunchInput::new(
///     "/runtime/RetroArch",
///     "/cores/test_libretro.dylib",
///     "/roms/game.rom",
/// )
/// .with_config("/sessions/123/retroarch.cfg");
///
/// assert_eq!(input.content(), std::path::Path::new("/roms/game.rom"));
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RetroArchLaunchInput {
    executable: PathBuf,
    core: PathBuf,
    content: PathBuf,
    config: Option<PathBuf>,
    environment: Vec<(OsString, OsString)>,
    working_directory: Option<PathBuf>,
}

impl RetroArchLaunchInput {
    /// Creates inputs for launching `content` with `core` through `executable`.
    ///
    /// The launch starts without a configuration file, without environment
    /// overrides, and without a working directory. Add them with
    /// [`with_config`](Self::with_config),
    /// [`with_environment`](Self::with_environment), and
    /// [`with_working_directory`](Self::with_working_directory).
    #[must_use]
    pub fn new(
        executable: impl Into<PathBuf>,
        core: impl Into<PathBuf>,
        content: impl Into<PathBuf>,
    ) -> Self {
        Self {
            executable: executable.into(),
            core: core.into(),
            content: content.into(),
            config: None,
            environment: Vec::new(),
            working_directory: None,
        }
    }

    /// Sets the generated RetroArch configuration file to load.
    ///
    /// The configuration is optional: without one RetroArch falls back to its
    /// own lookup rules, which is why an input without a config is complete
    /// rather than incomplete.
    #[must_use]
    pub fn with_config(mut self, config: impl Into<PathBuf>) -> Self {
        self.config = Some(config.into());
        self
    }

    /// Sets environment overrides for the process start.
    ///
    /// The list stays ordered exactly as given, and an empty list is valid. It
    /// holds additional or explicit values; the platform process adapter
    /// decides how they combine with the inherited environment.
    #[must_use]
    pub fn with_environment(mut self, environment: Vec<(OsString, OsString)>) -> Self {
        self.environment = environment;
        self
    }

    /// Sets the working directory the process should start in.
    #[must_use]
    pub fn with_working_directory(mut self, working_directory: impl Into<PathBuf>) -> Self {
        self.working_directory = Some(working_directory.into());
        self
    }

    /// Returns the RetroArch executable to start.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns the libretro core to load.
    #[must_use]
    pub fn core(&self) -> &Path {
        &self.core
    }

    /// Returns the content to launch.
    #[must_use]
    pub fn content(&self) -> &Path {
        &self.content
    }

    /// Returns the configuration file to load, if one was configured.
    #[must_use]
    pub fn config(&self) -> Option<&Path> {
        self.config.as_deref()
    }

    /// Returns the environment overrides, in the order they were supplied.
    #[must_use]
    pub fn environment(&self) -> &[(OsString, OsString)] {
        &self.environment
    }

    /// Returns the working directory, or [`None`] when the process adapter
    /// decides.
    #[must_use]
    pub fn working_directory(&self) -> Option<&Path> {
        self.working_directory.as_deref()
    }
}

/// The concrete RetroArch emulation backend.
///
/// The backend is stateless, and for now it has exactly one capability:
/// turning an already resolved [`RetroArchLaunchInput`] into a
/// [`PreparedLaunch`].
///
/// Preparing a launch cannot fail at this stage, so
/// [`prepare_launch`](Self::prepare_launch) has no error type. It resolves
/// nothing and inspects nothing, so there is no failure mode left to report;
/// whether a launch *may* proceed is answered by a readiness check before
/// preparation, not by preparation itself. An error framework is introduced
/// with the first step that can actually fail, together with the port that
/// carries it.
///
/// ```
/// use bitarchive_emulation::{RetroArchBackend, RetroArchLaunchInput};
///
/// let input = RetroArchLaunchInput::new(
///     "/runtime/RetroArch",
///     "/cores/test_libretro.dylib",
///     "/roms/game.rom",
/// );
///
/// let prepared = RetroArchBackend::new().prepare_launch(&input);
///
/// assert_eq!(prepared.arguments().len(), 3);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct RetroArchBackend;

impl RetroArchBackend {
    /// Creates the backend.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Builds the prepared launch for `input`.
    ///
    /// The arguments are assembled as the RetroArch CLI documents them, and
    /// every value becomes its own argument:
    ///
    /// ```text
    /// -L <core> <content>
    /// --config <config> -L <core> <content>
    /// ```
    ///
    /// The executable is not repeated inside the arguments, because it is the
    /// program the process API starts. A path containing spaces therefore
    /// reaches the process as one argument, without quotes and without any
    /// other shell escaping.
    #[must_use]
    pub fn prepare_launch(&self, input: &RetroArchLaunchInput) -> PreparedLaunch {
        // The config, when present, comes first so that the rest of the command
        // line keeps one fixed shape. Every element is pushed separately: the
        // values are never joined into a command string.
        let argument_count = if input.config().is_some() { 5 } else { 3 };
        let mut arguments = Vec::with_capacity(argument_count);

        if let Some(config) = input.config() {
            arguments.push(OsString::from(CONFIG_FLAG));
            arguments.push(config.as_os_str().to_owned());
        }

        arguments.push(OsString::from(CORE_FLAG));
        arguments.push(input.core().as_os_str().to_owned());
        arguments.push(input.content().as_os_str().to_owned());

        PreparedLaunch::new(input.executable(), arguments)
            .with_environment(input.environment().to_vec())
            .with_working_directory(input.working_directory().map(Path::to_path_buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The documented executable used by the launch tests.
    const EXECUTABLE: &str = "/runtime/RetroArch";
    /// The documented core used by the launch tests.
    const CORE: &str = "/cores/test_libretro.dylib";
    /// The documented content used by the launch tests.
    const CONTENT: &str = "/roms/game.rom";

    /// Builds the default input used by most tests.
    fn input() -> RetroArchLaunchInput {
        RetroArchLaunchInput::new(EXECUTABLE, CORE, CONTENT)
    }

    /// Asserts that `launch` carries exactly `expected` as its arguments.
    ///
    /// The comparison is exact on purpose: it pins the order and, in the same
    /// step, proves that nothing else was appended.
    fn assert_arguments(launch: &PreparedLaunch, expected: &[&str]) {
        let expected: Vec<OsString> = expected.iter().copied().map(OsString::from).collect();

        assert_eq!(
            launch.arguments(),
            expected.as_slice(),
            "the argument vector must be exactly the documented RetroArch form"
        );
        assert!(
            !launch.arguments().iter().any(|argument| {
                let argument = argument.to_string_lossy();

                // A joined command line would fuse a flag with its value.
                argument.contains(' ') && argument.starts_with('-')
            }),
            "no argument may be a joined command line"
        );
    }

    /// The executable is the program to start, so it never appears again as an
    /// argument.
    #[test]
    fn the_executable_is_not_repeated_inside_the_arguments() {
        let launch = RetroArchBackend::new().prepare_launch(&input());

        assert_eq!(launch.executable(), Path::new(EXECUTABLE));
        assert!(
            !launch
                .arguments()
                .iter()
                .any(|argument| argument == EXECUTABLE),
            "the executable is not argv[0] of the argument list"
        );
    }

    /// Without a configuration the command line is exactly the documented
    /// `-L <core> <content>` form: three arguments, nothing before them, and
    /// nothing appended.
    #[test]
    fn a_launch_without_config_uses_the_documented_core_and_content_form() {
        let launch = RetroArchBackend::new().prepare_launch(&input());

        assert_arguments(&launch, &[CORE_FLAG, CORE, CONTENT]);
        assert!(launch.environment().is_empty());
        assert_eq!(launch.working_directory(), None);
    }

    /// With a configuration the config is named explicitly and deterministically
    /// ahead of the core, so the generated file is the one RetroArch loads
    /// instead of whatever it would find on its own.
    #[test]
    fn a_launch_with_config_names_the_config_explicitly() {
        let config = "/sessions/123/retroarch.cfg";
        let input = input().with_config(config);

        let launch = RetroArchBackend::new().prepare_launch(&input);

        assert_arguments(&launch, &[CONFIG_FLAG, config, CORE_FLAG, CORE, CONTENT]);
    }

    /// Two preparations of the same input produce the same launch, so argument
    /// construction carries no hidden state, no ordering dependency, and no
    /// generated value.
    #[test]
    fn preparing_the_same_input_twice_is_deterministic() {
        let input = input().with_config("/sessions/123/retroarch.cfg");

        let first = RetroArchBackend::new().prepare_launch(&input);
        let second = RetroArchBackend::new().prepare_launch(&input);

        assert_eq!(first, second);
    }

    /// Paths containing spaces stay one argument each, and no shell quoting is
    /// introduced anywhere.
    #[test]
    fn paths_with_spaces_stay_atomic_arguments() {
        let core = "/Users/Test/Retro Cores/test_libretro.dylib";
        let content = "/Users/Test/My Games/game.rom";
        let config = "/Users/Test/My Sessions/123/retroarch.cfg";
        let input = RetroArchLaunchInput::new(EXECUTABLE, core, content).with_config(config);

        let launch = RetroArchBackend::new().prepare_launch(&input);

        assert_arguments(&launch, &[CONFIG_FLAG, config, CORE_FLAG, core, content]);

        // The paths are still single arguments, so quoting them would be wrong:
        // nothing may have wrapped them.
        for argument in [core, content, config] {
            assert!(
                launch
                    .arguments()
                    .iter()
                    .any(|candidate| candidate == argument),
                "{argument} must be present as one argument"
            );
        }

        for argument in launch.arguments() {
            let argument = argument.to_string_lossy();

            assert!(
                !argument.contains('\'') && !argument.contains('"'),
                "no argument may be quoted: {argument}"
            );
        }
    }

    /// A path that does not exist is prepared exactly like any other path,
    /// which is what makes preparation free of filesystem access: no existence
    /// check, no canonicalization, and no substitution happens here.
    #[test]
    fn preparation_neither_inspects_paths_nor_starts_a_process() {
        let executable = "/nonexistent/RetroArch";
        let core = "/nonexistent/cores/does_not_exist_libretro.dylib";
        let content = "/nonexistent/roms/does_not_exist.rom";
        let input = RetroArchLaunchInput::new(executable, core, content);

        let launch = RetroArchBackend::new().prepare_launch(&input);

        assert_arguments(&launch, &[CORE_FLAG, core, content]);
        assert_eq!(
            launch.executable(),
            Path::new(executable),
            "the executable is not resolved or replaced"
        );
    }

    /// Only the documented flags appear. No fullscreen, verbose, menu, shader,
    /// save-state, recording, or netplay default is added behind the caller's
    /// back.
    #[test]
    fn no_hidden_retroarch_flags_are_added() {
        let input = input().with_config("/sessions/123/retroarch.cfg");

        let launch = RetroArchBackend::new().prepare_launch(&input);

        let undocumented = [
            "--fullscreen",
            "-f",
            "--verbose",
            "-v",
            "--menu",
            "--appendconfig",
            "--set-shader",
            "--save",
            "--savestate",
            "--record",
            "--host",
            "--connect",
        ];

        for flag in undocumented {
            assert!(
                !launch.arguments().iter().any(|argument| argument == flag),
                "{flag} must not be added by the backend"
            );
        }

        assert_eq!(
            launch.arguments().len(),
            5,
            "only --config, -L, and their two values plus the content are passed"
        );
    }

    /// Environment overrides and the working directory reach the prepared
    /// launch unchanged, including values that contain spaces.
    #[test]
    fn environment_and_working_directory_are_transported_unchanged() {
        let environment = vec![
            (
                OsString::from("LIBRETRO_SYSTEM_DIRECTORY"),
                OsString::from("/Users/Test/My Firmware"),
            ),
            (
                OsString::from("LIBRETRO_DIRECTORY"),
                OsString::from("/Users/Test/Retro Cores"),
            ),
        ];
        let working_directory = "/Users/Test/My Sessions/123";
        let input = input()
            .with_environment(environment.clone())
            .with_working_directory(working_directory);

        let launch = RetroArchBackend::new().prepare_launch(&input);

        assert_eq!(launch.environment(), environment.as_slice());
        assert_eq!(
            launch.working_directory(),
            Some(Path::new(working_directory))
        );
    }

    /// The inputs an emulation start actually resolves are reachable through
    /// the input accessors, so a later step can inspect what was prepared
    /// without re-deriving it.
    #[test]
    fn the_input_keeps_the_values_it_was_created_with() {
        let input = input().with_config("/sessions/123/retroarch.cfg");

        assert_eq!(input.executable(), Path::new(EXECUTABLE));
        assert_eq!(input.core(), Path::new(CORE));
        assert_eq!(input.content(), Path::new(CONTENT));
        assert_eq!(
            input.config(),
            Some(Path::new("/sessions/123/retroarch.cfg"))
        );
        assert!(input.environment().is_empty());
        assert_eq!(input.working_directory(), None);
    }
}
