//! The state a launch negotiation reads, and the ports that answer it.
//!
//! Game launch preparation answers one question — can this game be started with
//! the currently configured and installed BitArchive state, and with which
//! concrete inputs? — and it answers it from two places: deterministic domain
//! rules, and state that only outer layers can know.
//!
//! This module owns the *shape* of that state. It contains no implementation: a
//! value here is produced by an adapter in an outer layer (the composition root,
//! `bitarchive-emulation`, `bitarchive-infrastructure`), and nothing in this crate
//! reads a store, the filesystem, the network, or a process.
//!
//! # Why the state is neutral
//!
//! The application layer must not depend on RetroArch, on a component store, or on
//! an OS API, so none of these types names one. An unusable runtime is
//! [`LaunchRuntimeResolution::Unavailable`] with a
//! [`RuntimeUnavailableReason`], not a concrete store error; a core is an identity plus
//! two paths, not a `ManagedCore`. The adapter that knows how to read a store
//! translates its own failures into these values, and that translation is the only
//! place the two vocabularies meet.
//!
//! # What can fail, and what is merely absent
//!
//! A negotiation never fails. Every fact it needs is either present or absent, and an
//! absence becomes a [`LaunchBlocker`](crate::game_launch::LaunchBlocker):
//!
//! ```text
//! runtime   resolved | not installed | broken installation
//! core      curated + installed, curated but not installed, not curated, unusable
//! firmware  present | missing      (per requirement, with the level deciding)
//! content   available | missing
//! ```
//!
//! # "Not installed" is not "unusable"
//!
//! Both component classes keep that distinction, in their own vocabularies:
//!
//! ```text
//! runtime   RuntimeUnavailableReason::{NotInstalled, ExecutableMissing, StoreUnreadable}
//! core      CoreUnusableReason::{NotInstalled, Unusable}
//! ```
//!
//! It matters because the first is repaired by installing the component and the second
//! is a broken installation that needs diagnosis — and because the stores refuse to
//! install a component they already have, so re-installing is not a repair. A caller
//! that received one generic "no runtime" answer could not tell a first run from a
//! corrupt store, and would offer the wrong action for one of them.
//!
//! A reason also carries no message. The types above name a condition; phrasing it for
//! a person belongs to the presentation layer.

use std::io;
use std::path::{Path, PathBuf};

use bitarchive_domain::system::{EmulatedSystemKey, SystemCore};

/// The identity of an installed runtime.
///
/// A value is deliberately not the runtime
/// [`definition`](bitarchive_domain::RuntimeDefinition): a negotiation reports
/// *which* runtime answered and where its program is, and copying the whole pin
/// into a result would let a caller mistake a reviewed definition for observed
/// state.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LaunchRuntime {
    id: String,
    version: String,
    executable: PathBuf,
}

impl LaunchRuntime {
    /// Creates a value for an installed runtime.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        executable: impl Into<PathBuf>,
    ) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            executable: executable.into(),
        }
    }

    /// Returns the identity of the runtime, for example `retroarch`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the installed version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the executable a launch should start.
    ///
    /// The path is always inside the managed component store, because the only
    /// adapter that produces a [`LaunchRuntime`] resolves it from an activated
    /// installation. Nothing in the application layer can construct it from a
    /// `PATH` lookup, an `/Applications` scan, or a developer argument.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }
}

/// Why no managed runtime can be started.
///
/// The variants are the two answers a caller needs, and they are not interchangeable
/// because the *repair* differs:
///
/// - [`NotInstalled`](Self::NotInstalled) is what a first run looks like, and the
///   pinned runtime can be acquired for it (ADR 0001);
/// - [`ExecutableMissing`](Self::ExecutableMissing) and
///   [`StoreUnreadable`](Self::StoreUnreadable) are a broken installation, and
///   acquiring the runtime again is **not** the repair: the component store refuses to
///   install a version it already has, so that state needs diagnosis instead.
///
/// This mirrors [`CoreUnusableReason`] for cores, which draws the same line for the
/// same reason.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RuntimeUnavailableReason {
    /// No runtime has been installed and activated yet.
    NotInstalled,
    /// The active installation does not contain the executable it should.
    ExecutableMissing {
        /// The executable path that was expected inside the installation.
        expected: PathBuf,
        /// The installed version whose installation is incomplete.
        version: String,
    },
    /// The component store could not be read, so the activation state is unknown.
    StoreUnreadable,
}

