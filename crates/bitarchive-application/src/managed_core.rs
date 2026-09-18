//! The ports of managed core acquisition.
//!
//! A libretro core is a versioned, immutable component that BitArchive installs
//! for the user (ARCHITECTURE.md §23), and it is a **different component class**
//! from the runtime. The *description* of such a component lives in the domain
//! ([`bitarchive_domain::managed_core`]); this module defines what the outer layers
//! must provide so that a curated description can become a verified, installed
//! library:
//!
//! ```text
//! CoreDefinition               ← domain: what is curated and pinned
//!       ↓
//! ArtifactDownloader           ← port: fetch the pinned bytes, verify as they arrive
//!       ↓
//! CoreArchiveExtractor         ← port: take the one expected library out of the archive
//!       ↓
//! CoreInstaller                ← port: stage, install atomically — and *not* activate
//!       ↓
//! ManagedCore                  ← the installed build + where its library is
//! ```
//!
//! # Why there is no activation here
//!
//! This is the one place where the core path deliberately diverges from the runtime
//! path, and the divergence is the point:
//!
//! ```text
//! Runtime   has an active record: one active runtime version per runtime
//! Core      has no active record:  several builds side by side, no global default
//! ```
//!
//! `ARCHITECTURE.md` §23.1 defines an activation registry for runtimes;
//! §20.4 and invariant 21 define how a core is chosen — `System → Game → optional
//! Release`, with no global core default. A "currently active core" would be a
//! second, contradictory answer to a question the resolution policy already
//! answers, so no type in this module can express one. Installing a core records
//! that it is installed, and nothing more.
//!
//! # Core resolution is not touched here
//!
//! Nothing in this module chooses a core for a game, stores a system default,
//! persists a game override, or ranks cores. The existing policy in
//! [`bitarchive_domain::core`] stays exactly as it is; this module answers only
//! "is this curated component installed, and where is its library?".
//!
//! # What is deliberately absent
//!
//! - No "latest build" query. A definition is pinned, so nothing asks what the
//!   newest build is (Issue #21 §10, §11).
//! - No signature check. ARCHITECTURE.md §24 describes signed distribution
//!   manifests and that step is not implemented yet; the pinned SHA-256 is the
//!   trust anchor until it is. The seam is the [`ArtifactDownloader`] boundary,
//!   exactly as for the runtime.
//! - No dynamic loading. A core library is never opened into the BitArchive
//!   process, not even to validate it (Issue #21 §20).
//! - No firmware. mGBA's firmware entries are all optional and no BIOS is
//!   acquired, bundled, or checked here (ARCHITECTURE.md §25, Issue #21 §28).
//!
//! [`ArtifactDownloader`]: crate::managed_runtime::ArtifactDownloader

use std::fmt;
use std::path::PathBuf;

use bitarchive_domain::managed_core::{CoreBuildId, CoreComponentId, CoreDefinition, CorePlatform};

use crate::managed_runtime::{Artifact, ArtifactRequest, RuntimeStoreError};

/// One installed core build.
///
/// The value exists only for a build that is installed and whose library is
/// present, so holding one is evidence that a later launch has a core library to
/// hand to RetroArch:
///
/// ```text
/// components/cores/<component-id>/<platform>/<build>/<library>
/// ```
///
/// It carries the definition it satisfies, so the library path is *derived* rather
/// than searched for:
///
/// ```text
/// directory.join(definition.library())
/// ```
///
/// ```
/// use bitarchive_application::managed_core::ManagedCore;
/// use bitarchive_domain::managed_core::{CorePlatform, mgba_bootstrap};
///
/// let installed = ManagedCore::new(
///     mgba_bootstrap(CorePlatform::MacOsArm64),
///     "/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d",
/// );
///
/// assert_eq!(
///     installed.library_path().to_string_lossy(),
///     "/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d/mgba_libretro.dylib",
/// );
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ManagedCore {
    definition: CoreDefinition,
    directory: PathBuf,
}

