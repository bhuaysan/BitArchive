//! Managed runtime components and their pinned definitions.
//!
//! A runtime is a versioned, immutable component that BitArchive manages for the
//! user — in the MVP that is RetroArch (ARCHITECTURE.md §23). This module owns
//! the *description* of such a component: which runtime it is, which version is
//! pinned, where the artifact comes from, which bytes are expected, how the
//! artifact is packaged, and where the executable ends up after installation.
//!
//! It deliberately owns nothing else. There is no HTTP client, no filesystem
//! access, no staging, no installation, and no activation here; those are
//! infrastructure concerns that implement the ports defined by the application
//! layer. What this module provides is the value that everything else is
//! verified against.
//!
//! # Pinned, not "latest"
//!
//! A [`RuntimeDefinition`] names one exact version and one exact artifact. There
//! is no resolution against a moving "latest" pointer, no version discovery, and
//! no way to obtain a definition whose digest was not decided by BitArchive
//! before the download started. The expected [`Sha256Digest`] is part of the
//! definition and never comes from the response that delivered the artifact.
//!
//! ```text
//! Pinned Runtime Definition      ← this module
//!         ↓
//! Download Artifact              ← infrastructure, against `ArtifactSource`
//!         ↓
//! Verify SHA-256                 ← against `Sha256Digest`
//!         ↓
//! Stage / Install / Activate     ← infrastructure, into
//!                                   `components/runtime/<platform>/<version>/`
//!         ↓
//! Executable Resolution          ← definition + installation record
//! ```
//!
//! # Runtime and cores stay separate
//!
//! Only runtimes are described here. A libretro core is a different component
//! class with its own identity, its own versioning, and its own licensing, and it
//! is managed by a later Issue (ARCHITECTURE.md §23.4, AGENTS.md §11). Nothing in
//! this module assumes that installing a runtime provides any core.
//!
//! # Signatures
//!
//! ARCHITECTURE.md §24 describes signed distribution manifests, and that step is
//! not implemented yet. Until it is, the pinned [`Sha256Digest`] in a definition
//! is the trust anchor: verification happens against a value BitArchive reviewed,
//! not against the artifact's own claims. See `docs/decisions/0001-*` for the
//! recorded decision and its follow-up.

use std::fmt;
use std::str::FromStr;

/// The shared artifact and provenance values a runtime definition is built from.
///
/// Re-exported here as well as from [`crate::component`], because the runtime API
/// named them before cores were managed and a caller of the runtime path should
/// not have to import from two modules to describe a runtime.
pub use crate::component::{
    ArtifactSource, ComponentArtifactSource, ComponentAttribution, LicenseIdentifier, RelativePath,
    Sha256Digest,
};

/// The host BitArchive downloads a managed runtime from.
///
/// Kept as a runtime-facing name because it is part of the runtime API that
/// existed before cores were managed. It is the same host, and therefore the same
/// value, as [`OFFICIAL_COMPONENT_HOST`](crate::component::OFFICIAL_COMPONENT_HOST).
pub const OFFICIAL_RUNTIME_HOST: &str = crate::component::OFFICIAL_COMPONENT_HOST;

/// The pinned URL of a managed runtime artifact.
///
/// The runtime-facing name of the shared
/// [`ComponentArtifactSource`], kept so
/// that the runtime path reads as the runtime path.
pub type RuntimeSource = ComponentArtifactSource;

/// The upstream and license metadata of a managed runtime.
///
/// The runtime-facing name of the shared
/// [`ComponentAttribution`]. A runtime and
/// a core record exactly the same provenance fields, so the type is shared rather
/// than declared twice.
pub type RuntimeAttribution = ComponentAttribution;

/// Upper bound for an accepted runtime version string.
///
/// The bound exists so that a definition cannot smuggle an unbounded value into
/// a store path component. It is far above any real version.
const MAX_VERSION_LENGTH: usize = 64;

/// The reason a [`RuntimeVersion`] was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VersionError {
    /// The version string was empty.
    Empty,
    /// The version string exceeded the maximum accepted length.
    TooLong,
    /// The version contained a character outside the accepted set.
    InvalidCharacter(char),
    /// The version did not start with an ASCII digit.
    MissingLeadingDigit,
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a runtime version must not be empty"),
            Self::TooLong => write!(
                f,
                "a runtime version must not exceed {MAX_VERSION_LENGTH} characters"
            ),
            Self::InvalidCharacter(character) => write!(
                f,
                "a runtime version must consist of ASCII letters, digits, '.', '-', '+' or '_', \
                 but contains {character:?}"
            ),
            Self::MissingLeadingDigit => {
                f.write_str("a runtime version must start with an ASCII digit")
            }
        }
    }
}

impl std::error::Error for VersionError {}

