//! The application contract for a launch wish.

use bitarchive_domain::GameId;

/// What the user asked BitArchive to do with a game.
///
/// The action is part of the launch request because `Play` and `Continue` share
/// the whole upstream resolution path and differ only in how the session
/// starts (PRODUCT.md §7, ARCHITECTURE.md §20).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LaunchAction {
    /// Start the game normally.
    Play,
    /// Resume the game from a save state.
    ///
    /// Which save state that is belongs to a later resolution step and is not
    /// part of this contract yet.
    Continue,
}

/// A request to launch a game.
///
/// A request names the game and the desired action. It deliberately carries no
/// release, content, core, runtime, configuration, or path: those are the
/// results of the resolution steps that run *after* a request exists
/// (ARCHITECTURE.md §20.1).
///
/// ```text
/// Play     → normal start
/// Continue → resume from the last compatible save state
/// ```
///
/// The request is an owned value. Fields stay private, so a request can only be
/// created through the constructors and never mutated afterwards.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LaunchRequest {
    game_id: GameId,
    action: LaunchAction,
}

impl LaunchRequest {
    /// Creates a request to launch `game_id` with `action`.
    #[must_use]
    pub const fn new(game_id: GameId, action: LaunchAction) -> Self {
        Self { game_id, action }
    }

    /// Creates a request to start `game_id` normally.
    #[must_use]
    pub const fn play(game_id: GameId) -> Self {
        Self::new(game_id, LaunchAction::Play)
    }

    /// Creates a request to resume `game_id` from a save state.
    #[must_use]
    pub const fn continue_game(game_id: GameId) -> Self {
        Self::new(game_id, LaunchAction::Continue)
    }

    /// Returns the game this request wants to launch.
    #[must_use]
    pub const fn game_id(&self) -> GameId {
        self.game_id
    }

    /// Returns the requested action.
    #[must_use]
    pub const fn action(&self) -> LaunchAction {
        self.action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request keeps the exact game it was created for. The identity is never
    /// replaced by a name, a path, or a newly generated identity.
    #[test]
    fn a_request_keeps_the_game_it_was_created_for() {
        let game = GameId::new();

        assert_eq!(LaunchRequest::play(game).game_id(), game);
        assert_eq!(LaunchRequest::continue_game(game).game_id(), game);
        assert_eq!(LaunchRequest::new(game, LaunchAction::Play).game_id(), game);
        assert_ne!(LaunchRequest::play(game).game_id(), GameId::new());
    }

    /// `Play` and `Continue` stay distinguishable for the same game, so a later
    /// resolution step can tell a normal start from a resume.
    #[test]
    fn play_and_continue_are_different_requests_for_the_same_game() {
        let game = GameId::new();

        let play = LaunchRequest::play(game);
        let resume = LaunchRequest::continue_game(game);

        assert_eq!(play.action(), LaunchAction::Play);
        assert_eq!(resume.action(), LaunchAction::Continue);
        assert_ne!(play.action(), resume.action());
        assert_ne!(play, resume, "the action distinguishes the two requests");
    }

    /// Requests for different games stay distinguishable even when the
    /// requested action is the same.
    #[test]
    fn requests_for_different_games_stay_distinguishable() {
        let first = LaunchRequest::play(GameId::new());
        let second = LaunchRequest::play(GameId::new());

        assert_eq!(first.action(), second.action());
        assert_ne!(first, second, "the game distinguishes the two requests");
    }
}
