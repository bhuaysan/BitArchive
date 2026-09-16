//! The ports of managed runtime acquisition.
//!
//! A runtime is a versioned, immutable component that BitArchive installs for the
//! user (ARCHITECTURE.md §23). The *description* of such a component lives in the
//! domain ([`bitarchive_domain::runtime`]); this module defines what the outer
//! layers must provide so that a description can become a verified, installed,
//! activated installation:
//!
//! ```text
//! RuntimeDefinition            ← domain: what is pinned
//!       ↓
//! ArtifactDownloader           ← port: fetch the pinned bytes, verify as they arrive
//!       ↓
//! ArtifactExtractor            ← port: unpack what the kind says is packaged
//!       ↓
//! RuntimeInstaller             ← port: stage, install atomically, activate
//!       ↓
//! InstalledRuntime             ← active version + where its executable is
//! ```
//!
//! # Why these are ports and not concrete types
//!
//! Downloading, unpacking, and installing all touch the network or the file
//! system. The application layer may not (its crate documentation forbids HTTP
//! clients and filesystem access), and the domain may not either. So the *shapes*
//! are declared here, in the layer that owns use cases, and the *mechanisms* are
//! implemented by `bitarchive-infrastructure` and `bitarchive-platform`.
//!
//! # What is deliberately absent
//!
//! - No "latest version" query. A definition is pinned, so nothing needs to ask
//!   what the newest release is (Issue #19).
//! - No signature check. ARCHITECTURE.md §24 describes signed distribution
//!   manifests and that step is not implemented yet; the pinned SHA-256 is the
//!   trust anchor until it is. The seam for it is the
//!   [`ArtifactDownloader`] boundary, which is where a signature would be
//!   checked before any bytes are trusted.
//! - No core management. A runtime and a core are different component classes
//!   (ARCHITECTURE.md §23.4) and nothing here assumes a runtime ships cores.
//! - No rollback. ARCHITECTURE.md §23.3 keeps an active and a previous version so
//!   that a one-step rollback stays possible, but the rollback operation itself
//!   belongs to a later Issue.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use bitarchive_domain::runtime::{RuntimeDefinition, RuntimeId, RuntimeVersion, Sha256Digest};

/// One verified artifact, on local disk.
///
/// Carries the digest it was verified against, so a later stage cannot be handed
/// an artifact without the evidence that it matched the pinned definition. The
/// fields are private and there is one constructor, which takes the digest: an
/// `Artifact` therefore always stands for "these bytes were checked against this
/// digest".
///
/// Creating one performs no I/O. It is the downloader's implementation that
/// computes the digest and returns the value only after the bytes matched.
///
/// ```
/// use bitarchive_application::managed_runtime::{Artifact, ArtifactSourceKind};
///
/// let artifact = Artifact::new(
///     "/tmp/RetroArch_Metal.dmg",
///     "81b79121ba26d539064ae13b4d0419a120c3d165afbe656cf5f5412b15fdb434"
///         .parse()
///         .unwrap(),
///     ArtifactSourceKind::Network,
/// );
///
/// assert_eq!(artifact.source(), ArtifactSourceKind::Network);
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Artifact {
    path: PathBuf,
    digest: Sha256Digest,
    source: ArtifactSourceKind,
    size: Option<u64>,
}

impl Artifact {
    /// Creates a verified artifact.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, digest: Sha256Digest, source: ArtifactSourceKind) -> Self {
        Self {
            path: path.into(),
            digest,
            source,
            size: None,
        }
    }

    /// Records the size of the artifact in bytes.
    ///
    /// The size is informational; it is never used to decide whether an artifact
    /// is trustworthy, because the digest already answers that.
    #[must_use]
    pub const fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Returns where the artifact is stored locally.
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Returns the digest the artifact was verified against.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns where the bytes came from.
    #[must_use]
    pub const fn source(&self) -> ArtifactSourceKind {
        self.source
    }

    /// Returns the size of the artifact in bytes, when the downloader knew it.
    #[must_use]
    pub const fn size(&self) -> Option<u64> {
        self.size
    }
}