impl ManagedCore {
    /// Creates a value for an installed core build.
    ///
    /// `directory` must be the installation directory of that build; the store
    /// produces the value only for a directory that exists and contains the pinned
    /// library.
    #[must_use]
    pub fn new(definition: CoreDefinition, directory: impl Into<PathBuf>) -> Self {
        Self {
            definition,
            directory: directory.into(),
        }
    }

    /// Returns the curated definition this installation satisfies.
    #[must_use]
    pub const fn definition(&self) -> &CoreDefinition {
        &self.definition
    }

    /// Returns the distribution identity of the component.
    #[must_use]
    pub const fn component_id(&self) -> &CoreComponentId {
        self.definition.component_id()
    }

    /// Returns the installed build identity.
    #[must_use]
    pub const fn build_id(&self) -> &CoreBuildId {
        self.definition.build_id()
    }

    /// Returns the platform the installed build is for.
    #[must_use]
    pub const fn platform(&self) -> CorePlatform {
        self.definition.platform()
    }

    /// Returns the name RetroArch addresses this core by.
    #[must_use]
    pub fn core_name(&self) -> &str {
        self.definition.core_name()
    }

    /// Returns the installation directory of this build.
    #[must_use]
    pub fn directory(&self) -> &std::path::Path {
        &self.directory
    }

    /// Returns the path below the installation directory that holds the library.
    #[must_use]
    pub fn library(&self) -> &bitarchive_domain::component::RelativePath {
        self.definition.library()
    }

    /// Returns the concrete path of the core library.
    ///
    /// This is the value a later launch step passes to
    /// [`RetroArchLaunchInput::new`](crate::managed_core) as the `-L` argument. It
    /// is always inside [`directory`](Self::directory), because the definition's
    /// library is a [`RelativePath`](bitarchive_domain::component::RelativePath)
    /// that cannot contain `..` or an absolute component.
    #[must_use]
    pub fn library_path(&self) -> PathBuf {
        let mut path = self.directory.clone();

        for component in self.definition.library().components() {
            path.push(component);
        }

        path
    }
}

impl fmt::Display for ManagedCore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} for {} at {}",
            self.definition.component_id(),
            self.definition.build_id(),
            self.definition.platform(),
            self.directory.display()
        )
    }
}

/// Takes one expected library out of a verified core archive.
///
/// # Contract
///
/// An implementation must:
///
/// 1. leave `artifact` itself untouched;
/// 2. look for **exactly** the member named by
///    [`CoreDefinition::expected_member`], refuse a missing one, and refuse an
///    archive that contains more than one entry with that name;
/// 3. never unpack the archive wholesale and never write a path that the archive
///    supplied: no absolute path, no `..`, and no symbolic link may decide where
///    anything is written. Anything that would leave `working_directory` is
///    refused;
/// 4. write only the expected member, so an unrelated member of the archive can
///    never overwrite or add a file;
/// 5. remove any scratch state it created, even when it fails, so the caller can
///    delete `working_directory` wholesale.
///
/// Nothing here verifies the archive: the artifact it receives has already been
/// checked against the pinned SHA-256 by the downloader, which is the only
/// component that may decide an artifact is trustworthy. Nothing here loads the
/// library either: a core is never opened into the BitArchive process, not even to
/// validate it. Whether the library is present and usable is a separate validation
/// step owned by the installer.
///
/// # Why the contract is this narrow
///
/// A generic "extract everything into a directory" is the shape that makes archive
/// extraction dangerous. For a core BitArchive knows the exact member name in
/// advance, so the narrow contract is both sufficient and strictly safer: there is
/// no code path in which an archive member decides a destination path.
pub trait CoreArchiveExtractor {
    /// Writes the expected member of `definition`'s archive into
    /// `working_directory`, at the path the definition pins for it.
    ///
    /// # Errors
    ///
    /// Returns [`CoreStoreError::ArchiveUnreadable`] when the archive cannot be
    /// read, [`CoreStoreError::ArchiveMemberMissing`] when it does not contain the
    /// expected member, [`CoreStoreError::ArchiveMemberDuplicated`] when it
    /// contains that member more than once, and
    /// [`CoreStoreError::ArchiveMemberUnsafe`] when the entry cannot be written
    /// inside `working_directory`.
    fn unpack(
        &self,
        definition: &CoreDefinition,
        artifact: &Artifact,
        working_directory: &std::path::Path,
    ) -> Result<(), CoreStoreError>;
}

