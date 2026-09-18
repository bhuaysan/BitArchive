//! Where the component store keeps things, for both component classes.
//!
//! A runtime and a core are different classes with different identities,
//! versioning, and install lifecycles (ARCHITECTURE.md §23.4), but they share one
//! component root, one staging area, and one artifact cache. Those three
//! directories and the rules for using them are defined here once, so the runtime
//! store and the core store cannot drift apart:
//!
//! ```text
//! <component root>/
//! ├── runtime/<id>/<platform>/<version>/        ← the runtime store
//! ├── cores/<component-id>/<platform>/<build>/ ← the core store
//! ├── staging/install-<…>/                     ← transient, shared, ours alone
//! └── artifacts/<…>                            ← verified downloads, rebuildable
//! ```
//!
//! What is *not* shared is as important: there is no shared installation pipeline,
//! no common `Component` type, and no common activation. A runtime has an
//! activation record and a core must never have one, so only the vocabulary below
//! is shared (Issue #21 §5, §12).

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use bitarchive_application::managed_runtime::RuntimeStoreError;
use bitarchive_domain::component::RelativePath;

/// The directory below the component root that holds installed runtimes.
pub(crate) const RUNTIME_DIRECTORY: &str = "runtime";

/// The directory below the component root that holds installed cores.
pub(crate) const CORES_DIRECTORY: &str = "cores";

/// The directory below the component root that holds transient staging state.
pub(crate) const STAGING_DIRECTORY: &str = "staging";

/// The directory below the component root that holds downloaded artifacts.
pub(crate) const ARTIFACT_DIRECTORY: &str = "artifacts";

/// The name of the record that says which runtime version is active.
pub(crate) const ACTIVE_RECORD: &str = "active";

/// The prefix of a staging directory, so an abandoned one is recognisable.
pub(crate) const STAGING_PREFIX: &str = "install-";

/// The suffix of a record that is being replaced.
pub(crate) const TEMPORARY_SUFFIX: &str = ".tmp";

/// The name of the staging directory that holds the payload of one installation.
pub(crate) const PAYLOAD_DIRECTORY: &str = "payload";

/// Resolves a definition's relative path below `base`.
///
/// The result is derived from the pinned [`RelativePath`] alone, never from a
/// name an archive, a URL, or a response supplied: that is the whole reason the
/// domain type exists. The returned path is therefore always inside `base`,
/// because a `RelativePath` cannot be absolute and cannot contain `..`.
pub(crate) fn below(base: &Path, relative: &RelativePath) -> PathBuf {
    let mut path = base.to_path_buf();

    for component in relative.components() {
        path.push(component);
    }

    path
}

/// Returns `true` when a rename failed because the two paths are on different
/// filesystems.
pub(crate) fn is_cross_device(cause: &std::io::Error) -> bool {
    cause.raw_os_error() == Some(18) // EXDEV
}

/// Removes staging directories an interrupted run left behind.
///
/// Startup cleanup is limited to what BitArchive clearly owns (ARCHITECTURE.md
/// §35.1, invariant 20): only directories directly below the store's own staging
/// directory that carry the staging prefix are removed, so an installed version or
/// build, an artifact, and anything a user placed elsewhere are never touched.
///
/// The function is shared by both stores because there is one staging directory:
/// an abandoned runtime staging directory and an abandoned core staging directory
/// are the same kind of leftover, and either store may clear it.
pub(crate) fn cleanup_staging(staging: &Path) -> Result<usize, RuntimeStoreError> {
    let entries = match fs::read_dir(staging) {
        Ok(entries) => entries,
        Err(cause) if cause.kind() == ErrorKind::NotFound => return Ok(0),
        Err(cause) => {
            return Err(RuntimeStoreError::UnreadableStore {
                directory: staging.to_path_buf(),
                cause,
            });
        }
    };

    let mut removed = 0;

    for entry in entries {
        let entry = entry.map_err(|cause| RuntimeStoreError::UnreadableStore {
            directory: staging.to_path_buf(),
            cause,
        })?;

        let name = entry.file_name();
        let name = name.to_string_lossy();

        if !name.starts_with(STAGING_PREFIX) {
            continue;
        }

        let path = entry.path();

        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|cause| {
                RuntimeStoreError::io("removing an abandoned staging directory", &path, cause)
            })?;

            removed += 1;
        }
    }

    Ok(removed)
}
