//! Curated, installable libretro core components.
//!
//! A core is a versioned, immutable component that BitArchive manages for the
//! user, and it is a **different component class** from the runtime
//! (ARCHITECTURE.md §23.4, §53.10). A runtime is an application and there is one
//! active runtime; a core is a libretro implementation, several cores and several
//! builds of one core can be installed side by side, and *which* one a launch uses
//! is decided later by the core resolution policy in [`crate::core`]. This module
//! owns the *description* of the distributable component:
//!
//! ```text
//! CoreDefinition
//! ├── component id        mgba                       ← distribution identity
//! ├── display name        mGBA
//! ├── core name           mgba_libretro              ← as RetroArch addresses it
//! ├── build id            mgba-0.11-212-7a12d6d      ← reviewed build identity
//! ├── platform            macos-arm64 | macos-x86_64 ← per architecture
//! ├── artifact            official https URL + pinned SHA-256 + published name
//! ├── kind                libretro-core-archive { member: "mgba_libretro.dylib" }
//! ├── library             mgba_libretro.dylib        ← below the install directory
//! ├── attribution         mGBA · mgba-emu/mgba · MPL-2.0
//! └── provenance          upstream version + revision the build was identified by
//! ```
//!
//! # Identity is not transport
//!
//! The official libretro build host publishes macOS core binaries **only** below a
//! rolling `latest` path, separately per architecture:
//!
//! ```text
//! https://buildbot.libretro.com/nightly/apple/osx/arm64/latest/mgba_libretro.dylib.zip
//! https://buildbot.libretro.com/nightly/apple/osx/x86_64/latest/mgba_libretro.dylib.zip
//! ```
//!
//! Those URLs are *not* immutable: the host replaces the file whenever it builds
//! the core again. This module therefore separates the two things the URL was
//! conflated with:
//!
//! - the **logical identity** of what is installed is the
//!   [`CoreBuildId`] together with the [`CorePlatform`] and the pinned
//!   [`Sha256Digest`] — that is what names an installation and what a resolution
//!   asks for;
//! - the **transport URL** is a moving pointer to whatever the host currently
//!   serves, and it is only ever used to *fetch* bytes that are then checked
//!   against the pinned digest.
//!
//! `latest` is deliberately **not** a build id, and it is not a version. BitArchive
//! does not model "the newest core" (see
//! `docs/decisions/0002-managed-core-acquisition.md`).
//!
//! # The digest is the trust anchor, and it never comes from the download
//!
//! The pinned SHA-256 is part of the definition. There is no code path that reads
//! a digest from a response header, a sidecar file, or a previously downloaded
//! artifact and stores it as trusted ("TOFU"). When the host replaces the file
//! behind a rolling URL, the downloaded bytes no longer match the pin and the
//! installation is **rejected**; BitArchive does not adopt the new digest by
//! itself. Raising the pin is a reviewed source change.
//!
//! # One curated core, and only curated cores
//!
//! B6 is not a core catalog. [`curated_cores`] is the complete allowlist: exactly
//! one reviewed definition, mGBA, in one build, for two macOS architectures. The
//! allowlist is what makes "BitArchive can only install a core for which a
//! reviewed definition exists" a property of the type rather than a rule to
//! remember — there is no constructor here for a user-supplied URL, core name, or
//! library path, and nothing enumerates the build host.
//!
//! # What this module does not do
//!
//! - **No selection policy.** Choosing a core for a game stays in
//!   [`crate::core`]: `Release > Game > System`, no global default
//!   (ARCHITECTURE.md §20.4, §23.5, invariant 21). Installing a core here decides
//!   nothing about which core a launch uses.
//! - **No active/global core.** There is no activation record for cores and no
//!   "currently active core" (ARCHITECTURE.md §23.1 records activation for
//!   runtimes only). Several builds of mGBA may be installed at once.
//! - **No firmware.** mGBA's firmware entries are all optional; this module
//!   records the requirement level and downloads no BIOS (ARCHITECTURE.md §25).
//! - **No I/O.** No HTTP client, no archive library, no filesystem access, no
//!   dynamic loading. Installing and extracting live in outer layers.

use std::fmt;
use std::str::FromStr;

use crate::component::{
    ArtifactReference, ArtifactSource, ComponentArtifactSource, ComponentAttribution, ComponentId,
    LicenseIdentifier, RelativePath, Sha256Digest,
};

/// Upper bound for an accepted core build identity.
///
/// The bound exists so that a definition cannot smuggle an unbounded value into a
/// store path component only. It is far above any real build identity.
pub const MAX_CORE_BUILD_ID_LENGTH: usize = 64;

/// Identifies the distributable component of a libretro core.
///
/// This is the curated distribution identity — `mgba` — and it is **not**
/// [`CoreId`](crate::CoreId):
///
/// ```text
/// CoreId             UUIDv7, fachliche Identität einer Core-Zuordnung
/// CoreComponentId    Slug, Identität einer installierbaren Komponente
/// core name          "mgba_libretro", wie RetroArch den Core anspricht
/// ```
///
/// A [`CoreId`](crate::CoreId) answers "which core did the user configure for this
/// game"; a `CoreComponentId` answers "which distributable thing does BitArchive
/// install". Using the UUID type as a download slug — or the slug as a foreign key
/// — would blur exactly the distinction ARCHITECTURE.md §7 and §11 require, so the
/// types stay separate and no conversion exists between them yet.
///
/// A component identity is also, deliberately, a single safe path component: the
/// accepted alphabet is lower-case ASCII letters, digits, and `-`, and it must
/// start with a letter.
///
/// ```
/// use bitarchive_domain::managed_core::CoreComponentId;
///
/// assert_eq!(CoreComponentId::MGBA, "mgba");
/// assert!("mgba".parse::<CoreComponentId>().is_ok());
/// ```
pub type CoreComponentId = ComponentId;