/// The exact version of a managed runtime component.
///
/// A version is an opaque identifier, not a parsed semantic version. BitArchive
/// compares versions for equality and orders them lexicographically; it does not
/// yet decide which of two versions is *newer*, because that decision needs the
/// resolution rules of a later update Issue and guessing here would be worse than
/// not answering.
///
/// The accepted alphabet is restricted to characters that are safe as a single
/// path component on every supported platform, and a version must start with a
/// digit. That keeps `..`, an empty name, a nested path, and a leading `-` out of
/// the component store layout by construction.
///
/// ```
/// use std::str::FromStr;
///
/// use bitarchive_domain::runtime::RuntimeVersion;
///
/// let version = RuntimeVersion::from_str("1.22.2").unwrap();
///
/// assert_eq!(version.as_str(), "1.22.2");
/// assert!(RuntimeVersion::from_str("latest").is_err());
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct RuntimeVersion(String);

impl RuntimeVersion {
    /// Returns the version as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RuntimeVersion {
    type Err = VersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(VersionError::Empty);
        }

        if value.chars().count() > MAX_VERSION_LENGTH {
            return Err(VersionError::TooLong);
        }

        if !value.starts_with(|character: char| character.is_ascii_digit()) {
            return Err(VersionError::MissingLeadingDigit);
        }

        if let Some(invalid) = value.chars().find(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+' | '_'))
        }) {
            return Err(VersionError::InvalidCharacter(invalid));
        }

        Ok(Self(value.to_owned()))
    }
}

impl fmt::Display for RuntimeVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identifies a managed runtime component.
///
/// A runtime identity is a stable, human-readable slug rather than a generated
/// UUID. There is a small, closed set of emulation runtimes a BitArchive
/// installation can manage, and the identity also names the component on disk, so
/// a readable identifier is the more useful representation here. It is still an
/// identity and not a path: the store decides where a runtime lives.
///
/// ```
/// use bitarchive_domain::runtime::RuntimeId;
///
/// assert_eq!(RuntimeId::RETROARCH, "retroarch");
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct RuntimeId(String);

impl RuntimeId {
    /// The RetroArch runtime, the only managed runtime in the MVP.
    pub const RETROARCH: &'static str = "retroarch";

    /// Returns the identity as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RuntimeId {
    type Err = RuntimeIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(RuntimeIdError::Empty);
        }

        if !value.starts_with(|character: char| character.is_ascii_lowercase()) {
            return Err(RuntimeIdError::MissingLeadingLetter);
        }

        if let Some(invalid) = value.chars().find(|character| {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || *character == '-')
        }) {
            return Err(RuntimeIdError::InvalidCharacter(invalid));
        }

        Ok(Self(value.to_owned()))
    }
}

/// The reason a [`RuntimeId`] was rejected.
///
/// The accepted alphabet is lower-case ASCII letters, digits, and `-`, and an
/// identity must start with a letter. That keeps `..`, an empty name, and an
/// upper-case spelling out of the identity space, so an identity is always usable
/// as a single path component.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RuntimeIdError {
    /// The identity was empty.
    Empty,
    /// The identity did not start with a lower-case ASCII letter.
    MissingLeadingLetter,
    /// The identity contained a character outside the accepted set.
    InvalidCharacter(char),
}

impl fmt::Display for RuntimeIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a runtime identity must not be empty"),
            Self::MissingLeadingLetter => {
                f.write_str("a runtime identity must start with a lower-case ASCII letter")
            }
            Self::InvalidCharacter(character) => write!(
                f,
                "a runtime identity must consist of lower-case ASCII letters, digits or '-', \
                 but contains {character:?}"
            ),
        }
    }
}

impl std::error::Error for RuntimeIdError {}

impl fmt::Display for RuntimeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The platform an artifact and its installation are built for.
///
/// The MVP supports macOS only (ARCHITECTURE.md §3.3). The single universal
/// variant is not a placeholder for a future matrix: a modern macOS RetroArch
/// build ships one artifact containing both the Apple Silicon and the Intel
/// slice, so "macOS, universal" is the honest description of what is installed.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RuntimePlatform {
    /// macOS, one artifact containing the `arm64` and `x86_64` slices.
    MacOsUniversal,
}

impl RuntimePlatform {
    /// Returns the stable slug used in the component store layout.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MacOsUniversal => "macos-universal",
        }
    }
}

impl fmt::Display for RuntimePlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a downloaded artifact is packaged.
///
/// The kind is metadata, not an instruction: it tells the installer and, more
/// importantly, a reviewer what the downloaded bytes actually are, and it makes
/// an unexpected artifact detectable instead of silently mis-unpacked.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ArtifactKind {
    /// An Apple Universal Disk Image (`.dmg`) containing an application bundle.
    AppleDiskImage {
        /// The bundle name as it appears at the root of the mounted image.
        bundle: String,
    },
}