/// Installs pinned core builds in the versioned component store.
///
/// # Contract
///
/// An implementation must:
///
/// 1. install into a versioned directory below the component store's core tree,
///    and nowhere else — never into a system location, never into RetroArch's own
///    core directory, and never into a user's existing core collection;
/// 2. separate the architectures: an ARM64 build and an Intel build never share
///    an installation path, and a host is never served the other architecture's
///    library;
/// 3. treat an installed build as immutable: [`CoreInstaller::install`] refuses a
///    build that is already installed instead of replacing it;
/// 4. stage everything before installing, so a failure never leaves a partial
///    installation at a final build path;
/// 5. make installation atomic with respect to the build directory;
/// 6. create **no** activation, default, or "current" record of any kind;
/// 7. report a requested build that is not installed as not installed, and never
///    substitute another build, another platform, or another core.
///
/// # Errors
///
/// Every failure category is a [`CoreStoreError`], so a caller can react to "this
/// build is already installed" differently from "the archive did not contain the
/// expected library" without parsing messages.
pub trait CoreInstaller {
    /// Runs the complete acquisition of `definition`.
    ///
    /// ```text
    /// download
    ///   → verify SHA-256
    ///   → extract the expected archive member
    ///   → validate the expected library exists
    ///   → install the build directory
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`CoreStoreError::UnsupportedPlatform`] when the definition targets a
    /// platform this build cannot install, and otherwise the error of the step that
    /// failed. A failure never leaves a final build directory behind.
    fn install(&self, definition: &CoreDefinition) -> Result<ManagedCore, CoreStoreError>;

    /// Returns `true` when `component_id` is installed in exactly `build_id` on
    /// `platform`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreStoreError::Store`] when the store exists but cannot be read.
    fn is_installed(
        &self,
        component_id: &CoreComponentId,
        platform: CorePlatform,
        build_id: &CoreBuildId,
    ) -> Result<bool, CoreStoreError>;

    /// Returns the builds of `component_id` installed on `platform`, ordered by
    /// build identity.
    ///
    /// This is what makes the store observable without a database: the installed
    /// set is derived from the build directories that exist.
    ///
    /// # Errors
    ///
    /// Returns [`CoreStoreError::Store`] when the store exists but cannot be
    /// listed.
    fn installed_builds(
        &self,
        component_id: &CoreComponentId,
        platform: CorePlatform,
    ) -> Result<Vec<CoreBuildId>, CoreStoreError>;

    /// Resolves exactly one installed build.
    ///
    /// `definition` names the component, the platform, and the build, so the answer
    /// is either that exact installation or a structured failure. Another build of
    /// the same core, another architecture's build, and another core entirely are
    /// all *not* answers.
    ///
    /// # Errors
    ///
    /// Returns [`CoreStoreError::NotInstalled`] when that exact build is not
    /// installed, [`CoreStoreError::LibraryMissing`] when the build directory
    /// exists but does not contain the pinned library, and
    /// [`CoreStoreError::Store`] when the store cannot be read.
    fn resolve(&self, definition: &CoreDefinition) -> Result<ManagedCore, CoreStoreError>;
}

