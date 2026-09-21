//! Answering the launch-readiness port from the managed component stores.
//!
//! The application layer asks two questions before a launch can be prepared — which
//! managed runtime is active, and which curated core build is installed for a
//! system — and it asks them through
//! [`ManagedEmulationState`](bitarchive_application::ManagedEmulationState). This
//! module is the RetroArch-specific implementation of that port: it joins the two
//! resolution steps B5 and B6 built and translates their failures into the neutral
//! vocabulary of the application layer.
//!
//! ```text
//! RuntimeDefinition  → ComponentStore::active_runtime  → ManagedRuntime
//! SystemCore         → CoreStore::resolve              → ManagedCore
//!         ↓
//! LaunchRuntimeResolution / SystemCoreState   ← the application's own types
//! ```
//!
//! # Why the translation exists
//!
//! A store error is not a readiness answer. [`RetroArchRuntimeError`] distinguishes
//! "nothing installed" from "the store cannot be read" from "the installed version
//! lost its executable", and only the caller of a *launch* can decide what those mean
//! for a readiness result. The adapter therefore reports them as states —
//! [`CoreUnusableReason::NotInstalled`] versus
//! [`CoreUnusableReason::Unusable`] — and nothing here decides whether a launch may
//! proceed.
//!
//! The distinction is deliberate: "not installed" is repaired by the existing
//! acquisition command, while "install present but broken" needs diagnosis, and the
//! store refuses to install a build it already has.
//!
//! # No fallback, no acquisition
//!
//! The runtime is resolved from the activation record and from nowhere else, so
//! neither this module nor anything that calls it can start a `PATH` RetroArch, a
//! system RetroArch, a `/Applications` copy, or a developer-supplied executable
//! (ADR 0001, ADR 0003). The core is resolved from the curated definition of the
//! system's platform, so no other build and no other architecture is substituted
//! (ADR 0002). Nothing here downloads, installs, activates, or repairs anything: a
//! state that cannot be answered is reported as such.

use std::cell::OnceCell;

use bitarchive_application::managed_core::{CoreInstaller, ManagedCore};
use bitarchive_application::managed_runtime::RuntimeInstaller;
use bitarchive_application::{
    CoreAvailability, CoreUnusableReason, InstalledCore, LaunchRuntime, LaunchRuntimeResolution,
    ManagedEmulationState, SystemCoreState,
};
use bitarchive_domain::runtime::{RuntimeDefinition, RuntimeId};
use bitarchive_domain::system::{EmulatedSystemKey, SystemCore, resolve_system_core};

use crate::managed_runtime::RetroArchRuntimeError;
use crate::{ManagedRuntime, retroarch_runtime};

/// Resolves the state of the managed components through their own stores.
pub struct ManagedEmulation<'a> {
    runtime_definition: &'a RuntimeDefinition,
    core_platform: bitarchive_domain::CorePlatform,
    runtime_installer: &'a dyn RuntimeInstaller,
    core_installer: &'a dyn CoreInstaller,
    runtime: OnceCell<ResolvedRuntime>,
}

/// What resolving the runtime produced.
///
/// The runtime is resolved once per state, because a negotiation may ask for it more
/// than once and a second store read would be work without an answer.
enum ResolvedRuntime {
    /// The active managed runtime.
    ///
    /// Boxed because a resolved runtime carries a pinned definition and the other
    /// variant is much smaller; the state is created once per launch, so the
    /// indirection costs nothing that matters.
    Runtime(Box<ManagedRuntime>),
    /// No usable runtime, and why.
    Unusable(RetroArchRuntimeError),
}

impl<'a> ManagedEmulation<'a> {
    /// Creates a state that resolves through `runtime_installer` and `core_installer`.
    ///
    /// `runtime_definition` is the pinned definition the runtime is resolved against
    /// and `core_platform` the platform the curated core definition is resolved for.
    /// Both are supplied by the composition root, which knows the host platform.
    #[must_use]
    pub fn new(
        runtime_definition: &'a RuntimeDefinition,
        core_platform: bitarchive_domain::CorePlatform,
        runtime_installer: &'a dyn RuntimeInstaller,
        core_installer: &'a dyn CoreInstaller,
    ) -> Self {
        Self {
            runtime_definition,
            core_platform,
            runtime_installer,
            core_installer,
            runtime: OnceCell::new(),
        }
    }