/// Where a verified artifact's bytes came from.
///
/// The distinction exists because ARCHITECTURE.md §44 plans a bundled bootstrap
/// runtime that is imported into the same component store as a downloaded one.
/// Recording the origin keeps that path honest instead of pretending every
/// artifact was fetched over the network.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArtifactSourceKind {
    /// The artifact was fetched from the pinned official URL.
    Network,
    /// The artifact was provided locally, for example by a test double or a
    /// later bundled bootstrap runtime.
    Local,
}

/// Fetches the artifact of a pinned definition and verifies it while doing so.
///
/// # Contract
///
/// An implementation must:
///
/// 1. download only from [`RuntimeDefinition::source`], and only from the
///    official host that source is pinned to;
/// 2. compute the SHA-256 of the bytes **as they are written**, so no window
///    exists in which unverified bytes are treated as verified;
/// 3. return [`Err`] when the computed digest differs from
///    [`RuntimeDefinition::digest`], removing whatever it wrote;
/// 4. never overwrite the target path of a previous successful result.
///
/// The expected digest is read from the definition and never from the response
/// that delivered the artifact: a download that succeeded is not a download that
/// is trusted (Issue #19, ARCHITECTURE.md §11).
pub trait ArtifactDownloader {
    /// Downloads `definition`'s artifact into `target` and verifies it.
    ///
    /// `target` is a file path that does not exist yet; the parent directory is
    /// created if needed. On failure nothing of the artifact may remain at
    /// `target`.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeStoreError::DownloadFailed`] for a transport, protocol,
    /// or target-host failure, and [`RuntimeStoreError::DigestMismatch`] when the
    /// downloaded bytes do not match the pinned digest.
    fn download(
        &self,
        definition: &RuntimeDefinition,
        target: &std::path::Path,
    ) -> Result<Artifact, RuntimeStoreError>;
}

/// Unpacks a verified artifact into a directory that is about to be installed.
///
/// # Contract
///
/// An implementation must:
///
/// 1. leave `artifact` itself untouched;
/// 2. place the unpacked payload directly inside `working_directory`, so that the
///    directory becomes the content of the component version;
/// 3. verify that the payload matches [`RuntimeDefinition::kind`] and report
///    [`RuntimeStoreError::UnexpectedArtifact`] otherwise;
/// 4. remove any temporary mount, scratch file, or helper state it created, even
///    when it fails, so the caller can delete `working_directory` wholesale.
///
/// Nothing here inspects the unpacked executable. Whether the executable named by
/// [`RuntimeDefinition::executable`] is present and usable is a separate
/// validation step, owned by the installer.
pub trait ArtifactExtractor {
    /// Unpacks `artifact` into `working_directory`.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeStoreError::UnpackFailed`] when the payload cannot be
    /// unpacked, and [`RuntimeStoreError::UnexpectedArtifact`] when the payload
    /// does not match the artifact kind the definition pinned.
    fn unpack(
        &self,
        definition: &RuntimeDefinition,
        artifact: &Artifact,
        working_directory: &std::path::Path,
    ) -> Result<(), RuntimeStoreError>;
}

/// Mounts an Apple disk image and reads one application bundle out of it.
///
/// This is the one step of artifact unpacking that needs the operating system: a
/// `.dmg` is mounted by the platform's disk-image service, and `bitarchive-platform`
/// owns that bridge. Naming it as a port keeps the installer free of OS details
/// and keeps the platform crate free of installation logic.
///
/// # Contract
///
/// An implementation must:
///
/// 1. mount `image` read-only, so nothing in the image can be modified;
/// 2. copy the bundle named by
///    [`AppleDiskImage`](bitarchive_domain::runtime::ArtifactKind::AppleDiskImage) out of the
///    mounted volume into `destination`, which already exists;
/// 3. detach the image again, whether or not the copy succeeded, so no mount
///    outlives one acquisition attempt;
/// 4. report [`RuntimeStoreError::UnexpectedArtifact`] when the mounted volume has
///    no such bundle, instead of guessing at what it found.
///
/// Nothing here validates the executable: whether the bundle contains the
/// executable the definition pins is checked by the installer, on the copied
/// payload, before anything is installed.
pub trait DiskImageExtractor {
    /// Copies the bundle of `definition`'s artifact out of `image` into
    /// `destination`.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeStoreError::UnpackFailed`] when the image cannot be
    /// mounted or read, and [`RuntimeStoreError::UnexpectedArtifact`] when the
    /// mounted volume does not contain the expected bundle.
    fn extract_bundle(
        &self,
        definition: &RuntimeDefinition,
        image: &std::path::Path,
        destination: &std::path::Path,
    ) -> Result<(), RuntimeStoreError>;
}