/// Everything that can go wrong while acquiring, installing, or resolving a
/// managed core.
///
/// The variants are failure *categories*, not messages: a caller can tell "this
/// build is already installed" from "the archive did not contain the expected
/// library" from "that exact build is not installed". They carry no user-facing
/// text and no localization (ARCHITECTURE.md §21, §33).
///
/// Download, digest, platform, and filesystem failures are reachable through
/// [`CoreStoreError::Store`] rather than duplicated here: the core store uses the
/// same downloader contract and the same filesystem vocabulary as the runtime
/// store, and inventing a second spelling for "the digest did not match" would
/// make the two paths harder to compare, not easier (Issue #21 §12).
#[derive(Debug)]
pub enum CoreStoreError {
    /// A step that both component classes share failed.
    ///
    /// Covers the download, the digest check, the platform check, the install
    /// move, and plain filesystem failures.
    Store {
        /// The underlying failure.
        cause: RuntimeStoreError,
    },
    /// The definition's artifact is internally inconsistent.
    ///
    /// The store never downloads an artifact whose pinned URL and recorded
    /// published name disagree, because that means the definition describes
    /// something other than what the URL serves.
    InconsistentDefinition {
        /// The component whose definition is inconsistent.
        component_id: CoreComponentId,
        /// The name the definition records.
        expected_file_name: String,
        /// The name the pinned URL ends in, if it names one.
        url_file_name: Option<String>,
    },
    /// The archive could not be read at all.
    ArchiveUnreadable {
        /// The artifact that could not be read.
        artifact: PathBuf,
        /// The underlying cause.
        cause: std::io::Error,
    },
    /// The archive does not contain the expected member.
    ArchiveMemberMissing {
        /// The archive that was read.
        artifact: PathBuf,
        /// The member name that was expected.
        expected: String,
        /// The members the archive does contain.
        found: Vec<String>,
    },
    /// The archive contains the expected member more than once.
    ///
    /// Which of the two would be installed is not a decision an archive may make,
    /// so the ambiguity is refused instead of resolved.
    ArchiveMemberDuplicated {
        /// The archive that was read.
        artifact: PathBuf,
        /// The member name that appears more than once.
        member: String,
        /// How many entries carry that name.
        occurrences: usize,
    },
    /// The archive contains an entry that cannot be written inside the staged
    /// payload.
    ///
    /// An absolute member path, a `..` component, a symbolic link, or any other
    /// entry whose destination cannot be derived from the member name alone is
    /// refused. Nothing is written for such an archive.
    ArchiveMemberUnsafe {
        /// The archive that was read.
        artifact: PathBuf,
        /// The member name that was refused.
        member: String,
        /// Why it was refused.
        reason: String,
    },
    /// The staged payload does not contain the pinned library.
    LibraryMissing {
        /// The library path that was expected.
        expected: PathBuf,
    },
    /// The version is already installed.
    ///
    /// Installed builds are immutable, so this is reported instead of overwriting
    /// one.
    AlreadyInstalled {
        /// The component that is already installed.
        component_id: CoreComponentId,
        /// The platform it is installed for.
        platform: CorePlatform,
        /// The build that is already installed.
        build_id: CoreBuildId,
        /// The directory that already holds it.
        directory: PathBuf,
    },
    /// That exact build is not installed.
    ///
    /// Reported instead of substituting another build, another platform, or
    /// another core.
    NotInstalled {
        /// The component that was requested.
        component_id: CoreComponentId,
        /// The platform that was requested.
        platform: CorePlatform,
        /// The build that was requested.
        build_id: CoreBuildId,
        /// The directory that would hold it.
        directory: PathBuf,
    },
    /// The platform of the definition is not the platform of this host.
    ///
    /// Installing the Intel build on Apple Silicon, or the other way round, is not
    /// a fallback; it is a different binary.
    UnsupportedPlatform {
        /// The platform the definition pins.
        platform: CorePlatform,
        /// The platform this build of BitArchive can install for.
        host: Option<CorePlatform>,
    },
}

impl CoreStoreError {
    /// Wraps a failure of a step both component classes share.
    #[must_use]
    pub const fn store(cause: RuntimeStoreError) -> Self {
        Self::Store { cause }
    }
}

impl From<RuntimeStoreError> for CoreStoreError {
    fn from(cause: RuntimeStoreError) -> Self {
        Self::Store { cause }
    }
}

