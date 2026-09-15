//! Strongly typed domain identities.
//!
//! Every domain entity is identified by its own type, so a [`GameId`] can never
//! be passed where a [`ReleaseId`] is expected, even though all of them wrap the
//! same UUID representation (ARCHITECTURE.md §7).
//!
//! Identities are UUIDv7. The underlying UUID is only reachable through the
//! explicit [`from_uuid`](GameId::from_uuid) / [`as_uuid`](GameId::as_uuid)
//! pair; there is no `Deref` and no implicit conversion.
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
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Wraps an existing UUID.
            ///
            /// The explicit counterpart of [`Self::as_uuid`], so an identity can
            /// be reconstructed from its own representation without the UUID
            /// ever crossing the type boundary implicitly.
            #[must_use]
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Returns the underlying UUID.
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

    /// Identities behave as values: equal UUIDs are equal identities and can be
    /// used as keys in hashed collections.
    #[test]
    fn identities_are_hashable_values() {
        let game = GameId::new();
        let mut games = HashSet::new();

        assert!(games.insert(game), "the first identity must be new");
        assert!(
            !games.insert(GameId::from_uuid(game.as_uuid())),
            "the same UUID must be the same identity"
        );
        assert!(
            games.insert(GameId::new()),
            "a different UUID must be a different identity"
        );
        assert_eq!(games.len(), 2);
    }

    /// The underlying UUID is reachable in both directions explicitly, and the
    /// textual form is that same UUID rather than a private re-encoding.
    #[test]
    fn the_underlying_uuid_is_only_reachable_explicitly() {
        let release = ReleaseId::new();

        assert_eq!(ReleaseId::from_uuid(release.as_uuid()), release);

        let parsed = Uuid::parse_str(&release.to_string()).expect("Display must render a UUID");
        assert_eq!(ReleaseId::from_uuid(parsed), release);

        assert_eq!(
            format!("{release:?}"),
            format!("ReleaseId({})", release.as_uuid()),
            "Debug must name the identity type and show the underlying UUID"
        );
    }
}