/// The platform a core artifact and its installation are built for.
///
/// The official libretro build host publishes macOS core binaries **separately**
/// per architecture — there is no universal core artifact — so this type has one
/// variant per architecture and no `universal` variant. Claiming a universal
/// artifact that upstream does not publish would be a false statement about the
/// bytes BitArchive installs, and it would let an Intel build be installed under
/// Apple Silicon.
///
/// Note that this is *not*
/// [`RuntimePlatform`](crate::runtime::RuntimePlatform): the RetroArch runtime is
/// published as a universal image and is modelled as such. Runtime and core
/// platform matrices are different because upstream publishes them differently.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum CorePlatform {
    /// macOS on Apple Silicon.
    MacOsArm64,
    /// macOS on Intel.
    MacOsX86_64,
}

impl CorePlatform {
    /// Every platform a managed core can be installed for, in layout order.
    pub const ALL: [Self; 2] = [Self::MacOsArm64, Self::MacOsX86_64];

    /// Returns the stable slug used in the component store layout.
    ///
    /// The slugs are the build host's own architecture directory names, so the
    /// store, the pin, and the upstream path all name an architecture the same
    /// way.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MacOsArm64 => "macos-arm64",
            Self::MacOsX86_64 => "macos-x86_64",
        }
    }

    /// Returns the build host's architecture directory for this platform.
    #[must_use]
    pub const fn build_host_architecture(self) -> &'static str {
        match self {
            Self::MacOsArm64 => "arm64",
            Self::MacOsX86_64 => "x86_64",
        }
    }
}

impl fmt::Display for CorePlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The platform of the machine BitArchive is running on.
///
/// The answer comes from the compiler's target architecture — a Rust platform
/// fact — and not from a shell command such as `uname -m` or `arch`
/// (ARCHITECTURE.md §2.5, Issue #21 §17). Nothing here spawns a process, and
/// nothing here inspects the operating system at run time.
#[must_use]
pub const fn host_core_platform() -> Option<CorePlatform> {
    if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            return Some(CorePlatform::MacOsArm64);
        }

        if cfg!(target_arch = "x86_64") {
            return Some(CorePlatform::MacOsX86_64);
        }
    }

    None
}

/// Why a host has no managed-core platform.
///
/// The variant carries the architecture and operating system the compiler was
/// configured for, so the message names the real reason instead of reporting a
/// generic failure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct UnsupportedCoreHost {
    /// The target architecture, as `std::env::consts::ARCH` spells it.
    pub architecture: &'static str,
    /// The target operating system, as `std::env::consts::OS` spells it.
    pub operating_system: &'static str,
}

impl UnsupportedCoreHost {
    /// Describes the host this build was compiled for.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            architecture: std::env::consts::ARCH,
            operating_system: std::env::consts::OS,
        }
    }

    /// Resolves the host platform or reports why it cannot be resolved.
    ///
    /// BitArchive never substitutes one architecture's core for another: an
    /// unsupported host is a structured error and not a reason to install the
    /// Intel build on Apple Silicon or the other way round.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedCoreHost`] when the host is not macOS on `aarch64` or
    /// `x86_64`.
    pub fn resolve() -> Result<CorePlatform, Self> {
        host_core_platform().ok_or_else(Self::current)
    }
}

impl fmt::Display for UnsupportedCoreHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BitArchive cannot install a managed libretro core on {}/{}; managed cores are \
             available for macOS on aarch64 and x86_64 only, and one architecture's core is \
             never substituted for another's",
            self.operating_system, self.architecture
        )
    }
}

impl std::error::Error for UnsupportedCoreHost {}

/// The reason a [`CoreBuildId`] was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CoreBuildIdError {
    /// The identity was empty.
    Empty,
    /// The identity exceeded the maximum accepted length.
    TooLong,
    /// The identity contained a character outside the accepted set.
    InvalidCharacter(char),
}

impl fmt::Display for CoreBuildIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a core build identity must not be empty"),
            Self::TooLong => write!(
                f,
                "a core build identity must not exceed {MAX_CORE_BUILD_ID_LENGTH} characters"
            ),
            Self::InvalidCharacter(character) => write!(
                f,
                "a core build identity must consist of ASCII letters, digits, '.', '-', '+' or '_', \
                 but contains {character:?}"
            ),
        }
    }
}

impl std::error::Error for CoreBuildIdError {}

/// The reviewed identity of one built core binary.
///
/// This is the value that names an installation directory and that a resolution
/// asks for. It is **not** the upstream project version, and it is deliberately
/// not spelled `latest`:
///
/// - the official build host publishes core binaries only below a rolling path and
///   attaches no version, revision, or digest metadata to the archive itself;
/// - mGBA's libretro core info reports `display_version = "0.10-dev"`, which
///   identifies neither a release nor a revision and has been stale for several
///   years, so it cannot serve as an identity.
///
/// What the binary *does* carry is its own upstream revision: mGBA links
/// `_gitCommit`, `_gitCommitShort`, and a version string built by
/// `version.cmake` as `<lib-version>-<commit-count>-<short-sha>`. Verified for the
/// pinned artifacts, both architectures report:
///
/// ```text
/// version string   0.11-212-7a12d6d
/// git commit       7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6
/// ```
///
/// The build identity is derived from that: [`CoreComponentId::MGBA`] plus the
/// upstream version string — `mgba-0.11-212-7a12d6d`. The component prefix keeps
/// the store layout readable for a later second core; the upstream revision is
/// carried in full by [`CoreProvenance`] and by the pinned [`Sha256Digest`], which
/// is what actually names the bytes.
///
/// A build identity is an opaque identifier: BitArchive compares build identities
/// for equality and orders them lexicographically, and it does not claim to know
/// which of two builds is newer.
///
/// ```
/// use std::str::FromStr;
///
/// use bitarchive_domain::managed_core::CoreBuildId;
///
/// let build = CoreBuildId::from_str("mgba-0.11-212-7a12d6d").unwrap();
///
/// assert_eq!(build.as_str(), "mgba-0.11-212-7a12d6d");
/// assert!(
///     CoreBuildId::from_str("../escape").is_err(),
///     "a build identity is one safe path component"
/// );
/// ```
///
/// The accepted alphabet is restricted to characters that are safe as a single
/// path component on every supported platform, so `..`, an empty name, a nested
/// path, and a leading `-` cannot reach the component store layout.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CoreBuildId(String);

