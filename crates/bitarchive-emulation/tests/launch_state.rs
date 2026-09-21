//! Answering the launch-readiness port from the managed component stores.
//!
//! The two store ports are answered by stubs, so every state a launch negotiation can
//! observe is exercised without a component store, a download, a filesystem, a
//! process, or the network:
//!
//! ```text
//! runtime      active | not installed | executable gone | store unreadable
//! core         curated + installed | curated but not installed | broken | uncurated
//! ```
//!
//! What these tests pin is the *translation*: a store failure must arrive as the
//! neutral state the application layer understands, and "not installed" must stay
//! distinguishable from "installed but unusable".

use std::cell::RefCell;
use std::path::PathBuf;
use std::str::FromStr;

use bitarchive_application::managed_core::{CoreInstaller, CoreStoreError, ManagedCore};
use bitarchive_application::managed_runtime::{InstalledRuntime, RuntimeInstaller};
use bitarchive_application::{CoreUnusableReason, ManagedEmulationState, SystemCoreState};
use bitarchive_domain::ComponentId;
use bitarchive_domain::component::RelativePath;
use bitarchive_domain::managed_core::{
    CoreBuildId, CoreComponentId, CoreDefinition, CorePlatform, mgba_bootstrap,
};
use bitarchive_domain::runtime::{RuntimeDefinition, RuntimeId, RuntimeVersion};
use bitarchive_domain::system::{EmulatedSystemKey, SystemCore};
use bitarchive_emulation::ManagedEmulation;
use bitarchive_emulation::{PINNED_RETROARCH_EXECUTABLE, pinned_retroarch_runtime};

/// The curated Game Boy Advance key.
const GBA: &str = "gba";

/// A system key from the curated list.
fn key(value: &str) -> EmulatedSystemKey {
    EmulatedSystemKey::from_str(value).expect("the curated system key is valid")
}

/// Builds an installed runtime whose executable exists, by writing a file into a
/// temporary directory.
struct FakeRuntimeInstaller {
    /// The installation to report, or the error to fail with.
    active: Result<Option<InstalledRuntime>, &'static str>,
    asked: RefCell<Vec<RuntimeId>>,
}

impl FakeRuntimeInstaller {
    /// An installer that reports `installed` as the active runtime.
    fn with_active(installed: Option<InstalledRuntime>) -> Self {
        Self {
            active: Ok(installed),
            asked: RefCell::new(Vec::new()),
        }
    }