impl ArtifactKind {
    /// Returns a short label for diagnostics and metadata.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AppleDiskImage { .. } => "apple-disk-image",
        }
    }
}

/// Which component, in which exact version, for which platform.
///
/// A definition is built from four groups rather than from eight loose values, so
/// that each part of the pin is named where it is written and a review can read
/// the identity, the artifact, the layout, and the provenance separately.
///
/// ```
/// use std::str::FromStr;
///
/// use bitarchive_domain::runtime::{
///     ArtifactKind, ArtifactSource, LicenseIdentifier, RelativePath, RuntimeAttribution,
///     RuntimeDefinition, RuntimeId, RuntimeParts, RuntimePlatform, RuntimeSource,
///     RuntimeVersion, Sha256Digest,
/// };
///
/// let definition = RuntimeDefinition::new(RuntimeParts {
///     identity: (
///         RuntimeId::from_str(RuntimeId::RETROARCH).unwrap(),
///         RuntimeVersion::from_str("1.22.2").unwrap(),
///         RuntimePlatform::MacOsUniversal,
///     ),
///     artifact: (
///         RuntimeSource::official(
///             ArtifactSource::new("https://buildbot.libretro.com/stable/1.22.2/a.dmg").unwrap(),
///         ),
///         Sha256Digest::from_str("81b79121ba26d539064ae13b4d0419a120c3d165afbe656cf5f5412b15fdb434")
///             .unwrap(),
///     ),
///     layout: (
///         ArtifactKind::AppleDiskImage { bundle: String::from("RetroArch.app") },
///         RelativePath::from_str("RetroArch.app/Contents/MacOS/RetroArch").unwrap(),
///     ),
///     attribution: RuntimeAttribution {
///         component: String::from("RetroArch"),
///         upstream_project: String::from("libretro/RetroArch"),
///         upstream_url: String::from("https://github.com/libretro/RetroArch"),
///         license: LicenseIdentifier::from_str(LicenseIdentifier::GPL_3_0_OR_LATER).unwrap(),
///     },
/// });
///
/// assert_eq!(definition.version().as_str(), "1.22.2");
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RuntimeParts {
    /// The runtime, its exact version, and the platform it is built for.
    pub identity: (RuntimeId, RuntimeVersion, RuntimePlatform),
    /// The pinned URL and the SHA-256 the downloaded bytes must have.
    pub artifact: (RuntimeSource, Sha256Digest),
    /// How the artifact is packaged and where the executable sits below the
    /// installation directory.
    pub layout: (ArtifactKind, RelativePath),
    /// The upstream and license metadata of the component.
    pub attribution: RuntimeAttribution,
}

/// One pinned, immutable runtime version.
///
/// Every value in a definition is reviewable in the source that produces it, and
/// none of them is discovered at runtime. In particular the expected
/// [`Sha256Digest`] is decided here and not taken from the response that delivered
/// the artifact.
///
/// ```text
/// RuntimeDefinition
/// ├── id           retroarch
/// ├── version      1.22.2
/// ├── platform     macos-universal
/// ├── source       https://buildbot.libretro.com/…/RetroArch_Metal.dmg
/// ├── digest       81b79121…fdb434
/// ├── kind         apple-disk-image { bundle: "RetroArch.app" }
/// ├── executable   RetroArch.app/Contents/MacOS/RetroArch
/// └── attribution  RetroArch / libretro / GPL-3.0-or-later
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RuntimeDefinition {
    id: RuntimeId,
    version: RuntimeVersion,
    platform: RuntimePlatform,
    source: RuntimeSource,
    digest: Sha256Digest,
    kind: ArtifactKind,
    executable: RelativePath,
    attribution: RuntimeAttribution,
}

impl RuntimeDefinition {
    /// Creates a pinned definition from already validated parts.
    #[must_use]
    pub fn new(parts: RuntimeParts) -> Self {
        let RuntimeParts {
            identity: (id, version, platform),
            artifact: (source, digest),
            layout: (kind, executable),
            attribution,
        } = parts;

        Self {
            id,
            version,
            platform,
            source,
            digest,
            kind,
            executable,
            attribution,
        }
    }

    /// Returns the runtime this definition installs.
    #[must_use]
    pub const fn id(&self) -> &RuntimeId {
        &self.id
    }

    /// Returns the exact pinned version.
    #[must_use]
    pub const fn version(&self) -> &RuntimeVersion {
        &self.version
    }

    /// Returns the platform the artifact and the installation are built for.
    #[must_use]
    pub const fn platform(&self) -> RuntimePlatform {
        self.platform
    }