impl CoreBuildId {
    /// Returns the identity as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for CoreBuildId {
    type Err = CoreBuildIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(CoreBuildIdError::Empty);
        }

        if value.chars().count() > MAX_CORE_BUILD_ID_LENGTH {
            return Err(CoreBuildIdError::TooLong);
        }

        if !value.starts_with(|character: char| character.is_ascii_alphanumeric()) {
            return Err(CoreBuildIdError::InvalidCharacter(
                value.chars().next().unwrap_or(' '),
            ));
        }

        if let Some(invalid) = value.chars().find(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+' | '_'))
        }) {
            return Err(CoreBuildIdError::InvalidCharacter(invalid));
        }

        Ok(Self(value.to_owned()))
    }
}

impl fmt::Display for CoreBuildId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How the pinned core binary was identified upstream.
///
/// This is the reviewable evidence behind the [`CoreBuildId`], recorded next to the
/// pin so a reviewer can re-check it without re-deriving it from the binary:
///
/// - `version` — the version string the upstream project built into this binary,
///   in the project's own format;
/// - `revision` — the full upstream commit the binary reports, which is the
///   authoritative part of the identity;
/// - `revision_short` — the abbreviated form, as displayed;
/// - `channel_date` — the build date the official build host publishes for this
///   artifact in its `.index-extended` listing;
/// - `channel_crc32` — the CRC-32 the official build host publishes for the
///   *uncompressed* library.
///
/// The two channel fields are recorded as **change hints, not as integrity
/// evidence**: the official build host publishes no cryptographic digest for core
/// binaries at all, which is precisely why BitArchive pins its own SHA-256 of the
/// downloaded archive and treats that pin as the trust anchor. CRC-32 is not
/// collision resistant and must never be used to decide whether bytes are
/// trustworthy (ARCHITECTURE.md §24, invariant 10).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CoreProvenance {
    /// The version string the upstream project built into the binary.
    pub version: String,
    /// The full upstream commit the binary reports.
    pub revision: String,
    /// The abbreviated upstream commit, as the project displays it.
    pub revision_short: String,
    /// The build date the official build host publishes for the artifact.
    pub channel_date: String,
    /// The CRC-32 the official build host publishes for the uncompressed library.
    pub channel_crc32: String,
}

/// How a downloaded core artifact is packaged.
///
/// Unlike the runtime's artifact kind, a core artifact is an archive that contains
/// exactly one expected library. Naming the member here is what lets the extractor
/// be narrow: it looks for *this* member, refuses a missing or duplicated one, and
/// never unpacks the archive wholesale.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CoreArtifactKind {
    /// A ZIP archive that contains exactly one libretro core library.
    LibretroCoreArchive {
        /// The exact member name the archive must contain.
        ///
        /// This is a bare file name at the archive root, not a path: the official
        /// build host packages one library per archive, and there is no reason for
        /// a core archive to nest one.
        member: String,
    },
}

impl CoreArtifactKind {
    /// Returns a short label for diagnostics and metadata.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::LibretroCoreArchive { .. } => "libretro-core-archive",
        }
    }

    /// Returns the archive member the kind expects.
    #[must_use]
    pub fn member(&self) -> &str {
        match self {
            Self::LibretroCoreArchive { member } => member,
        }
    }
}

/// Which component, in which build, for which platform.
///
/// A definition is built from five groups rather than from loose values, so that
/// each part of the pin is named where it is written and a review can read the
/// identity, the artifact, the layout, the provenance, and the attribution
/// separately.
///
/// ```
/// use std::str::FromStr;
///
/// use bitarchive_domain::managed_core::{
///     CoreArtifactKind, CoreBuildId, CoreComponentId, CoreDefinition, CoreParts, CorePlatform,
///     CoreProvenance,
/// };
/// use bitarchive_domain::component::{
///     ArtifactReference, ArtifactSource, ComponentArtifactSource, ComponentAttribution,
///     LicenseIdentifier, RelativePath, Sha256Digest,
/// };
///
/// let definition = CoreDefinition::new(CoreParts {
///     identity: (
///         CoreComponentId::MGBA.parse().unwrap(),
///         "mGBA".to_owned(),
///         "mgba_libretro".to_owned(),
///         CoreBuildId::from_str("mgba-0.11-212-7a12d6d").unwrap(),
///         CorePlatform::MacOsArm64,
///     ),
///     artifact: ArtifactReference::with_file_name(
///         ComponentArtifactSource::official(
///             ArtifactSource::new(
///                 "https://buildbot.libretro.com/nightly/apple/osx/arm64/latest/\
///                  mgba_libretro.dylib.zip",
///             )
///             .unwrap(),
///         ),
///         Sha256Digest::from_str(
///             "1aa000e5a88c2ea2afb788cdee89853f86d1c8639fc0df5d2ed6f261c2d81462",
///         )
///         .unwrap(),
///         "mgba_libretro.dylib.zip",
///     )
///     .unwrap(),
///     layout: (
///         CoreArtifactKind::LibretroCoreArchive {
///             member: String::from("mgba_libretro.dylib"),
///         },
///         RelativePath::from_str("mgba_libretro.dylib").unwrap(),
///     ),
///     provenance: CoreProvenance {
///         version: String::from("0.11-212-7a12d6d"),
///         revision: String::from("7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6"),
///         revision_short: String::from("7a12d6d"),
///         channel_date: String::from("2026-09-17"),
///         channel_crc32: String::from("69c0d054"),
///     },
///     attribution: ComponentAttribution {
///         component: String::from("mGBA"),
///         upstream_project: String::from("mgba-emu/mgba"),
///         upstream_url: String::from("https://github.com/mgba-emu/mgba"),
///         license: LicenseIdentifier::from_str("MPL-2.0").unwrap(),
///     },
/// });
///
/// assert_eq!(definition.build_id().as_str(), "mgba-0.11-212-7a12d6d");
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CoreParts {
    /// The component identity, the display name, the libretro core name, the
    /// reviewed build identity, and the platform.
    pub identity: (CoreComponentId, String, String, CoreBuildId, CorePlatform),
    /// The pinned URL, digest, and published file name of the artifact.
    pub artifact: ArtifactReference,
    /// How the artifact is packaged and where the library sits below the
    /// installation directory.
    pub layout: (CoreArtifactKind, RelativePath),
    /// How the pinned binary was identified upstream.
    pub provenance: CoreProvenance,
    /// The upstream and license metadata of the component.
    pub attribution: ComponentAttribution,
}

