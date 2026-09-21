//! The curated systems BitArchive supports, and the content formats that select
//! them.
//!
//! BitArchive ships a curated system list and does not support custom systems in
//! the MVP (PRODUCT.md §13). This module owns the smallest useful part of that
//! list: which systems exist, which content formats select them, and which
//! curated core component runs them.
//!
//! # What is modelled, and what is not
//!
//! Only what the launch decision actually needs. A system here is an identity, a
//! display name, the content formats it accepts, and the core that runs it. There
//! are deliberately no manufacturers, release years, logos, artwork, capability
//! flags, or format compatibility matrices: those belong to the curated catalogue
//! (`ARCHITECTURE.md` §15) and nothing in the launch path reads them yet.
//!
//! # What the list contains, and why
//!
//! Exactly the systems the one curated core serves:
//!
//! ```text
//! gba  Game Boy Advance   .gba        → mgba
//! gbc  Game Boy Color     .gbc        → mgba
//! gb   Game Boy           .gb         → mgba
//! ```
//!
//! mGBA covers all three (see [`mgba_bootstrap`](crate::managed_core::mgba_bootstrap)),
//! so the list is a true statement about what BitArchive can run today rather
//! than a slice of the planned MVP system list. Listing NES, SNES, or PlayStation
//! here would promise a system that no curated core can start.
//!
//! # Systems are not `SystemId`s
//!
//! [`EmulatedSystemKey`] is a stable, curated slug — the same kind of value as
//! [`CoreComponentId`] — and it is **not** [`SystemId`](crate::SystemId), which is the
//! UUIDv7 identity a persisted release carries. No conversion between the two exists
//! yet: turning a system key into a release's `SystemId` is a persistence question, and
//! no store exists that could answer it.
//!
//! # How a system is selected
//!
//! ```text
//! content path → EmulatedSystemCatalog::system_for_content → Option<&EmulatedSystem>
//! ```
//!
//! The file extension decides, because it is the only system evidence the
//! product has before an index exists. Nothing here inspects file *content*: a
//! `.gb` file stays Game Boy even when a later scan might classify it as Game Boy
//! Color, and no system is ever guessed from a file name, a folder name, or a
//! size.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use crate::managed_core::{CoreComponentId, CoreDefinition, CorePlatform, curated_core};

/// The stable, curated identity of a system BitArchive supports.
///
/// A system key is one safe path component: lower-case ASCII letters, digits, and
/// `-`, starting with a letter. It is a curated constant
/// ([`EmulatedSystemKey::GAME_BOY_ADVANCE`] and friends), not a value derived from
/// user input or from a file name.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EmulatedSystemKey(String);

impl EmulatedSystemKey {
    /// The curated key of Game Boy Advance.
    pub const GAME_BOY_ADVANCE: &'static str = "gba";
    /// The curated key of Game Boy Color.
    pub const GAME_BOY_COLOR: &'static str = "gbc";
    /// The curated key of Game Boy.
    pub const GAME_BOY: &'static str = "gb";

    /// Returns the key as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EmulatedSystemKey {
    /// Renders the key the way it is spelled in code and in diagnostics.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a string is not a valid [`EmulatedSystemKey`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EmulatedSystemKeyError {
    /// The key was empty.
    Empty,
    /// The key started with something that is not an ASCII letter.
    InvalidStart(char),
    /// The key contained a character outside the accepted alphabet.
    InvalidCharacter(char),
}

impl fmt::Display for EmulatedSystemKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "a system key must not be empty"),
            Self::InvalidStart(character) => write!(
                f,
                "a system key must start with a lower-case ASCII letter, but starts with \
                 {character:?}"
            ),
            Self::InvalidCharacter(character) => write!(
                f,
                "a system key may only contain lower-case ASCII letters, digits, and '-', \
                 but contains {character:?}"
            ),
        }
    }
}

impl std::error::Error for EmulatedSystemKeyError {}

impl FromStr for EmulatedSystemKey {
    type Err = EmulatedSystemKeyError;

    /// Parses a curated system key.
    ///
    /// # Errors
    ///
    /// Returns [`EmulatedSystemKeyError`] when the value is empty, does not start
    /// with a lower-case ASCII letter, or contains a character outside
    /// `a-z`, `0-9`, and `-`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut characters = value.chars();

