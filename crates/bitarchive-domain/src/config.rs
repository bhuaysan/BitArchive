//! The launch configuration hierarchy.
//!
//! RetroArch settings are structured, typed BitArchive state and no `.cfg` file is
//! their source of truth (ARCHITECTURE.md §26). This module owns the resolution
//! *rule* those settings follow:
//!
//! ```text
//! Global
//!   ↓
//! System
//!   ↓
//! Game
//!
//! game > system > global
//! ```
//!
//! Only explicit overrides replace an inherited value, and the most specific
//! scope that sets a key wins.
//!
//! # What is modelled, and what is not
//!
//! A launch configuration is a set of typed key/value overrides. It is
//! deliberately not the whole RetroArch option surface: there is no
//! `SettingDefinition` registry here, no default values, no validation of what a
//! key means, and no knowledge of which scopes a concrete key allows. Those
//! belong to the settings subsystem (`ARCHITECTURE.md` §26.2), which does not
//! exist yet, and inventing them now would mean inventing a RetroArch option
//! catalogue nothing reads.
//!
//! What *is* enforced is that a key is well formed. A configuration only ever
//! contains [`ConfigKey`] and [`ConfigValue`] values, so a stored override cannot
//! silently become an unresolvable string.
//!
//! # Generated configuration files are not this
//!
//! This module produces the *effective launch configuration* — a value. The
//! `.cfg` file that a later step generates from it is an output artifact and never
//! becomes authoritative (ARCHITECTURE.md §2.6, §28).

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

/// The scope a configuration value was configured on.
///
/// The scope is part of the resolution result on purpose: a caller needs to know
/// *why* a value is effective, for example to explain an override or to show where
/// a value comes from (ARCHITECTURE.md §26.1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ConfigScope {
    /// The application-wide default.
    Global,
    /// The system the launched content belongs to.
    System,
    /// The concrete game.
    ///
    /// This is the most specific launch scope. RetroArch settings deliberately
    /// have no release scope, unlike core selection (PRODUCT.md §17.2,
    /// ARCHITECTURE.md §20.4).
    Game,
}

impl ConfigScope {
    /// Returns the scope name as the architecture documents spell it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::System => "System",
            Self::Game => "Game",
        }
    }
}

impl fmt::Display for ConfigScope {
    /// Renders the scope name as it appears in the architecture documents.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not a valid [`ConfigKey`].
///
/// The error always carries the rejected text, because an invalid key is preserved
/// for diagnosis rather than deleted (`ARCHITECTURE.md` invariant 19) and a caller
/// can only preserve what it can still name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConfigKeyError {
    key: String,
    reason: ConfigKeyRejection,
}

/// The rule a rejected key broke.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ConfigKeyRejection {
    /// The key was empty.
    Empty,
    /// The key started with something that is not a lower-case ASCII letter.
    InvalidStart(char),
    /// The key contained a character outside the accepted alphabet.
    InvalidCharacter(char),
}

impl ConfigKeyError {
    /// Returns the key text that was rejected.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }
}

impl fmt::Display for ConfigKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reason {
            ConfigKeyRejection::Empty => write!(f, "a configuration key must not be empty"),
            ConfigKeyRejection::InvalidStart(character) => write!(
                f,
                "the configuration key {:?} must start with a lower-case ASCII letter, but starts \
                 with {character:?}",
                self.key
            ),
            ConfigKeyRejection::InvalidCharacter(character) => write!(
                f,
                "the configuration key {:?} may only contain lower-case ASCII letters, digits, and \
                 '_', but contains {character:?}",
                self.key
            ),
        }
    }
}

impl std::error::Error for ConfigKeyError {}

/// The technical key of a configuration value.
///
/// The key is the exact identifier the backend's configuration file uses, and it
/// is spelled lower case, so it is neither translated nor localized. It is a
/// strong type for one reason: a stored override can then only exist in a shape
/// that cannot contain whitespace, a newline, or a path separator, which is what
/// keeps an invalid stored value from being written into a generated file later.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConfigKey(String);

impl ConfigKey {
    /// Returns the key as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConfigKey {
    /// Renders the technical key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ConfigKey {
    type Err = ConfigKeyError;

