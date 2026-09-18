//! Shared value types for managed components and their artifacts.
//!
//! Runtime and cores are **different component classes** (ARCHITECTURE.md §23.4,
//! §53.10): a runtime is an application, a core is a libretro implementation, and
//! they have different identities, versioning, and licensing. What they genuinely
//! share is smaller and narrower than "being a component":
//!
//! - where a pinned artifact comes from ([`ArtifactSource`], [`LoopbackSource`],
//!   [`ComponentArtifactSource`])
//! - which bytes are expected ([`Sha256Digest`])
//! - how the artifact is packaged and what it is called
//!   ([`ArtifactReference`])
//! - where something sits below an installation directory ([`RelativePath`])
//! - who made it and under which license ([`LicenseIdentifier`],
//!   [`ComponentAttribution`])
//!
//! Those are exactly the values that must behave identically for both classes —
//! a mirror must be just as impossible for a core as for a runtime — so they live
//! here once instead of being duplicated per component class. Everything that
//! differs (versioning, platform matrix, install layout, artifact packaging)
//! stays in the module of the class it belongs to.
//!
//! # This is a value module, not a framework
//!
//! There is deliberately no `Component` trait, no generic installation pipeline,
//! and no shared "component" life cycle here. Rust cannot put a core and an
//! application behind one useful trait without erasing what makes them different,
//! and the store logic that looks shareable is not: a runtime has an activation
//! record and a core must never have one. Sharing is limited to the values below
//! plus one immutable-installation primitive in `bitarchive-infrastructure`.
//!
//! # Transport-neutral
//!
//! Nothing here performs I/O. A source is a reviewed, validated URL *string*, a
//! digest is 32 bytes, and a path is a sequence of components below an
//! installation directory. Nothing in this module opens, reads, or writes
//! anything (ARCHITECTURE.md §2.4, §5.1).

use std::fmt;
use std::str::FromStr;

/// Number of lower-case hex digits in a SHA-256 digest.
const SHA256_HEX_LENGTH: usize = 64;

/// The official host BitArchive downloads managed components from.
///
/// Pinning the host in one place is what makes "official source only" checkable
/// rather than a comment: a definition cannot point at a mirror, a redirect
/// target, or an arbitrary third party, because construction rejects it. The rule
/// holds for runtimes and cores alike, which is why the constant is shared.
pub const OFFICIAL_COMPONENT_HOST: &str = "buildbot.libretro.com";

/// The host a loopback artifact source is pinned to.
///
/// Only the IPv4 loopback address is accepted, so a loopback source can never
/// reach a machine other than the one running the test.
const LOOPBACK_HOST: &str = "127.0.0.1";

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
/// use bitarchive_domain::component::Sha256Digest;
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
/// The type exists so that a definition can name where an executable or a library
/// ends up without being able to name a location outside the managed component
/// store. `..`, an absolute path, and an empty component are rejected when the
/// value is parsed, so joining the path onto an installation directory can never
/// escape it.
///
/// ```
/// use std::str::FromStr;
///
/// use bitarchive_domain::component::RelativePath;
///
/// assert!(RelativePath::from_str("RetroArch.app/Contents/MacOS/RetroArch").is_ok());
/// assert!(RelativePath::from_str("mgba_libretro.dylib").is_ok());
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

    /// Returns the last component, which is the file name.
    ///
    /// A [`RelativePath`] is never empty, so this always names something.
    #[must_use]
    pub fn file_name(&self) -> &str {
        // A path is built from at least one non-empty component, so the last one
        // exists. `expect` documents that invariant rather than hiding it behind a
        // silent fallback.
        self.0
            .last()
            .map(String::as_str)
            .expect("a relative path has at least one component")
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

/// An SPDX license identifier.
///
/// The identifier is recorded so that redistribution stays traceable
/// (ARCHITECTURE.md §23.4). It is deliberately an opaque string rather than an
/// enum: BitArchive must be able to record the license of a component whose
/// license it does not yet model, and rejecting an unfamiliar identifier would
/// lose information instead of keeping it.
///
/// ```
/// use std::str::FromStr;
///
/// use bitarchive_domain::component::LicenseIdentifier;
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

    /// The license the mGBA project is distributed under.
    ///
    /// mGBA's own `LICENSE` file is the text of the Mozilla Public License,
    /// version 2.0, and upstream's own `mgba_libretro.info` records
    /// `MPLv2.0`. `MPL-2.0` is the SPDX identifier for that text.
    pub const MPL_2_0: &'static str = "MPL-2.0";

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

/// The reason a [`ComponentId`] was rejected.
///
/// The accepted alphabet is lower-case ASCII letters, digits, and `-`, and an
/// identity must start with a letter. That keeps `..`, an empty name, a nested
/// path, and an upper-case spelling out of the identity space, so an identity is
/// always usable as a single path component.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ComponentIdError {
    /// The identity was empty.
    Empty,
    /// The identity did not start with a lower-case ASCII letter.
    MissingLeadingLetter,
    /// The identity contained a character outside the accepted set.
    InvalidCharacter(char),
}