/// One pinned, curated core component.
///
/// Every value in a definition is reviewable in the source that produces it, and
/// none of them is discovered at runtime. In particular the expected
/// [`Sha256Digest`] is decided here and never taken from the response that
/// delivered the artifact.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CoreDefinition {
    component_id: CoreComponentId,
    display_name: String,
    core_name: String,
    build_id: CoreBuildId,
    platform: CorePlatform,
    artifact: ArtifactReference,
    kind: CoreArtifactKind,
    library: RelativePath,
    provenance: CoreProvenance,
    attribution: ComponentAttribution,
}

impl CoreDefinition {
    /// Creates a pinned definition from already validated parts.
    #[must_use]
    pub fn new(parts: CoreParts) -> Self {
        let CoreParts {
            identity: (component_id, display_name, core_name, build_id, platform),
            artifact,
            layout: (kind, library),
            provenance,
            attribution,
        } = parts;

        Self {
            component_id,
            display_name,
            core_name,
            build_id,
            platform,
            artifact,
            kind,
            library,
            provenance,
            attribution,
        }
    }

    /// Returns the distribution identity of the component.
    #[must_use]
    pub const fn component_id(&self) -> &CoreComponentId {
        &self.component_id
    }

    /// Returns the component name as the upstream project spells it.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Returns the name RetroArch addresses this core by.
    ///
    /// This is the value a later launch step turns into a `-L` argument; it is
    /// metadata here and never a path.
    #[must_use]
    pub fn core_name(&self) -> &str {
        &self.core_name
    }

    /// Returns the reviewed identity of the built core binary.
    #[must_use]
    pub const fn build_id(&self) -> &CoreBuildId {
        &self.build_id
    }

    /// Returns the platform the artifact and the installation are built for.
    #[must_use]
    pub const fn platform(&self) -> CorePlatform {
        self.platform
    }

    /// Returns the pinned artifact: URL, digest, and published file name.
    #[must_use]
    pub const fn artifact(&self) -> &ArtifactReference {
        &self.artifact
    }

    /// Returns the URL the artifact is downloaded from.
    ///
    /// For a production definition this is always
    /// [`Official`](ComponentArtifactSource::Official); a
    /// [`Loopback`](ComponentArtifactSource::Loopback) can only be constructed for
    /// a test. The URL is a rolling transport path for the official build host and
    /// is **not** part of the component's identity — see the module documentation.
    #[must_use]
    pub const fn source(&self) -> &ComponentArtifactSource {
        &self.artifact.source
    }

    /// Returns the SHA-256 digest the downloaded artifact must have.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.artifact.digest
    }

    /// Returns how the artifact is packaged.
    #[must_use]
    pub const fn kind(&self) -> &CoreArtifactKind {
        &self.kind
    }

    /// Returns the archive member the artifact must contain.
    #[must_use]
    pub fn expected_member(&self) -> &str {
        self.kind.member()
    }

    /// Returns where the library sits below the installation directory.
    #[must_use]
    pub const fn library(&self) -> &RelativePath {
        &self.library
    }

    /// Returns how the pinned binary was identified upstream.
    #[must_use]
    pub const fn provenance(&self) -> &CoreProvenance {
        &self.provenance
    }

    /// Returns the upstream and license metadata of the component.
    #[must_use]
    pub const fn attribution(&self) -> &ComponentAttribution {
        &self.attribution
    }

    /// Returns the same definition with a different artifact URL and digest.
    ///
    /// This exists for the automated tests, which download from a loopback server
    /// instead of the public network, and it keeps the rest of the pin — identity,
    /// layout, membership, attribution — exactly as reviewed. It is the only
    /// mutation a definition has.
    #[must_use]
    pub fn with_artifact(mut self, artifact: ArtifactReference) -> Self {
        self.artifact = artifact;
        self
    }
}

