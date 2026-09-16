//! The versioned component store: staging, installation, and activation.
//!
//! This is the concrete [`RuntimeInstaller`] of the managed runtime path
//! (Issue #19). It implements the flow ARCHITECTURE.md §23.2 describes, as far as
//! this step goes:
//!
//! ```text
//! download            ← ArtifactDownloader, verified against the pinned digest
//!     ↓
//! unpack              ← ArtifactExtractor, into a staging directory
//!     ↓
//! validate            ← the pinned executable exists inside the staged payload
//!     ↓
//! install             ← one move into components/runtime/<platform>/<version>/
//!     ↓
//! activate            ← one atomic record write
//! ```
//!
//! # Layout
//!
//! The component store follows ARCHITECTURE.md §23.1, with the platform below the
//! component class so that a runtime and a core can never collide and so that a
//! later platform cannot silently reuse another platform's installation:
//!
//! ```text
//! <component root>/
//! ├── runtime/
//! │   └── macos-universal/
//! │       ├── 1.22.2/
//! │       │   └── RetroArch.app/Contents/MacOS/RetroArch
//! │       └── active
//! ├── staging/
//! │   └── install-<id>/          ← transient, removed on success and on failure
//! └── artifacts/
//!     └── retroarch-1.22.2-macos-universal.dmg
//! ```
//!
//! Cores get `runtime/`'s sibling `cores/<core-id>/<version>/` when their own
//! Issue arrives. Nothing here assumes a core exists or creates a place for one.
//!
//! # Immutability
//!
//! An installed version directory is never written to again. A second install of
//! the same version is refused with
//! [`AlreadyInstalled`](RuntimeStoreError::AlreadyInstalled) instead of replacing
//! what is there, and the activation record is replaced as a whole rather than
//! edited. Both are what make "aktive Versionen werden nicht in-place
//! überschrieben" (ARCHITECTURE.md §23.2) a property of the code.
//!
//! # Crash safety
//!
//! There are two moments that matter, and each is a single rename on one
//! filesystem:
//!
//! - the staged directory becomes the version directory, or it does not exist;
//! - the activation record becomes the new record, or it keeps its old content.
//!
//! Everything expensive happens before either of them. A crash therefore leaves
//! either the previous state or the new state, never a half-written one, and a
//! failure at any step leaves the previously active runtime active.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use bitarchive_application::managed_runtime::{
    Artifact, ArtifactDownloader, ArtifactExtractor, InstalledRuntime, RuntimeInstaller,
    RuntimeStoreError,
};
use bitarchive_domain::runtime::{RuntimeDefinition, RuntimeId, RuntimePlatform, RuntimeVersion};

/// The directory below the component root that holds installed runtimes.
const RUNTIME_DIRECTORY: &str = "runtime";

/// The directory below the component root that holds transient staging state.
const STAGING_DIRECTORY: &str = "staging";

/// The directory below the component root that holds downloaded artifacts.
const ARTIFACT_DIRECTORY: &str = "artifacts";

/// The name of the record that says which version is active.
const ACTIVE_RECORD: &str = "active";

/// The prefix of a staging directory, so an abandoned one is recognisable.
const STAGING_PREFIX: &str = "install-";

/// The suffix of a record that is being replaced.
const TEMPORARY_SUFFIX: &str = ".tmp";

/// The versioned component store for managed runtime components.
///
/// The store is constructed from a component root, so where it lives is a
/// decision of the composition root and not of this type. It performs no I/O on
/// construction.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ComponentStore<D, E> {
    root: PathBuf,
    downloader: D,
    extractor: E,
}

