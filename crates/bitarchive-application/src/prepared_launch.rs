//! The backend-neutral contract for an already prepared process launch.
//!
//! [`PreparedLaunch`] is the last value the inner layers produce before a launch
//! leaves them. It describes *what* should be started in exactly the shape a
//! process API consumes, and it starts nothing (ARCHITECTURE.md §20.2, §20.3).
//!
//! ```text
//! resolution steps
//!       ↓
//! PreparedLaunch   ← this contract, still backend-neutral
//!       ↓
//! Process Spawn    ← later, outside this crate
//! ```
//!
//! # Boundary
//!
//! This is the one place in the application layer that carries concrete
//! technical values: an executable, process arguments, environment overrides,
//! and an optional working directory. [`LaunchRequest`](crate::LaunchRequest)
//! still names only a game and an action. The concrete paths appear here
//! because this type is the *result* of resolving a request, not the request
//! itself.
//!
//! Carrying them does not make this crate an adapter. It still performs no I/O,
//! never reaches for [`std::process`], and defines no RetroArch-specific type.
//!
//! # No shell
//!
//! The values use the standard library path and [`OsString`] types on purpose:
//!
//! - a path is not a `String`, so a launch never assumes UTF-8 file names;
//! - every argument is its own [`OsString`], so a path containing spaces stays
//!   exactly one argument;
//! - there is no joined command line, no shell script, and no quoting or
//!   escaping for a shell, because no shell takes part in the launch
//!   (ARCHITECTURE.md §20.3).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A fully resolved process launch, ready for a process adapter.
///
/// A prepared launch is a description, not an action. Creating one performs no
/// I/O, validates nothing, and starts no process; the process adapter that runs
/// it later is a separate component in an outer layer.
///
/// # Arguments
///
/// [`arguments`](Self::arguments) holds the arguments that follow the
/// executable, so the executable is not repeated as the first element. It is a
/// plain ordered list: the order is significant, duplicates are meaningful, and
/// each entry is one argument exactly as the process should receive it.
///
/// # Environment
///
/// [`environment`](Self::environment) holds additional or explicit values for
/// the process start. It deliberately does **not** mean "replace the whole
/// parent environment": whether an override is merged into the inherited
/// environment is decided by the platform process adapter, not by this
/// contract.
///
/// ```
/// use std::ffi::OsString;
///
/// use bitarchive_application::PreparedLaunch;
///
/// let launch = PreparedLaunch::new(
///     "/runtime/RetroArch",
///     vec![OsString::from("-L"), OsString::from("/cores/test_libretro.dylib")],
/// );
///
/// assert_eq!(launch.arguments().len(), 2);
/// assert!(launch.environment().is_empty());
/// assert_eq!(launch.working_directory(), None);
/// ```
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PreparedLaunch {
    executable: PathBuf,
    arguments: Vec<OsString>,
    environment: Vec<(OsString, OsString)>,
    working_directory: Option<PathBuf>,
}

impl PreparedLaunch {
    /// Creates a prepared launch for `executable` with `arguments`.
    ///
    /// The launch starts without environment overrides and without a working
    /// directory, so the process adapter inherits both from the launcher. Add
    /// them with [`with_environment`](Self::with_environment) and
    /// [`with_working_directory`](Self::with_working_directory).
    #[must_use]
    pub fn new(executable: impl Into<PathBuf>, arguments: Vec<OsString>) -> Self {
        Self {
            executable: executable.into(),
            arguments,
            environment: Vec::new(),
            working_directory: None,
        }
    }

    /// Sets the environment overrides for the process start.
    ///
    /// The list stays ordered exactly as given. It is a `Vec` rather than a map
    /// because the order is part of the deterministic result; an empty list is
    /// a valid prepared launch.
    #[must_use]
    pub fn with_environment(mut self, environment: Vec<(OsString, OsString)>) -> Self {
        self.environment = environment;
        self
    }

