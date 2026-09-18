//! The versioned store for managed libretro cores: staging, installation, and
//! resolution — and no activation.
//!
//! This is the concrete [`CoreInstaller`] of the curated core path (Issue #21). It
//! runs the flow the port describes, as far as this step goes:
//!
//! ```text
//! download            ← ArtifactDownloader, verified against the pinned digest
//!     ↓
//! unpack              ← CoreArchiveExtractor, exactly one member, into staging
//!     ↓
//! validate            ← the pinned library exists inside the staged payload
//!     ↓
//! install             ← one move into
//!                       components/cores/<component-id>/<platform>/<build-id>/
//!     ↓
//! (no activation)     ← there is no "active core" (invariant 21)
//! ```
//!
//! # Layout
//!
//! ```text
//! <component root>/
//! ├── cores/
//! │   └── mgba/
//! │       ├── macos-arm64/
//! │       │   └── mgba-0.11-212-7a12d6d/
//! │       │       └── mgba_libretro.dylib
//! │       └── macos-x86_64/
//! │           └── mgba-0.11-212-7a12d6d/
//! │               └── mgba_libretro.dylib
//! ├── staging/
//! │   └── install-<component>-<build>-<platform>/   ← transient
//! └── artifacts/
//!     └── mgba-mgba-0.11-212-7a12d6d-macos-arm64
//! ```
//!
//! The platform sits below the component, exactly as it does in the runtime tree,
//! so an Apple Silicon build and an Intel build can never share a path
//! (ARCHITECTURE.md §23.1). Several builds of one core coexist; nothing here
//! decides which of them a launch would use — that stays with the core resolution
//! policy in [`bitarchive_domain::core`].
//!
//! # Immutability and crash safety
//!
//! An installed build directory is never written to again. A second install of the
//! same build is refused with [`AlreadyInstalled`](CoreStoreError::AlreadyInstalled)
//! instead of replacing what is there. The final step is one `rename` inside the
//! component store, so a failure or a crash leaves either no build directory or a
//! complete one — never a half-written one — and there is no copy fallback for the
//! same reason the runtime store has none (ARCHITECTURE.md §23.2).
//!
//! Staging and the artifact cache are shared with
//! [`ComponentStore`](crate::ComponentStore); the directories themselves, and the
//! rule that only `install-`-prefixed directories below `staging/` may ever be
//! removed, are defined in [`store_layout`](crate::store_layout).

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use bitarchive_application::managed_core::{
    CoreArchiveExtractor, CoreInstaller, CoreStoreError, ManagedCore, artifact_request,
};
use bitarchive_application::managed_runtime::{
    ArtifactDownloader, ArtifactRequest, RuntimeStoreError,
};
use bitarchive_domain::managed_core::{
    CoreBuildId, CoreComponentId, CoreDefinition, CorePlatform, host_core_platform,
};

use crate::store_layout::{
    ARTIFACT_DIRECTORY, CORES_DIRECTORY, PAYLOAD_DIRECTORY, STAGING_DIRECTORY, STAGING_PREFIX,
    below, is_cross_device,
};

/// The versioned component store for managed libretro cores.
///
/// The store is constructed from a component root, so where it lives is a decision
/// of the composition root and not of this type. It performs no I/O on
/// construction.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CoreStore<D, E> {
    root: PathBuf,
    downloader: D,
    extractor: E,
}

impl<D, E> CoreStore<D, E>
where
    D: ArtifactDownloader,
    E: CoreArchiveExtractor,
{
    /// Creates a store rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, downloader: D, extractor: E) -> Self {
        Self {
            root: root.into(),
            downloader,
            extractor,
        }
    }

    /// Returns the component root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the directory that holds the installed builds of `component_id` on
    /// `platform`.
    #[must_use]
    pub fn builds_directory(
        &self,
        component_id: &CoreComponentId,
        platform: CorePlatform,
    ) -> PathBuf {
        self.root
            .join(CORES_DIRECTORY)
            .join(component_id.as_str())
            .join(platform.as_str())
    }

    /// Returns the installation directory of one exact build.
    ///
    /// This is the directory that is created once and never written to again.
    #[must_use]
    pub fn installation_directory(
        &self,
        component_id: &CoreComponentId,
        platform: CorePlatform,
        build_id: &CoreBuildId,
    ) -> PathBuf {
        self.builds_directory(component_id, platform)
            .join(build_id.as_str())
    }

    /// Returns the path an artifact of `definition` is downloaded to.
    ///
    /// The name is derived from the definition, never from the URL, so a remote
    /// name can never decide a local path. It carries the component, the build, and
    /// the platform, so two builds of one core and two architectures of one build
    /// cannot overwrite each other's artifact.
    #[must_use]
    pub fn artifact_path(&self, definition: &CoreDefinition) -> PathBuf {
        self.root.join(ARTIFACT_DIRECTORY).join(format!(
            "{}-{}-{}",
            definition.component_id(),
            definition.build_id(),
            definition.platform()
        ))
    }

    /// Returns the staging directory the store uses.
    #[must_use]
    pub fn staging_directory(&self) -> PathBuf {
        self.root.join(STAGING_DIRECTORY)
    }

    /// Removes staging directories an interrupted run left behind.
    ///
    /// BitArchive owns the staging area together with the runtime store, so this is
    /// the same conservative cleanup: only `install-`-prefixed directories directly
    /// below `staging/` are removed (ARCHITECTURE.md §35.1, invariant 20).
    ///
    /// # Errors
    ///
    /// Returns [`CoreStoreError::Store`] when the staging directory exists but
    /// cannot be listed.
    pub fn cleanup_staging(&self) -> Result<usize, CoreStoreError> {
        crate::store_layout::cleanup_staging(&self.staging_directory())
            .map_err(CoreStoreError::store)
    }
}

