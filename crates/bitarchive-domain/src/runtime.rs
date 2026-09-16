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

/// Upper bound for an accepted runtime version string.
///
/// The bound exists so that a definition cannot smuggle an unbounded value into
/// a store path component. It is far above any real version.
const MAX_VERSION_LENGTH: usize = 64;

/// Number of lower-case hex digits in a SHA-256 digest.
const SHA256_HEX_LENGTH: usize = 64;

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

/// The reason a SHA-256 digest was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DigestError {
    /// The digest did not consist of exactly 64 hexadecimal digits.
    InvalidLength(usize),
    /// The digest contained a character that is not a hexadecimal digit.
    InvalidCharacter(char),
}

impl fmt::Display for DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => write!(
                f,
                "a SHA-256 digest must consist of exactly {SHA256_HEX_LENGTH} hexadecimal digits, \
                 but has {length}"
            ),
            Self::InvalidCharacter(character) => write!(
                f,
                "a SHA-256 digest must be lower-case hexadecimal, but contains {character:?}"
            ),
        }
    }
}

impl std::error::Error for DigestError {}

/// The SHA-256 digest of one artifact.
///
/// The value is normalised to lower-case hexadecimal on construction, so two
/// digests that differ only in case compare equal and a rendered digest always
/// has one spelling. A digest is a fingerprint and never a primary key
/// (ARCHITECTURE.md §11, invariant 3).
///
/// ```
/// use std::str::FromStr;
///
/// use bitarchive_domain::runtime::Sha256Digest;
///
/// let digest = Sha256Digest::from_str(
///     "81B79121BA26D539064AE13B4D0419A120C3D165AFBE656CF5F5412B15FDB434",
/// )
/// .unwrap();
///
/// assert_eq!(digest.as_str().len(), 64);
/// assert!(digest.as_str().chars().all(|character| !character.is_ascii_uppercase()));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Creates a digest from the 32 raw bytes of a SHA-256 hash.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the digest as lower-case hexadecimal.
    #[must_use]
    pub fn as_str(&self) -> String {
        let mut rendered = String::with_capacity(SHA256_HEX_LENGTH);

        for byte in self.0 {
            rendered.push(hex_digit(byte >> 4));
            rendered.push(hex_digit(byte & 0x0f));
        }

        rendered
    }
}

impl FromStr for Sha256Digest {
    type Err = DigestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.chars().count() != SHA256_HEX_LENGTH {
            return Err(DigestError::InvalidLength(value.chars().count()));
        }

        let mut bytes = [0_u8; 32];

        for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let high = hex_value(pair[0]).ok_or(DigestError::InvalidCharacter(pair[0] as char))?;
            let low = hex_value(pair[1]).ok_or(DigestError::InvalidCharacter(pair[1] as char))?;

            bytes[index] = (high << 4) | low;
        }

        Ok(Self(bytes))
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// Maps a nibble onto its lower-case hexadecimal digit.
const fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'a' + (nibble - 10)) as char,
    }
}

/// Maps one ASCII hexadecimal digit onto its value.
const fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

/// The reason a [`RelativePath`] was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RelativePathError {
    /// The path was empty.
    Empty,
    /// The path was absolute.
    Absolute,
    /// The path contained a `.` or `..` component.
    NonCanonical,
    /// The path contained an empty component, for example a doubled separator.
    EmptyComponent,
}

impl fmt::Display for RelativePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a relative path must not be empty"),
            Self::Absolute => f.write_str("a relative path must not be absolute"),
            Self::NonCanonical => {
                f.write_str("a relative path must not contain a '.' or '..' component")
            }
            Self::EmptyComponent => {
                f.write_str("a relative path must not contain an empty component")
            }
        }
    }
}

impl std::error::Error for RelativePathError {}

/// A path below the installation directory of a component.
///
/// The type exists so that a definition can name where an executable ends up
/// without being able to name a location outside the managed component store.
/// `..`, an absolute path, and an empty component are rejected when the value is
/// parsed, so joining the path onto an installation directory can never escape
/// it.
///
/// ```
/// use std::str::FromStr;
///
/// use bitarchive_domain::runtime::RelativePath;
///
/// assert!(RelativePath::from_str("RetroArch.app/Contents/MacOS/RetroArch").is_ok());
/// assert!(RelativePath::from_str("../../Applications/RetroArch.app").is_err());
/// ```
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RelativePath(Vec<String>);

impl RelativePath {
    /// Returns the components of the path, outermost first.
    #[must_use]
    pub fn components(&self) -> &[String] {
        &self.0
    }
}