        match characters.next() {
            None => return Err(EmulatedSystemKeyError::Empty),
            Some(first) if !first.is_ascii_lowercase() => {
                return Err(EmulatedSystemKeyError::InvalidStart(first));
            }
            Some(_) => {}
        }

        if let Some(invalid) = value.chars().find(|character| {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || *character == '-')
        }) {
            return Err(EmulatedSystemKeyError::InvalidCharacter(invalid));
        }

        Ok(Self(value.to_owned()))
    }
}

/// One curated system.
///
/// A system is described by what the launch decision needs: its key, the name the
/// product shows for it, the content formats that select it, and the curated core
/// component that runs it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EmulatedSystem {
    key: EmulatedSystemKey,
    display_name: &'static str,
    content_extensions: &'static [&'static str],
    core: CoreComponentId,
}

impl EmulatedSystem {
    /// Creates a curated system description.
    ///
    /// `content_extensions` are lower-case extensions without a leading dot. The
    /// caller is a curated constant table in this module, which is why the values
    /// are static and why creating a system performs no validation.
    #[must_use]
    pub const fn new(
        key: EmulatedSystemKey,
        display_name: &'static str,
        content_extensions: &'static [&'static str],
        core: CoreComponentId,
    ) -> Self {
        Self {
            key,
            display_name,
            content_extensions,
            core,
        }
    }

    /// Returns the stable key of this system.
    #[must_use]
    pub const fn key(&self) -> &EmulatedSystemKey {
        &self.key
    }

    /// Returns the name the product shows for this system.
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        self.display_name
    }

    /// Returns the content extensions that select this system.
    #[must_use]
    pub const fn content_extensions(&self) -> &'static [&'static str] {
        self.content_extensions
    }

    /// Returns the curated core component that runs this system.
    #[must_use]
    pub const fn core_component(&self) -> &CoreComponentId {
        &self.core
    }
}

/// The curated list of supported systems.
///
/// The catalogue is a fixed table, constructed once, and read-only afterwards. It
/// is not a registry: a system cannot be added at run time, and nothing here reads
/// the filesystem, the network, or a configuration file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EmulatedSystemCatalog {
    systems: Vec<EmulatedSystem>,
}

impl EmulatedSystemCatalog {
    /// Creates a catalogue from an ordered list of systems.
    ///
    /// The order is significant: [`system_for_content`](Self::system_for_content)
    /// answers with the first system whose format matches, so the order decides
    /// which system wins if two entries ever accepted the same format.
    #[must_use]
    pub fn new(systems: Vec<EmulatedSystem>) -> Self {
        Self { systems }
    }

    /// Returns every curated system, in catalogue order.
    #[must_use]
    pub fn systems(&self) -> &[EmulatedSystem] {
        &self.systems
    }

    /// Returns the curated system with `key`, if the catalogue has one.
    #[must_use]
    pub fn system(&self, key: &EmulatedSystemKey) -> Option<&EmulatedSystem> {
        self.systems.iter().find(|system| system.key() == key)
    }

    /// Returns the system whose content format `content` has, if any.
    ///
    /// Only the extension decides. The comparison is case-insensitive, because a
    /// `.GBA` file is the same format as a `.gba` file, but nothing else about the
    /// path is interpreted: a directory name, a file stem, an archive suffix, and
    /// a missing extension all select no system.
    #[must_use]
    pub fn system_for_content(&self, content: &Path) -> Option<&EmulatedSystem> {
        let extension = content.extension()?.to_str()?.to_ascii_lowercase();

        self.systems.iter().find(|system| {
            system
                .content_extensions()
                .iter()
                .any(|accepted| *accepted == extension)
        })
    }
}

/// Returns the curated system list BitArchive ships.
///
/// Order matters for the format lookup; the systems are listed from the most
/// specific format to the least specific one. Game Boy and Game Boy Color are
/// deliberately separate systems even though one core serves both: they are
/// different hardware, and merging them would be a product statement this Issue
/// is not allowed to make.
#[must_use]
pub fn curated_systems() -> EmulatedSystemCatalog {
    EmulatedSystemCatalog::new(vec![
        EmulatedSystem::new(
            EmulatedSystemKey::from_str(EmulatedSystemKey::GAME_BOY_ADVANCE)
                .expect("the curated system key is valid"),
            "Game Boy Advance",
            &["gba"],
            CoreComponentId::from_str(CoreComponentId::MGBA)
                .expect("the curated core component identity is valid"),
        ),
        EmulatedSystem::new(
            EmulatedSystemKey::from_str(EmulatedSystemKey::GAME_BOY_COLOR)
                .expect("the curated system key is valid"),
            "Game Boy Color",
            &["gbc"],
            CoreComponentId::from_str(CoreComponentId::MGBA)
                .expect("the curated core component identity is valid"),
        ),
        EmulatedSystem::new(
            EmulatedSystemKey::from_str(EmulatedSystemKey::GAME_BOY)
                .expect("the curated system key is valid"),
            "Game Boy",
            &["gb"],
            CoreComponentId::from_str(CoreComponentId::MGBA)
                .expect("the curated core component identity is valid"),
        ),
    ])
}