    /// Returns the resolved runtime, resolving it on first use.
    fn resolved_runtime(&self) -> &ResolvedRuntime {
        self.runtime.get_or_init(|| {
            match retroarch_runtime::resolve(self.runtime_definition, self.runtime_installer) {
                Ok(runtime) => ResolvedRuntime::Runtime(Box::new(runtime)),
                Err(error) => ResolvedRuntime::Unusable(error),
            }
        })
    }

    /// Translates a resolved core build into the application's installation value.
    fn installed_core(core: &ManagedCore) -> InstalledCore {
        InstalledCore::new(
            core.build_id().as_str(),
            core.directory(),
            core.library_path(),
        )
    }
}

impl ManagedEmulationState for ManagedEmulation<'_> {
    /// Resolves the active managed RetroArch runtime.
    ///
    /// The answer is [`LaunchRuntimeResolution::Resolved`] only for an activated
    /// installation whose executable is actually present, and
    /// [`LaunchRuntimeResolution::Missing`] for every other outcome. Both
    /// "not installed" and "installed but broken" are reported as missing *here*,
    /// because the runtime half of the readiness vocabulary has no separate state for
    /// a broken installation: either a launch has a program to start or it has not.
    fn runtime(&self) -> LaunchRuntimeResolution {
        match self.resolved_runtime() {
            ResolvedRuntime::Runtime(runtime) => {
                LaunchRuntimeResolution::Resolved(LaunchRuntime::new(
                    runtime.definition().id().as_str(),
                    runtime.version().as_str(),
                    runtime.executable_path(),
                ))
            }
            ResolvedRuntime::Unusable(RetroArchRuntimeError::NotInstalled { id }) => {
                LaunchRuntimeResolution::Missing {
                    id: id.as_str().to_owned(),
                }
            }
            ResolvedRuntime::Unusable(RetroArchRuntimeError::ExecutableMissing {
                expected,
                version,
            }) => LaunchRuntimeResolution::Missing {
                id: format!(
                    "{} (the installed {version} has no executable at {})",
                    RuntimeId::RETROARCH,
                    expected.display()
                ),
            },
            ResolvedRuntime::Unusable(RetroArchRuntimeError::StoreUnreadable { cause }) => {
                LaunchRuntimeResolution::Missing {
                    id: format!(
                        "{} (the component store could not be read: {cause})",
                        RuntimeId::RETROARCH
                    ),
                }
            }
        }
    }

    /// Resolves the curated core of `system` and the state of its installation.
    ///
    /// The definition comes from the curated allowlist for the system and the
    /// configured platform, and the installation from the core store's build
    /// resolution. A system whose core is not curated is reported as such and is never
    /// answered from another core or another architecture.
    fn core_for_system(&self, system: &EmulatedSystemKey) -> SystemCoreState {
        // The curated system list is the same table the negotiation used, so the
        // definition resolved here is the one the system names.
        let catalog = bitarchive_domain::curated_systems();

        let Some(curated) = catalog.system(system) else {
            // A key the catalogue does not contain has no system, so there is no
            // component identity to ask the store for. The negotiation only asks about
            // systems it resolved from the same catalogue, which makes this unreachable
            // from the launch path; it is answered rather than panicked on, and it is
            // answered as "no curated core", because installing something could not
            // repair it.
            return SystemCoreState::NotCurated(SystemCore::Unavailable {
                reason: bitarchive_domain::CoreUnavailableReason::SystemNotCurated,
            });
        };

        match resolve_system_core(curated, self.core_platform) {
            SystemCore::Curated(definition) => {
                let availability = match self.core_installer.resolve(&definition) {
                    Ok(core) => CoreAvailability::Installed(Self::installed_core(&core)),
                    Err(bitarchive_application::CoreStoreError::NotInstalled { .. }) => {
                        CoreAvailability::Unusable {
                            reason: CoreUnusableReason::NotInstalled,
                        }
                    }
                    Err(_) => {
                        // Everything else — an unreadable store, an installed build
                        // without its library, a platform mismatch — is a broken
                        // installation rather than a missing one, and needs diagnosis
                        // instead of an install.
                        CoreAvailability::Unusable {
                            reason: CoreUnusableReason::Unusable,
                        }
                    }
                };

                SystemCoreState::Curated {
                    availability,
                    core: SystemCore::Curated(definition),
                }
            }
            not_curated @ SystemCore::Unavailable { .. } => {
                SystemCoreState::NotCurated(not_curated)
            }
        }
    }
}