impl<D, E> CoreInstaller for CoreStore<D, E>
where
    D: ArtifactDownloader,
    E: CoreArchiveExtractor,
{
    fn install(&self, definition: &CoreDefinition) -> Result<ManagedCore, CoreStoreError> {
        let platform = definition.platform();
        let host = host_core_platform();

        // One architecture's library is never substituted for another's: an Intel
        // build on Apple Silicon is a different binary, not a fallback.
        if host != Some(platform) {
            return Err(CoreStoreError::UnsupportedPlatform { platform, host });
        }

        let request = artifact_request(definition);

        // A definition whose URL and recorded published name disagree describes
        // something other than what the URL serves, so it is not downloaded at all.
        if !request.published_name_matches_url() {
            return Err(CoreStoreError::InconsistentDefinition {
                component_id: definition.component_id().clone(),
                expected_file_name: request.expected_file_name().unwrap_or_default().to_owned(),
                url_file_name: request.url_file_name().map(ToOwned::to_owned),
            });
        }

        let installation =
            self.installation_directory(definition.component_id(), platform, definition.build_id());

        if installation.exists() {
            return Err(CoreStoreError::AlreadyInstalled {
                component_id: definition.component_id().clone(),
                platform,
                build_id: definition.build_id().clone(),
                directory: installation,
            });
        }

        let staging = self.create_staging_directory(definition)?;
        let staged_payload = staging.join(PAYLOAD_DIRECTORY);

        // From here on, every failure path removes the staging directory. No
        // installed build is touched by any of them, and there is no activation
        // record that could be left pointing at something that does not exist.
        let staged = self.stage(definition, &request, &staged_payload);

        let _ = fs::remove_dir_all(&staging);

        staged
    }

    fn is_installed(
        &self,
        component_id: &CoreComponentId,
        platform: CorePlatform,
        build_id: &CoreBuildId,
    ) -> Result<bool, CoreStoreError> {
        let directory = self.installation_directory(component_id, platform, build_id);

        match fs::metadata(&directory) {
            Ok(metadata) => Ok(metadata.is_dir()),
            Err(cause) if cause.kind() == ErrorKind::NotFound => Ok(false),
            Err(cause) => Err(CoreStoreError::store(RuntimeStoreError::io(
                "reading an installed core build",
                &directory,
                cause,
            ))),
        }
    }

    fn installed_builds(
        &self,
        component_id: &CoreComponentId,
        platform: CorePlatform,
    ) -> Result<Vec<CoreBuildId>, CoreStoreError> {
        let directory = self.builds_directory(component_id, platform);

        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(cause) if cause.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(cause) => {
                return Err(CoreStoreError::store(RuntimeStoreError::UnreadableStore {
                    directory,
                    cause,
                }));
            }
        };

        let mut builds = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|cause| {
                CoreStoreError::store(RuntimeStoreError::UnreadableStore {
                    directory: directory.clone(),
                    cause,
                })
            })?;

            if !entry.path().is_dir() {
                continue;
            }

            let name = entry.file_name();
            let name = name.to_string_lossy();

            // A directory whose name is not a build identity is not an installed
            // build. It is reported by being absent from the list rather than by an
            // error, and it is never removed: cleanup is conservative
            // (ARCHITECTURE.md §35.1).
            let Ok(build_id) = name.parse::<CoreBuildId>() else {
                continue;
            };

            builds.push(build_id);
        }

        builds.sort();

        Ok(builds)
    }

    fn resolve(&self, definition: &CoreDefinition) -> Result<ManagedCore, CoreStoreError> {
        let directory = self.installation_directory(
            definition.component_id(),
            definition.platform(),
            definition.build_id(),
        );

        if !directory.is_dir() {
            return Err(CoreStoreError::NotInstalled {
                component_id: definition.component_id().clone(),
                platform: definition.platform(),
                build_id: definition.build_id().clone(),
                directory,
            });
        }

        let library = below(&directory, definition.library());

        match fs::symlink_metadata(&library) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(CoreStoreError::LibraryMissing { expected: library }),
            Err(cause) if cause.kind() == ErrorKind::NotFound => {
                return Err(CoreStoreError::LibraryMissing { expected: library });
            }
            Err(cause) => {
                return Err(CoreStoreError::store(RuntimeStoreError::io(
                    "reading the installed core library",
                    &library,
                    cause,
                )));
            }
        }

        Ok(ManagedCore::new(definition.clone(), directory))
    }
}

