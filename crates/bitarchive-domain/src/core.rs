//! Core selection policy.
//!
//! Choosing a core is deterministic Fachlogik, so it lives in the domain layer
//! (ARCHITECTURE.md §5.1, §20.4). This module contains the whole rule and
//! nothing else: no persistence, no RetroArch details, no core paths, no core
//! versions, no installation state, and no global default.
//!
//! # Precedence
//!
//! A core can be configured on three scopes. The most specific existing value
//! wins (ARCHITECTURE.md §6 invariant 21, §20.4):
//!
//! ```text
//! System Default
//!       ↓
//! Game Override
//!       ↓
//! Release Override
//!
//! Release > Game > System
//! ```
//!
//! A release override is the most specific scope, so it wins over the game
//! override, which in turn wins over the system default.
//!
//! # No global default
//!
//! There is deliberately no global core default. When no scope is configured,
//! [`resolve_core`] returns [`None`], which means "not resolvable". It never
//! falls back to a hidden or built-in core.

use std::fmt;

use crate::CoreId;

/// The scope a resolved core was configured on.
///
/// The source is part of the resolution result on purpose: callers need to know
/// why a core was chosen, for example to explain an override or to show where a
/// value comes from (ARCHITECTURE.md §20.4).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CoreSelectionSource {
    /// The system default was used.
    System,
    /// A game override was used.
    Game,
    /// A release override was used.
    Release,
}

impl fmt::Display for CoreSelectionSource {
    /// Renders the scope name as it appears in the architecture documents.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::System => "System",
            Self::Game => "Game",
            Self::Release => "Release",
        };

        f.write_str(name)
    }
}

/// A successfully resolved core together with the scope that provided it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ResolvedCore {
    core_id: CoreId,
    source: CoreSelectionSource,
}

impl ResolvedCore {
    /// Creates a resolved core that came from `source`.
    #[must_use]
    pub const fn new(core_id: CoreId, source: CoreSelectionSource) -> Self {
        Self { core_id, source }
    }

    /// Returns the effective core.
    #[must_use]
    pub const fn core_id(&self) -> CoreId {
        self.core_id
    }

    /// Returns the scope the effective core was configured on.
    #[must_use]
    pub const fn source(&self) -> CoreSelectionSource {
        self.source
    }
}

/// Resolves the effective core for a launch.
///
/// The parameters are the configured values of the three core scopes:
///
/// - `system_default` — the core configured for the release's system
/// - `game_override` — the core configured for the game
/// - `release_override` — the core configured for the concrete release
///
/// The most specific configured scope wins:
///
/// ```text
/// release_override → game_override → system_default
/// ```
///
/// Returns [`None`] when no scope is configured. Without a configured core the
/// launch is not resolvable, and there is no global default to fall back to.
#[must_use]
pub fn resolve_core(
    system_default: Option<CoreId>,
    game_override: Option<CoreId>,
    release_override: Option<CoreId>,
) -> Option<ResolvedCore> {
    // Checked from the most specific scope to the least specific one, so the
    // precedence is visible in the order of the code.
    if let Some(core_id) = release_override {
        return Some(ResolvedCore::new(core_id, CoreSelectionSource::Release));
    }

    if let Some(core_id) = game_override {
        return Some(ResolvedCore::new(core_id, CoreSelectionSource::Game));
    }

    if let Some(core_id) = system_default {
        return Some(ResolvedCore::new(core_id, CoreSelectionSource::System));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only a system default is configured, so it is used and reported as the
    /// system scope.
    #[test]
    fn system_default_is_used_when_nothing_more_specific_exists() {
        let system = CoreId::new();

        let resolved = resolve_core(Some(system), None, None).expect("a system default was set");

        assert_eq!(resolved.core_id(), system);
        assert_eq!(resolved.source(), CoreSelectionSource::System);
    }

    /// A game override is more specific than the system default and wins.
    #[test]
    fn game_override_wins_over_system_default() {
        let system = CoreId::new();
        let game = CoreId::new();

        let resolved =
            resolve_core(Some(system), Some(game), None).expect("a game override was set");

        assert_eq!(resolved.core_id(), game);
        assert_eq!(resolved.source(), CoreSelectionSource::Game);
    }

    /// A release override is the most specific scope and wins over both lower
    /// scopes.
    #[test]
    fn release_override_wins_over_every_lower_scope() {
        let system = CoreId::new();
        let game = CoreId::new();
        let release = CoreId::new();

        let resolved = resolve_core(Some(system), Some(game), Some(release))
            .expect("a release override was set");

        assert_eq!(resolved.core_id(), release);
        assert_eq!(resolved.source(), CoreSelectionSource::Release);
    }

    /// A release override resolves on its own, without any lower scope
    /// configured.
    #[test]
    fn release_override_resolves_without_lower_scopes() {
        let release = CoreId::new();

        let resolved = resolve_core(None, None, Some(release)).expect("a release override was set");

        assert_eq!(resolved.core_id(), release);
        assert_eq!(resolved.source(), CoreSelectionSource::Release);
    }

    /// A game override resolves on its own, without a system default.
    #[test]
    fn game_override_resolves_without_system_default() {
        let game = CoreId::new();

        let resolved = resolve_core(None, Some(game), None).expect("a game override was set");

        assert_eq!(resolved.core_id(), game);
        assert_eq!(resolved.source(), CoreSelectionSource::Game);
    }

    /// With no scope configured the launch has no core: there is no global
    /// default and no hidden fallback core.
    #[test]
    fn nothing_configured_is_not_resolvable() {
        assert_eq!(resolve_core(None, None, None), None);
    }

    /// The reported source always names the scope that supplied the effective
    /// core, not merely the most specific scope that happens to be configured.
    ///
    /// Every case configures the same three values, so the effective core and
    /// the source must move together as the more specific override appears.
    #[test]
    fn the_reported_source_matches_the_scope_the_core_came_from() {
        let system = CoreId::new();
        let game = CoreId::new();
        let release = CoreId::new();

        // From the least to the most specific override, each case adds one more
        // configured scope.
        let cases = [
            (None, None, system, CoreSelectionSource::System),
            (None, Some(game), game, CoreSelectionSource::Game),
            (
                Some(release),
                Some(game),
                release,
                CoreSelectionSource::Release,
            ),
            (Some(release), None, release, CoreSelectionSource::Release),
        ];

        for (release_override, game_override, expected_core, expected_source) in cases {
            let resolved = resolve_core(Some(system), game_override, release_override)
                .expect("at least the system default was set");

            assert_eq!(
                resolved.core_id(),
                expected_core,
                "the effective core must be the value of the reported scope"
            );
            assert_eq!(
                resolved.source(),
                expected_source,
                "the source must name the scope the core came from"
            );
        }
    }

    /// Every source renders the scope name used by the architecture documents,
    /// so diagnostics can name the scope without inventing a second vocabulary.
    #[test]
    fn sources_render_their_scope_name() {
        assert_eq!(CoreSelectionSource::System.to_string(), "System");
        assert_eq!(CoreSelectionSource::Game.to_string(), "Game");
        assert_eq!(CoreSelectionSource::Release.to_string(), "Release");
    }
}
