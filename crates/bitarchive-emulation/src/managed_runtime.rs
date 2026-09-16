//! Resolving the executable of the managed RetroArch runtime.
//!
//! This is the end of the acquisition path and the beginning of the launch path.
//! It answers one question — which RetroArch executable should a launch use? — and
//! it answers it from the component store alone:
//!
//! ```text
//! RuntimeInstaller::active_runtime(retroarch)
//!         ↓
//! InstalledRuntime                 ← the activated version and its directory
//!         ↓  definition.executable()
//! components/runtime/retroarch/macos-universal/<version>/RetroArch.app/Contents/MacOS/RetroArch
//!         ↓
//! ManagedRuntime::executable_path()
//!         ↓
//! RetroArchLaunchInput::new(...)   ← a later step (B7), not this one
//! ```
//!
//! # No fallback of any kind
//!
//! There is no search of `/Applications`, no `PATH` lookup, no environment
//! variable, and no user setting. `PRODUCT.md` §16.1 states that an existing
//! RetroArch installation is not required for normal operation, and
//! `PRODUCT.md` §16.3 that BitArchive owns the runtime's version, install state,
//! integrity, and rollback. A fallback would quietly undo both, and it would make
//! "which RetroArch ran?" unanswerable. So a runtime that is not installed is
//! reported as not installed.
//!
//! # Two ways to have no runtime
//!
//! "Not installed yet" and "the active runtime is broken" are different answers,
//! because they lead to different actions: the first is what onboarding installs,
//! the second is an integrity problem. [`RetroArchRuntime::resolve`] therefore
//! separates them instead of collapsing both into `None`.
//!
//! # Why the executable is checked here
//!
//! The store already validates the executable before it installs a version, so a
//! missing executable means the installation changed after the fact — by hand, by
//! a partial cleanup, or by a failed copy. Reporting that as
//! [`RetroArchRuntimeError::ExecutableMissing`] keeps the launch path from handing
//! a nonexistent program to a process adapter, where the failure would be a raw
//! I/O error far away from its cause.

use std::fmt;
use std::path::PathBuf;

use bitarchive_application::managed_runtime::{RuntimeInstaller, RuntimeStoreError};
use bitarchive_domain::runtime::{RuntimeDefinition, RuntimeId, RuntimeVersion};

/// The active, installed RetroArch runtime and the definition it satisfies.
///
/// The value exists only for a version that is installed and whose executable is
/// present, so holding one is evidence that a launch has a runtime to start.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ManagedRuntime {
    definition: RuntimeDefinition,
    directory: PathBuf,
    executable: PathBuf,
}

impl ManagedRuntime {
    /// Creates a value for an activated installation.
    ///
    /// `installed` must be the active installation of the runtime `definition`
    /// describes; [`resolve`] is the way to obtain one.
    #[must_use]
    pub fn new(
        definition: RuntimeDefinition,
        installed: &bitarchive_application::managed_runtime::InstalledRuntime,
    ) -> Self {
        Self {
            directory: installed.directory().to_path_buf(),
            executable: installed.executable_path(),
            definition,
        }
    }

    /// Returns the pinned definition this installation satisfies.
    #[must_use]
    pub const fn definition(&self) -> &RuntimeDefinition {
        &self.definition
    }

    /// Returns the installed version.
    #[must_use]
    pub fn version(&self) -> &RuntimeVersion {
        self.definition.version()
    }

    /// Returns the directory the version is installed in.
    #[must_use]
    pub fn directory(&self) -> &std::path::Path {
        &self.directory
    }

    /// Returns the concrete path of the RetroArch executable.
    ///
    /// This is the value a later launch step passes to
    /// [`RetroArchLaunchInput::new`](crate::RetroArchLaunchInput::new). It is
    /// always inside [`directory`](Self::directory), because the definition's
    /// executable is a relative path that cannot contain `..`.
    #[must_use]
    pub fn executable_path(&self) -> &std::path::Path {
        &self.executable
    }
}

impl fmt::Display for ManagedRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} at {}",
            self.definition.id(),
            self.version(),
            self.directory.display()
        )
    }
}

/// Why no managed RetroArch runtime could be resolved.
///
/// The variants are the two answers a caller needs: install one, or repair one.
#[derive(Debug)]
pub enum RetroArchRuntimeError {
    /// No runtime has been installed and activated yet.
    ///
    /// This is the normal state of a fresh installation and the state onboarding
    /// resolves by acquiring the pinned runtime. It is not an error condition of
    /// the acquisition path itself.
    NotInstalled {
        /// The runtime that is not installed.
        id: RuntimeId,
    },
    /// The component store could not be read.
    ///
    /// Covers a store that cannot be listed, an activation record that cannot be
    /// parsed, and a record that names a version which is no longer installed —
    /// that is, everything [`RuntimeStoreError`] reports about reading state.
    StoreUnreadable {
        /// The underlying cause.
        cause: RuntimeStoreError,
    },
    /// The active installation does not contain the executable it should.
    ExecutableMissing {
        /// The executable path that was expected.
        expected: PathBuf,
        /// The version whose installation is incomplete.
        version: RuntimeVersion,
    },
}