/// Installs and activates pinned runtime versions in the versioned component
/// store.
///
/// # Contract
///
/// An implementation must:
///
/// 1. install into a versioned directory below the component store, and nowhere
///    else — never into a system location, never into `/Applications`, and never
///    into a user's own RetroArch installation;
/// 2. treat an installed version as immutable: [`RuntimeInstaller::install`]
///    refuses a version that is already installed instead of replacing it;
/// 3. stage everything before installing, so a failure never leaves a partial
///    *active* runtime;
/// 4. make installation atomic with respect to the version directory, and
///    activation atomic with respect to its record;
/// 5. leave the active runtime untouched when any step fails;
/// 6. point the active runtime only at a version that is actually installed.
///
/// # Errors
///
/// Every failure category is a [`RuntimeStoreError`], so a caller can react to
/// "this version is already installed" differently from "the download failed"
/// without parsing messages.
pub trait RuntimeInstaller {
    /// Runs the complete acquisition of `definition`.
    ///
    /// ```text
    /// download
    ///   → verify SHA-256
    ///   → unpack
    ///   → validate the expected executable exists
    ///   → install the version directory
    ///   → activate
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeStoreError::UnsupportedPlatform`] when the definition
    /// targets a platform this build cannot install, and otherwise the error of
    /// the step that failed. A failure never activates a runtime and never
    /// leaves the previous active runtime changed.
    fn install(
        &self,
        definition: &RuntimeDefinition,
    ) -> Result<InstalledRuntime, RuntimeStoreError>;

    /// Returns the runtime version that is currently active for `id`.
    ///
    /// [`None`] means nothing is active — no version was ever activated, or the
    /// active record was removed. It never means "fall back to something else":
    /// there is no user path and no system installation to fall back to.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeStoreError::ActiveRecordUnreadable`] when activation
    /// state exists but cannot be read or understood, and
    /// [`RuntimeStoreError::ActiveRuntimeMissing`] when it names a version that
    /// is no longer installed.
    fn active_runtime(&self, id: &RuntimeId)
    -> Result<Option<InstalledRuntime>, RuntimeStoreError>;

    /// Returns the versions of `id` that are installed, ordered by version.
    ///
    /// This is what makes the store observable without a database: the installed
    /// set is derived from the versioned directories that exist.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeStoreError::UnreadableStore`] when the store exists but
    /// cannot be listed.
    fn installed_versions(&self, id: &RuntimeId) -> Result<Vec<RuntimeVersion>, RuntimeStoreError>;
}

/// One runtime version that is installed in the component store and active.
///
/// The value ties a pinned definition to a concrete installation directory, so
/// the executable a launch will use is derived rather than searched for:
///
/// ```text
/// installation_directory.join(definition.executable())
/// ```
///
/// ```
/// use bitarchive_application::managed_runtime::InstalledRuntime;
///
/// let installed = InstalledRuntime::new(
///     "retroarch".parse().unwrap(),
///     "1.22.2".parse().unwrap(),
///     "/components/runtime/macos-universal/1.22.2",
///     "RetroArch.app/Contents/MacOS/RetroArch".parse().unwrap(),
/// );
///
/// assert_eq!(
///     installed.executable_path().to_string_lossy(),
///     "/components/runtime/macos-universal/1.22.2/RetroArch.app/Contents/MacOS/RetroArch",
/// );
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InstalledRuntime {
    id: RuntimeId,
    version: RuntimeVersion,
    directory: PathBuf,
    executable: bitarchive_domain::runtime::RelativePath,
}