/// Returns the curated bootstrap core BitArchive can install: mGBA.
///
/// # Why mGBA
///
/// - it is an established, actively developed libretro core with a single clear
///   upstream project;
/// - upstream mGBA is licensed `MPL-2.0`, file-level copyleft that is
///   redistribution-friendly and easy to record honestly;
/// - it covers Game Boy Advance as well as Game Boy and Game Boy Color, so one
///   core serves three systems;
/// - for Game Boy Advance the external BIOS is optional, so a first real launch
///   slice does not have to solve firmware before it can start content.
///
/// # What is pinned
///
/// ```text
/// core:      mgba
/// name:      mgba_libretro
/// version:   0.11-212-7a12d6d          (upstream version string, from the binary)
/// revision:  7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6
/// build:     mgba-0.11-212-7a12d6d
/// platform:  macos-arm64 | macos-x86_64
/// member:    mgba_libretro.dylib
/// library:   mgba_libretro.dylib
/// license:   MPL-2.0
/// ```
///
/// The artifact URL is the official build host's rolling `latest` path for the
/// platform, and the digest is the SHA-256 of that exact artifact as reviewed. See
/// the module documentation and `docs/decisions/0002-managed-core-acquisition.md`
/// for what the rolling path does and does not mean.
#[must_use]
pub fn mgba_bootstrap(platform: CorePlatform) -> CoreDefinition {
    CoreDefinition::new(CoreParts {
        identity: (
            CoreComponentId::from_str(CoreComponentId::MGBA)
                .expect("the curated component identity is valid"),
            String::from(CoreComponentId::MGBA_DISPLAY_NAME),
            String::from(CoreComponentId::MGBA_CORE_NAME),
            CoreBuildId::from_str(CoreComponentId::MGBA_BUILD_ID)
                .expect("the curated build identity is valid"),
            platform,
        ),
        artifact: ArtifactReference::with_file_name(
            ComponentArtifactSource::official(
                ArtifactSource::new(&format!(
                    "{}{}/latest/{}",
                    CoreComponentId::MGBA_ARTIFACT_DIRECTORY,
                    platform.build_host_architecture(),
                    CoreComponentId::MGBA_ARTIFACT_FILE_NAME
                ))
                .expect("the curated artifact URL is an official https URL"),
            ),
            Sha256Digest::from_str(match platform {
                CorePlatform::MacOsArm64 => CoreComponentId::MGBA_ARTIFACT_SHA256_ARM64,
                CorePlatform::MacOsX86_64 => CoreComponentId::MGBA_ARTIFACT_SHA256_X86_64,
            })
            .expect("the curated digest is a SHA-256 digest"),
            CoreComponentId::MGBA_ARTIFACT_FILE_NAME,
        )
        .expect("the curated published file name is a plain file name"),
        layout: (
            CoreArtifactKind::LibretroCoreArchive {
                member: String::from(CoreComponentId::MGBA_LIBRARY),
            },
            RelativePath::from_str(CoreComponentId::MGBA_LIBRARY)
                .expect("the curated library name is relative and canonical"),
        ),
        provenance: CoreProvenance {
            version: String::from(CoreComponentId::MGBA_VERSION),
            revision: String::from(CoreComponentId::MGBA_REVISION),
            revision_short: String::from(CoreComponentId::MGBA_REVISION_SHORT),
            channel_date: String::from(match platform {
                CorePlatform::MacOsArm64 => CoreComponentId::MGBA_CHANNEL_DATE_ARM64,
                CorePlatform::MacOsX86_64 => CoreComponentId::MGBA_CHANNEL_DATE_X86_64,
            }),
            channel_crc32: String::from(match platform {
                CorePlatform::MacOsArm64 => CoreComponentId::MGBA_CHANNEL_CRC32_ARM64,
                CorePlatform::MacOsX86_64 => CoreComponentId::MGBA_CHANNEL_CRC32_X86_64,
            }),
        },
        attribution: ComponentAttribution {
            component: String::from(CoreComponentId::MGBA_DISPLAY_NAME),
            upstream_project: String::from(CoreComponentId::MGBA_UPSTREAM_PROJECT),
            upstream_url: String::from(CoreComponentId::MGBA_UPSTREAM_URL),
            license: LicenseIdentifier::from_str(LicenseIdentifier::MPL_2_0)
                .expect("the curated license identifier is valid"),
        },
    })
}

/// The complete curated allowlist of the cores B6 can install.
///
/// Exactly one core, in one build, for both macOS architectures:
///
/// ```text
/// curated_core(mgba, macos-arm64)  → Some(definition)
/// curated_core(mgba, macos-x86_64) → Some(definition)
/// curated_core(anything else, …)   → None
/// ```
///
/// This is what enforces "BitArchive installs only cores with a reviewed
/// definition": a component identity that is not in this list has no definition,
/// and a definition cannot be constructed from a user-supplied URL, core name, or
/// library path anywhere in the workspace.
///
/// Adding a core means adding a reviewed function like [`mgba_bootstrap`] and
/// listing it here — not extending a general downloader.
#[must_use]
pub fn curated_cores() -> [CoreDefinition; 2] {
    [
        mgba_bootstrap(CorePlatform::MacOsArm64),
        mgba_bootstrap(CorePlatform::MacOsX86_64),
    ]
}

/// Returns the curated definition of `component_id` for `platform`, if the
/// allowlist has one.
///
/// # Errors
///
/// Returns [`CuratedCoreError`] when the allowlist has no reviewed definition for
/// that component or that platform. A core BitArchive does not curate is *not*
/// installed from somewhere else, and an unsupported platform is not served by
/// another architecture's artifact.
pub fn curated_core(
    component_id: &CoreComponentId,
    platform: CorePlatform,
) -> Result<CoreDefinition, CuratedCoreError> {
    curated_cores()
        .into_iter()
        .find(|definition| {
            definition.component_id() == component_id && definition.platform() == platform
        })
        .ok_or_else(|| CuratedCoreError {
            component_id: component_id.clone(),
            platform,
        })
}

