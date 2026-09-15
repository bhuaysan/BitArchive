//! A concrete release of a game.

use crate::{GameId, ReleaseId, SystemId};

/// A release: a concrete publication or variant of a [`Game`].
///
/// A release belongs to exactly one game and targets exactly one system. It
/// carries no title, region, language, release type, hash, or file path yet;
/// those are added once the Issues that need them exist (PRODUCT.md §8.2,
/// ARCHITECTURE.md §13.2).
///
/// The relationship to contents is stored on the [`Content`] side: one release
/// has many contents, and every content carries the [`ReleaseId`] it belongs
/// to.
///
/// [`Game`]: crate::Game
/// [`Content`]: crate::Content
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Release {
    id: ReleaseId,
    game: GameId,
    system: SystemId,
}

impl Release {
    /// Creates a release of `game` that targets `system`.
    #[must_use]
    pub fn new(game: GameId, system: SystemId) -> Self {
        Self {
            id: ReleaseId::new(),
            game,
            system,
        }
    }

    /// Returns the identity of this release.
    #[must_use]
    pub fn id(&self) -> ReleaseId {
        self.id
    }

    /// Returns the game this release belongs to.
    #[must_use]
    pub fn game_id(&self) -> GameId {
        self.game
    }

    /// Returns the system this release targets.
    #[must_use]
    pub fn system_id(&self) -> SystemId {
        self.system
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A release belongs to the game it was created for.
    #[test]
    fn release_keeps_the_game_it_was_created_for() {
        let game = GameId::new();
        let release = Release::new(game, SystemId::new());

        assert_eq!(release.game_id(), game);
    }

    /// A release targets the system it was created for.
    #[test]
    fn release_keeps_the_system_it_targets() {
        let system = SystemId::new();
        let release = Release::new(GameId::new(), system);

        assert_eq!(release.system_id(), system);
    }

    /// One game has many releases: several releases of the same game stay
    /// distinct entities and all of them keep pointing at that game, while each
    /// release keeps its own system.
    #[test]
    fn a_game_can_own_several_releases() {
        let game = GameId::new();

        let first = Release::new(game, SystemId::new());
        let second = Release::new(game, SystemId::new());

        assert_ne!(first.id(), second.id(), "releases are separate entities");
        assert_eq!(first.game_id(), game);
        assert_eq!(second.game_id(), game);
        assert_ne!(first.system_id(), second.system_id());
    }
}