impl FromStr for RelativePath {
    type Err = RelativePathError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(RelativePathError::Empty);
        }

        if value.starts_with('/') {
            return Err(RelativePathError::Absolute);
        }

        let mut components = Vec::new();

        for component in value.split('/') {
            match component {
                "" => return Err(RelativePathError::EmptyComponent),
                "." | ".." => return Err(RelativePathError::NonCanonical),
                component => components.push(component.to_owned()),
            }
        }

        Ok(Self(components))
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join("/"))
    }
}

/// The reason a URL was rejected as an [`ArtifactSource`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceError {
    /// The URL did not use `https`.
    NotHttps,
    /// The URL named no host.
    MissingHost,
    /// The host was not the one this source is pinned to.
    UnexpectedHost,
    /// The URL contained a character that cannot appear in a URL.
    InvalidCharacter(char),
    /// The URL contained a control character or a space.
    Whitespace,
    /// The URL carried user information or an explicit port.
    ///
    /// Either would make the host check ambiguous, so both are rejected rather
    /// than interpreted.
    AmbiguousAuthority,
    /// A loopback source was given something other than one absolute path.
    NotLoopbackPath,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotHttps => f.write_str("an artifact source must use https"),
            Self::MissingHost => f.write_str("an artifact source must name a host"),
            Self::UnexpectedHost => {
                f.write_str("an artifact source must be hosted on the pinned official host")
            }
            Self::InvalidCharacter(character) => {
                write!(f, "an artifact source contains {character:?}")
            }
            Self::Whitespace => {
                f.write_str("an artifact source must not contain whitespace or control characters")
            }
            Self::AmbiguousAuthority => f.write_str(
                "an artifact source must not carry user information or an explicit port",
            ),
            Self::NotLoopbackPath => f.write_str(
                "a loopback source must name one absolute path without '.' or '..' components",
            ),
        }
    }
}

impl std::error::Error for SourceError {}

/// The official host BitArchive downloads managed runtimes from.
///
/// Pinning the host in one place is what makes "official source only" checkable
/// rather than a comment: a definition cannot point at a mirror, a redirect
/// target, or an arbitrary third party, because construction rejects it.
pub const OFFICIAL_RUNTIME_HOST: &str = "buildbot.libretro.com";

/// The host a loopback artifact source is pinned to.
///
/// Only the IPv4 loopback address is accepted, so a loopback source can never
/// reach a machine other than the one running the test.
const LOOPBACK_HOST: &str = "127.0.0.1";

/// The single official URL an artifact is downloaded from.
///
/// The URL is validated on construction: it must use `https`, it must name
/// [`OFFICIAL_RUNTIME_HOST`], and it must not contain whitespace, control
/// characters, user information, or an explicit port. There are no mirrors and no
/// redirects to an unpinned host — a redirect away from the official host is
/// rejected by the downloader, not followed.
///
/// ```
/// use bitarchive_domain::runtime::ArtifactSource;
///
/// let source = ArtifactSource::new(
///     "https://buildbot.libretro.com/stable/1.22.2/apple/osx/universal/RetroArch_Metal.dmg",
/// )
/// .unwrap();
///
/// assert_eq!(source.host(), "buildbot.libretro.com");
/// assert!(ArtifactSource::new("https://example.invalid/RetroArch_Metal.dmg").is_err());
/// ```
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ArtifactSource {
    url: String,
    host: String,
}

impl ArtifactSource {
    /// Validates `url` and creates the source.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError`] when the URL is not an `https` URL on
    /// [`OFFICIAL_RUNTIME_HOST`], or when it contains whitespace, a control
    /// character, user information, an explicit port, or a non-ASCII character.
    pub fn new(url: &str) -> Result<Self, SourceError> {
        validate_url(url, "https://")?;

        let authority = authority_of(url, "https://")?;

        if authority.contains('@') || authority.contains(':') {
            return Err(SourceError::AmbiguousAuthority);
        }

        let host = authority.to_ascii_lowercase();

        if host != OFFICIAL_RUNTIME_HOST {
            return Err(SourceError::UnexpectedHost);
        }

        Ok(Self {
            url: url.to_owned(),
            host,
        })
    }

    /// Returns the full URL.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// Returns the host the artifact is downloaded from.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }
}