impl InstalledRuntime {
    /// Creates a value for an installed, active runtime version.
    #[must_use]
    pub fn new(
        id: RuntimeId,
        version: RuntimeVersion,
        directory: impl Into<PathBuf>,
        executable: bitarchive_domain::runtime::RelativePath,
    ) -> Self {
        Self {
            id,
            version,
            directory: directory.into(),
            executable,
        }
    }

    /// Returns the runtime identity this installation belongs to.
    #[must_use]
    pub const fn id(&self) -> &RuntimeId {
        &self.id
    }

    /// Returns the installed version.
    #[must_use]
    pub const fn version(&self) -> &RuntimeVersion {
        &self.version
    }

    /// Returns the installation directory of this version.
    #[must_use]
    pub fn directory(&self) -> &std::path::Path {
        &self.directory
    }

    /// Returns the path below the installation directory that holds the
    /// executable.
    #[must_use]
    pub const fn executable(&self) -> &bitarchive_domain::runtime::RelativePath {
        &self.executable
    }

    /// Returns the concrete path of the runtime executable.
    ///
    /// This is the value a later launch step hands to
    /// `RetroArchLaunchInput::new(...)`. It is always inside
    /// [`directory`](Self::directory), because
    /// [`RelativePath`](bitarchive_domain::runtime::RelativePath) cannot contain
    /// `..` or an absolute component.
    #[must_use]
    pub fn executable_path(&self) -> PathBuf {
        let mut path = self.directory.clone();

        for component in self.executable.components() {
            path.push(component);
        }

        path
    }
}

/// Everything that can go wrong while acquiring, installing, or resolving a
/// managed runtime.
///
/// The variants are failure *categories*, not messages: a caller can tell "this
/// version is already installed" from "the download failed" from "the active
/// record is broken". They carry no user-facing text and no localization
/// (ARCHITECTURE.md §21, §33).
#[derive(Debug)]
pub enum RuntimeStoreError {
    /// The definition targets a platform this build cannot install.
    UnsupportedPlatform {
        /// The platform the definition pins.
        platform: bitarchive_domain::runtime::RuntimePlatform,
    },
    /// The artifact could not be fetched.
    DownloadFailed {
        /// The URL that was attempted.
        url: String,
        /// The underlying cause.
        cause: std::io::Error,
    },
    /// The downloaded bytes did not match the pinned digest.
    ///
    /// The artifact has been discarded and nothing was installed.
    DigestMismatch {
        /// The digest the definition pins.
        expected: Sha256Digest,
        /// The digest of what was actually downloaded.
        actual: Sha256Digest,
    },
    /// The artifact could not be unpacked.
    UnpackFailed {
        /// The artifact kind that was being unpacked.
        kind: String,
        /// The underlying cause.
        cause: std::io::Error,
    },
    /// A disk image could not be mounted, read, or detached.
    DiskImageFailed {
        /// The image that was being read.
        image: PathBuf,
        /// What the platform reported.
        cause: String,
    },
    /// The unpacked payload did not match the pinned artifact kind.
    UnexpectedArtifact {
        /// The artifact kind the definition pins.
        expected: String,
        /// What was found instead.
        found: String,
    },
    /// The unpacked payload does not contain the pinned executable.
    ExecutableMissing {
        /// The executable path that was expected.
        expected: PathBuf,
    },
    /// Staging and the component store are on different filesystems.
    ///
    /// Installing a version is one `rename`, which only works within one
    /// filesystem. A copy fallback would make the installation multi-step and
    /// could leave a partial directory at the path that means "complete
    /// installation", so a cross-filesystem configuration is reported instead of
    /// being repaired non-atomically.
    ///
    /// Nothing was installed: the rename failed before anything appeared at
    /// `installation`, and the active runtime is untouched. This error means the
    /// store is configured in a way the installation contract cannot honour.
    CrossDeviceInstallation {
        /// The staged payload that could not be moved.
        staged_payload: PathBuf,
        /// The version directory it could not be moved to.
        installation: PathBuf,
    },
    /// The version is already installed.
    ///
    /// Installed versions are immutable, so this is reported instead of
    /// overwriting one.
    AlreadyInstalled {
        /// The runtime that is already installed.
        id: RuntimeId,
        /// The version that is already installed.
        version: RuntimeVersion,
        /// The directory that already holds it.
        directory: PathBuf,
    },
    /// The component store exists but cannot be read.
    UnreadableStore {
        /// The directory that could not be listed.
        directory: PathBuf,
        /// The underlying cause.
        cause: std::io::Error,
    },
    /// The active runtime record exists but cannot be read or understood.
    ActiveRecordUnreadable {
        /// The record that could not be read.
        path: PathBuf,
        /// Why it could not be read.
        cause: String,
    },
    /// The active runtime record names a version that is not installed.
    ActiveRuntimeMissing {
        /// The runtime whose record is broken.
        id: RuntimeId,
        /// The version the record names.
        version: RuntimeVersion,
        /// The directory that should hold it.
        directory: PathBuf,
    },
    /// A filesystem operation failed.
    Io {
        /// What was being attempted.
        operation: &'static str,
        /// The path the operation was about.
        path: PathBuf,
        /// The underlying cause.
        cause: std::io::Error,
    },
}