impl<D, E> CoreStore<D, E>
where
    D: ArtifactDownloader,
    E: CoreArchiveExtractor,
{
    /// Creates a fresh staging directory for `definition`.
    fn create_staging_directory(
        &self,
        definition: &CoreDefinition,
    ) -> Result<PathBuf, CoreStoreError> {
        let staging_root = self.staging_directory();

        fs::create_dir_all(&staging_root).map_err(|cause| {
            CoreStoreError::store(RuntimeStoreError::io(
                "creating the staging root",
                &staging_root,
                cause,
            ))
        })?;

        let staging = staging_root.join(format!(
            "{STAGING_PREFIX}{}-{}-{}",
            definition.component_id(),
            definition.build_id(),
            definition.platform()
        ));

        // Staging is transient, so a leftover from an interrupted run is removed
        // rather than reused. Anything unrelated keeps its name and is untouched.
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|cause| {
                CoreStoreError::store(RuntimeStoreError::io(
                    "clearing an abandoned staging directory",
                    &staging,
                    cause,
                ))
            })?;
        }

        fs::create_dir_all(&staging).map_err(|cause| {
            CoreStoreError::store(RuntimeStoreError::io(
                "creating the staging directory",
                &staging,
                cause,
            ))
        })?;

        Ok(staging)
    }

    /// Runs everything between "staging directory exists" and "build installed".
    fn stage(
        &self,
        definition: &CoreDefinition,
        request: &ArtifactRequest,
        staged_payload: &Path,
    ) -> Result<ManagedCore, CoreStoreError> {
        let artifact = self
            .downloader
            .download(request, &self.artifact_path(definition))?;

        // The downloaded archive is verified by now, so it is safe to hand to the
        // extractor. It stays in the artifact directory as a rebuildable local copy
        // instead of being downloaded again by a later step.
        fs::create_dir_all(staged_payload).map_err(|cause| {
            CoreStoreError::store(RuntimeStoreError::io(
                "creating the staged payload directory",
                staged_payload,
                cause,
            ))
        })?;

        self.extractor
            .unpack(definition, &artifact, staged_payload)?;
        self.validate_library(definition, staged_payload)?;

        let installation = self.installation_directory(
            definition.component_id(),
            definition.platform(),
            definition.build_id(),
        );

        self.move_into_place(staged_payload, &installation)?;

        Ok(ManagedCore::new(definition.clone(), installation))
    }

    /// Checks that the staged payload contains the library the definition pins, and
    /// that it is a regular file.
    ///
    /// The check happens on the staged copy, before anything is installed, so a
    /// payload without the library never becomes a build directory. A symbolic link
    /// is not a library either: the archive extractor already refuses one, and this
    /// second check keeps the store's own guarantee independent of which extractor
    /// it was handed.
    fn validate_library(
        &self,
        definition: &CoreDefinition,
        staged_payload: &Path,
    ) -> Result<(), CoreStoreError> {
        let expected = below(staged_payload, definition.library());

        let metadata = match fs::symlink_metadata(&expected) {
            Ok(metadata) => metadata,
            Err(_) => return Err(CoreStoreError::LibraryMissing { expected }),
        };

        if !metadata.is_file() {
            return Err(CoreStoreError::LibraryMissing { expected });
        }

        Ok(())
    }

    /// Moves the staged payload to the build directory in one step.
    ///
    /// The move is a single `rename` on one filesystem, and that is the whole
    /// mechanism. There is deliberately no copy fallback: a copy into the final
    /// build directory would make the installation multi-step, and a failure
    /// halfway through it would leave a *partial* directory at the path that is
    /// supposed to mean "this build is installed completely"
    /// (ARCHITECTURE.md §23.2). Staging lives inside the component store for
    /// exactly this reason; a cross-filesystem configuration is a controlled
    /// failure, not a repair.
    fn move_into_place(
        &self,
        staged_payload: &Path,
        installation: &Path,
    ) -> Result<(), CoreStoreError> {
        if let Some(parent) = installation.parent() {
            fs::create_dir_all(parent).map_err(|cause| {
                CoreStoreError::store(RuntimeStoreError::io(
                    "creating the builds directory",
                    parent,
                    cause,
                ))
            })?;
        }

        fs::rename(staged_payload, installation).map_err(|cause| {
            // A failed rename is atomic by definition: nothing appeared at
            // `installation`, so there is no partial build directory to clean up.
            // The staged payload stays where it is and the caller removes the
            // staging directory it created.
            if is_cross_device(&cause) {
                CoreStoreError::store(RuntimeStoreError::CrossDeviceInstallation {
                    staged_payload: staged_payload.to_path_buf(),
                    installation: installation.to_path_buf(),
                })
            } else {
                CoreStoreError::store(RuntimeStoreError::io(
                    "installing the build directory",
                    installation,
                    cause,
                ))
            }
        })
    }
}