/// A loopback URL an artifact is downloaded from in a test.
///
/// This is the only other source a [`RuntimeDefinition`] can carry, and it exists
/// so that the real download, streaming, hashing, and cleanup path can be tested
/// end to end against a local HTTP server
/// (ARCHITECTURE.md §45.4) without ever weakening the official-source rule for
/// production definitions.
///
/// The type is deliberately incapable of naming anything except the IPv4
/// loopback address and a fixed port, so it cannot be used to reach a remote
/// host, a private network, or the official host over plaintext.
///
/// ```
/// use bitarchive_domain::runtime::LoopbackSource;
///
/// let source = LoopbackSource::new(8123, "/stable/1.22.2/fixture.dmg").unwrap();
///
/// assert_eq!(source.as_str(), "http://127.0.0.1:8123/stable/1.22.2/fixture.dmg");
/// assert!(LoopbackSource::new(8123, "https://buildbot.libretro.com/a.dmg").is_err());
/// ```
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct LoopbackSource {
    url: String,
    host: String,
}

impl LoopbackSource {
    /// Creates a loopback source for `port` and an absolute `path`.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError`] when `path` is not a single absolute path with no
    /// whitespace, no control characters, and no parent or current-directory
    /// component.
    pub fn new(port: u16, path: &str) -> Result<Self, SourceError> {
        if !path.starts_with('/') {
            return Err(SourceError::NotLoopbackPath);
        }

        if path
            .split('/')
            .any(|component| matches!(component, "." | ".."))
        {
            return Err(SourceError::NotLoopbackPath);
        }

        let url = format!("http://{LOOPBACK_HOST}:{port}{path}");
        validate_url(&url, "http://")?;

        Ok(Self {
            url,
            host: String::from(LOOPBACK_HOST),
        })
    }

    /// Returns the full URL.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// Returns the host the artifact is downloaded from.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }
}

/// The URL a pinned artifact is downloaded from.
///
/// A definition can only carry one of two sources: the official host
/// ([`ArtifactSource`]) or, in a test, the loopback address
/// ([`LoopbackSource`]). There is no third case in which a host could be chosen
/// freely, which is what makes "official source only" a property of the type
/// rather than a rule to remember.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum RuntimeSource {
    /// The pinned official build host.
    Official(ArtifactSource),
    /// A loopback server used by an automated test.
    Loopback(LoopbackSource),
}

impl RuntimeSource {
    /// Returns the pinned official source.
    #[must_use]
    pub const fn official(source: ArtifactSource) -> Self {
        Self::Official(source)
    }

    /// Returns the full URL.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Official(source) => source.as_str(),
            Self::Loopback(source) => source.as_str(),
        }
    }

    /// Returns the host the artifact is downloaded from.
    #[must_use]
    pub fn host(&self) -> &str {
        match self {
            Self::Official(source) => source.host(),
            Self::Loopback(source) => source.host(),
        }
    }

    /// Returns `true` when this is the pinned official source.
    #[must_use]
    pub const fn is_official(&self) -> bool {
        matches!(self, Self::Official(_))
    }

    /// Returns the file name the URL ends in, if it names one.
    ///
    /// Used for diagnostics only. The store decides where the artifact is
    /// written; the remote name never becomes a local path.
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        let path = self.as_str().split(['?', '#']).next()?;
        let name = path.rsplit('/').next()?;

        (!name.is_empty()).then_some(name)
    }
}

impl From<ArtifactSource> for RuntimeSource {
    fn from(source: ArtifactSource) -> Self {
        Self::Official(source)
    }
}

impl From<LoopbackSource> for RuntimeSource {
    fn from(source: LoopbackSource) -> Self {
        Self::Loopback(source)
    }
}

impl fmt::Display for RuntimeSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Rejects a URL that is not absolute, ASCII, and free of whitespace.
fn validate_url(url: &str, scheme: &str) -> Result<(), SourceError> {
    if !url.starts_with(scheme) {
        return Err(SourceError::NotHttps);
    }

    if url.chars().any(char::is_whitespace) {
        return Err(SourceError::Whitespace);
    }

    if let Some(invalid) = url
        .chars()
        .find(|character| !character.is_ascii() || character.is_control())
    {
        if invalid.is_ascii_whitespace() || invalid.is_control() {
            return Err(SourceError::Whitespace);
        }

        return Err(SourceError::InvalidCharacter(invalid));
    }

    Ok(())
}

/// Returns the authority part of an absolute URL.
fn authority_of<'a>(url: &'a str, scheme: &str) -> Result<&'a str, SourceError> {
    url.strip_prefix(scheme)
        .ok_or(SourceError::NotHttps)?
        .split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty())
        .ok_or(SourceError::MissingHost)
}