impl RuntimeStoreError {
    /// Creates an [`Io`](Self::Io) error.
    #[must_use]
    pub fn io(operation: &'static str, path: impl Into<PathBuf>, cause: std::io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            cause,
        }
    }
}

impl fmt::Display for RuntimeStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform { platform } => write!(
                f,
                "this build cannot install a runtime for platform {platform}"
            ),
            Self::DownloadFailed { url, cause } => {
                write!(f, "downloading {url} failed: {cause}")
            }
            Self::DigestMismatch { expected, actual } => write!(
                f,
                "the downloaded artifact does not match the pinned SHA-256: expected {expected}, \
                 got {actual}"
            ),
            Self::UnpackFailed { kind, cause } => {
                write!(f, "unpacking a {kind} artifact failed: {cause}")
            }
            Self::DiskImageFailed { image, cause } => write!(
                f,
                "the disk image at {} could not be read: {cause}",
                image.display()
            ),
            Self::UnexpectedArtifact { expected, found } => write!(
                f,
                "expected a {expected} artifact, but the payload is {found}"
            ),
            Self::ExecutableMissing { expected } => write!(
                f,
                "the installed runtime does not contain the expected executable at {}",
                expected.display()
            ),
            Self::CrossDeviceInstallation {
                staged_payload,
                installation,
            } => write!(
                f,
                "the staged runtime at {} cannot be installed at {} in one step, because the two \
                 are on different filesystems; nothing was installed and the active runtime is \
                 unchanged",
                staged_payload.display(),
                installation.display()
            ),
            Self::AlreadyInstalled {
                id,
                version,
                directory,
            } => write!(
                f,
                "{id} {version} is already installed at {}; installed versions are immutable",
                directory.display()
            ),
            Self::UnreadableStore { directory, cause } => write!(
                f,
                "the component store at {} could not be read: {cause}",
                directory.display()
            ),
            Self::ActiveRecordUnreadable { path, cause } => write!(
                f,
                "the active runtime record at {} could not be read: {cause}",
                path.display()
            ),
            Self::ActiveRuntimeMissing {
                id,
                version,
                directory,
            } => write!(
                f,
                "the active {id} version {version} is not installed at {}",
                directory.display()
            ),
            Self::Io {
                operation,
                path,
                cause,
            } => write!(f, "{operation} failed for {}: {cause}", path.display()),
        }
    }
}

impl std::error::Error for RuntimeStoreError {}