    /// An installer whose store cannot be read.
    fn unreadable() -> Self {
        Self {
            active: Err("the store could not be listed"),
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl RuntimeInstaller for FakeRuntimeInstaller {
    fn install(
        &self,
        _definition: &RuntimeDefinition,
    ) -> Result<InstalledRuntime, bitarchive_application::RuntimeStoreError> {
        panic!("a readiness check never installs a runtime")
    }

    fn active_runtime(
        &self,
        id: &RuntimeId,
    ) -> Result<Option<InstalledRuntime>, bitarchive_application::RuntimeStoreError> {
        self.asked.borrow_mut().push(id.clone());

        match &self.active {
            Ok(installed) => Ok(installed.clone()),
            Err(message) => Err(unreadable_store(message)),
        }
    }

    fn installed_versions(
        &self,
        _id: &RuntimeId,
    ) -> Result<Vec<RuntimeVersion>, bitarchive_application::RuntimeStoreError> {
        panic!("a readiness check never enumerates installed runtime versions")
    }
}

/// The store failure a test uses for an unreadable store.
fn unreadable_store(message: &'static str) -> bitarchive_application::RuntimeStoreError {
    bitarchive_application::RuntimeStoreError::io(
        "list the store",
        PathBuf::from("/components"),
        std::io::Error::other(message),
    )
}

/// Builds an installation whose executable is a real file below `root`.
fn installed_runtime(root: &std::path::Path) -> InstalledRuntime {
    let directory = root.join("1.22.2");
    let executable = directory.join(PINNED_RETROARCH_EXECUTABLE);

    std::fs::create_dir_all(
        executable
            .parent()
            .expect("the executable has a parent directory"),
    )
    .expect("the installation directory can be created");
    std::fs::write(&executable, b"not a real binary").expect("the executable can be written");

    InstalledRuntime::new(
        RuntimeId::from_str(RuntimeId::RETROARCH).expect("the curated runtime identity"),
        RuntimeVersion::from_str("1.22.2").expect("a valid version"),
        directory,
        RelativePath::from_str(PINNED_RETROARCH_EXECUTABLE)
            .expect("the pinned executable is a relative path"),
    )
}

/// The state a core store reports for a test.
enum FakeCoreState {
    /// The curated build is installed.
    Installed,
    /// The curated build is not installed.
    NotInstalled,
    /// The store cannot be read.
    Unreadable,
}

/// A core installer that reports exactly the state a test wants.
struct FakeCoreInstaller {
    state: FakeCoreState,
}

impl FakeCoreInstaller {
    /// An installer in `state`.
    fn new(state: FakeCoreState) -> Self {
        Self { state }
    }

    /// An installer that resolves `definition` as an installed build.
    fn installed(_definition: &CoreDefinition) -> Self {
        Self::new(FakeCoreState::Installed)
    }

    /// An installer whose store holds no such build.
    fn not_installed(_definition: &CoreDefinition) -> Self {
        Self::new(FakeCoreState::NotInstalled)
    }

    /// An installer whose store cannot be read.
    fn unreadable() -> Self {
        Self::new(FakeCoreState::Unreadable)
    }
}

impl CoreInstaller for FakeCoreInstaller {
    fn install(&self, _definition: &CoreDefinition) -> Result<ManagedCore, CoreStoreError> {
        panic!("a readiness check never installs a core")
    }

    fn is_installed(
        &self,
        _component_id: &CoreComponentId,
        _platform: CorePlatform,
        _build_id: &CoreBuildId,
    ) -> Result<bool, CoreStoreError> {
        panic!("a readiness check resolves a build, it does not ask whether one is installed")
    }

    fn installed_builds(
        &self,
        _component_id: &ComponentId,
        _platform: CorePlatform,
    ) -> Result<Vec<CoreBuildId>, CoreStoreError> {
        panic!("a readiness check never enumerates installed core builds")
    }

    fn resolve(&self, definition: &CoreDefinition) -> Result<ManagedCore, CoreStoreError> {
        match self.state {
            FakeCoreState::Installed => Ok(ManagedCore::new(
                definition.clone(),
                PathBuf::from("/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d"),
            )),
            FakeCoreState::NotInstalled => Err(CoreStoreError::NotInstalled {
                component_id: definition.component_id().clone(),
                platform: definition.platform(),
                build_id: definition.build_id().clone(),
                directory: PathBuf::from("/components/cores/mgba"),
            }),
            FakeCoreState::Unreadable => Err(CoreStoreError::store(unreadable_store(
                "the store could not be listed",
            ))),
        }
    }
}

/// A uniquely named directory that removes itself afterwards.
struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    /// Creates an empty directory below the system temporary directory.
    fn create(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "bitarchive-emulation-state-{label}-{}-{nanos}",
            std::process::id()
        ));

        std::fs::create_dir_all(&path).expect("the temporary directory can be created");

        Self { path }
    }