impl fmt::Display for ArtifactSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url)
    }
}

/// An SPDX license identifier.
///
/// The identifier is recorded so that redistribution stays traceable
/// (ARCHITECTURE.md §23.4, Issue #19). It is deliberately an opaque string rather
/// than an enum: BitArchive must be able to record the license of a component
/// whose license it does not yet model, and rejecting an unfamiliar identifier
/// would lose information instead of keeping it.
///
/// ```
/// use std::str::FromStr;
///
/// use bitarchive_domain::runtime::LicenseIdentifier;
///
/// let license = LicenseIdentifier::from_str(LicenseIdentifier::GPL_3_0_OR_LATER).unwrap();
/// assert_eq!(license.as_str(), "GPL-3.0-or-later");
/// ```
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct LicenseIdentifier(String);

impl LicenseIdentifier {
    /// The license RetroArch itself is distributed under.
    ///
    /// RetroArch's own source headers state "either version 3 of the License, or
    /// (at your option) any later version", which is `GPL-3.0-or-later` in SPDX
    /// terms. `GPL-3.0-only` would describe a grant upstream does not make.
    pub const GPL_3_0_OR_LATER: &'static str = "GPL-3.0-or-later";

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for LicenseIdentifier {
    type Err = LicenseIdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(LicenseIdentifierError::Empty);
        }

        if value.contains(char::is_whitespace) || value.contains(char::is_control) {
            return Err(LicenseIdentifierError::Whitespace);
        }

        if let Some(invalid) = value.chars().find(|character| !character.is_ascii()) {
            return Err(LicenseIdentifierError::InvalidCharacter(invalid));
        }

        Ok(Self(value.to_owned()))
    }
}

/// The reason a [`LicenseIdentifier`] was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LicenseIdentifierError {
    /// The identifier was empty.
    Empty,
    /// The identifier contained whitespace or a control character.
    Whitespace,
    /// The identifier contained a non-ASCII character.
    InvalidCharacter(char),
}

impl fmt::Display for LicenseIdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a license identifier must not be empty"),
            Self::Whitespace => f.write_str("a license identifier must not contain whitespace"),
            Self::InvalidCharacter(character) => {
                write!(
                    f,
                    "a license identifier must be ASCII, but contains {character:?}"
                )
            }
        }
    }
}

impl std::error::Error for LicenseIdentifierError {}

