//! Developer smoke launcher: start a real RetroArch process with a real core
//! and real content.
//!
//! This example is **developer tooling**, not a product flow. It is the first
//! place where the launch path built in B1–B4 is composed into one running
//! process, and it exists to answer exactly one question by hand:
//!
//! > Can BitArchive start a real RetroArch process with a real core and real
//! > content through the intended architecture?
//!
//! ```text
//! developer supplies concrete paths
//!         ↓
//! RetroArchLaunchInput                  (bitarchive-emulation)
//!         ↓
//! RetroArchBackend::prepare_launch()    (bitarchive-emulation)
//!         ↓
//! PreparedLaunch                        (bitarchive-application)
//!         ↓
//! ProcessController::spawn()            (bitarchive-platform)
//!         ↓
//! SpawnedProcess
//!         ↓
//! real RetroArch process, waited on until it exits
//! ```
//!
//! It runs in the desktop package because that package is the composition root
//! (ARCHITECTURE.md §5.7): composing a concrete emulation backend with a
//! concrete platform service is precisely what a composition root does, and it
//! is the only layer allowed to know both.
//!
//! # Usage
//!
//! ```text
//! cargo run --locked -p bitarchive-desktop --example retroarch_smoke -- \
//!   <retroarch-executable> <core-library> <content> [config]
//! ```
//!
//! The three paths are required, the RetroArch configuration file is optional.
//! Paths are passed as separate process arguments, so a path containing spaces
//! works without any shell quoting.
//!
//! # What this launcher deliberately is not
//!
//! - **Not a user-facing feature.** It adds no UI, no setting, no persisted
//!   value, and no product configuration. A RetroArch executable path is not a
//!   user setting here: it is an argument of one manual developer invocation.
//! - **Not a play flow.** Starting games from the BitArchive UI is a later
//!   step. There is no `LaunchService`, no resolution from stored data, and no
//!   application orchestration.
//! - **Not a readiness check.** No path is inspected, no core compatibility is
//!   decided, no firmware is looked for, no content is validated, and no hash
//!   is computed. Those capabilities do not exist yet, and this launcher works
//!   with concrete paths a developer already knows instead of pretending to
//!   resolve them.
//! - **Not a session.** There is no session manager, no single-session rule, no
//!   playtime, no recovery, and no session persistence
//!   (ARCHITECTURE.md §22).
//! - **Not a process supervisor.** The launcher starts RetroArch, reports its
//!   process id, waits for it, and reaps it. It never terminates, signals,
//!   kills, or forces the process to exit, and it never polls in the
//!   background. During a manual smoke test the developer quits RetroArch
//!   normally, exactly as a user would.
//! - **Not a RetroArch command-line builder.** `-L`, `--config`, and the core
//!   argument order are the sole responsibility of `RetroArchBackend`. This
//!   example passes paths on and never assembles an argument list itself.
//! - **Not a resource installer.** It downloads and copies nothing. Only
//!   already present, legally available local resources are used.
//!
//! # Outcome
//!
//! The process exits successfully when RetroArch started and ended
//! successfully, and reports the technical reason on standard error otherwise:
//! a usage error, a launch that could not be started, a failed wait, or a
//! non-zero RetroArch exit are all distinguishable and all fail the process.

// The launcher owns no unsafe code and needs none: starting and waiting for a
// process is fully expressed by the standard library.
#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::path::PathBuf;

use bitarchive_emulation::{RetroArchBackend, RetroArchLaunchInput};
use bitarchive_platform::ProcessController;

/// The program name shown in the usage text.
const PROGRAM: &str = "retroarch_smoke";

/// The number of path arguments a smoke run must supply.
const REQUIRED_ARGUMENTS: usize = 3;

/// The number of path arguments a smoke run may supply at most, the fourth
/// being the optional RetroArch configuration file.
const MAXIMUM_ARGUMENTS: usize = REQUIRED_ARGUMENTS + 1;

/// What one smoke run was asked to start.
struct SmokeRun {
    /// The RetroArch executable to start.
    executable: PathBuf,
    /// The libretro core to load.
    core: PathBuf,
    /// The content to launch.
    content: PathBuf,
    /// The RetroArch configuration file to load, when one was supplied.
    config: Option<PathBuf>,
}

fn main() -> std::process::ExitCode {
    // `args_os` rather than `args`: a technical path is a sequence of platform
    // characters, not necessarily UTF-8, so it must never have to survive a
    // text conversion that could fail or silently replace bytes.
    let arguments: Vec<OsString> = std::env::args_os().collect();

    match SmokeRun::from_arguments(&arguments) {
        Ok(run) => run.execute(),
        Err(UsageError) => {
            eprintln!("{PROGRAM}: {}\n\n{}", UsageError, usage());
            std::process::ExitCode::FAILURE
        }
    }
}

