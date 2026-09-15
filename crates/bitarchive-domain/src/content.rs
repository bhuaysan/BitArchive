//! An indexed content of a release.

use crate::{ContentId, ReleaseId};

/// A content: an indexed physical or in-container content of a [`Release`].
///
/// Content is not the release itself. A release can carry several contents, for
/// example the discs of a multi-disc game (PRODUCT.md §9, ARCHITECTURE.md
/// §13.3).
///
/// A content carries no path, location, hash, file name, or validation state
/// yet; those are added once the Issues that need them exist. Paths are
/// locations and hashes are fingerprints, so neither becomes a domain identity
/// (ARCHITECTURE.md §2.3, §11).
///
/// [`Release`]: crate::Release
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Content {
    id: ContentId,
    release: ReleaseId,
}

impl Content {
    /// Creates a content that belongs to `release`.
    #[must_use]
    pub fn new(release: ReleaseId) -> Self {
        Self {
            id: ContentId::new(),
            release,
        }
    }

    /// Returns the identity of this content.
    #[must_use]
    pub fn id(&self) -> ContentId {
        self.id
    }

    /// Returns the release this content belongs to.
    #[must_use]
    pub fn release_id(&self) -> ReleaseId {
        self.release
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A content belongs to the release it was created for.
    #[test]
    fn content_keeps_the_release_it_belongs_to() {
        let release = ReleaseId::new();
        let content = Content::new(release);

        assert_eq!(content.release_id(), release);
    }

    /// One release has many contents: separate contents of the same release stay
    /// distinct entities while all of them keep pointing at that release.
    #[test]
    fn a_release_can_own_several_contents() {
        let release = ReleaseId::new();

        let first = Content::new(release);
        let second = Content::new(release);

        assert_ne!(first.id(), second.id(), "contents are separate entities");
        assert_eq!(first.release_id(), release);
        assert_eq!(second.release_id(), release);
    }
}