    /// Parses a technical configuration key.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigKeyError`] when the value is empty, does not start with a
    /// lower-case ASCII letter, or contains a character outside `a-z`, `0-9`, and
    /// `_`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let reject = |reason| ConfigKeyError {
            key: value.to_owned(),
            reason,
        };

        let mut characters = value.chars();

        match characters.next() {
            None => return Err(reject(ConfigKeyRejection::Empty)),
            Some(first) if !first.is_ascii_lowercase() => {
                return Err(reject(ConfigKeyRejection::InvalidStart(first)));
            }
            Some(_) => {}
        }

        if let Some(invalid) = value.chars().find(|character| {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || *character == '_')
        }) {
            return Err(reject(ConfigKeyRejection::InvalidCharacter(invalid)));
        }

        Ok(Self(value.to_owned()))
    }
}

/// The value of a configuration key.
///
/// The variants mirror `ARCHITECTURE.md` §26.2. A value is typed rather than a
/// string so that a later generator does not have to guess how to render it, and
/// so that a boolean can never be stored as the strings `"true"`, `"yes"`, and
/// `"1"` in three places.
#[derive(Clone, Debug)]
pub enum ConfigValue {
    /// A boolean switch.
    Bool(bool),
    /// A whole number.
    Integer(i64),
    /// A fractional number.
    Float(f64),
    /// One value of a known set.
    ///
    /// The set itself belongs to the settings subsystem; nothing here validates
    /// membership.
    Choice(String),
    /// Free text.
    Text(String),
}

impl PartialEq for ConfigValue {
    /// Compares two values by variant and content.
    ///
    /// [`f64`] is not [`Eq`], but a resolved configuration has to be comparable
    /// and deterministic, so two floats are compared by their bits. That makes the
    /// comparison total and `NaN == NaN` true, which is what a stored
    /// configuration needs; it does not claim anything about numeric equality.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Integer(left), Self::Integer(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => left.to_bits() == right.to_bits(),
            (Self::Choice(left), Self::Choice(right)) | (Self::Text(left), Self::Text(right)) => {
                left == right
            }
            _ => false,
        }
    }
}

impl Eq for ConfigValue {}

/// One scope's configuration values.
///
/// The entries are kept sorted by key, so the resolved result is deterministic
/// regardless of the order the values were configured in.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct LaunchConfig {
    values: BTreeMap<ConfigKey, ConfigValue>,
}

impl LaunchConfig {
    /// Creates an empty configuration.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Creates a configuration from typed entries, the last entry of a key
    /// winning.
    #[must_use]
    pub fn from_entries(entries: impl IntoIterator<Item = (ConfigKey, ConfigValue)>) -> Self {
        Self {
            values: entries.into_iter().collect(),
        }
    }

    /// Creates a configuration from raw string pairs.
    ///
    /// This is how a stored override enters the launch path. Every pair is
    /// parsed, and the first key that is not well formed is reported: a stored
    /// override is preserved for diagnosis rather than silently dropped
    /// (ARCHITECTURE.md invariant 19), so the caller decides what to do with an
    /// invalid one instead of this function discarding it.
    ///
    /// A value is accepted as [`ConfigValue::Text`]; interpreting it belongs to the
    /// settings subsystem, which knows the type of a key.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigKeyError`] for the first malformed key.
    pub fn from_pairs<K, V>(pairs: impl IntoIterator<Item = (K, V)>) -> Result<Self, ConfigKeyError>
    where
        K: AsRef<str>,
        V: Into<String>,
    {
        let mut values = BTreeMap::new();

        for (key, value) in pairs {
            let key = ConfigKey::from_str(key.as_ref())?;

            values.insert(key, ConfigValue::Text(value.into()));
        }

        Ok(Self { values })
    }

    /// Sets `key` to `value`, replacing a previous value.
    pub fn set(&mut self, key: ConfigKey, value: ConfigValue) {
        self.values.insert(key, value);
    }

    /// Returns the value of `key`, if this configuration sets one.
    #[must_use]
    pub fn get(&self, key: &ConfigKey) -> Option<&ConfigValue> {
        self.values.get(key)
    }