    /// Returns the URL the artifact is downloaded from.
    ///
    /// For a production definition this is always
    /// [`RuntimeSource::Official`]; a [`RuntimeSource::Loopback`] can only be
    /// constructed for a test.
    #[must_use]
    pub const fn source(&self) -> &RuntimeSource {
        &self.source
    }

    /// Returns the SHA-256 digest the downloaded artifact must have.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns how the artifact is packaged.
    #[must_use]
    pub const fn kind(&self) -> &ArtifactKind {
        &self.kind
    }

    /// Returns where the executable sits below the installation directory.
    #[must_use]
    pub const fn executable(&self) -> &RelativePath {
        &self.executable
    }

    /// Returns the upstream and license metadata of the component.
    #[must_use]
    pub const fn attribution(&self) -> &RuntimeAttribution {
        &self.attribution
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{ArtifactSource, LicenseIdentifier};

    fn digest(hex: &str) -> Sha256Digest {
        Sha256Digest::from_str(hex).expect("the test digest is valid")
    }

    /// A version is accepted only when it is a single, safe path component.
    #[test]
    fn a_version_must_be_a_single_safe_path_component() {
        for accepted in [
            "1.22.2",
            "1.22.2+metal",
            "2.0.0-rc.1",
            "1_0",
            "1.22.2_Metal",
        ] {
            assert!(
                RuntimeVersion::from_str(accepted).is_ok(),
                "{accepted} must be accepted"
            );
        }

        for (rejected, expected) in [
            ("", VersionError::Empty),
            ("..", VersionError::MissingLeadingDigit),
            (".", VersionError::MissingLeadingDigit),
            ("-1.0", VersionError::MissingLeadingDigit),
            ("latest", VersionError::MissingLeadingDigit),
            ("1.22/2", VersionError::InvalidCharacter('/')),
            ("1.22 2", VersionError::InvalidCharacter(' ')),
            ("1.22\\2", VersionError::InvalidCharacter('\\')),
            ("1.22é", VersionError::InvalidCharacter('é')),
        ] {
            assert_eq!(
                RuntimeVersion::from_str(rejected),
                Err(expected),
                "{rejected:?} must be rejected"
            );
        }

        let too_long = format!("1{}", "0".repeat(MAX_VERSION_LENGTH));
        assert_eq!(
            RuntimeVersion::from_str(&too_long),
            Err(VersionError::TooLong)
        );
    }

    /// The pinned definition exposes exactly what it was built from, so the
    /// installer, the verifier, and the resolver cannot disagree about it.
    #[test]
    fn a_definition_keeps_every_pinned_value() {
        let definition = RuntimeDefinition::new(RuntimeParts {
            identity: (
                RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity"),
                RuntimeVersion::from_str("1.22.2").expect("a valid version"),
                RuntimePlatform::MacOsUniversal,
            ),
            artifact: (
                RuntimeSource::official(
                    ArtifactSource::new("https://buildbot.libretro.com/stable/1.22.2/a.dmg")
                        .expect("a valid source"),
                ),
                digest("81b79121ba26d539064ae13b4d0419a120c3d165afbe656cf5f5412b15fdb434"),
            ),
            layout: (
                ArtifactKind::AppleDiskImage {
                    bundle: String::from("RetroArch.app"),
                },
                RelativePath::from_str("RetroArch.app/Contents/MacOS/RetroArch")
                    .expect("a valid relative path"),
            ),
            attribution: RuntimeAttribution {
                component: String::from("RetroArch"),
                upstream_project: String::from("libretro/RetroArch"),
                upstream_url: String::from("https://github.com/libretro/RetroArch"),
                license: LicenseIdentifier::from_str(LicenseIdentifier::GPL_3_0_OR_LATER)
                    .expect("a valid license identifier"),
            },
        });

        assert_eq!(
            definition.id(),
            &RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity")
        );
        assert_eq!(definition.version().as_str(), "1.22.2");
        assert_eq!(definition.platform(), RuntimePlatform::MacOsUniversal);
        assert_eq!(definition.platform().as_str(), "macos-universal");
        assert_eq!(definition.source().host(), OFFICIAL_RUNTIME_HOST);
        assert!(definition.source().is_official());
        assert_eq!(definition.source().file_name(), Some("a.dmg"));
        assert_eq!(definition.digest().as_str().len(), 64);
        assert_eq!(definition.kind().as_str(), "apple-disk-image");
        assert_eq!(
            definition.executable().to_string(),
            "RetroArch.app/Contents/MacOS/RetroArch"
        );

        let attribution = definition.attribution();
        assert_eq!(attribution.component, "RetroArch");
        assert_eq!(attribution.upstream_project, "libretro/RetroArch");
        assert_eq!(
            attribution.upstream_url,
            "https://github.com/libretro/RetroArch"
        );
        assert_eq!(attribution.license.as_str(), "GPL-3.0-or-later");
    }
}