impl fmt::Display for LicenseIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a managed component comes from and under which license.
///
/// This is the metadata a later distribution step and a later license/About
/// surface need. It is recorded now, while the pinned definition is written, so
/// that redistribution stays traceable without a second pass over the sources.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RuntimeAttribution {
    /// The component name as the upstream project spells it.
    pub component: String,
    /// The upstream project the component belongs to.
    pub upstream_project: String,
    /// The upstream project's canonical URL.
    pub upstream_url: String,
    /// The license the component is distributed under.
    pub license: LicenseIdentifier,
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

    /// A digest normalises to lower-case hexadecimal, so one artifact has exactly
    /// one spelling and two equivalent digests compare equal.
    #[test]
    fn a_digest_normalises_to_lower_case_hexadecimal() {
        let upper = "81B79121BA26D539064AE13B4D0419A120C3D165AFBE656CF5F5412B15FDB434";
        let lower = "81b79121ba26d539064ae13b4d0419a120c3d165afbe656cf5f5412b15fdb434";

        assert_eq!(digest(upper).as_str(), lower);
        assert_eq!(digest(upper), digest(lower));
    }

    /// The raw bytes and the rendered text describe the same digest, so a streamed
    /// hash can be rendered and compared without a second representation.
    #[test]
    fn raw_digest_bytes_and_rendered_text_agree() {
        let rendered = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        let bytes: [u8; 32] = core::array::from_fn(|index| index as u8);

        assert_eq!(Sha256Digest::from_bytes(bytes), digest(rendered));
        assert_eq!(Sha256Digest::from_bytes(bytes).as_str(), rendered);
    }

    /// A malformed digest is rejected rather than silently truncated or padded.
    #[test]
    fn a_malformed_digest_is_rejected() {
        for rejected in ["", "00", &"a".repeat(63), &"a".repeat(65)] {
            assert!(
                matches!(
                    Sha256Digest::from_str(rejected),
                    Err(DigestError::InvalidLength(_))
                ),
                "{rejected:?} must be rejected for its length"
            );
        }

        let mut non_hex = "a".repeat(64);
        non_hex.replace_range(0..1, "z");
        assert_eq!(
            Sha256Digest::from_str(&non_hex),
            Err(DigestError::InvalidCharacter('z'))
        );
    }

    /// A relative path can never leave the directory it is joined onto.
    #[test]
    fn a_relative_path_cannot_escape_its_installation_directory() {
        let executable = RelativePath::from_str("RetroArch.app/Contents/MacOS/RetroArch")
            .expect("the documented executable path is relative");

        assert_eq!(
            executable.components(),
            ["RetroArch.app", "Contents", "MacOS", "RetroArch"]
        );

        for (rejected, expected) in [
            ("", RelativePathError::Empty),
            ("/Applications/RetroArch.app", RelativePathError::Absolute),
            ("../RetroArch.app", RelativePathError::NonCanonical),
            ("RetroArch.app/..", RelativePathError::NonCanonical),
            ("RetroArch.app/./RetroArch", RelativePathError::NonCanonical),
            (
                "RetroArch.app//RetroArch",
                RelativePathError::EmptyComponent,
            ),
        ] {
            assert_eq!(
                RelativePath::from_str(rejected),
                Err(expected),
                "{rejected:?} must be rejected"
            );
        }
    }

    /// Only the official host is accepted, over https, so a definition cannot be
    /// pointed at a mirror or a third party.
    #[test]
    fn only_the_official_https_source_is_accepted() {
        let official = ArtifactSource::new(
            "https://buildbot.libretro.com/stable/1.22.2/apple/osx/universal/RetroArch_Metal.dmg",
        )
        .expect("the official source is valid");

        assert_eq!(official.host(), OFFICIAL_RUNTIME_HOST);
        assert_eq!(
            RuntimeSource::from(official.clone()).file_name(),
            Some("RetroArch_Metal.dmg")
        );

        for (rejected, expected) in [
            (
                "http://buildbot.libretro.com/stable/1.22.2/RetroArch_Metal.dmg",
                SourceError::NotHttps,
            ),
            ("https://", SourceError::MissingHost),
            (
                "https://mirror.example.invalid/RetroArch_Metal.dmg",
                SourceError::UnexpectedHost,
            ),
            (
                "https://buildbot.libretro.com.evil.invalid/RetroArch_Metal.dmg",
                SourceError::UnexpectedHost,
            ),
            (
                "https://user@buildbot.libretro.com/RetroArch_Metal.dmg",
                SourceError::AmbiguousAuthority,
            ),
            (
                "https://buildbot.libretro.com:8443/RetroArch_Metal.dmg",
                SourceError::AmbiguousAuthority,
            ),
            (
                "https://buildbot.libretro.com/RetroArch Metal.dmg",
                SourceError::Whitespace,
            ),
        ] {
            assert_eq!(
                ArtifactSource::new(rejected),
                Err(expected),
                "{rejected:?} must be rejected"
            );
        }
    }

    /// A loopback source can name nothing but the loopback address and one
    /// absolute path, so it can never stand in for a production download target.
    #[test]
    fn a_loopback_source_cannot_name_a_remote_host() {
        let loopback = LoopbackSource::new(8123, "/stable/1.22.2/fixture.dmg")
            .expect("a loopback path is valid");

        assert_eq!(
            loopback.as_str(),
            "http://127.0.0.1:8123/stable/1.22.2/fixture.dmg"
        );
        assert_eq!(loopback.host(), LOOPBACK_HOST);

        for (rejected, expected) in [
            (
                "https://buildbot.libretro.com/a.dmg",
                SourceError::NotLoopbackPath,
            ),
            ("stable/1.22.2/fixture.dmg", SourceError::NotLoopbackPath),
            ("", SourceError::NotLoopbackPath),
            ("/a/../b.dmg", SourceError::NotLoopbackPath),
            ("/a/./b.dmg", SourceError::NotLoopbackPath),
            ("http://mirror.invalid/a.dmg", SourceError::NotLoopbackPath),
            ("/a b.dmg", SourceError::Whitespace),
        ] {
            assert_eq!(
                LoopbackSource::new(8123, rejected),
                Err(expected),
                "{rejected:?} must not become a loopback source"
            );
        }

        // Only the official source counts as official.
        assert!(!RuntimeSource::from(loopback).is_official());
        assert!(
            RuntimeSource::from(
                ArtifactSource::new("https://buildbot.libretro.com/a.dmg")
                    .expect("a valid official source")
            )
            .is_official()
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
        assert_eq!(definition.digest().as_str().len(), SHA256_HEX_LENGTH);
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
