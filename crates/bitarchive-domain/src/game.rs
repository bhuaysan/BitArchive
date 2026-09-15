//! The logical work.

use crate::GameId;

/// A game: the logical work, independent of any concrete release.
///
/// A game owns its identity and nothing else yet. Metadata such as title,
/// description, genre, developer, or publisher is added once the metadata
/// architecture needs it (PRODUCT.md §8.1, ARCHITECTURE.md §13.1).
///
/// The relationship to releases is stored on the [`Release`] side: one game has
/// many releases, and every release carries the [`GameId`] it belongs to.
///
/// [`Release`]: crate::Release
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Game {
    id: GameId,
}

impl Game {
    /// Creates a game with a fresh identity.
    #[must_use]
    pub fn new() -> Self {
        Self { id: GameId::new() }
    }

    /// Returns the identity of this game.
    #[must_use]
    pub fn id(&self) -> GameId {
        self.id
    }
}

impl Default for Game {
    /// Creates a new game, matching [`Game::new`].
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A game is identified once and keeps that identity; two games are never
    /// the same game.
    #[test]
    fn a_game_keeps_the_identity_it_was_created_with() {
        let game = Game::new();

        assert_eq!(game.id(), game.id());
        assert_ne!(game.id(), Game::new().id());
    }
}