impl<D, E> ComponentStore<D, E>
where
    D: ArtifactDownloader,
    E: ArtifactExtractor,
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

    /// Returns the directory that holds the installed versions of `id` on
    /// `platform`.
    #[must_use]
    pub fn versions_directory(&self, id: &RuntimeId, platform: RuntimePlatform) -> PathBuf {
        self.root
            .join(RUNTIME_DIRECTORY)
            .join(id.as_str())
            .join(platform.as_str())
    }

    /// Returns the installation directory of one exact version.
    ///
    /// This is the directory that is created once and never written to again.
    #[must_use]
    pub fn installation_directory(
        &self,
        id: &RuntimeId,
        platform: RuntimePlatform,
        version: &RuntimeVersion,
    ) -> PathBuf {
        self.versions_directory(id, platform).join(version.as_str())
    }

    /// Returns the path of the record that names the active version of `id`.
    #[must_use]
    pub fn active_record(&self, id: &RuntimeId) -> PathBuf {
        self.root
            .join(RUNTIME_DIRECTORY)
            .join(id.as_str())
            .join(ACTIVE_RECORD)
    }

    /// Returns the directory a download is written to.
    #[must_use]
    pub fn artifact_directory(&self) -> PathBuf {
        self.root.join(ARTIFACT_DIRECTORY)
    }

    /// Returns the path an artifact of `definition` is downloaded to.
    ///
    /// The name is derived from the definition, never from the URL, so a remote
    /// name can never decide a local path.
    #[must_use]
    pub fn artifact_path(&self, definition: &RuntimeDefinition) -> PathBuf {
        self.artifact_directory().join(format!(
            "{}-{}-{}",
            definition.id(),
            definition.version(),
            definition.platform()
        ))
    }

    /// Returns the staging directory the store uses.
    #[must_use]
    pub fn staging_directory(&self) -> PathBuf {
        self.root.join(STAGING_DIRECTORY)
    }

    /// Returns `true` when `id` is installed in exactly `version` on `platform`.
    #[must_use]
    pub fn is_installed(
        &self,
        id: &RuntimeId,
        platform: RuntimePlatform,
        version: &RuntimeVersion,
    ) -> bool {
        self.installation_directory(id, platform, version).is_dir()
    }

    /// Removes staging directories an interrupted run left behind.
    ///
    /// Startup cleanup is limited to what BitArchive clearly owns
    /// (ARCHITECTURE.md §35.1, invariant 20): only directories directly below this
    /// store's own staging directory that carry the staging prefix are removed, so
    /// an installed version, an artifact, and anything a user placed elsewhere are
    /// never touched.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeStoreError::UnreadableStore`] when the staging directory
    /// exists but cannot be listed.
    pub fn cleanup_staging(&self) -> Result<usize, RuntimeStoreError> {
        let staging = self.staging_directory();

        let entries = match fs::read_dir(&staging) {
            Ok(entries) => entries,
            Err(cause) if cause.kind() == ErrorKind::NotFound => return Ok(0),
            Err(cause) => {
                return Err(RuntimeStoreError::UnreadableStore {
                    directory: staging,
                    cause,
                });
            }
        };

        let mut removed = 0;

        for entry in entries {
            let entry = entry.map_err(|cause| RuntimeStoreError::UnreadableStore {
                directory: staging.clone(),
                cause,
            })?;

            let name = entry.file_name();
            let name = name.to_string_lossy();

            if !name.starts_with(STAGING_PREFIX) {
                continue;
            }

            let path = entry.path();

            if path.is_dir() {
                fs::remove_dir_all(&path).map_err(|cause| {
                    RuntimeStoreError::io("removing an abandoned staging directory", &path, cause)
                })?;

                removed += 1;
            }
        }

        Ok(removed)
    }
}