impl fmt::Display for RetroArchRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled { id } => write!(
                f,
                "no managed {id} runtime is installed; the pinned runtime has to be acquired first"
            ),
            Self::StoreUnreadable { cause } => {
                write!(f, "the managed runtime store could not be read: {cause}")
            }
            Self::ExecutableMissing { expected, version } => write!(
                f,
                "the installed RetroArch {version} does not contain the expected executable at {}",
                expected.display()
            ),
        }
    }
}

impl std::error::Error for RetroArchRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StoreUnreadable { cause } => Some(cause),
            Self::NotInstalled { .. } | Self::ExecutableMissing { .. } => None,
        }
    }
}

/// Resolves the executable of the active managed RetroArch runtime.
///
/// # Errors
///
/// Returns [`RetroArchRuntimeError::NotInstalled`] when nothing is activated,
/// [`RetroArchRuntimeError::StoreUnreadable`] when activation state cannot be
/// read, and [`RetroArchRuntimeError::ExecutableMissing`] when the activated
/// version does not contain the executable the definition pins.
pub fn resolve(
    definition: &RuntimeDefinition,
    installer: &dyn RuntimeInstaller,
) -> Result<ManagedRuntime, RetroArchRuntimeError> {
    let id = definition.id();

    let installed = installer
        .active_runtime(id)
        .map_err(|cause| RetroArchRuntimeError::StoreUnreadable { cause })?
        .ok_or_else(|| RetroArchRuntimeError::NotInstalled { id: id.clone() })?;

    let executable = installed.executable_path();

    if !executable.is_file() {
        return Err(RetroArchRuntimeError::ExecutableMissing {
            expected: executable,
            version: installed.version().clone(),
        });
    }

    Ok(ManagedRuntime::new(definition.clone(), &installed))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use bitarchive_application::managed_runtime::InstalledRuntime;
    use bitarchive_domain::runtime::RelativePath;
    use std::str::FromStr;

    use super::*;
    use crate::runtime::pinned_retroarch_runtime;

    /// An installer stub that reports exactly the state a test wants.
    struct StubInstaller {
        active: Result<Option<InstalledRuntime>, &'static str>,
        asked: RefCell<Vec<RuntimeId>>,
    }

    impl StubInstaller {
        fn with_active(installed: Option<InstalledRuntime>) -> Self {
            Self {
                active: Ok(installed),
                asked: RefCell::new(Vec::new()),
            }
        }

        fn failing() -> Self {
            Self {
                active: Err("the store could not be listed"),
                asked: RefCell::new(Vec::new()),
            }
        }
    }

    impl RuntimeInstaller for StubInstaller {
        fn install(
            &self,
            _definition: &RuntimeDefinition,
        ) -> Result<InstalledRuntime, RuntimeStoreError> {
            unreachable!("resolution never installs anything")
        }

        fn active_runtime(
            &self,
            id: &RuntimeId,
        ) -> Result<Option<InstalledRuntime>, RuntimeStoreError> {
            self.asked.borrow_mut().push(id.clone());

            match &self.active {
                Ok(installed) => Ok(installed.clone()),
                Err(message) => Err(RuntimeStoreError::UnreadableStore {
                    directory: PathBuf::from("/components/runtime"),
                    cause: std::io::Error::other(*message),
                }),
            }
        }

        fn installed_versions(
            &self,
            _id: &RuntimeId,
        ) -> Result<Vec<RuntimeVersion>, RuntimeStoreError> {
            unreachable!("resolution never lists versions")
        }
    }

    /// A temporary component root that removes itself.
    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "bitarchive-b5-resolve-{label}-{}",
                std::process::id()
            ));

            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a temporary directory");

            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Creates an installation directory with the pinned executable inside it.
    fn install(root: &TempRoot, version: &str) -> InstalledRuntime {
        let definition = pinned_retroarch_runtime();
        let directory = root
            .path()
            .join("runtime/retroarch/macos-universal")
            .join(version);

        let executable = directory.join(PINNED_EXECUTABLE_RELATIVE);
        std::fs::create_dir_all(executable.parent().expect("a parent directory"))
            .expect("the bundle is created");
        std::fs::write(&executable, b"fake retroarch").expect("the executable is written");

        InstalledRuntime::new(
            definition.id().clone(),
            RuntimeVersion::from_str(version).expect("a valid version"),
            directory,
            RelativePath::from_str(crate::runtime::PINNED_RETROARCH_EXECUTABLE)
                .expect("a valid relative path"),
        )
    }

    /// The executable path the pinned definition names, below a version
    /// directory.
    const PINNED_EXECUTABLE_RELATIVE: &str = "RetroArch.app/Contents/MacOS/RetroArch";

    /// The active installation's executable is resolved to a concrete path inside
    /// the managed version directory — the value a later launch step consumes.
    #[test]
    fn the_active_installation_resolves_to_a_concrete_executable_path() {
        let root = TempRoot::new("active");
        let definition = pinned_retroarch_runtime();
        let installed = install(&root, "1.22.2");
        let installer = StubInstaller::with_active(Some(installed.clone()));

        let runtime = resolve(&definition, &installer).expect("the runtime resolves");

        assert_eq!(runtime.version().as_str(), "1.22.2");
        assert_eq!(runtime.directory(), installed.directory());
        assert_eq!(runtime.executable_path(), installed.executable_path());
        assert!(runtime.executable_path().is_file());
        assert!(runtime.executable_path().starts_with(runtime.directory()));
        assert_eq!(runtime.definition(), &definition);

        assert_eq!(
            installer.asked.borrow().as_slice(),
            [definition.id().clone()],
            "resolution asks the store for the pinned runtime and nothing else"
        );
    }

    /// The resolved runtime can be turned into the launch input of the next step
    /// without any further lookup.
    #[test]
    fn the_resolved_executable_is_usable_as_a_launch_input() {
        let root = TempRoot::new("launch-input");
        let definition = pinned_retroarch_runtime();
        let installer = StubInstaller::with_active(Some(install(&root, "1.22.2")));

        let runtime = resolve(&definition, &installer).expect("the runtime resolves");

        let input = crate::RetroArchLaunchInput::new(
            runtime.executable_path(),
            "/cores/test_libretro.dylib",
            "/roms/game.rom",
        );

        assert_eq!(input.executable(), runtime.executable_path());
    }

    /// A fresh installation has no runtime, and that is reported as "not
    /// installed" rather than as a failure.
    #[test]
    fn a_missing_runtime_is_reported_as_not_installed() {
        let definition = pinned_retroarch_runtime();
        let installer = StubInstaller::with_active(None);

        let error = resolve(&definition, &installer)
            .expect_err("nothing is installed, so nothing resolves");

        assert!(matches!(error, RetroArchRuntimeError::NotInstalled { .. }));
        assert!(error.to_string().contains("retroarch"));
        assert!(std::error::Error::source(&error).is_none());
    }

    /// A store that cannot be read is a different answer from "nothing is
    /// installed", because it needs a different response.
    #[test]
    fn an_unreadable_store_is_reported_separately_from_a_missing_runtime() {
        let definition = pinned_retroarch_runtime();
        let installer = StubInstaller::failing();

        let error =
            resolve(&definition, &installer).expect_err("a broken store cannot resolve a runtime");

        match &error {
            RetroArchRuntimeError::StoreUnreadable { cause } => {
                assert!(matches!(cause, RuntimeStoreError::UnreadableStore { .. }));
            }
            other => panic!("expected an unreadable store, got {other}"),
        }

        assert!(
            std::error::Error::source(&error).is_some(),
            "the store failure stays reachable as the cause"
        );
    }

    /// An activated version whose executable disappeared is reported with the path
    /// that was expected, so the problem is diagnosable.
    #[test]
    fn an_installation_without_its_executable_is_reported_with_the_expected_path() {
        let root = TempRoot::new("broken");
        let definition = pinned_retroarch_runtime();
        let installed = install(&root, "1.22.2");

        std::fs::remove_file(installed.executable_path()).expect("the executable is removed");

        let installer = StubInstaller::with_active(Some(installed.clone()));

        let error = resolve(&definition, &installer)
            .expect_err("an installation without its executable is broken");

        match error {
            RetroArchRuntimeError::ExecutableMissing { expected, version } => {
                assert_eq!(expected, installed.executable_path());
                assert_eq!(version.as_str(), "1.22.2");
            }
            other => panic!("expected a missing executable, got {other}"),
        }
    }

    /// The resolved runtime describes itself, so a log or an error can name the
    /// version and the directory without reaching into the store again.
    #[test]
    fn the_resolved_runtime_describes_itself() {
        let root = TempRoot::new("display");
        let definition = pinned_retroarch_runtime();
        let installer = StubInstaller::with_active(Some(install(&root, "1.22.2")));

        let runtime = resolve(&definition, &installer).expect("the runtime resolves");
        let rendered = runtime.to_string();

        assert!(rendered.contains("retroarch"));
        assert!(rendered.contains("1.22.2"));
        assert!(rendered.contains("macos-universal"));
    }
}