impl SmokeRun {
    /// Reads the three required paths and the optional config path from the
    /// process arguments.
    ///
    /// The first argument is the program name and is therefore skipped. Every
    /// remaining argument is one path, taken verbatim: nothing is split, joined,
    /// quoted, unquoted, expanded, or interpreted.
    ///
    /// # Errors
    ///
    /// Returns [`UsageError`] when the number of path arguments is neither
    /// [`REQUIRED_ARGUMENTS`] nor [`MAXIMUM_ARGUMENTS`]. A caller that receives
    /// this error must report usage and start no process.
    fn from_arguments(arguments: &[OsString]) -> Result<Self, UsageError> {
        let paths = arguments.get(1..).unwrap_or_default();

        // The length is checked explicitly rather than left to a slice pattern:
        // a pattern with a `..` rest would silently ignore surplus paths, and an
        // invocation that names more paths than this launcher understands must
        // be refused instead of quietly dropping them.
        if !(REQUIRED_ARGUMENTS..=MAXIMUM_ARGUMENTS).contains(&paths.len()) {
            return Err(UsageError);
        }

        let [executable, core, content, config @ ..] = paths else {
            return Err(UsageError);
        };

        Ok(Self {
            executable: executable.into(),
            core: core.into(),
            content: content.into(),
            config: config.first().map(PathBuf::from),
        })
    }

    /// Composes the launcher from the existing components and waits for the
    /// started process.
    ///
    /// The only work done here is composition: the launch input is built, the
    /// RetroArch backend prepares it, and the platform process controller starts
    /// exactly that prepared launch.
    fn execute(self) -> std::process::ExitCode {
        let Self {
            executable,
            core,
            content,
            config,
        } = self;

        // The concrete paths go into the emulation input unchanged. Preparation
        // never inspects them, so this call cannot fail: whether a path exists
        // is decided by the operating system when the process is actually
        // started.
        let input = RetroArchLaunchInput::new(&executable, &core, &content);
        let input = match config {
            Some(config) => input.with_config(config),
            None => input,
        };

        // The argument list is built by the backend and nowhere else. This
        // example never names `-L`, `--config`, or an argument position.
        let prepared = RetroArchBackend::new().prepare_launch(&input);

        // Starting the process is the one platform capability composed in here.
        // There is no shell: the executable is the program, and the prepared
        // arguments are handed over as separate values.
        let mut process = match ProcessController::new().spawn(&prepared) {
            Ok(process) => process,
            Err(error) => {
                eprintln!(
                    "{PROGRAM}: could not start {}: {error}",
                    executable.display()
                );

                return std::process::ExitCode::FAILURE;
            }
        };

        // The process id is available as soon as the process was started, which
        // is what makes the running RetroArch observable to the developer.
        let id = process.id();
        println!("RetroArch started (PID {id})");
        println!(
            "Waiting for RetroArch to exit; quit RetroArch normally to finish the smoke test."
        );

        // Waiting is unconditional: every started child is reaped, whatever its
        // exit status turns out to be. A started process that was never waited
        // on would stay a zombie on Unix.
        let status = match process.wait() {
            Ok(status) => status,
            Err(error) => {
                eprintln!("{PROGRAM}: could not wait for RetroArch (PID {id}): {error}");

                return std::process::ExitCode::FAILURE;
            }
        };

        if status.is_success() {
            println!("RetroArch exited successfully (PID {id})");

            return std::process::ExitCode::SUCCESS;
        }

        // A non-zero exit code and an operating system without a numeric code
        // are different facts and are reported as such. No status is retried,
        // reinterpreted, or repaired.
        match status.code() {
            Some(code) => eprintln!("RetroArch exited with code {code} (PID {id})"),
            None => eprintln!(
                "RetroArch did not exit with an exit code (PID {id}); it was most likely \
                 terminated by a signal"
            ),
        }

        std::process::ExitCode::FAILURE
    }
}

/// The one usage failure this launcher has: the number of path arguments is
/// wrong.
///
/// It is deliberately not an error framework. A wrong invocation is decided
/// before anything is started, so it needs no cause chain and no typed
/// categories — it needs a usage text and a non-zero exit code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct UsageError;

impl std::fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expected {REQUIRED_ARGUMENTS} paths (<retroarch-executable> <core-library> <content>) \
             and at most one optional <config>"
        )
    }
}