/// A curated core of a supported system, or why there is none.
///
/// Exactly one of the two variants holds, so "there is a core" and "there is a reason
/// there is none" cannot contradict each other.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SystemCore {
    /// The curated definition of the core that runs the system.
    ///
    /// Boxed because a definition is a large value and the other variant is tiny; an
    /// unboxed definition would make every `SystemCore` — including the one that
    /// carries no definition at all — as large as the whole pin.
    Curated(Box<CoreDefinition>),
    /// No curated core can run this system here.
    Unavailable {
        /// Why no definition was available.
        reason: CoreUnavailableReason,
    },
}

/// Why a system has no usable curated core.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CoreUnavailableReason {
    /// The system is not in BitArchive's curated system list at all.
    ///
    /// Custom systems are not part of the MVP (PRODUCT.md §13), so nothing is
    /// substituted for one: there is no component identity to ask for, because no
    /// curated system names one.
    SystemNotCurated,
    /// The system is supported in principle, but this build of BitArchive has no
    /// reviewed core definition for it on this platform.
    ///
    /// The official build host publishes macOS core binaries separately per
    /// architecture, so a platform without a curated artifact cannot be served by
    /// another architecture's binary.
    CoreNotCuratedForPlatform {
        /// The component identity the system asks for.
        component_id: CoreComponentId,
        /// The platform no curated definition exists for.
        platform: CorePlatform,
    },
}

impl fmt::Display for CoreUnavailableReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SystemNotCurated => write!(
                f,
                "BitArchive's curated system list does not contain this system, and custom \
                 systems are not supported"
            ),
            Self::CoreNotCuratedForPlatform {
                component_id,
                platform,
            } => write!(
                f,
                "BitArchive curates no core component {} for {}, and no other architecture's \
                 build is substituted for it",
                component_id.as_str(),
                platform
            ),
        }
    }
}

impl std::error::Error for CoreUnavailableReason {}