    /// Returns the number of configured values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether this configuration sets no value at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates over the entries in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&ConfigKey, &ConfigValue)> {
        self.values.iter()
    }
}

/// One configuration scope with the stored values configured on it.
///
/// The scope names *where* the values came from. It is not decoration: it is what the
/// resolution result reports, so a caller can explain an override without re-deriving
/// which scope a value was found in.
///
/// # Why the values stay raw
///
/// A stored override is external data, and external data can be malformed. Keeping the
/// pairs as they were stored — instead of parsing them while the scope is built —
/// means a malformed key cannot make a scope unrepresentable and cannot be dropped on
/// the way in. The resolution is the single place that decides which keys are usable,
/// and it reports the ones that are not (`ARCHITECTURE.md` invariant 19).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ScopedConfig {
    scope: ConfigScope,
    values: Vec<(ConfigKey, ConfigValue)>,
    stored: Vec<(String, String)>,
}

impl ScopedConfig {
    /// Creates a scope from already typed values.
    #[must_use]
    pub fn new(
        scope: ConfigScope,
        values: impl IntoIterator<Item = (ConfigKey, ConfigValue)>,
    ) -> Self {
        Self {
            scope,
            values: values.into_iter().collect(),
            stored: Vec::new(),
        }
    }

    /// Creates a scope from raw string pairs, as they were stored.
    ///
    /// Nothing is validated here: a key that is not usable is reported by
    /// [`resolve_launch_config`], which is also the only place that can decide what an
    /// unusable key means for the effective configuration.
    #[must_use]
    pub fn stored<K, V>(scope: ConfigScope, pairs: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: AsRef<str>,
        V: Into<String>,
    {
        Self {
            scope,
            values: Vec::new(),
            stored: pairs
                .into_iter()
                .map(|(key, value)| (key.as_ref().to_owned(), value.into()))
                .collect(),
        }
    }

    /// Returns the scope these values are configured on.
    #[must_use]
    pub const fn scope(&self) -> ConfigScope {
        self.scope
    }

    /// Returns the typed values of this scope, in the order they were supplied.
    #[must_use]
    pub fn values(&self) -> &[(ConfigKey, ConfigValue)] {
        &self.values
    }

    /// Returns the raw stored pairs of this scope, in the order they were supplied.
    #[must_use]
    pub fn stored_values(&self) -> &[(String, String)] {
        &self.stored
    }
}

/// Which scope a value came from, and which less specific scopes it replaced.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConfigTrace {
    /// The most specific scope that configured this key.
    pub source: ConfigScope,
    /// The scopes whose value for this key was not used, in the order they were
    /// applied.
    ///
    /// A conflict is reported rather than treated as an error: an intentional game
    /// override *is* a conflict with a system value, and the hierarchy resolves it.
    /// Keeping the losing scopes is what lets a caller explain a value.
    pub overridden: Vec<ConfigScope>,
}

/// The effective launch configuration and where every value came from.
///
/// Every key appears once, with the value of the most specific scope that
/// configured it. The trace is keyed by the same keys, so a caller can ask both
/// what is effective and which scope supplied it.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct EffectiveLaunchConfig {
    values: BTreeMap<ConfigKey, ConfigValue>,
    trace: BTreeMap<ConfigKey, ConfigTrace>,
}

impl EffectiveLaunchConfig {
    /// Creates an empty effective configuration.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            values: BTreeMap::new(),
            trace: BTreeMap::new(),
        }
    }

    /// Returns the effective value of `key`, if any scope configured one.
    #[must_use]
    pub fn get(&self, key: &ConfigKey) -> Option<&ConfigValue> {
        self.values.get(key)
    }

    /// Returns where the effective value of `key` came from.
    #[must_use]
    pub fn trace(&self, key: &ConfigKey) -> Option<&ConfigTrace> {
        self.trace.get(key)
    }

    /// Returns the number of effective values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether no scope configured anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates over the effective values in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&ConfigKey, &ConfigValue)> {
        self.values.iter()
    }
}