/// The usage text, written to standard error when the invocation is wrong.
///
/// The accepted invocation is derived from [`REQUIRED_ARGUMENTS`] and
/// [`MAXIMUM_ARGUMENTS`] instead of being spelled out a second time, so the
/// documented form and the accepted form cannot drift apart.
fn usage() -> String {
    let executable = std::env::args_os()
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(PROGRAM));

    let required = "<retroarch-executable> <core-library> <content>";
    let form = if MAXIMUM_ARGUMENTS > REQUIRED_ARGUMENTS {
        format!("{required} [config]")
    } else {
        required.to_owned()
    };

    format!(
        "\
Usage:
  {PROGRAM} {form}

Arguments:
  <retroarch-executable>  the RetroArch executable to start
  <core-library>          the libretro core to load
  <content>               the content to launch
  [config]                optional RetroArch configuration file to load

Pass every path as one argument; paths containing spaces need no quoting.

Runs as a developer example:
  cargo run --locked -p bitarchive-desktop --example retroarch_smoke -- \
    {form}

This is developer tooling, not a product flow. It starts the given RetroArch
with the given core and content, reports the process id, waits until RetroArch
exits, and never terminates the process itself.

Started from: {}",
        executable.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an argument vector with a program name and the given paths, so the
    /// tests describe invocation shapes instead of constructing vectors by hand.
    fn arguments(paths: &[&str]) -> Vec<OsString> {
        std::iter::once(OsString::from(PROGRAM))
            .chain(paths.iter().map(OsString::from))
            .collect()
    }

    /// The three required paths are enough, and the run carries them unchanged.
    #[test]
    fn three_paths_are_accepted_without_a_config() {
        let run = SmokeRun::from_arguments(&arguments(&[
            "/runtime/RetroArch",
            "/cores/test_libretro.dylib",
            "/roms/game.rom",
        ]))
        .expect("three paths are a complete invocation");

        assert_eq!(run.executable, PathBuf::from("/runtime/RetroArch"));
        assert_eq!(run.core, PathBuf::from("/cores/test_libretro.dylib"));
        assert_eq!(run.content, PathBuf::from("/roms/game.rom"));
        assert_eq!(run.config, None, "the config path is optional");
    }

    /// A fourth path is the optional configuration file and is kept as such.
    #[test]
    fn a_fourth_path_is_the_optional_config() {
        let run = SmokeRun::from_arguments(&arguments(&[
            "/runtime/RetroArch",
            "/cores/test_libretro.dylib",
            "/roms/game.rom",
            "/sessions/123/retroarch.cfg",
        ]))
        .expect("four paths are a complete invocation with a config");

        assert_eq!(
            run.config,
            Some(PathBuf::from("/sessions/123/retroarch.cfg"))
        );
    }

    /// Paths are taken verbatim. A path with spaces stays one path, and a path
    /// that looks like an option is not treated as one: this launcher has no
    /// option syntax at all.
    #[test]
    fn paths_are_taken_verbatim() {
        let core = "/Users/Test/Retro Cores/test_libretro.dylib";
        let content = "/Users/Test/My Games/game.rom";
        let config = "--not-an-option";

        let run =
            SmokeRun::from_arguments(&arguments(&["/runtime/RetroArch", core, content, config]))
                .expect("four paths are a complete invocation");

        assert_eq!(run.core, PathBuf::from(core), "a space is not a separator");
        assert_eq!(run.content, PathBuf::from(content));
        assert_eq!(
            run.config,
            Some(PathBuf::from(config)),
            "the fourth path is a path, not a flag to interpret"
        );
    }

    /// Every wrong argument count is a usage failure, including the invocation
    /// without paths and the invocation with one path too many. None of them may
    /// reach a process start.
    ///
    /// Each case states its own expected outcome, so the accepted and rejected
    /// shapes are both pinned explicitly: a table that merely compared the
    /// outcome against the length again would accept a wrong case as long as it
    /// agreed with itself.
    #[test]
    fn a_wrong_number_of_paths_is_a_usage_error() {
        let invocations: [(&[&str], bool); 6] = [
            (&[], false),
            (&["/runtime/RetroArch"], false),
            (&["/runtime/RetroArch", "/cores/test_libretro.dylib"], false),
            (
                &[
                    "/runtime/RetroArch",
                    "/cores/test_libretro.dylib",
                    "/roms/game.rom",
                    "/sessions/123/retroarch.cfg",
                    "one/too/many",
                ],
                false,
            ),
            (&["/runtime/RetroArch", "", ""], true),
            (
                &["/runtime/RetroArch", "/cores/test_libretro.dylib", ""],
                true,
            ),
        ];

        for (paths, accepted) in invocations {
            assert_eq!(
                SmokeRun::from_arguments(&arguments(paths)).is_ok(),
                accepted,
                "{paths:?} must be accepted exactly when it carries {REQUIRED_ARGUMENTS} or \
                 {MAXIMUM_ARGUMENTS} paths"
            );
        }
    }

    /// The usage failure reads as a sentence and the usage text names the
    /// program, all four positions, and the fact that this is a developer
    /// example rather than a product flow.
    #[test]
    fn the_usage_text_names_the_invocation() {
        let usage = usage();

        assert!(!UsageError.to_string().is_empty());
        assert!(usage.contains(PROGRAM));
        assert!(usage.contains("<retroarch-executable>"));
        assert!(usage.contains("<core-library>"));
        assert!(usage.contains("<content>"));
        assert!(usage.contains("[config]"));
        assert!(usage.contains("--example retroarch_smoke"));
        assert!(
            usage.contains("developer tooling, not a product flow"),
            "the usage text must not present this launcher as a product feature"
        );
        assert!(
            usage.contains("never terminates the process"),
            "the usage text must state that the launcher only waits"
        );
    }
}