/// Why the allowlist has no definition for a requested core component.
///
/// The variant carries what was asked for, so the caller learns *which* component
/// or platform BitArchive does not curate instead of a generic failure.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CuratedCoreError {
    /// The component identity that was requested.
    pub component_id: CoreComponentId,
    /// The platform that was requested.
    pub platform: CorePlatform,
}

impl fmt::Display for CuratedCoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BitArchive does not curate a core component {:?} for {}; only reviewed definitions \
             can be installed, and there is no generic download for arbitrary cores",
            self.component_id.as_str(),
            self.platform
        )
    }
}

impl std::error::Error for CuratedCoreError {}

/// The constants and identity of the one curated bootstrap core.
///
/// The values live in an inherent impl block rather than as free constants so that
/// the identity, the names, the artifact, and the provenance of the curated core
/// are readable in one place — which is what a review of a pin needs.
impl CoreComponentId {
    /// The curated component identity of mGBA.
    pub const MGBA: &'static str = "mgba";

    /// The component name as the mGBA project spells it.
    pub const MGBA_DISPLAY_NAME: &'static str = "mGBA";

    /// The name RetroArch addresses the mGBA core by.
    ///
    /// This is the libretro core name, not a file name and not a component
    /// identity. It is what a later launch step turns into a `-L` argument.
    pub const MGBA_CORE_NAME: &'static str = "mgba_libretro";

    /// The reviewed build identity of the pinned mGBA binary.
    ///
    /// Derived from [`MGBA_VERSION`](Self::MGBA_VERSION) and prefixed with the
    /// component identity: `mgba-0.11-212-7a12d6d`.
    pub const MGBA_BUILD_ID: &'static str = "mgba-0.11-212-7a12d6d";

    /// The upstream version string the pinned binaries report.
    ///
    /// mGBA builds this from `version.cmake` as
    /// `<lib-version>-<commit-count>-<short-sha>`, so the commit count is an
    /// artifact of the build environment and only the version and the revision are
    /// meaningful identity. The full revision is
    /// [`MGBA_REVISION`](Self::MGBA_REVISION).
    pub const MGBA_VERSION: &'static str = "0.11-212-7a12d6d";

    /// The full upstream revision both pinned binaries report.
    ///
    /// This is the authoritative part of the build identity. Both macOS
    /// architectures were built from this commit.
    pub const MGBA_REVISION: &'static str = "7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6";

    /// The abbreviated upstream revision both pinned binaries report.
    pub const MGBA_REVISION_SHORT: &'static str = "7a12d6d";

    /// The build date the official build host publishes for the arm64 artifact.
    pub const MGBA_CHANNEL_DATE_ARM64: &'static str = "2026-09-17";

    /// The build date the official build host publishes for the x86_64 artifact.
    pub const MGBA_CHANNEL_DATE_X86_64: &'static str = "2026-09-17";

    /// The CRC-32 the official build host publishes for the arm64 library.
    ///
    /// A change hint from the build host's own listing, never an integrity check:
    /// the build host publishes no cryptographic digest for core binaries, which
    /// is why BitArchive pins its own SHA-256 of the archive.
    pub const MGBA_CHANNEL_CRC32_ARM64: &'static str = "69c0d054";

    /// The CRC-32 the official build host publishes for the x86_64 library.
    pub const MGBA_CHANNEL_CRC32_X86_64: &'static str = "c543643a";

    /// The prefix of the official build host path the core artifact is fetched
    /// from. The platform's architecture directory and `/latest/<file>` follow.
    pub const MGBA_ARTIFACT_DIRECTORY: &'static str =
        "https://buildbot.libretro.com/nightly/apple/osx/";

    /// The file name the official build host publishes the core artifact under.
    pub const MGBA_ARTIFACT_FILE_NAME: &'static str = "mgba_libretro.dylib.zip";

    /// The library the archive contains and the installation installs.
    pub const MGBA_LIBRARY: &'static str = "mgba_libretro.dylib";

    /// The SHA-256 of the pinned `macos-arm64` artifact.
    pub const MGBA_ARTIFACT_SHA256_ARM64: &'static str =
        "1aa000e5a88c2ea2afb788cdee89853f86d1c8639fc0df5d2ed6f261c2d81462";

    /// The SHA-256 of the pinned `macos-x86_64` artifact.
    pub const MGBA_ARTIFACT_SHA256_X86_64: &'static str =
        "1530880845ec16538187c9c1299073bc010064979df8087cc100041edb396d22";

    /// The upstream project mGBA belongs to.
    pub const MGBA_UPSTREAM_PROJECT: &'static str = "mgba-emu/mgba";

    /// The canonical URL of the mGBA project.
    pub const MGBA_UPSTREAM_URL: &'static str = "https://github.com/mgba-emu/mgba";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The curated core is one consistent pin: every constant describes the same
    /// component, so a partial edit cannot produce a definition that mixes two
    /// builds or two architectures.
    #[test]
    fn the_curated_pin_describes_one_consistent_component() {
        for platform in CorePlatform::ALL {
            let definition = mgba_bootstrap(platform);

            assert_eq!(definition.component_id().as_str(), CoreComponentId::MGBA);
            assert_eq!(definition.display_name(), "mGBA");
            assert_eq!(definition.core_name(), "mgba_libretro");
            assert_eq!(definition.build_id().as_str(), "mgba-0.11-212-7a12d6d");
            assert_eq!(definition.platform(), platform);

            assert_eq!(definition.expected_member(), CoreComponentId::MGBA_LIBRARY);
            assert_eq!(
                definition.library().to_string(),
                CoreComponentId::MGBA_LIBRARY
            );
            assert_eq!(
                definition.library().file_name(),
                definition.expected_member(),
                "the installed library is the archive member"
            );
        }
    }