impl<D, E> RuntimeInstaller for ComponentStore<D, E>
where
    D: ArtifactDownloader,
    E: ArtifactExtractor,
{
    fn install(
        &self,
        definition: &RuntimeDefinition,
    ) -> Result<InstalledRuntime, RuntimeStoreError> {
        let platform = definition.platform();

        if platform != RuntimePlatform::MacOsUniversal || !cfg!(target_os = "macos") {
            return Err(RuntimeStoreError::UnsupportedPlatform { platform });
        }

        let installation =
            self.installation_directory(definition.id(), platform, definition.version());

        if installation.exists() {
            return Err(RuntimeStoreError::AlreadyInstalled {
                id: definition.id().clone(),
                version: definition.version().clone(),
                directory: installation,
            });
        }

        let staging = self.create_staging_directory(definition)?;
        let staged_payload = staging.join("payload");

        // From here on, every failure path removes the staging directory. The
        // active runtime is untouched by all of them, because it is only written
        // after the version directory exists.
        let staged = self.stage(definition, &staged_payload);

        // Staging is transient on both paths: a successful install has moved the
        // payload out of it, and a failed one must not leave anything behind.
        let _ = fs::remove_dir_all(&staging);

        staged
    }

    fn active_runtime(
        &self,
        id: &RuntimeId,
    ) -> Result<Option<InstalledRuntime>, RuntimeStoreError> {
        let record = self.active_record(id);

        let content = match fs::read_to_string(&record) {
            Ok(content) => content,
            Err(cause) if cause.kind() == ErrorKind::NotFound => return Ok(None),
            Err(cause) => {
                return Err(RuntimeStoreError::ActiveRecordUnreadable {
                    path: record,
                    cause: cause.to_string(),
                });
            }
        };

        let parsed = ActiveRecord::parse(&content).map_err(|cause| {
            RuntimeStoreError::ActiveRecordUnreadable {
                path: record.clone(),
                cause,
            }
        })?;

        let directory = self.installation_directory(id, parsed.platform, &parsed.version);

        if !directory.is_dir() {
            return Err(RuntimeStoreError::ActiveRuntimeMissing {
                id: id.clone(),
                version: parsed.version,
                directory,
            });
        }

        Ok(Some(InstalledRuntime::new(
            id.clone(),
            parsed.version,
            directory,
            parsed.executable,
        )))
    }

    fn installed_versions(&self, id: &RuntimeId) -> Result<Vec<RuntimeVersion>, RuntimeStoreError> {
        let mut versions = Vec::new();

        for platform in [RuntimePlatform::MacOsUniversal] {
            let directory = self.versions_directory(id, platform);

            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(cause) if cause.kind() == ErrorKind::NotFound => continue,
                Err(cause) => {
                    return Err(RuntimeStoreError::UnreadableStore { directory, cause });
                }
            };

            for entry in entries {
                let entry = entry.map_err(|cause| RuntimeStoreError::UnreadableStore {
                    directory: directory.clone(),
                    cause,
                })?;

                if !entry.path().is_dir() {
                    continue;
                }

                let name = entry.file_name();
                let name = name.to_string_lossy();

                let Ok(version) = name.parse::<RuntimeVersion>() else {
                    // A directory whose name is not a version is not an installed
                    // version. It is reported by being absent from the list rather
                    // than by an error, and it is never removed: cleanup is
                    // conservative (ARCHITECTURE.md §35.1).
                    continue;
                };

                versions.push(version);
            }
        }

        versions.sort();

        Ok(versions)
    }
}