/// Resolves the curated core that runs `system` on `platform`.
///
/// The resolution is a lookup in the curated allowlist and nothing else: the system
/// names one core component, and
/// [`curated_core`] answers whether BitArchive has
/// a reviewed definition for that component on this platform. There is no catalogue
/// search, no scoring, no "best match", and no alternative core — a system has exactly
/// one curated core until an Issue introduces more.
///
/// The system is expected to come from [`curated_systems`]; a system outside that list
/// resolves to [`CoreUnavailableReason::SystemNotCurated`] rather than to a core.
#[must_use]
pub fn resolve_system_core(system: &EmulatedSystem, platform: CorePlatform) -> SystemCore {
    let component_id = system.core_component().clone();

    if !curated_systems().system(system.key()).is_some() {
        return SystemCore::Unavailable {
            reason: CoreUnavailableReason::SystemNotCurated,
        };
    }

    match curated_core(&component_id, platform) {
        Ok(definition) => SystemCore::Curated(Box::new(definition)),
        Err(_) => SystemCore::Unavailable {
            reason: CoreUnavailableReason::CoreNotCuratedForPlatform {
                component_id,
                platform,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The curated key of mGBA, which every curated system names.
    fn mgba() -> CoreComponentId {
        CoreComponentId::from_str(CoreComponentId::MGBA).expect("the curated component identity")
    }

    /// The catalogue lists exactly the systems a curated core can run, and every
    /// one of them names that core.
    #[test]
    fn every_curated_system_names_the_curated_core() {
        let catalog = curated_systems();

        assert_eq!(
            catalog.systems().len(),
            3,
            "the curated list covers the three systems the one curated core serves"
        );

        for system in catalog.systems() {
            assert_eq!(
                system.core_component(),
                &mgba(),
                "{} must name the curated core",
                system.key()
            );
            assert!(
                !system.content_extensions().is_empty(),
                "{} must accept at least one content format",
                system.key()
            );
        }
    }

    /// Every curated constant key resolves in the curated catalogue, so the
    /// constants and the table cannot drift apart.
    #[test]
    fn every_curated_key_resolves_in_the_catalogue() {
        let catalog = curated_systems();

        let cases = [
            (EmulatedSystemKey::GAME_BOY_ADVANCE, "Game Boy Advance"),
            (EmulatedSystemKey::GAME_BOY_COLOR, "Game Boy Color"),
            (EmulatedSystemKey::GAME_BOY, "Game Boy"),
        ];

        for (key, display_name) in cases {
            let key = EmulatedSystemKey::from_str(key).expect("the curated key is valid");

            let system = catalog
                .system(&key)
                .unwrap_or_else(|| panic!("{key} must be in the curated catalogue"));

            assert_eq!(system.display_name(), display_name);
            assert_eq!(system.key(), &key);
        }
    }

    /// An unknown system key resolves to nothing instead of a default system, so
    /// a stored key BitArchive does not know cannot silently select content.
    #[test]
    fn an_unknown_system_key_selects_no_system() {
        let catalog = curated_systems();
        let unknown = EmulatedSystemKey::from_str("nes").expect("a syntactically valid key");

        assert_eq!(catalog.system(&unknown), None);
    }

    /// Each curated content format selects its own system, so the extension is
    /// what decides.
    #[test]
    fn content_formats_select_their_system() {
        let catalog = curated_systems();

        let cases = [
            (
                "/library/advance-wars.gba",
                EmulatedSystemKey::GAME_BOY_ADVANCE,
            ),
            (
                "/library/links-awakening.gbc",
                EmulatedSystemKey::GAME_BOY_COLOR,
            ),
            ("/library/tetris.gb", EmulatedSystemKey::GAME_BOY),
        ];

        for (content, expected) in cases {
            let expected = EmulatedSystemKey::from_str(expected).expect("the curated key");

            let system = catalog
                .system_for_content(Path::new(content))
                .unwrap_or_else(|| panic!("{content} must select a system"));

            assert_eq!(system.key(), &expected);
        }
    }

    /// A format that no curated system accepts selects no system, and neither does
    /// a file without an extension.
    #[test]
    fn unknown_formats_select_no_system() {
        let catalog = curated_systems();

        for content in [
            "/library/game.nes",
            "/library/disc.cue",
            "/library/archive.zip",
            "/library/no-extension",
            "/library/trailing-dot.",
            "/library/",
        ] {
            assert_eq!(
                catalog.system_for_content(Path::new(content)),
                None,
                "{content} must not select a curated system"
            );
        }
    }

    /// The extension comparison is case-insensitive, because a `.GBA` file is the
    /// same format as a `.gba` file.
    #[test]
    fn the_content_extension_comparison_ignores_case() {
        let catalog = curated_systems();
        let expected = EmulatedSystemKey::from_str(EmulatedSystemKey::GAME_BOY_ADVANCE)
            .expect("the curated key");

        for content in [
            "/library/game.GBA",
            "/library/game.Gba",
            "/library/game.gBa",
        ] {
            let system = catalog
                .system_for_content(Path::new(content))
                .unwrap_or_else(|| panic!("{content} must select a system"));

            assert_eq!(system.key(), &expected);
        }
    }

    /// Only the extension decides: a system name in a directory or in a file stem
    /// selects nothing by itself.
    #[test]
    fn only_the_extension_decides() {
        let catalog = curated_systems();
        let game_boy =
            EmulatedSystemKey::from_str(EmulatedSystemKey::GAME_BOY).expect("the curated key");

        assert_eq!(
            catalog
                .system_for_content(Path::new("/gba/game.rom"))
                .map(EmulatedSystem::key),
            None,
            "a directory named after a system is not evidence for one"
        );
        assert_eq!(
            catalog
                .system_for_content(Path::new("/library/gba"))
                .map(EmulatedSystem::key),
            None,
            "a file whose name is a system key has no system format"
        );

        let system = catalog
            .system_for_content(Path::new("/library/gba.gb"))
            .expect("the extension decides");
        assert_eq!(system.key(), &game_boy);

        assert_eq!(
            catalog.system_for_content(Path::new("/gba/tetris.gb")),
            catalog.system_for_content(Path::new("/elsewhere/other.gb")),
            "the directory never influences the selected system"
        );
    }

    /// A supported system resolves to the curated definition of its core.
    #[test]
    fn a_supported_system_resolves_its_curated_core() {
        let catalog = curated_systems();
        let key = EmulatedSystemKey::from_str(EmulatedSystemKey::GAME_BOY_ADVANCE)
            .expect("the curated key");
        let system = catalog.system(&key).expect("the system is curated");

        let resolved = resolve_system_core(system, CorePlatform::MacOsArm64);

        match resolved {
            SystemCore::Curated(definition) => {
                assert_eq!(definition.component_id(), &mgba());
                assert_eq!(definition.platform(), CorePlatform::MacOsArm64);
            }
            SystemCore::Unavailable { reason, .. } => {
                panic!("the curated allowlist must cover {key}: {reason:?}")
            }
        }
    }

    /// The resolution is per platform: the same system resolves to the definition
    /// built for the requested platform and never to another architecture's
    /// binary.
    #[test]
    fn the_resolved_definition_matches_the_requested_platform() {
        let catalog = curated_systems();
        let key =
            EmulatedSystemKey::from_str(EmulatedSystemKey::GAME_BOY).expect("the curated key");
        let system = catalog.system(&key).expect("the system is curated");

        for platform in CorePlatform::ALL {
            match resolve_system_core(system, platform) {
                SystemCore::Curated(definition) => {
                    assert_eq!(definition.platform(), platform);
                }
                SystemCore::Unavailable { reason, .. } => {
                    panic!("the curated allowlist must cover {platform}: {reason:?}")
                }
            }
        }
    }

    /// A system whose core is not curated for the platform says so, and names the
    /// component and the platform instead of reporting a generic failure.
    #[test]
    fn a_system_without_a_curated_core_says_why() {
        let component_id =
            CoreComponentId::from_str("not-curated").expect("a syntactically valid component");
        let system = EmulatedSystem::new(
            EmulatedSystemKey::from_str("gba").expect("a curated key"),
            "Game Boy Advance",
            &["gba"],
            component_id.clone(),
        );

        // The catalogue is asked for a component it does not curate by resolving a
        // definition whose core is not in the allowlist.
        let uncurated = EmulatedSystem::new(
            system.key().clone(),
            system.display_name(),
            system.content_extensions(),
            component_id.clone(),
        );

        let resolved = match resolve_system_core(&uncurated, CorePlatform::MacOsArm64) {
            SystemCore::Unavailable { reason } => reason,
            SystemCore::Curated(_) => panic!("the allowlist cannot have this definition"),
        };

        assert_eq!(
            resolved,
            CoreUnavailableReason::CoreNotCuratedForPlatform {
                component_id,
                platform: CorePlatform::MacOsArm64
            }
        );
        assert!(!resolved.to_string().is_empty());
    }

    /// A system that is not in the curated list has no core to ask for, and says that
    /// instead of naming a component BitArchive does not curate.
    #[test]
    fn a_system_outside_the_curated_list_has_no_core_to_ask_for() {
        let system = EmulatedSystem::new(
            EmulatedSystemKey::from_str("custom").expect("a syntactically valid key"),
            "Custom",
            &["custom"],
            CoreComponentId::from_str(CoreComponentId::MGBA).expect("a curated component"),
        );

        assert_eq!(
            resolve_system_core(&system, CorePlatform::MacOsArm64),
            SystemCore::Unavailable {
                reason: CoreUnavailableReason::SystemNotCurated
            }
        );
    }

    /// A system key is a safe single path component, so it can never escape a
    /// store path or smuggle a nested path into one.
    #[test]
    fn a_system_key_is_one_safe_path_component() {
        assert!(EmulatedSystemKey::from_str("").is_err());
        assert!(EmulatedSystemKey::from_str("../escape").is_err());
        assert!(EmulatedSystemKey::from_str("game boy").is_err());
        assert!(EmulatedSystemKey::from_str("GBA").is_err());
        assert!(EmulatedSystemKey::from_str("1gba").is_err());
        assert!(EmulatedSystemKey::from_str("gba/extra").is_err());

        assert_eq!(
            EmulatedSystemKey::from_str("game-boy-2")
                .expect("lower-case letters, digits, and hyphens are accepted")
                .as_str(),
            "game-boy-2"
        );
    }
}