impl fmt::Display for ComponentIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a component identity must not be empty"),
            Self::MissingLeadingLetter => {
                f.write_str("a component identity must start with a lower-case ASCII letter")
            }
            Self::InvalidCharacter(character) => write!(
                f,
                "a component identity must consist of lower-case ASCII letters, digits or '-', \
                 but contains {character:?}"
            ),
        }
    }
}

impl std::error::Error for ComponentIdError {}

/// A managed component's distribution identity.
///
/// This is the stable, human-readable slug that names a component in the store
/// and in a curated allowlist — `retroarch`, `mgba`. It is **not** a fachliche
/// domain identity: it is not a UUID, it is not minted, and it is never a primary
/// key (ARCHITECTURE.md §7, §11). A fachliche [`CoreId`](crate::CoreId) says
/// *which* core a user configured for a game; a
/// [`CoreComponentId`](crate::managed_core::CoreComponentId) says *which
/// distributable component* BitArchive installs. Keeping them apart is what stops
/// a download slug from becoming a foreign key.
///
/// ```
/// use std::str::FromStr;
///
/// use bitarchive_domain::component::ComponentId;
///
/// assert_eq!(ComponentId::from_str("mgba").unwrap().as_str(), "mgba");
/// assert!(ComponentId::from_str("..").is_err());
/// assert!(ComponentId::from_str("mgba/extra").is_err());
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ComponentId(String);

impl ComponentId {
    /// Returns the identity as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ComponentId {
    type Err = ComponentIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(ComponentIdError::Empty);
        }

        if !value.starts_with(|character: char| character.is_ascii_lowercase()) {
            return Err(ComponentIdError::MissingLeadingLetter);
        }

        if let Some(invalid) = value.chars().find(|character| {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || *character == '-')
        }) {
            return Err(ComponentIdError::InvalidCharacter(invalid));
        }

        Ok(Self(value.to_owned()))
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
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