impl fmt::Display for CoreStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store { cause } => write!(f, "{cause}"),
            Self::InconsistentDefinition {
                component_id,
                expected_file_name,
                url_file_name,
            } => write!(
                f,
                "the curated definition of {component_id} records the published artifact name \
                 {expected_file_name:?}, but its pinned URL ends in {url_file_name:?}"
            ),
            Self::ArchiveUnreadable { artifact, cause } => write!(
                f,
                "the core archive at {} could not be read: {cause}",
                artifact.display()
            ),
            Self::ArchiveMemberMissing {
                artifact,
                expected,
                found,
            } => write!(
                f,
                "the core archive at {} does not contain {expected:?}; it contains {found:?}",
                artifact.display()
            ),
            Self::ArchiveMemberDuplicated {
                artifact,
                member,
                occurrences,
            } => write!(
                f,
                "the core archive at {} contains {member:?} {occurrences} times, so which library \
                 to install is ambiguous",
                artifact.display()
            ),
            Self::ArchiveMemberUnsafe {
                artifact,
                member,
                reason,
            } => write!(
                f,
                "the core archive at {} contains {member:?}, which cannot be installed: {reason}",
                artifact.display()
            ),
            Self::LibraryMissing { expected } => write!(
                f,
                "the installed core does not contain the expected library at {}",
                expected.display()
            ),
            Self::AlreadyInstalled {
                component_id,
                platform,
                build_id,
                directory,
            } => write!(
                f,
                "{component_id} build {build_id} for {platform} is already installed at {}; \
                 installed core builds are immutable",
                directory.display()
            ),
            Self::NotInstalled {
                component_id,
                platform,
                build_id,
                directory,
            } => write!(
                f,
                "{component_id} build {build_id} for {platform} is not installed at {}; another \
                 build, platform, or core is never substituted",
                directory.display()
            ),
            Self::UnsupportedPlatform { platform, host } => match host {
                Some(host) => write!(
                    f,
                    "the definition targets {platform}, but this BitArchive build installs managed \
                     cores for {host} only; one architecture's core is never substituted for \
                     another's"
                ),
                None => write!(
                    f,
                    "the definition targets {platform}, but this BitArchive build cannot install a \
                     managed core at all"
                ),
            },
        }
    }
}

impl std::error::Error for CoreStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store { cause } => Some(cause),
            Self::ArchiveUnreadable { cause, .. } => Some(cause),
            Self::InconsistentDefinition { .. }
            | Self::ArchiveMemberMissing { .. }
            | Self::ArchiveMemberDuplicated { .. }
            | Self::ArchiveMemberUnsafe { .. }
            | Self::LibraryMissing { .. }
            | Self::AlreadyInstalled { .. }
            | Self::NotInstalled { .. }
            | Self::UnsupportedPlatform { .. } => None,
        }
    }
}