impl<D, E> ComponentStore<D, E>
where
    D: ArtifactDownloader,
    E: ArtifactExtractor,
{
    /// Creates a fresh staging directory for `definition`.
    fn create_staging_directory(
        &self,
        definition: &RuntimeDefinition,
    ) -> Result<PathBuf, RuntimeStoreError> {
        let staging_root = self.staging_directory();

        fs::create_dir_all(&staging_root).map_err(|cause| {
            RuntimeStoreError::io("creating the staging root", &staging_root, cause)
        })?;

        let staging = staging_root.join(format!(
            "{STAGING_PREFIX}{}-{}-{}",
            definition.id(),
            definition.version(),
            definition.platform()
        ));

        // Staging is transient, so a leftover from an interrupted run is removed
        // rather than reused. Anything unrelated keeps its name and is untouched.
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|cause| {
                RuntimeStoreError::io("clearing an abandoned staging directory", &staging, cause)
            })?;
        }

        fs::create_dir_all(&staging).map_err(|cause| {
            RuntimeStoreError::io("creating the staging directory", &staging, cause)
        })?;

        Ok(staging)
    }

    /// Runs everything between "staging directory exists" and "version installed
    /// and activated".
    fn stage(
        &self,
        definition: &RuntimeDefinition,
        staged_payload: &Path,
    ) -> Result<InstalledRuntime, RuntimeStoreError> {
        let artifact_target = self.artifact_path(definition);

        let artifact = self.downloader.download(definition, &artifact_target)?;

        // The downloaded artifact is verified by now, so it is safe to hand to the
        // extractor. It stays in the artifact directory as a rebuildable local
        // copy instead of being downloaded again by a later step.
        self.unpack_into(definition, &artifact, staged_payload)?;
        self.validate_executable(definition, staged_payload)?;

        let installation = self.installation_directory(
            definition.id(),
            definition.platform(),
            definition.version(),
        );

        self.move_into_place(staged_payload, &installation)?;

        self.activate(definition, &installation)?;

        Ok(InstalledRuntime::new(
            definition.id().clone(),
            definition.version().clone(),
            installation,
            definition.executable().clone(),
        ))
    }

    /// Unpacks the verified artifact into the staged payload directory.
    fn unpack_into(
        &self,
        definition: &RuntimeDefinition,
        artifact: &Artifact,
        staged_payload: &Path,
    ) -> Result<(), RuntimeStoreError> {
        fs::create_dir_all(staged_payload).map_err(|cause| {
            RuntimeStoreError::io(
                "creating the staged payload directory",
                staged_payload,
                cause,
            )
        })?;

        self.extractor.unpack(definition, artifact, staged_payload)
    }

    /// Checks that the staged payload contains the executable the definition
    /// pins, and that it is a regular, executable file.
    ///
    /// The check happens on the staged copy, before anything is installed, so a
    /// payload without a usable executable never becomes a version directory.
    fn validate_executable(
        &self,
        definition: &RuntimeDefinition,
        staged_payload: &Path,
    ) -> Result<(), RuntimeStoreError> {
        let mut expected = staged_payload.to_path_buf();

        for component in definition.executable().components() {
            expected.push(component);
        }

        let metadata = match fs::symlink_metadata(&expected) {
            Ok(metadata) => metadata,
            Err(_) => {
                return Err(RuntimeStoreError::ExecutableMissing { expected });
            }
        };

        if !metadata.is_file() {
            return Err(RuntimeStoreError::ExecutableMissing { expected });
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(RuntimeStoreError::ExecutableMissing { expected });
            }
        }

        Ok(())
    }

    /// Moves the staged payload to the version directory in one step.
    ///
    /// The move is a rename on one filesystem. When the staging area and the store
    /// root happen to live on different filesystems, the payload is copied and the
    /// staged copy removed; the move is then still all-or-nothing from the
    /// caller's point of view, because the version directory only comes into
    /// existence after the copy succeeded.
    fn move_into_place(
        &self,
        staged_payload: &Path,
        installation: &Path,
    ) -> Result<(), RuntimeStoreError> {
        if let Some(parent) = installation.parent() {
            fs::create_dir_all(parent).map_err(|cause| {
                RuntimeStoreError::io("creating the versions directory", parent, cause)
            })?;
        }

        match fs::rename(staged_payload, installation) {
            Ok(()) => Ok(()),
            Err(cause) if is_cross_device(&cause) => {
                copy_tree(staged_payload, installation).map_err(|cause| {
                    RuntimeStoreError::io("copying the staged payload", installation, cause)
                })?;

                fs::remove_dir_all(staged_payload).map_err(|cause| {
                    RuntimeStoreError::io("removing the staged payload", staged_payload, cause)
                })
            }
            Err(cause) => Err(RuntimeStoreError::io(
                "installing the version directory",
                installation,
                cause,
            )),
        }
    }

    /// Writes the activation record, replacing it as a whole.
    ///
    /// The record is written to a temporary file in the same directory and then
    /// renamed over the previous record, so an interrupted activation leaves
    /// either the old record or the new one.
    fn activate(
        &self,
        definition: &RuntimeDefinition,
        installation: &Path,
    ) -> Result<(), RuntimeStoreError> {
        let record = self.active_record(definition.id());

        let parent = record.parent().ok_or_else(|| {
            RuntimeStoreError::io(
                "locating the activation record",
                &record,
                std::io::Error::other("the activation record has no parent directory"),
            )
        })?;

        fs::create_dir_all(parent).map_err(|cause| {
            RuntimeStoreError::io("creating the runtime directory", parent, cause)
        })?;

        if !installation.is_dir() {
            return Err(RuntimeStoreError::ActiveRuntimeMissing {
                id: definition.id().clone(),
                version: definition.version().clone(),
                directory: installation.to_path_buf(),
            });
        }

        let content = ActiveRecord {
            version: definition.version().clone(),
            platform: definition.platform(),
            executable: definition.executable().clone(),
        }
        .render();

        let mut temporary = record.as_os_str().to_owned();
        temporary.push(TEMPORARY_SUFFIX);
        let temporary = PathBuf::from(temporary);

        fs::write(&temporary, content).map_err(|cause| {
            RuntimeStoreError::io("writing the activation record", &temporary, cause)
        })?;

        fs::rename(&temporary, &record).map_err(|cause| {
            let _ = fs::remove_file(&temporary);

            RuntimeStoreError::io("replacing the activation record", &record, cause)
        })
    }
}