impl RuntimeUnavailableReason {
    /// Returns whether acquiring the runtime could resolve this state.
    ///
    /// Only a runtime that is genuinely not installed has an installing answer. A
    /// caller that offered one anyway would send a user to a command that cannot
    /// repair the state.
    #[must_use]
    pub const fn is_installable(&self) -> bool {
        matches!(self, Self::NotInstalled)
    }
}

/// Whether a launch has a runtime to start.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LaunchRuntimeResolution {
    /// An installed, activated runtime was resolved.
    Resolved(LaunchRuntime),
    /// No runtime can be started, and why.
    Unavailable {
        /// The runtime identity that was asked for.
        id: String,
        /// Why no runtime could be resolved.
        reason: RuntimeUnavailableReason,
    },
}

impl LaunchRuntimeResolution {
    /// Returns the resolved runtime, if there is one.
    #[must_use]
    pub const fn runtime(&self) -> Option<&LaunchRuntime> {
        match self {
            Self::Resolved(runtime) => Some(runtime),
            Self::Unavailable { .. } => None,
        }
    }

    /// Returns the identity of the runtime this answer is about.
    ///
    /// A resolved runtime carries its own identity, so "which runtime is this about?"
    /// has an answer in both states.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Resolved(runtime) => runtime.id(),
            Self::Unavailable { id, .. } => id,
        }
    }

    /// Returns why no runtime could be resolved, if there is none.
    #[must_use]
    pub const fn unavailable_reason(&self) -> Option<&RuntimeUnavailableReason> {
        match self {
            Self::Resolved(_) => None,
            Self::Unavailable { reason, .. } => Some(reason),
        }
    }
}

/// An installed build of a core.
///
/// The directory and the library path are concrete because installing a core
/// *decides* where its bytes live; keeping them here is what lets a prepared launch
/// name the exact library a process would load without the application layer
/// knowing the store layout.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InstalledCore {
    build_id: String,
    directory: PathBuf,
    library: PathBuf,
}

impl InstalledCore {
    /// Creates a value for an installed core build.
    #[must_use]
    pub fn new(
        build_id: impl Into<String>,
        directory: impl Into<PathBuf>,
        library: impl Into<PathBuf>,
    ) -> Self {
        Self {
            build_id: build_id.into(),
            directory: directory.into(),
            library: library.into(),
        }
    }

    /// Returns the reviewed build identity that is installed.
    #[must_use]
    pub fn build_id(&self) -> &str {
        &self.build_id
    }

    /// Returns the directory the build is installed in.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Returns the concrete path of the core library.
    #[must_use]
    pub fn library(&self) -> &Path {
        &self.library
    }
}

/// Why a curated core definition has no usable installation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CoreUnusableReason {
    /// The build is not installed.
    NotInstalled,
    /// A build is present, but its installation is incomplete or unreadable.
    Unusable,
}

/// A core definition with the state of its installation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CoreAvailability {
    /// The definition's build is installed and the library is available.
    Installed(InstalledCore),
    /// The definition exists, but no usable build is installed.
    Unusable {
        /// Why the definition has no usable installation.
        reason: CoreUnusableReason,
    },
}

/// The curated core of a system and the state of its installation.
///
/// This is [`SystemCore`] with the installation state attached, so a caller learns
/// both "which core should run this system" and "is that core ready" from one
/// value.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SystemCoreState {
    /// No curated core definition can run this system on this platform.
    NotCurated(SystemCore),
    /// A curated definition exists; its `availability` says whether it is usable.
    Curated {
        /// The curated definition and the state of its installation.
        availability: CoreAvailability,
        /// The definition itself, as the domain resolved it.
        core: SystemCore,
    },
}

impl SystemCoreState {
    /// Returns the curated definition, if one exists.
    #[must_use]
    pub const fn definition(&self) -> Option<&SystemCore> {
        match self {
            Self::NotCurated(core) => Some(core),
            Self::Curated { core, .. } => Some(core),
        }
    }

    /// Returns the installed build, if the curated definition is installed.
    #[must_use]
    pub const fn installed(&self) -> Option<&InstalledCore> {
        match self {
            Self::Curated {
                availability: CoreAvailability::Installed(installed),
                ..
            } => Some(installed),
            Self::NotCurated(_)
            | Self::Curated {
                availability: CoreAvailability::Unusable { .. },
                ..
            } => None,
        }
    }