/// How long a caller is willing to wait for one artifact download.
///
/// A timeout is part of the download contract rather than an implementation
/// detail, because a caller that cannot bound the wait cannot honour
/// cancellation (ARCHITECTURE.md §31, AGENTS.md §10). An idle timeout is separate
/// from the overall budget: a large artifact may legitimately take minutes while
/// never stalling for long.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DownloadTimeout {
    /// The overall budget for one download.
    pub overall: Duration,
    /// The longest a single read may stall.
    pub idle: Duration,
}

impl DownloadTimeout {
    /// Returns the timeout used for a managed runtime artifact.
    ///
    /// The artifact of a runtime is a few hundred megabytes, so the budget is
    /// generous while the idle timeout stays short enough that a dead connection
    /// is noticed quickly.
    #[must_use]
    pub const fn runtime_artifact() -> Self {
        Self {
            overall: Duration::from_secs(30 * 60),
            idle: Duration::from_secs(60),
        }
    }
}

impl Default for DownloadTimeout {
    fn default() -> Self {
        Self::runtime_artifact()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use bitarchive_domain::runtime::RelativePath;

    fn executable() -> RelativePath {
        RelativePath::from_str("RetroArch.app/Contents/MacOS/RetroArch")
            .expect("the documented executable path is relative")
    }

    /// The executable path is derived from the installation directory and the
    /// pinned relative path, and it stays inside that directory.
    #[test]
    fn the_executable_path_is_derived_from_the_installation_directory() {
        let installed = InstalledRuntime::new(
            RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity"),
            RuntimeVersion::from_str("1.22.2").expect("a valid version"),
            "/components/runtime/macos-universal/1.22.2",
            executable(),
        );

        assert_eq!(installed.version().as_str(), "1.22.2");
        assert_eq!(installed.id().as_str(), RuntimeId::RETROARCH);
        assert_eq!(
            installed.directory(),
            std::path::Path::new("/components/runtime/macos-universal/1.22.2")
        );
        assert_eq!(
            installed.executable_path(),
            std::path::Path::new(
                "/components/runtime/macos-universal/1.22.2/RetroArch.app/Contents/MacOS/RetroArch"
            )
        );
        assert!(
            installed
                .executable_path()
                .starts_with(installed.directory()),
            "the executable always stays inside the managed version directory"
        );
    }

    /// An artifact always carries the digest it was verified against, so a later
    /// stage cannot receive one without that evidence.
    #[test]
    fn an_artifact_carries_its_verified_digest() {
        let digest: Sha256Digest =
            "81b79121ba26d539064ae13b4d0419a120c3d165afbe656cf5f5412b15fdb434"
                .parse()
                .expect("a valid digest");

        let artifact = Artifact::new(
            "/downloads/artifacts/retroarch.dmg",
            digest,
            ArtifactSourceKind::Network,
        )
        .with_size(232_801_022);

        assert_eq!(artifact.digest(), digest);
        assert_eq!(artifact.source(), ArtifactSourceKind::Network);
        assert_eq!(artifact.size(), Some(232_801_022));
        assert_eq!(
            artifact.path(),
            std::path::Path::new("/downloads/artifacts/retroarch.dmg")
        );
    }

    /// A caller can bound a download, so a stalled transfer cannot hang forever.
    #[test]
    fn a_download_timeout_is_bounded_and_idle_is_shorter_than_the_total() {
        let timeout = DownloadTimeout::runtime_artifact();

        assert!(timeout.idle < timeout.overall);
        assert_eq!(DownloadTimeout::default(), timeout);
    }

    /// The failure categories stay distinguishable, so a caller can treat an
    /// installed version differently from a transport failure.
    #[test]
    fn failure_categories_remain_distinguishable() {
        let already = RuntimeStoreError::AlreadyInstalled {
            id: RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity"),
            version: RuntimeVersion::from_str("1.22.2").expect("a valid version"),
            directory: PathBuf::from("/components/runtime/macos-universal/1.22.2"),
        };

        assert!(already.to_string().contains("immutable"));

        let io = RuntimeStoreError::io(
            "creating the staging directory",
            "/caches/staging",
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        );

        assert!(io.to_string().contains("staging"));
        assert!(matches!(io, RuntimeStoreError::Io { .. }));
    }
}