/// The single official URL an artifact is downloaded from.
///
/// The URL is validated on construction: it must use `https`, it must name
/// [`OFFICIAL_COMPONENT_HOST`], and it must not contain whitespace, control
/// characters, user information, or an explicit port. There are no mirrors and no
/// redirects to an unpinned host — a redirect away from the official host is
/// rejected by the downloader, not followed.
///
/// Note what this type does **not** claim. It validates the *host and scheme* of a
/// URL; it says nothing about whether the content behind that URL is immutable.
/// The libretro build host publishes core binaries only below a rolling `latest`
/// path, and that fact is recorded in the definition and in
/// `docs/decisions/0002-managed-core-acquisition.md` rather than being papered
/// over here. The trust anchor is the pinned [`Sha256Digest`], not the URL.
///
/// ```
/// use bitarchive_domain::component::ArtifactSource;
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
    /// [`OFFICIAL_COMPONENT_HOST`], or when it contains whitespace, a control
    /// character, user information, an explicit port, or a non-ASCII character.
    pub fn new(url: &str) -> Result<Self, SourceError> {
        validate_url(url, "https://")?;

        let authority = authority_of(url, "https://")?;

        if authority.contains('@') || authority.contains(':') {
            return Err(SourceError::AmbiguousAuthority);
        }

        let host = authority.to_ascii_lowercase();

        if host != OFFICIAL_COMPONENT_HOST {
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
/// This is the only other source a definition can carry, and it exists so that
/// the real download, streaming, hashing, archive extraction, and cleanup path
/// can be tested end to end against a local HTTP server
/// (ARCHITECTURE.md §45.4) without ever weakening the official-source rule for
/// production definitions.
///
/// The type is deliberately incapable of naming anything except the IPv4
/// loopback address and a fixed port, so it cannot be used to reach a remote
/// host, a private network, or the official host over plaintext.
///
/// ```
/// use bitarchive_domain::component::LoopbackSource;
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
pub enum ComponentArtifactSource {
    /// The pinned official build host.
    Official(ArtifactSource),
    /// A loopback server used by an automated test.
    Loopback(LoopbackSource),
}

impl ComponentArtifactSource {
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

impl From<ArtifactSource> for ComponentArtifactSource {
    fn from(source: ArtifactSource) -> Self {
        Self::Official(source)
    }
}

impl From<LoopbackSource> for ComponentArtifactSource {
    fn from(source: LoopbackSource) -> Self {
        Self::Loopback(source)
    }
}

impl fmt::Display for ComponentArtifactSource {
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

/// Where a managed component comes from and under which license.
///
/// This is the metadata a later distribution step and a later license/About
/// surface need. It is recorded now, while the pinned definition is written, so
/// that redistribution stays traceable without a second pass over the sources.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ComponentAttribution {
    /// The component name as the upstream project spells it.
    pub component: String,
    /// The upstream project the component belongs to.
    pub upstream_project: String,
    /// The upstream project's canonical URL.
    pub upstream_url: String,
    /// The license the component is distributed under.
    pub license: LicenseIdentifier,
}

/// The exact bytes of one pinned artifact.
///
/// One value instead of three loose fields, because the three belong together:
/// the digest only means something for the artifact the URL actually serves, and
/// describing one without the other is how a pin becomes ambiguous.
///
/// [`file_name`](Self::file_name) is the name of the artifact *as published*, used
/// for diagnostics and for naming a local copy. It is never joined onto a path
/// without being validated, and a store derives its own file name from the
/// component identity rather than from a remote name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ArtifactReference {
    /// The URL the artifact is downloaded from.
    pub source: ComponentArtifactSource,
    /// The SHA-256 the downloaded bytes must have.
    pub digest: Sha256Digest,
    /// The file name the artifact is published under, if it is known.
    pub file_name: Option<String>,
}

impl ArtifactReference {
    /// Creates an artifact reference with a known published file name.
    ///
    /// # Errors
    ///
    /// Returns a message when `file_name` is not a single, safe file name: an
    /// empty name, a name containing `/` or `\\`, or a `.`/`..` name would make
    /// the value ambiguous and is rejected here rather than at a path join.
    pub fn with_file_name(
        source: ComponentArtifactSource,
        digest: Sha256Digest,
        file_name: &str,
    ) -> Result<Self, String> {
        if file_name.is_empty() {
            return Err(String::from("an artifact file name must not be empty"));
        }

        if file_name.contains('/') || file_name.contains('\\') {
            return Err(format!(
                "an artifact file name must not contain a path separator, but {file_name:?} does"
            ));
        }

        if matches!(file_name, "." | "..") {
            return Err(format!("an artifact file name must not be {file_name:?}"));
        }

        Ok(Self {
            source,
            digest,
            file_name: Some(file_name.to_owned()),
        })
    }

    /// Creates an artifact reference whose published file name is not recorded.
    #[must_use]
    pub const fn without_file_name(source: ComponentArtifactSource, digest: Sha256Digest) -> Self {
        Self {
            source,
            digest,
            file_name: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(hex: &str) -> Sha256Digest {
        Sha256Digest::from_str(hex).expect("the test digest is valid")
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

    /// A relative path can never leave the directory it is joined onto, for an
    /// executable and for a library alike.
    #[test]
    fn a_relative_path_cannot_escape_its_installation_directory() {
        let executable = RelativePath::from_str("RetroArch.app/Contents/MacOS/RetroArch")
            .expect("the documented executable path is relative");

        assert_eq!(
            executable.components(),
            ["RetroArch.app", "Contents", "MacOS", "RetroArch"]
        );
        assert_eq!(executable.file_name(), "RetroArch");

        let library =
            RelativePath::from_str("mgba_libretro.dylib").expect("a bare library name is valid");

        assert_eq!(library.file_name(), "mgba_libretro.dylib");

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

        assert_eq!(official.host(), OFFICIAL_COMPONENT_HOST);
        assert_eq!(
            ComponentArtifactSource::from(official.clone()).file_name(),
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

    /// The same official-source rule holds for a core artifact as for a runtime
    /// artifact: the host check is a property of the shared source type, not a
    /// rule the runtime path happens to apply.
    #[test]
    fn the_official_source_rule_is_the_same_for_every_component_class() {
        let core = ArtifactSource::new(
            "https://buildbot.libretro.com/nightly/apple/osx/arm64/latest/mgba_libretro.dylib.zip",
        )
        .expect("the official core source is valid");

        assert_eq!(core.host(), OFFICIAL_COMPONENT_HOST);

        assert_eq!(
            ArtifactSource::new(
                "https://buildbot.libretro.com.evil.invalid/nightly/apple/osx/arm64/latest/a.zip"
            ),
            Err(SourceError::UnexpectedHost)
        );
    }

    /// A loopback source can name nothing but the loopback address and one
    /// absolute path, so it can never stand in for a production download target.
    #[test]
    fn a_loopback_source_cannot_name_a_remote_host() {
        let loopback = LoopbackSource::new(8123, "/nightly/apple/osx/arm64/latest/fixture.zip")
            .expect("a loopback path is valid");

        assert_eq!(
            loopback.as_str(),
            "http://127.0.0.1:8123/nightly/apple/osx/arm64/latest/fixture.zip"
        );
        assert_eq!(loopback.host(), LOOPBACK_HOST);

        for (rejected, expected) in [
            (
                "https://buildbot.libretro.com/a.zip",
                SourceError::NotLoopbackPath,
            ),
            ("nightly/a.zip", SourceError::NotLoopbackPath),
            ("", SourceError::NotLoopbackPath),
            ("/a/../b.zip", SourceError::NotLoopbackPath),
            ("/a/./b.zip", SourceError::NotLoopbackPath),
            ("http://mirror.invalid/a.zip", SourceError::NotLoopbackPath),
            ("/a b.zip", SourceError::Whitespace),
        ] {
            assert_eq!(
                LoopbackSource::new(8123, rejected),
                Err(expected),
                "{rejected:?} must not become a loopback source"
            );
        }

        // Only the official source counts as official.
        assert!(!ComponentArtifactSource::from(loopback).is_official());
        assert!(
            ComponentArtifactSource::from(
                ArtifactSource::new("https://buildbot.libretro.com/a.zip")
                    .expect("a valid official source")
            )
            .is_official()
        );
    }

    /// A component identity is a single safe path component, so it can name a
    /// store directory without a second validation.
    #[test]
    fn a_component_identity_is_a_single_safe_path_component() {
        for accepted in ["retroarch", "mgba", "sameboy", "mesen-s", "bsnes2014"] {
            assert!(
                ComponentId::from_str(accepted).is_ok(),
                "{accepted} must be accepted"
            );
        }

        for (rejected, expected) in [
            ("", ComponentIdError::Empty),
            ("MGBA", ComponentIdError::MissingLeadingLetter),
            ("..", ComponentIdError::MissingLeadingLetter),
            ("-mgba", ComponentIdError::MissingLeadingLetter),
            ("mgba/extra", ComponentIdError::InvalidCharacter('/')),
            ("mgba extra", ComponentIdError::InvalidCharacter(' ')),
            ("mgba.dylib", ComponentIdError::InvalidCharacter('.')),
            ("mgbaé", ComponentIdError::InvalidCharacter('é')),
        ] {
            assert_eq!(
                ComponentId::from_str(rejected),
                Err(expected),
                "{rejected:?} must be rejected"
            );
        }
    }

    /// The license identifier records what upstream grants, including the two
    /// identifiers this repository pins.
    #[test]
    fn a_license_identifier_records_the_identifier_it_was_given() {
        assert_eq!(
            LicenseIdentifier::from_str(LicenseIdentifier::MPL_2_0)
                .expect("a valid identifier")
                .as_str(),
            "MPL-2.0"
        );
        assert_eq!(
            LicenseIdentifier::from_str(LicenseIdentifier::GPL_3_0_OR_LATER)
                .expect("a valid identifier")
                .as_str(),
            "GPL-3.0-or-later"
        );

        for rejected in ["", "MPL 2.0", "MPL-2.0\n", "MPL-2.0é"] {
            assert!(
                LicenseIdentifier::from_str(rejected).is_err(),
                "{rejected:?} must be rejected"
            );
        }
    }

    /// An artifact reference carries the name it was published under, and refuses
    /// a name that could not be a single file.
    #[test]
    fn an_artifact_reference_records_only_a_safe_published_file_name() {
        let source = ComponentArtifactSource::official(
            ArtifactSource::new(
                "https://buildbot.libretro.com/nightly/apple/osx/arm64/latest/\
                 mgba_libretro.dylib.zip",
            )
            .expect("a valid official source"),
        );

        let reference = ArtifactReference::with_file_name(
            source.clone(),
            digest("1aa000e5a88c2ea2afb788cdee89853f86d1c8639fc0df5d2ed6f261c2d81462"),
            "mgba_libretro.dylib.zip",
        )
        .expect("the published name is a plain file name");

        assert_eq!(
            reference.file_name.as_deref(),
            Some("mgba_libretro.dylib.zip")
        );
        assert_eq!(reference.source, source);

        for rejected in ["", "cores/mgba.zip", "cores\\mgba.zip", ".", ".."] {
            assert!(
                ArtifactReference::with_file_name(
                    source.clone(),
                    digest("1aa000e5a88c2ea2afb788cdee89853f86d1c8639fc0df5d2ed6f261c2d81462"),
                    rejected,
                )
                .is_err(),
                "{rejected:?} must not become a published file name"
            );
        }

        // Omitting the name is honest for an artifact whose published name is not
        // part of the pin.
        assert!(
            ArtifactReference::without_file_name(
                source,
                digest("1aa000e5a88c2ea2afb788cdee89853f86d1c8639fc0df5d2ed6f261c2d81462")
            )
            .file_name
            .is_none()
        );
    }
}
