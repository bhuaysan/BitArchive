//! Strongly typed domain identities.
//!
//! Every domain entity is identified by its own type, so a [`GameId`] can never
//! be passed where a [`ReleaseId`] is expected, even though all of them wrap the
//! same UUID representation (ARCHITECTURE.md §7).
//!
//! Identities are UUIDv7. [`new`](GameId::new) is the only way to create one,
//! so an identity can never wrap an arbitrary UUID; the underlying UUID is
//! readable through the explicit [`as_uuid`](GameId::as_uuid) accessor. There is
//! no `Deref`, no implicit conversion, and no infallible `Uuid` conversion —
//! reconstructing persisted identities belongs to the Issue that introduces
//! persistence and must validate the UUIDv7 invariant there.
//!
//! Paths, hashes, file names, names, and slugs are never domain identities
//! (ARCHITECTURE.md §2.3, §11).

use std::fmt;

use uuid::Uuid;

/// Declares one strong identity type around a UUIDv7.
///
/// Kept private to this module: the crate exposes the concrete identity types
/// instead of a public generic identity abstraction.
macro_rules! strong_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates a new identity from a freshly generated UUIDv7.
            ///
            /// This is the only constructor: a domain identity always wraps a
            /// UUIDv7 and can never be built from an arbitrary UUID.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Returns the underlying UUID.
            ///
            /// Read-only on purpose. Turning a UUID back into an identity is not
            /// part of the domain contract yet.
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            /// Creates a new identity, matching [`Self::new`].
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

strong_id! {
    /// Identifies a [`Game`](crate::Game), the logical work.
    GameId
}

strong_id! {
    /// Identifies a [`Release`](crate::Release), a concrete publication or
    /// variant of a game.
    ReleaseId
}

strong_id! {
    /// Identifies a [`Content`](crate::Content), an indexed content of a
    /// release.
    ContentId
}

strong_id! {
    /// Identifies a system (platform) that releases target.
    ///
    /// Only the identity exists so far. System metadata, defaults, and
    /// resolution belong to later Issues.
    SystemId
}

strong_id! {
    /// Identifies a core.
    ///
    /// Only the identity exists so far. Core resolution, overrides, versions,
    /// manifests, paths, installation, and capabilities belong to later Issues.
    CoreId
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every identity type mints UUIDv7 values with the RFC 4122 variant
    /// (ARCHITECTURE.md §7).
    #[test]
    fn new_ids_are_uuidv7() {
        fn assert_uuid_v7(uuid: Uuid) {
            assert_eq!(
                uuid.get_version(),
                Some(uuid::Version::SortRand),
                "a new identity must be a UUIDv7, but was {uuid}"
            );
            assert_eq!(
                uuid.get_variant(),
                uuid::Variant::RFC4122,
                "a new identity must use the RFC 4122 variant, but was {uuid}"
            );
        }

        assert_uuid_v7(GameId::new().as_uuid());
        assert_uuid_v7(ReleaseId::new().as_uuid());
        assert_uuid_v7(ContentId::new().as_uuid());
        assert_uuid_v7(SystemId::new().as_uuid());
        assert_uuid_v7(CoreId::new().as_uuid());
    }

    /// Separately created identities never collide, including identities of the
    /// same kind created back to back.
    #[test]
    fn separately_created_ids_differ() {
        assert_ne!(GameId::new(), GameId::new());
        assert_ne!(ReleaseId::new(), ReleaseId::new());
        assert_ne!(ContentId::new(), ContentId::new());
        assert_ne!(SystemId::new(), SystemId::new());
        assert_ne!(CoreId::new(), CoreId::new());
    }

    /// Identities behave as values: copies are one key, and separately created
    /// identities are distinct keys in hashed collections.
    #[test]
    fn identities_are_hashable_values() {
        let game = GameId::new();
        let mut games = HashSet::new();

        assert!(games.insert(game), "the first identity must be new");
        assert!(
            !games.insert(game),
            "a copy of an identity must be the same identity"
        );
        assert!(
            games.insert(GameId::new()),
            "a separately created identity must be distinct"
        );
        assert_eq!(games.len(), 2);
    }

    /// The underlying UUID is readable, and the textual forms show that same
    /// UUID instead of a private re-encoding.
    #[test]
    fn the_underlying_uuid_is_readable() {
        let release = ReleaseId::new();

        assert_eq!(
            release.as_uuid().get_version(),
            Some(uuid::Version::SortRand),
            "the readable UUID must be the UUIDv7 the identity was created with"
        );
        assert_eq!(
            release.to_string(),
            release.as_uuid().to_string(),
            "Display must render the underlying UUID"
        );
        assert_eq!(
            format!("{release:?}"),
            format!("ReleaseId({})", release.as_uuid()),
            "Debug must name the identity type and show the underlying UUID"
        );
    }
}