/// Builds the download request for a curated core definition.
///
/// The conversion lives here rather than in the domain because the request type is
/// an application-layer contract. The digest is carried over unchanged: nothing in
/// this conversion may compute, adjust, or discover one.
#[must_use]
pub fn artifact_request(definition: &CoreDefinition) -> ArtifactRequest {
    let request = ArtifactRequest::new(
        definition.component_id().as_str(),
        definition.source().clone(),
        definition.digest(),
    );

    match &definition.artifact().file_name {
        Some(file_name) => request.with_expected_file_name(file_name.clone()),
        None => request,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bitarchive_domain::managed_core::{CorePlatform, mgba_bootstrap};

    use super::*;

    /// The library path is derived from the installation directory and the pinned
    /// relative path, and it stays inside that directory.
    #[test]
    fn the_library_path_is_derived_from_the_installation_directory() {
        let definition = mgba_bootstrap(CorePlatform::MacOsArm64);
        let directory = Path::new("/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d");

        let installed = ManagedCore::new(definition.clone(), directory);

        assert_eq!(installed.component_id(), definition.component_id());
        assert_eq!(installed.build_id(), definition.build_id());
        assert_eq!(installed.platform(), CorePlatform::MacOsArm64);
        assert_eq!(installed.core_name(), "mgba_libretro");
        assert_eq!(installed.directory(), directory);
        assert_eq!(
            installed.library_path(),
            directory.join("mgba_libretro.dylib")
        );
        assert!(
            installed.library_path().starts_with(installed.directory()),
            "the library always stays inside the managed build directory"
        );
        assert_eq!(installed.library().file_name(), "mgba_libretro.dylib");
    }

    /// The resolved core describes itself, so a log or an error can name the
    /// component, the build, the platform, and the directory.
    #[test]
    fn the_resolved_core_describes_itself() {
        let installed = ManagedCore::new(
            mgba_bootstrap(CorePlatform::MacOsX86_64),
            "/components/cores/mgba/macos-x86_64/mgba-0.11-212-7a12d6d",
        );

        let rendered = installed.to_string();

        assert!(rendered.contains("mgba"));
        assert!(rendered.contains("mgba-0.11-212-7a12d6d"));
        assert!(rendered.contains("macos-x86_64"));
    }

    /// The definition is turned into a download request without changing the pin,
    /// and the request names the component, the host, and the published file.
    #[test]
    fn the_download_request_carries_the_pin_unchanged() {
        let definition = mgba_bootstrap(CorePlatform::MacOsArm64);
        let request = artifact_request(&definition);

        assert_eq!(request.component(), "mgba");
        assert_eq!(request.source(), definition.source());
        assert_eq!(request.digest(), definition.digest());
        assert_eq!(request.host(), "buildbot.libretro.com");
        assert_eq!(
            request.expected_file_name(),
            Some("mgba_libretro.dylib.zip")
        );
        assert_eq!(request.url_file_name(), Some("mgba_libretro.dylib.zip"));
        assert!(
            request.published_name_matches_url(),
            "the curated definition is internally consistent"
        );
    }

    /// A definition whose recorded published name disagrees with its URL is
    /// detectable, so a stale pin cannot be downloaded as if it were consistent.
    #[test]
    fn a_request_notices_a_name_the_url_does_not_end_in() {
        let definition = mgba_bootstrap(CorePlatform::MacOsArm64);
        let request =
            artifact_request(&definition).with_expected_file_name("mgba_libretro.dll.zip");

        assert!(!request.published_name_matches_url());
        assert_eq!(request.url_file_name(), Some("mgba_libretro.dylib.zip"));
    }

    /// A download timeout for a core is bounded and shorter than the runtime
    /// budget, because a core archive is smaller by orders of magnitude.
    #[test]
    fn a_core_download_timeout_is_bounded_and_shorter_than_the_runtime_budget() {
        let core = crate::managed_runtime::DownloadTimeout::core_artifact();
        let runtime = crate::managed_runtime::DownloadTimeout::runtime_artifact();

        assert!(core.idle < core.overall);
        assert!(core.overall < runtime.overall);
        assert!(core.idle < runtime.idle);
    }

    /// The failure categories stay distinguishable, so a caller can treat "already
    /// installed" differently from "the archive was wrong".
    #[test]
    fn failure_categories_remain_distinguishable() {
        let definition = mgba_bootstrap(CorePlatform::MacOsArm64);

        let already = CoreStoreError::AlreadyInstalled {
            component_id: definition.component_id().clone(),
            platform: definition.platform(),
            build_id: definition.build_id().clone(),
            directory: PathBuf::from("/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d"),
        };

        assert!(already.to_string().contains("immutable"));

        let missing = CoreStoreError::NotInstalled {
            component_id: definition.component_id().clone(),
            platform: definition.platform(),
            build_id: definition.build_id().clone(),
            directory: PathBuf::from("/components/cores/mgba/macos-arm64/mgba-0.11-212-7a12d6d"),
        };

        assert!(missing.to_string().contains("never substituted"));

        let unsupported = CoreStoreError::UnsupportedPlatform {
            platform: CorePlatform::MacOsX86_64,
            host: Some(CorePlatform::MacOsArm64),
        };

        assert!(unsupported.to_string().contains("never substituted"));

        // A shared failure stays reachable as the cause and keeps its own spelling.
        let shared = CoreStoreError::store(RuntimeStoreError::DigestMismatch {
            expected: definition.digest(),
            actual: definition.digest(),
        });

        assert!(matches!(shared, CoreStoreError::Store { .. }));
        assert!(std::error::Error::source(&shared).is_some());
        assert!(shared.to_string().contains("pinned SHA-256"));
    }
}