/// The effective launch configuration and the stored keys that could not be used.
///
/// The two are returned together on purpose. An unusable stored override is preserved
/// for diagnosis rather than deleted (`ARCHITECTURE.md` invariant 19), so a resolution
/// reports it *and* still resolves every remaining override: a launch decision can be
/// shown together with the stored value that needs attention.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LaunchConfigResolution {
    /// The effective configuration, built from the usable overrides.
    pub effective: EffectiveLaunchConfig,
    /// The stored keys that were rejected, in the order they were supplied.
    pub errors: Vec<ConfigKeyError>,
}

/// Resolves the effective launch configuration from scoped overrides.
///
/// The sources are applied from the least specific scope to the most specific one, so
/// a later source replaces an earlier value for the same key and a key only one scope
/// sets keeps that scope's value:
///
/// ```text
/// Global → System → Game      game > system > global
/// ```
///
/// The function is order-sensitive by contract: it applies the sources in the order
/// given, so the caller passes `Global`, then `System`, then `Game`. Passing a less
/// specific scope after a more specific one would let the less specific one win, which
/// is why the expected order is part of this contract and not inferred.
///
/// Which system and which game a `System` and a `Game` scope belong to is decided
/// *before* this function runs: the caller selects the scoped values for the concrete
/// launch, and this function only applies the precedence.
///
/// # Errors
///
/// Returns [`LaunchConfigResolution`] with a non-empty `errors` list when a stored key
/// is not well formed. The effective configuration is still resolved from the usable
/// overrides, so a caller that reports the error loses nothing.
pub fn resolve_launch_config(
    sources: impl IntoIterator<Item = ScopedConfig>,
) -> Result<EffectiveLaunchConfig, LaunchConfigResolution> {
    let mut values: BTreeMap<ConfigKey, ConfigValue> = BTreeMap::new();
    let mut trace: BTreeMap<ConfigKey, ConfigTrace> = BTreeMap::new();
    let mut errors: Vec<ConfigKeyError> = Vec::new();

    for source in sources {
        let scope = source.scope();

        // The typed values come first, then the raw stored ones, so a scope's usable
        // overrides all take part even when one stored key is rejected.
        for (key, value) in
            source
                .values()
                .iter()
                .cloned()
                .chain(source.stored_values().iter().filter_map(|(key, value)| {
                    ConfigKey::from_str(key)
                        .ok()
                        .map(|key| (key, ConfigValue::Text(value.clone())))
                }))
        {
            apply(&mut values, &mut trace, scope, key, value);
        }

        for (key, _) in source.stored_values() {
            if let Err(error) = ConfigKey::from_str(key) {
                errors.push(error);
            }
        }
    }

    let effective = EffectiveLaunchConfig { values, trace };

    if errors.is_empty() {
        Ok(effective)
    } else {
        Err(LaunchConfigResolution { effective, errors })
    }
}