    /// Sets the working directory the process should start in.
    ///
    /// [`None`] means the process adapter decides, which normally means
    /// inheriting the launcher's working directory.
    #[must_use]
    pub fn with_working_directory(mut self, working_directory: Option<PathBuf>) -> Self {
        self.working_directory = working_directory;
        self
    }

    /// Returns the executable to start.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns the arguments that follow the executable, in launch order.
    #[must_use]
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The executable survives unchanged, including characters that are not
    /// valid UTF-8 boundaries in every encoding, and is never rewritten,
    /// resolved, or replaced by a name.
    #[test]
    fn the_executable_is_kept_exactly() {
        let executable = PathBuf::from("/runtime/RetroArch.app/Contents/MacOS/RetroArch");

        let launch = PreparedLaunch::new(executable.clone(), Vec::new());

        assert_eq!(launch.executable(), executable.as_path());
    }

    /// Arguments keep the order they were given in, and repeated or
    /// flag-looking entries stay separate arguments rather than being merged,
    /// deduplicated, or reordered.
    #[test]
    fn argument_order_is_preserved() {
        let arguments: Vec<OsString> = ["--config", "/a.cfg", "-L", "/a.cfg"]
            .into_iter()
            .map(OsString::from)
            .collect();

        let launch = PreparedLaunch::new("/runtime/RetroArch", arguments.clone());

        assert_eq!(launch.arguments(), arguments.as_slice());
        assert_eq!(
            launch.arguments()[0],
            OsString::from("--config"),
            "the first argument stays first"
        );
        assert_eq!(
            launch.arguments()[3],
            OsString::from("/a.cfg"),
            "a repeated value is not deduplicated and keeps its position"
        );
    }

    /// One argument holding spaces stays a single argument: nothing splits it
    /// into several arguments and nothing quotes it.
    #[test]
    fn an_argument_containing_spaces_stays_one_argument() {
        let core = OsString::from("/Users/Test/Retro Cores/test_libretro.dylib");

        let launch = PreparedLaunch::new(
            "/runtime/RetroArch",
            vec![OsString::from("-L"), core.clone()],
        );

        assert_eq!(launch.arguments().len(), 2, "the path is one argument");
        assert_eq!(launch.arguments()[1], core);
        assert!(
            !launch.arguments()[1].to_string_lossy().contains('\''),
            "no shell quoting is added to the value"
        );
    }

    /// Without environment overrides the launch carries an empty list, so
    /// "no override" is representable and is the default.
    #[test]
    fn a_launch_without_overrides_has_an_empty_environment() {
        let launch = PreparedLaunch::new("/runtime/RetroArch", Vec::new());

        assert!(launch.environment().is_empty());
        assert_eq!(launch.working_directory(), None);
    }

    /// Environment overrides keep both their values and their order, so the
    /// deterministic result of a preparation survives into the process start.
    #[test]
    fn environment_overrides_are_kept_unchanged_in_order() {
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

        let launch = PreparedLaunch::new("/runtime/RetroArch", Vec::new())
            .with_environment(environment.clone());

        assert_eq!(launch.environment(), environment.as_slice());
        assert_eq!(
            launch.environment()[0].0,
            OsString::from("LIBRETRO_SYSTEM_DIRECTORY"),
            "the first override stays first"
        );
    }

    /// A working directory survives unchanged, and setting one does not disturb
    /// the other fields.
    #[test]
    fn the_working_directory_is_kept() {
        let launch = PreparedLaunch::new("/runtime/RetroArch", vec![OsString::from("-L")])
            .with_working_directory(Some(PathBuf::from("/sessions/123")));

        assert_eq!(launch.working_directory(), Some(Path::new("/sessions/123")));
        assert_eq!(launch.executable(), Path::new("/runtime/RetroArch"));
        assert_eq!(launch.arguments(), [OsString::from("-L")]);
    }
}