    /// The pin records how the binary was identified upstream, and the build
    /// identity is derived from that evidence rather than invented.
    #[test]
    fn the_curated_pin_records_reviewable_provenance_behind_its_build_identity() {
        for platform in CorePlatform::ALL {
            let definition = mgba_bootstrap(platform);
            let provenance = definition.provenance();

            assert_eq!(provenance.version, CoreComponentId::MGBA_VERSION);
            assert_eq!(provenance.revision, CoreComponentId::MGBA_REVISION);
            assert_eq!(
                provenance.revision_short,
                CoreComponentId::MGBA_REVISION_SHORT
            );
            assert_eq!(provenance.channel_date, "2026-09-17");

            // The build identity is derivable from the recorded evidence, so the
            // two cannot drift apart silently.
            assert_eq!(
                definition.build_id().as_str(),
                format!(
                    "{}-{}",
                    CoreComponentId::MGBA,
                    CoreComponentId::MGBA_VERSION
                )
            );
            assert!(
                provenance.version.contains(&provenance.revision_short),
                "the version string carries the abbreviated revision"
            );
            assert_eq!(provenance.revision.len(), 40);
            assert!(provenance.revision.starts_with(&provenance.revision_short));
        }

        assert_eq!(
            mgba_bootstrap(CorePlatform::MacOsArm64)
                .provenance()
                .channel_crc32,
            CoreComponentId::MGBA_CHANNEL_CRC32_ARM64
        );
        assert_eq!(
            mgba_bootstrap(CorePlatform::MacOsX86_64)
                .provenance()
                .channel_crc32,
            CoreComponentId::MGBA_CHANNEL_CRC32_X86_64
        );
    }

    /// The build host's published CRC-32 is recorded as a change hint and is never
    /// the trust anchor, because the build host publishes no cryptographic digest
    /// for core binaries.
    #[test]
    fn the_channel_crc32_is_not_the_trust_anchor() {
        let definition = mgba_bootstrap(CorePlatform::MacOsArm64);
        let provenance = definition.provenance();

        assert_eq!(provenance.channel_crc32.len(), 8, "CRC-32 is 32 bits");
        assert_eq!(
            definition.digest().as_str().len(),
            64,
            "SHA-256 is 256 bits"
        );
        assert_ne!(
            definition.digest().as_str(),
            provenance.channel_crc32,
            "the pinned digest is BitArchive's own SHA-256, not the build host's CRC-32"
        );
    }

    /// The curated core carries its upstream and license metadata, so a
    /// redistribution can see which obligations apply.
    #[test]
    fn the_curated_pin_records_mpl_2_0_and_the_upstream_project() {
        let definition = mgba_bootstrap(CorePlatform::MacOsArm64);
        let attribution = definition.attribution();

        assert_eq!(attribution.component, "mGBA");
        assert_eq!(attribution.upstream_project, "mgba-emu/mgba");
        assert_eq!(attribution.upstream_url, "https://github.com/mgba-emu/mgba");
        assert_eq!(attribution.license.as_str(), "MPL-2.0");
        assert_eq!(attribution.license.as_str(), LicenseIdentifier::MPL_2_0);
    }

    /// The two macOS architectures are separate definitions with separate URLs and
    /// separate digests, and neither claims to be universal.
    #[test]
    fn the_two_macos_architectures_are_separate_artifacts() {
        let arm64 = mgba_bootstrap(CorePlatform::MacOsArm64);
        let intel = mgba_bootstrap(CorePlatform::MacOsX86_64);

        assert_ne!(arm64.source().as_str(), intel.source().as_str());
        assert_ne!(arm64.digest(), intel.digest());
        assert_ne!(arm64.platform(), intel.platform());

        assert!(arm64.source().as_str().contains("/arm64/latest/"));
        assert!(intel.source().as_str().contains("/x86_64/latest/"));

        for definition in [&arm64, &intel] {
            assert!(
                !definition.source().as_str().contains("universal"),
                "upstream publishes no universal core artifact: {}",
                definition.source()
            );
            assert!(definition.platform().as_str().starts_with("macos-"));
        }

        assert_eq!(arm64.platform().as_str(), "macos-arm64");
        assert_eq!(intel.platform().as_str(), "macos-x86_64");
        assert_eq!(arm64.platform().build_host_architecture(), "arm64");
        assert_eq!(intel.platform().build_host_architecture(), "x86_64");
    }

    /// The source is the official host over https, so a mirror cannot be pinned by
    /// accident and the download cannot be plaintext. The rolling `latest` segment
    /// is expected here and is *not* used as a version.
    #[test]
    fn the_curated_pin_uses_the_official_https_source_only_and_never_latest_as_a_version() {
        for platform in CorePlatform::ALL {
            let definition = mgba_bootstrap(platform);

            assert!(definition.source().is_official());
            assert_eq!(
                definition.source().host(),
                crate::component::OFFICIAL_COMPONENT_HOST
            );
            assert!(definition.source().as_str().starts_with("https://"));

            // `latest` is the transport path, and it must never be the identity.
            assert!(definition.source().as_str().contains("/latest/"));
            assert!(!definition.build_id().as_str().contains("latest"));
            assert_ne!(definition.build_id().as_str(), "latest");
        }
    }

    /// The digest is a real SHA-256, is well formed, and is not derived from
    /// anything at run time.
    #[test]
    fn the_curated_pin_carries_well_formed_digests() {
        for (platform, expected) in [
            (
                CorePlatform::MacOsArm64,
                CoreComponentId::MGBA_ARTIFACT_SHA256_ARM64,
            ),
            (
                CorePlatform::MacOsX86_64,
                CoreComponentId::MGBA_ARTIFACT_SHA256_X86_64,
            ),
        ] {
            let definition = mgba_bootstrap(platform);

            assert_eq!(definition.digest().as_str(), expected);
            assert_eq!(definition.digest().as_str().len(), 64);
            assert_eq!(
                definition.digest(),
                Sha256Digest::from_str(expected).expect("the pinned digest parses")
            );
        }
    }