    /// Returns why the curated definition has no usable installation.
    #[must_use]
    pub const fn unusable_reason(&self) -> Option<&CoreUnusableReason> {
        match self {
            Self::Curated {
                availability: CoreAvailability::Unusable { reason },
                ..
            } => Some(reason),
            Self::NotCurated(_)
            | Self::Curated {
                availability: CoreAvailability::Installed(_),
                ..
            } => None,
        }
    }
}

/// The firmware requirement one system's launch is checked against.
///
/// Every requirement carries the outcome for *its* system, so a requirement that
/// does not apply to the launched system is absent from this list entirely rather
/// than present with an empty outcome.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FirmwareOutcome {
    /// How binding the requirement is.
    pub level: bitarchive_domain::FirmwareRequirementLevel,
    /// The file names the core looks for.
    pub expected: Vec<String>,
    /// The expected file names that are available to the core.
    pub present: Vec<String>,
    /// The expected file names that are not available.
    pub missing: Vec<String>,
}

impl FirmwareOutcome {
    /// Returns whether every file the core looks for is available.
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        self.missing.is_empty()
    }
}

/// The managed state a launch negotiation reads.
///
/// One port, because every method answers a question about the same subject: what
/// BitArchive currently has installed and activated. A separate trait per component
/// class would force the caller to pass three objects that are always produced
/// together.
///
/// Every method is read-only. Nothing here installs, activates, downloads, or
/// repairs anything: acquisition is an explicit, deliberate step (ADR 0001,
/// ADR 0002) and a launch never triggers it.
pub trait ManagedEmulationState {
    /// Resolves the active managed runtime.
    fn runtime(&self) -> LaunchRuntimeResolution;

    /// Resolves the curated core of `system` and the state of its installation.
    ///
    /// The answer is either
    /// [`SystemCoreState::NotCurated`] — the product has no reviewed core for this
    /// system here — or [`SystemCoreState::Curated`] with the availability of the
    /// build. It is never a store error: an unreadable store is reported as
    /// [`CoreUnusableReason::Unusable`], because a launch that cannot read its own
    /// component store has no usable core.
    fn core_for_system(&self, system: &EmulatedSystemKey) -> SystemCoreState;
}

/// Whether firmware files are available to a core.
///
/// The port answers only about file names it was asked about, so an implementation
/// never has to enumerate a directory that may be large, and a launch never depends
/// on what else happens to be there.
pub trait FirmwareChecker {
    /// Returns the subset of `expected` that is available to the core.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] when the firmware location itself cannot be read.
    /// A firmware location that simply does not exist is not an error: it is the
    /// normal state of a fresh installation and reports an empty subset.
    fn available(&self, expected: &[String]) -> io::Result<Vec<String>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitarchive_domain::system::{curated_systems, resolve_system_core};
    use bitarchive_domain::{CorePlatform, EmulatedSystemKey};
    use std::str::FromStr;

    /// A runtime resolution either holds a runtime or names the reason none could be
    /// resolved, and never both.
    #[test]
    fn a_runtime_resolution_is_either_resolved_or_unavailable() {
        let resolved = LaunchRuntimeResolution::Resolved(LaunchRuntime::new(
            "retroarch",
            "1.22.2",
            "/components/runtime/retroarch/RetroArch",
        ));
        let missing = LaunchRuntimeResolution::Unavailable {
            id: String::from("retroarch"),
            reason: RuntimeUnavailableReason::NotInstalled,
        };

        let runtime = resolved.runtime().expect("the runtime is resolved");

        assert_eq!(runtime.id(), "retroarch");
        assert_eq!(runtime.version(), "1.22.2");
        assert_eq!(
            runtime.executable(),
            Path::new("/components/runtime/retroarch/RetroArch")
        );
        assert_eq!(missing.runtime(), None);
        assert_eq!(missing.id(), "retroarch");
        assert_eq!(
            missing.unavailable_reason(),
            Some(&RuntimeUnavailableReason::NotInstalled)
        );
        assert_eq!(
            resolved.unavailable_reason(),
            None,
            "a resolved runtime has no reason"
        );
    }