    /// Returns the directory path.
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// An active installation whose executable exists resolves to the managed executable,
/// so the prepared launch has a program to start.
#[test]
fn an_active_runtime_resolves_its_executable() {
    let root = TempRoot::create("active");
    let definition = pinned_retroarch_runtime();
    let installer = FakeRuntimeInstaller::with_active(Some(installed_runtime(root.path())));
    let cores = FakeCoreInstaller::unreadable();

    let state = ManagedEmulation::new(&definition, CorePlatform::MacOsArm64, &installer, &cores);
    let resolution = state.runtime();

    let runtime = resolution
        .runtime()
        .expect("an activated installation with its executable is resolved");

    assert_eq!(runtime.id(), RuntimeId::RETROARCH);
    assert_eq!(runtime.version(), "1.22.2");
    assert!(runtime.executable().ends_with(PINNED_RETROARCH_EXECUTABLE));
    assert_eq!(resolution.id(), runtime.id());
}

/// Nothing installed is reported as a missing runtime, and the identity of the runtime
/// that is missing is named.
#[test]
fn nothing_activated_is_a_missing_runtime() {
    let definition = pinned_retroarch_runtime();
    let installer = FakeRuntimeInstaller::with_active(None);
    let cores = FakeCoreInstaller::unreadable();

    let state = ManagedEmulation::new(&definition, CorePlatform::MacOsArm64, &installer, &cores);
    let resolution = state.runtime();

    assert!(resolution.runtime().is_none());
    assert_eq!(resolution.id(), RuntimeId::RETROARCH);
}

/// An activated version whose executable disappeared is reported as a missing runtime
/// rather than handed to a process adapter that would fail on it.
#[test]
fn an_installation_without_its_executable_is_not_resolved() {
    let root = TempRoot::create("no-executable");
    let definition = pinned_retroarch_runtime();
    // The installation is reported as active, but nothing was written to disk.
    let installer = FakeRuntimeInstaller::with_active(Some(installed_runtime(root.path())));
    let cores = FakeCoreInstaller::unreadable();

    std::fs::remove_file(root.path().join("1.22.2").join(PINNED_RETROARCH_EXECUTABLE))
        .expect("the executable can be removed for the test");

    let state = ManagedEmulation::new(&definition, CorePlatform::MacOsArm64, &installer, &cores);

    assert!(state.runtime().runtime().is_none());
}

/// An unreadable store is reported as a missing runtime instead of being hidden: the
/// caller learns that no runtime could be resolved, and the identity says why.
#[test]
fn an_unreadable_runtime_store_is_reported() {
    let definition = pinned_retroarch_runtime();
    let installer = FakeRuntimeInstaller::unreadable();
    let cores = FakeCoreInstaller::unreadable();

    let state = ManagedEmulation::new(&definition, CorePlatform::MacOsArm64, &installer, &cores);
    let resolution = state.runtime();

    assert!(resolution.runtime().is_none());
    assert!(
        resolution.id().contains("could not be read"),
        "the answer explains the store failure: {}",
        resolution.id()
    );
}

/// A curated definition whose build is installed resolves to the concrete library the
/// launch path loads.
#[test]
fn an_installed_curated_core_resolves_its_library() {
    let definition = pinned_retroarch_runtime();
    let installer = FakeRuntimeInstaller::with_active(None);
    let core_definition = mgba_bootstrap(CorePlatform::MacOsArm64);
    let cores = FakeCoreInstaller::installed(&core_definition);

    let state = ManagedEmulation::new(&definition, CorePlatform::MacOsArm64, &installer, &cores);
    let system_core = state.core_for_system(&key(GBA));

    let installed = system_core
        .installed()
        .expect("the curated build is installed");

    assert_eq!(installed.build_id(), "mgba-0.11-212-7a12d6d");
    assert!(installed.library().ends_with("mgba_libretro.dylib"));
    assert!(installed.library().starts_with(installed.directory()));
}

/// A curated definition whose build is not installed keeps its definition and reports
/// the installation as missing, so the caller can offer the installing command.
#[test]
fn an_uninstalled_curated_core_keeps_its_definition() {
    let definition = pinned_retroarch_runtime();
    let installer = FakeRuntimeInstaller::with_active(None);
    let core_definition = mgba_bootstrap(CorePlatform::MacOsArm64);
    let cores = FakeCoreInstaller::not_installed(&core_definition);

    let state = ManagedEmulation::new(&definition, CorePlatform::MacOsArm64, &installer, &cores);
    let system_core = state.core_for_system(&key(GBA));

    assert!(system_core.installed().is_none());
    assert_eq!(
        system_core.unusable_reason(),
        Some(&CoreUnusableReason::NotInstalled),
        "a missing build stays distinguishable from a broken one"
    );

    match system_core.definition() {
        Some(SystemCore::Curated(resolved)) => {
            assert_eq!(resolved.component_id(), core_definition.component_id());
            assert_eq!(resolved.build_id(), core_definition.build_id());
        }
        other => panic!("the curated definition must be kept: {other:?}"),
    }
}

/// A store that cannot be read is reported as an unusable installation rather than as
/// a missing one, because installing the same build again could not repair it.
#[test]
fn an_unreadable_core_store_is_unusable_not_missing() {
    let definition = pinned_retroarch_runtime();
    let installer = FakeRuntimeInstaller::with_active(None);
    let cores = FakeCoreInstaller::unreadable();

    let state = ManagedEmulation::new(&definition, CorePlatform::MacOsArm64, &installer, &cores);
    let system_core = state.core_for_system(&key(GBA));

    assert_eq!(
        system_core.unusable_reason(),
        Some(&CoreUnusableReason::Unusable)
    );
    assert_eq!(
        system_core.installed(),
        None,
        "no library is invented for a store that could not be read"
    );
}

/// The core store is never asked to install anything by a readiness check, and the
/// resolution asks for the curated definition of the *requested* system.
#[test]
fn the_resolution_asks_the_store_for_the_curated_definition() {
    let definition = pinned_retroarch_runtime();
    let installer = FakeRuntimeInstaller::with_active(None);
    let core_definition = mgba_bootstrap(CorePlatform::MacOsArm64);
    let cores = FakeCoreInstaller::installed(&core_definition);

    let state = ManagedEmulation::new(&definition, CorePlatform::MacOsArm64, &installer, &cores);

    for system in [GBA, "gbc", "gb"] {
        let system_core = state.core_for_system(&key(system));

        match system_core.definition() {
            Some(SystemCore::Curated(resolved)) => {
                assert_eq!(
                    resolved.component_id().as_str(),
                    CoreComponentId::MGBA,
                    "{system} is served by the one curated core"
                );
            }
            other => panic!("{system} must resolve a curated definition: {other:?}"),
        }
    }
}

/// A system key outside the curated list has no component to ask the store for, so the
/// resolution answers "not curated" without consulting the store at all.
#[test]
fn a_system_outside_the_catalogue_does_not_reach_the_store() {
    let definition = pinned_retroarch_runtime();
    let installer = FakeRuntimeInstaller::with_active(None);
    let cores = FakeCoreInstaller::unreadable();

    let state = ManagedEmulation::new(&definition, CorePlatform::MacOsArm64, &installer, &cores);
    let unknown = EmulatedSystemKey::from_str("custom").expect("a syntactically valid key");

    let system_core = state.core_for_system(&unknown);

    assert!(
        matches!(system_core, SystemCoreState::NotCurated(_)),
        "an uncurated system is never answered from an unusable store"
    );
    assert!(system_core.installed().is_none());
}

/// The platform decides which curated definition is resolved, so another
/// architecture's build is never substituted.
#[test]
fn the_resolved_definition_matches_the_configured_platform() {
    let definition = pinned_retroarch_runtime();
    let installer = FakeRuntimeInstaller::with_active(None);

    for platform in CorePlatform::ALL {
        let core_definition = mgba_bootstrap(platform);
        let cores = FakeCoreInstaller::installed(&core_definition);
        let state = ManagedEmulation::new(&definition, platform, &installer, &cores);

        match state.core_for_system(&key(GBA)).definition() {
            Some(SystemCore::Curated(resolved)) => assert_eq!(resolved.platform(), platform),
            other => panic!("{platform} must resolve a curated definition: {other:?}"),
        }
    }
}

/// The runtime is resolved at most once per state, so a negotiation that asks twice
/// does not read the store twice.
#[test]
fn the_runtime_is_resolved_once() {
    let root = TempRoot::create("once");
    let definition = pinned_retroarch_runtime();
    let installer = FakeRuntimeInstaller::with_active(Some(installed_runtime(root.path())));
    let cores = FakeCoreInstaller::unreadable();

    let state = ManagedEmulation::new(&definition, CorePlatform::MacOsArm64, &installer, &cores);

    let first = state.runtime();
    let second = state.runtime();

    assert_eq!(first, second);
    assert_eq!(
        installer.asked.borrow().len(),
        1,
        "the store is read once, not once per question"
    );
}