/// Applies one value of `scope` to the resolution in progress.
///
/// The replaced scope keeps its place in the history and the new scope becomes the
/// source, so the trace always names the scope the effective value came from.
fn apply(
    values: &mut BTreeMap<ConfigKey, ConfigValue>,
    trace: &mut BTreeMap<ConfigKey, ConfigTrace>,
    scope: ConfigScope,
    key: ConfigKey,
    value: ConfigValue,
) {
    match trace.get_mut(&key) {
        Some(existing) => {
            existing.overridden.push(existing.source);
            existing.source = scope;
        }
        None => {
            trace.insert(
                key.clone(),
                ConfigTrace {
                    source: scope,
                    overridden: Vec::new(),
                },
            );
        }
    }

    values.insert(key, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shorthand for a configuration scope with typed values.
    fn scoped(scope: ConfigScope, entries: &[(&str, &str)]) -> ScopedConfig {
        let values = entries.iter().map(|(key, value)| {
            (
                ConfigKey::from_str(key).expect("the test keys are well formed"),
                ConfigValue::Text(String::from(*value)),
            )
        });

        ScopedConfig::new(scope, values)
    }

    /// Shorthand for a configuration scope holding raw stored pairs.
    fn stored(scope: ConfigScope, entries: &[(&str, &str)]) -> ScopedConfig {
        ScopedConfig::stored(scope, entries.iter().copied())
    }

    /// Resolves and asserts that every key was usable.
    fn resolve(sources: impl IntoIterator<Item = ScopedConfig>) -> EffectiveLaunchConfig {
        resolve_launch_config(sources).expect("the test keys are well formed")
    }

    /// Two values are equal when variant and content match, and different
    /// variants are never equal even when they render the same.
    #[test]
    fn config_values_compare_by_variant_and_content() {
        assert_eq!(ConfigValue::Bool(true), ConfigValue::Bool(true));
        assert_ne!(ConfigValue::Bool(true), ConfigValue::Bool(false));
        assert_eq!(ConfigValue::Integer(60), ConfigValue::Integer(60));
        assert_ne!(
            ConfigValue::Integer(60),
            ConfigValue::Choice(String::from("60")),
            "different variants are never the same value"
        );
        assert_eq!(ConfigValue::Float(1.5), ConfigValue::Float(1.5));
        assert_eq!(
            ConfigValue::Text(String::from("x")),
            ConfigValue::Text(String::from("x"))
        );
        assert_eq!(
            ConfigValue::Float(f64::NAN),
            ConfigValue::Float(f64::NAN),
            "a resolved configuration compares totally, which is what makes it deterministic"
        );
    }

    /// A key that cannot be stored as a technical key is rejected, and the rejected
    /// text is kept so a caller can name the stored value.
    #[test]
    fn a_malformed_config_key_is_rejected_with_its_text() {
        assert!(ConfigKey::from_str("video_vsync").is_ok());

        for malformed in [
            "",
            "Video_Vsync",
            "_leading",
            "with space",
            "with-dash",
            "with/slash",
        ] {
            let error =
                ConfigKey::from_str(malformed).expect_err("a malformed key must be rejected");

            assert_eq!(
                error.key(),
                malformed,
                "the rejected key is preserved instead of being dropped"
            );
            assert!(
                error.to_string().contains(malformed) || malformed.is_empty(),
                "the message names the stored value: {error}"
            );
        }
    }

    /// An unusable stored key is reported and preserved, and the usable overrides of
    /// the same scope still resolve.
    #[test]
    fn an_unusable_stored_key_is_reported_and_the_rest_still_resolves() {
        let resolution = resolve_launch_config([stored(
            ConfigScope::Global,
            &[("video_vsync", "true"), ("Video_Vsync", "oops")],
        )])
        .expect_err("the malformed key must be reported");

        assert_eq!(
            resolution.errors.len(),
            1,
            "every rejected key is reported exactly once"
        );
        assert_eq!(resolution.errors[0].key(), "Video_Vsync");
        assert_eq!(
            resolution.effective.len(),
            1,
            "the usable override takes part even though one stored key was rejected"
        );
        assert_eq!(
            resolution
                .effective
                .get(&ConfigKey::from_str("video_vsync").expect("a well formed key")),
            Some(&ConfigValue::Text(String::from("true")))
        );
    }

    /// Only the global scope is configured, so every value is effective and comes
    /// from `Global`.
    #[test]
    fn a_global_value_is_effective_when_nothing_overrides_it() {
        let effective = resolve([scoped(ConfigScope::Global, &[("video_vsync", "true")])]);

        let key = ConfigKey::from_str("video_vsync").expect("a well formed key");

        assert_eq!(effective.len(), 1);
        assert_eq!(
            effective.get(&key).expect("the global value"),
            &ConfigValue::Text(String::from("true"))
        );
        assert_eq!(
            effective.trace(&key).expect("a trace").source,
            ConfigScope::Global
        );
        assert!(
            effective
                .trace(&key)
                .expect("a trace")
                .overridden
                .is_empty(),
            "nothing was replaced"
        );
    }

    /// A system value replaces the global one and is reported as the source, while
    /// the replaced scope is recorded.
    #[test]
    fn a_system_value_overrides_the_global_one() {
        let effective = resolve([
            scoped(
                ConfigScope::Global,
                &[("video_vsync", "true"), ("audio_volume", "1.0")],
            ),
            scoped(ConfigScope::System, &[("video_vsync", "false")]),
        ]);

        let overridden_key = ConfigKey::from_str("video_vsync").expect("a well formed key");
        let inherited_key = ConfigKey::from_str("audio_volume").expect("a well formed key");

        assert_eq!(
            effective.get(&overridden_key).expect("the system value"),
            &ConfigValue::Text(String::from("false"))
        );
        assert_eq!(
            effective.trace(&overridden_key).expect("a trace").source,
            ConfigScope::System
        );
        assert_eq!(
            effective
                .trace(&overridden_key)
                .expect("a trace")
                .overridden,
            [ConfigScope::Global],
            "the replaced scope is still explainable"
        );
        assert_eq!(
            effective.trace(&inherited_key).expect("a trace").source,
            ConfigScope::Global,
            "a key only the global scope sets keeps its global value"
        );
    }

    /// A game value is the most specific one and wins over both lower scopes.
    #[test]
    fn a_game_value_overrides_the_system_and_the_global_value() {
        let effective = resolve([
            scoped(ConfigScope::Global, &[("video_vsync", "global")]),
            scoped(ConfigScope::System, &[("video_vsync", "system")]),
            scoped(ConfigScope::Game, &[("video_vsync", "game")]),
        ]);

        let key = ConfigKey::from_str("video_vsync").expect("a well formed key");

        assert_eq!(
            effective.get(&key).expect("the game value"),
            &ConfigValue::Text(String::from("game"))
        );
        assert_eq!(
            effective.trace(&key).expect("a trace").source,
            ConfigScope::Game
        );
        assert_eq!(
            effective.trace(&key).expect("a trace").overridden,
            [ConfigScope::Global, ConfigScope::System],
            "every replaced scope stays explainable, in application order"
        );
    }

    /// Unconfigured scopes take part without changing anything, so an empty system
    /// or game scope does not erase an inherited value.
    #[test]
    fn empty_scopes_do_not_replace_anything() {
        let effective = resolve([
            scoped(ConfigScope::Global, &[("video_vsync", "true")]),
            scoped(ConfigScope::System, &[]),
            scoped(ConfigScope::Game, &[]),
        ]);

        let key = ConfigKey::from_str("video_vsync").expect("a well formed key");

        assert_eq!(effective.len(), 1);
        assert_eq!(
            effective.trace(&key).expect("a trace").source,
            ConfigScope::Global
        );
    }

    /// No configured scope produces an empty effective configuration rather than a
    /// configuration with invented defaults.
    #[test]
    fn no_scopes_produce_an_empty_effective_configuration() {
        let effective = resolve([]);

        assert!(effective.is_empty());
        assert_eq!(effective.len(), 0);
        assert_eq!(effective.iter().count(), 0);
    }

    /// The resolution is deterministic: the entries are keyed, so the effective
    /// values are identical no matter which order the scopes list them in.
    #[test]
    fn the_effective_values_do_not_depend_on_entry_order() {
        let first = resolve([scoped(
            ConfigScope::Global,
            &[("b_key", "1"), ("a_key", "2"), ("c_key", "3")],
        )]);
        let second = resolve([scoped(
            ConfigScope::Global,
            &[("c_key", "3"), ("a_key", "2"), ("b_key", "1")],
        )]);

        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>(),
            ["a_key", "b_key", "c_key"],
            "the effective values are ordered by key"
        );
    }

    /// Raw stored values and typed values describe the same scope, so a stored
    /// override resolves exactly like the typed value it stands for.
    #[test]
    fn a_stored_scope_resolves_like_a_typed_one() {
        let from_stored = resolve([stored(ConfigScope::Global, &[("video_vsync", "true")])]);
        let from_typed = resolve([scoped(ConfigScope::Global, &[("video_vsync", "true")])]);

        assert_eq!(from_stored, from_typed);
    }

    /// Every scope renders the name the architecture documents use.
    #[test]
    fn scopes_render_their_name() {
        assert_eq!(ConfigScope::Global.to_string(), "Global");
        assert_eq!(ConfigScope::System.to_string(), "System");
        assert_eq!(ConfigScope::Game.to_string(), "Game");
    }
}