    /// Only a runtime that is not installed is installable: a broken installation and
    /// an unreadable store are not repaired by acquiring the runtime again.
    #[test]
    fn only_a_missing_runtime_is_installable() {
        assert!(RuntimeUnavailableReason::NotInstalled.is_installable());
        assert!(
            !RuntimeUnavailableReason::ExecutableMissing {
                expected: PathBuf::from("/components/runtime/retroarch/RetroArch"),
                version: String::from("1.22.2"),
            }
            .is_installable()
        );
        assert!(!RuntimeUnavailableReason::StoreUnreadable.is_installable());
    }

    /// A reason carries the concrete evidence a presentation layer needs: which
    /// executable was expected and which version is incomplete.
    #[test]
    fn a_broken_installation_reason_names_the_expected_executable() {
        let reason = RuntimeUnavailableReason::ExecutableMissing {
            expected: PathBuf::from("/components/runtime/retroarch/1.22.2/RetroArch"),
            version: String::from("1.22.2"),
        };

        match &reason {
            RuntimeUnavailableReason::ExecutableMissing { expected, version } => {
                assert_eq!(
                    expected,
                    Path::new("/components/runtime/retroarch/1.22.2/RetroArch")
                );
                assert_eq!(version, "1.22.2");
            }
            other => panic!("the reason must keep its evidence: {other:?}"),
        }
    }

    /// An installed core carries the concrete installation, so a prepared launch
    /// can name the library without knowing the store layout.
    #[test]
    fn an_installed_core_names_its_build_and_library() {
        let installed = InstalledCore::new(
            "mgba-0.11-212-7a12d6d",
            "/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d",
            "/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d/mgba_libretro.dylib",
        );

        assert_eq!(installed.build_id(), "mgba-0.11-212-7a12d6d");
        assert_eq!(
            installed.directory(),
            Path::new("/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d")
        );
        assert!(installed.library().ends_with("mgba_libretro.dylib"));
    }

    /// A curated definition reports its installation state, and the three states
    /// stay distinguishable: not curated, unusable, installed.
    #[test]
    fn a_system_core_state_distinguishes_installation_states() {
        let catalog = curated_systems();
        let key = EmulatedSystemKey::from_str(EmulatedSystemKey::GAME_BOY_ADVANCE)
            .expect("the curated key");
        let system = catalog.system(&key).expect("the system is curated");
        let definition = resolve_system_core(system, CorePlatform::MacOsArm64);

        let not_curated =
            SystemCoreState::NotCurated(resolve_system_core(system, CorePlatform::MacOsArm64));
        let unusable = SystemCoreState::Curated {
            availability: CoreAvailability::Unusable {
                reason: CoreUnusableReason::NotInstalled,
            },
            core: definition.clone(),
        };
        let installed = SystemCoreState::Curated {
            availability: CoreAvailability::Installed(InstalledCore::new(
                "mgba-0.11-212-7a12d6d",
                "/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d",
                "/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d/mgba_libretro.dylib",
            )),
            core: definition,
        };

        assert_eq!(
            not_curated.unusable_reason(),
            None,
            "a system without a curated core is not an unusable installation"
        );
        assert_eq!(
            unusable.unusable_reason(),
            Some(&CoreUnusableReason::NotInstalled)
        );
        assert_eq!(unusable.installed(), None);
        assert!(installed.installed().is_some());
        assert_eq!(installed.unusable_reason(), None);
        assert_ne!(unusable, installed);
    }

    /// A firmware outcome answers "is this satisfied" from the missing list alone,
    /// so a requirement that is fully available is never reported as unmet.
    #[test]
    fn a_firmware_outcome_is_satisfied_when_nothing_is_missing() {
        use bitarchive_domain::FirmwareRequirementLevel;

        let satisfied = FirmwareOutcome {
            level: FirmwareRequirementLevel::Optional,
            expected: vec![String::from("gba_bios.bin")],
            present: vec![String::from("gba_bios.bin")],
            missing: Vec::new(),
        };
        let unsatisfied = FirmwareOutcome {
            level: FirmwareRequirementLevel::Required,
            expected: vec![String::from("bios.bin")],
            present: Vec::new(),
            missing: vec![String::from("bios.bin")],
        };

        assert!(satisfied.is_satisfied());
        assert!(!unsatisfied.is_satisfied());
    }
}