    /// The definition is deterministic and purely a value: reading it twice yields
    /// the same pin.
    #[test]
    fn the_curated_pin_is_deterministic() {
        assert_eq!(
            mgba_bootstrap(CorePlatform::MacOsArm64),
            mgba_bootstrap(CorePlatform::MacOsArm64)
        );
    }

    /// The allowlist has exactly the curated core for both architectures and
    /// nothing else — there is no entry for an arbitrary core name.
    #[test]
    fn the_allowlist_contains_exactly_the_curated_core() {
        let allowlist = curated_cores();

        assert_eq!(allowlist.len(), 2);

        let mgba = CoreComponentId::from_str(CoreComponentId::MGBA).expect("a valid identity");

        for platform in CorePlatform::ALL {
            assert_eq!(
                curated_core(&mgba, platform).expect("the curated core is allowlisted"),
                mgba_bootstrap(platform)
            );
        }

        for unknown in ["snes9x", "genesis-plus-gx", "retroarch", "mesen"] {
            let unknown =
                CoreComponentId::from_str(unknown).expect("a syntactically valid identity");

            let error = curated_core(&unknown, CorePlatform::MacOsArm64)
                .expect_err("an uncurated core must not resolve to a definition");

            assert_eq!(error.component_id, unknown);
            assert_eq!(error.platform, CorePlatform::MacOsArm64);
            assert!(error.to_string().contains("does not curate"));
        }
    }

    /// A core identity is not the retroArch core name and not a fachliche UUID.
    #[test]
    fn the_component_identity_is_a_distribution_slug_and_not_a_core_id() {
        let definition = mgba_bootstrap(CorePlatform::MacOsArm64);

        assert_eq!(definition.component_id().as_str(), "mgba");
        assert_eq!(definition.core_name(), "mgba_libretro");
        assert_ne!(
            definition.component_id().as_str(),
            definition.core_name(),
            "the distribution identity and the RetroArch core name are different things"
        );

        // A `CoreId` is a fachliche UUID identity and cannot be spelled like a
        // slug; the two live in different namespaces on purpose.
        assert_ne!(
            crate::CoreId::new().to_string(),
            definition.component_id().as_str()
        );
    }

    /// An unsupported host is reported as a structured error naming the host, and
    /// no other architecture is substituted.
    #[test]
    fn an_unsupported_host_is_reported_and_never_substituted() {
        let host = UnsupportedCoreHost::current();

        assert!(!host.architecture.is_empty());
        assert!(!host.operating_system.is_empty());
        assert!(host.to_string().contains("never substituted"));

        match UnsupportedCoreHost::resolve() {
            Ok(platform) => {
                assert!(CorePlatform::ALL.contains(&platform));
                assert_eq!(
                    platform,
                    if cfg!(target_arch = "aarch64") {
                        CorePlatform::MacOsArm64
                    } else {
                        CorePlatform::MacOsX86_64
                    }
                );
            }
            Err(error) => {
                assert_eq!(error, host);
                assert_eq!(
                    host_core_platform(),
                    None,
                    "a host without a managed-core platform is exactly a host the resolver refuses"
                );
            }
        }
    }

    /// A build identity is one safe path component, so it can name a store
    /// directory without a second validation.
    #[test]
    fn a_build_identity_is_a_single_safe_path_component() {
        for accepted in ["mgba-0.11-212-7a12d6d", "0.10.5", "2026-09-17", "build+1"] {
            assert!(
                CoreBuildId::from_str(accepted).is_ok(),
                "{accepted} must be accepted"
            );
        }

        for (rejected, expected) in [
            ("", CoreBuildIdError::Empty),
            ("..", CoreBuildIdError::InvalidCharacter('.')),
            ("-1", CoreBuildIdError::InvalidCharacter('-')),
            ("/absolute", CoreBuildIdError::InvalidCharacter('/')),
            ("a/b", CoreBuildIdError::InvalidCharacter('/')),
            ("a b", CoreBuildIdError::InvalidCharacter(' ')),
            ("2026-09-17\n", CoreBuildIdError::InvalidCharacter('\n')),
        ] {
            assert_eq!(
                CoreBuildId::from_str(rejected),
                Err(expected),
                "{rejected:?} must be rejected"
            );
        }

        let too_long = "a".repeat(MAX_CORE_BUILD_ID_LENGTH + 1);

        assert_eq!(
            CoreBuildId::from_str(&too_long),
            Err(CoreBuildIdError::TooLong)
        );
    }

    /// The artifact kind names the exact member and the packaging, so the
    /// extractor and a reviewer read the same expectation.
    #[test]
    fn the_artifact_kind_names_the_expected_member() {
        let definition = mgba_bootstrap(CorePlatform::MacOsArm64);

        assert_eq!(definition.kind().as_str(), "libretro-core-archive");
        assert_eq!(definition.kind().member(), "mgba_libretro.dylib");
        assert_eq!(definition.expected_member(), "mgba_libretro.dylib");
    }

    /// The published artifact file name is recorded, and the store does not need
    /// it to derive a local path.
    #[test]
    fn the_published_artifact_name_is_recorded() {
        let definition = mgba_bootstrap(CorePlatform::MacOsX86_64);

        assert_eq!(
            definition.artifact().file_name.as_deref(),
            Some("mgba_libretro.dylib.zip")
        );
        assert_eq!(
            definition.source().file_name(),
            Some("mgba_libretro.dylib.zip")
        );
    }
}