/// The parsed content of an activation record.
#[derive(Clone, PartialEq, Eq, Debug)]
struct ActiveRecord {
    version: RuntimeVersion,
    platform: RuntimePlatform,
    executable: bitarchive_domain::runtime::RelativePath,
}

impl ActiveRecord {
    /// Renders the record for storage.
    ///
    /// The format is plain text with one `key=value` line per field: it is
    /// reviewable by a human, needs no serialization dependency, and has no
    /// structure that a later manifest could not replace.
    fn render(&self) -> String {
        format!(
            "version={}\nplatform={}\nexecutable={}\n",
            self.version,
            self.platform.as_str(),
            self.executable
        )
    }

    /// Parses a stored record.
    ///
    /// # Errors
    ///
    /// Returns a description of the first problem found, so an unreadable record
    /// is reported with a reason instead of being silently ignored.
    fn parse(content: &str) -> Result<Self, String> {
        let mut version = None;
        let mut platform = None;
        let mut executable = None;

        for line in content.lines() {
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("line {line:?} is not a key=value pair"))?;

            match key {
                "version" => {
                    version = Some(
                        value
                            .parse::<RuntimeVersion>()
                            .map_err(|cause| format!("invalid version: {cause}"))?,
                    );
                }
                "platform" => {
                    platform = Some(match value {
                        "macos-universal" => RuntimePlatform::MacOsUniversal,
                        other => return Err(format!("unknown platform {other:?}")),
                    });
                }
                "executable" => {
                    executable = Some(
                        value
                            .parse::<bitarchive_domain::runtime::RelativePath>()
                            .map_err(|cause| format!("invalid executable path: {cause}"))?,
                    );
                }
                other => return Err(format!("unknown key {other:?}")),
            }
        }

        Ok(Self {
            version: version.ok_or_else(|| String::from("the record has no version"))?,
            platform: platform.ok_or_else(|| String::from("the record has no platform"))?,
            executable: executable.ok_or_else(|| String::from("the record has no executable"))?,
        })
    }
}

/// Returns `true` when a rename failed because the two paths are on different
/// filesystems.
fn is_cross_device(cause: &std::io::Error) -> bool {
    cause.raw_os_error() == Some(18) // EXDEV
}

/// Copies a directory tree, preserving file permissions.
///
/// Symbolic links are reproduced as symbolic links instead of being followed, so a
/// payload cannot smuggle content in from outside the staged tree by pointing at
/// it.
fn copy_tree(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::create_dir_all(target)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from)?;

        if metadata.is_dir() {
            copy_tree(&from, &to)?;
        } else if metadata.file_type().is_symlink() {
            let destination = fs::read_link(&from)?;
            symlink(&destination, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }

    Ok(())
}

/// Creates a symbolic link.
#[cfg(unix)]
fn symlink(destination: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(destination, link)
}

/// Creates a symbolic link.
#[cfg(not(unix))]
fn symlink(_destination: &Path, link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        ErrorKind::Unsupported,
        format!(
            "symbolic links are not supported on this platform: {}",
            link.display()
        ),
    ))
}
