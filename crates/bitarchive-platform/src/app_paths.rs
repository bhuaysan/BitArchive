//! The central application path service.
//!
//! BitArchive keeps its files in one known place, and this module is the single
//! authority on where that is (ARCHITECTURE.md §5.5, §35, decision 27). Nothing
//! else concatenates a home directory, and nothing else decides what
//! `components` or `staging` is called.
//!
//! macOS:
//!
//! ```text
//! ~/Library/Application Support/BitArchive/     ← persistent
//! ├── database/
//! ├── components/        ← the component store root
//! │   └── runtime/<platform>/<version>/
//! ├── media/
//! ├── generated/
//! ├── sessions/
//! ├── logs/
//! └── state/
//!
//! ~/Library/Caches/BitArchive/                  ← rebuildable
//! ├── downloads/
//! ├── staging/
//! ├── extracted/
//! └── images/
//! ```
//!
//! # Injected roots, not a discovered home
//!
//! [`AppPaths`] is constructed from two roots rather than reaching for
//! [`std::env::var`] itself. The composition root resolves them once at startup
//! (ARCHITECTURE.md §6.1 step 1), and a test constructs the same type over a
//! temporary directory (ARCHITECTURE.md §45.2). That is what keeps a test from
//! touching a real user's component store, and it is why this type needs no
//! environment variable and no injection hook.
//!
//! # No directories are created here
//!
//! Every accessor is a pure path computation. Creating a directory is the job of
//! the component that is about to write into it, so asking where something lives
//! never has a side effect.

use std::path::{Path, PathBuf};

/// The directory name of the application inside the operating system's
/// application-support and cache locations.
pub const APPLICATION_DIRECTORY_NAME: &str = "BitArchive";

/// The directory below the application support root that holds user data.
const DATABASE_DIRECTORY: &str = "database";

/// The directory below the application support root that holds managed
/// components.
const COMPONENTS_DIRECTORY: &str = "components";

/// The directory below the application support root that holds managed media.
const MEDIA_DIRECTORY: &str = "media";

/// The directory below the application support root that holds user firmware.
const FIRMWARE_DIRECTORY: &str = "firmware";

/// The directory below the application support root that holds generated files.
const GENERATED_DIRECTORY: &str = "generated";

/// The directory below the application support root that holds session
/// artifacts.
const SESSIONS_DIRECTORY: &str = "sessions";

/// The directory below the application support root that holds logs.
const LOGS_DIRECTORY: &str = "logs";

/// The directory below the application support root that holds application
/// state.
const STATE_DIRECTORY: &str = "state";

/// The directory below the cache root that holds downloaded artifacts.
const DOWNLOADS_DIRECTORY: &str = "downloads";

/// The directory below the cache root that holds transient staging state.
const STAGING_DIRECTORY: &str = "staging";

/// The directory below the cache root that holds extracted payloads.
const EXTRACTED_DIRECTORY: &str = "extracted";

/// The directory below the cache root that holds derived images.
const IMAGES_DIRECTORY: &str = "images";

/// The platform locations BitArchive keeps its data in.
///
/// The two roots are the only inputs: everything else is a fixed directory name
/// below one of them, so a path can never be assembled from an arbitrary string
/// somewhere else in the codebase.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AppPaths {
    application_support: PathBuf,
    caches: PathBuf,
}

impl AppPaths {
    /// Creates the paths from an application-support root and a cache root.
    #[must_use]
    pub fn new(application_support: impl Into<PathBuf>, caches: impl Into<PathBuf>) -> Self {
        Self {
            application_support: application_support.into(),
            caches: caches.into(),
        }
    }

    /// Creates the paths below one root, for a test or a portable development
    /// setup.
    ///
    /// `root` takes the place of the operating system's application-support
    /// directory and `root/cache` the place of its cache directory, so the whole
    /// layout is reproduced inside one directory that can be removed afterwards.
    #[must_use]
    pub fn below(root: impl Into<PathBuf>) -> Self {
        let root: PathBuf = root.into();

        Self {
            caches: root.join("cache"),
            application_support: root,
        }
    }

    /// Returns the application-support root.
    #[must_use]
    pub fn application_support(&self) -> &Path {
        &self.application_support
    }

    /// Returns the cache root.
    #[must_use]
    pub fn caches(&self) -> &Path {
        &self.caches
    }

    /// Returns the directory that holds the SQLite database.
    #[must_use]
    pub fn database(&self) -> PathBuf {
        self.application_support.join(DATABASE_DIRECTORY)
    }

    /// Returns the component store root.
    ///
    /// This is the root the runtime store is constructed from; the store decides
    /// the layout below it (ARCHITECTURE.md §23.1).
    #[must_use]
    pub fn components(&self) -> PathBuf {
        self.application_support.join(COMPONENTS_DIRECTORY)
    }

    /// Returns the managed media store root.
    #[must_use]
    pub fn media(&self) -> PathBuf {
        self.application_support.join(MEDIA_DIRECTORY)
    }

    /// Returns the firmware root.
    ///
    /// This is the one folder the user's firmware files are read from
    /// (ARCHITECTURE.md §25). BitArchive only ever *reads* it: firmware is a
    /// user-owned external resource, so nothing here copies, renames, repairs, or
    /// deletes a file in it, and asking for the path creates nothing.
    #[must_use]
    pub fn firmware(&self) -> PathBuf {
        self.application_support.join(FIRMWARE_DIRECTORY)
    }

    /// Returns the generated-files root.
    #[must_use]
    pub fn generated(&self) -> PathBuf {
        self.application_support.join(GENERATED_DIRECTORY)
    }

    /// Returns the session-artifacts root.
    #[must_use]
    pub fn sessions(&self) -> PathBuf {
        self.application_support.join(SESSIONS_DIRECTORY)
    }

    /// Returns the log root.
    #[must_use]
    pub fn logs(&self) -> PathBuf {
        self.application_support.join(LOGS_DIRECTORY)
    }

    /// Returns the application-state root.
    #[must_use]
    pub fn state(&self) -> PathBuf {
        self.application_support.join(STATE_DIRECTORY)
    }

    /// Returns the download root for artifacts.
    #[must_use]
    pub fn downloads(&self) -> PathBuf {
        self.caches.join(DOWNLOADS_DIRECTORY)
    }

    /// Returns the staging root.
    #[must_use]
    pub fn staging(&self) -> PathBuf {
        self.caches.join(STAGING_DIRECTORY)
    }

    /// Returns the extraction root.
    #[must_use]
    pub fn extracted(&self) -> PathBuf {
        self.caches.join(EXTRACTED_DIRECTORY)
    }

    /// Returns the derived-image cache root.
    #[must_use]
    pub fn images(&self) -> PathBuf {
        self.caches.join(IMAGES_DIRECTORY)
    }
}

impl Default for AppPaths {
    /// Returns the standard locations for this platform.
    ///
    /// macOS: `~/Library/Application Support/BitArchive` and
    /// `~/Library/Caches/BitArchive`. When `HOME` is not set — which a normal
    /// desktop launch always has, but a stripped-down environment might not — the
    /// paths fall back to a relative location instead of panicking, so a missing
    /// environment variable cannot crash the application before it can report
    /// anything.
    fn default() -> Self {
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);

        Self {
            application_support: home
                .join("Library/Application Support")
                .join(APPLICATION_DIRECTORY_NAME),
            caches: home.join("Library/Caches").join(APPLICATION_DIRECTORY_NAME),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout below one root reproduces the documented structure, so a test
    /// can exercise the real directory names inside a temporary directory.
    #[test]
    fn the_layout_below_one_root_matches_the_documented_structure() {
        let paths = AppPaths::below("/tmp/bitarchive-app-paths");

        assert_eq!(
            paths.application_support(),
            Path::new("/tmp/bitarchive-app-paths")
        );
        assert_eq!(paths.caches(), Path::new("/tmp/bitarchive-app-paths/cache"));
        assert_eq!(
            paths.components(),
            Path::new("/tmp/bitarchive-app-paths/components")
        );
        assert_eq!(
            paths.database(),
            Path::new("/tmp/bitarchive-app-paths/database")
        );
        assert_eq!(paths.media(), Path::new("/tmp/bitarchive-app-paths/media"));
        assert_eq!(
            paths.firmware(),
            Path::new("/tmp/bitarchive-app-paths/firmware")
        );
        assert_eq!(
            paths.generated(),
            Path::new("/tmp/bitarchive-app-paths/generated")
        );
        assert_eq!(
            paths.sessions(),
            Path::new("/tmp/bitarchive-app-paths/sessions")
        );
        assert_eq!(paths.logs(), Path::new("/tmp/bitarchive-app-paths/logs"));
        assert_eq!(paths.state(), Path::new("/tmp/bitarchive-app-paths/state"));
        assert_eq!(
            paths.downloads(),
            Path::new("/tmp/bitarchive-app-paths/cache/downloads")
        );
        assert_eq!(
            paths.staging(),
            Path::new("/tmp/bitarchive-app-paths/cache/staging")
        );
        assert_eq!(
            paths.extracted(),
            Path::new("/tmp/bitarchive-app-paths/cache/extracted")
        );
        assert_eq!(
            paths.images(),
            Path::new("/tmp/bitarchive-app-paths/cache/images")
        );
    }

    /// Persistent data and rebuildable data live in different roots, so clearing
    /// a cache can never remove an installed component (ARCHITECTURE.md §35.1).
    #[test]
    fn persistent_and_rebuildable_paths_live_in_different_roots() {
        let paths = AppPaths::new(
            "/tmp/bitarchive-separation/application-support",
            "/tmp/bitarchive-separation/caches",
        );

        assert!(paths.components().starts_with(paths.application_support()));
        assert!(!paths.components().starts_with(paths.caches()));

        for rebuildable in [paths.downloads(), paths.staging(), paths.extracted()] {
            assert!(rebuildable.starts_with(paths.caches()));
            assert!(!rebuildable.starts_with(paths.application_support()));
        }

        // The standard locations keep the same separation.
        let standard = AppPaths::default();

        assert!(!standard.components().starts_with(standard.caches()));
        assert!(
            !standard
                .downloads()
                .starts_with(standard.application_support())
        );
    }

    /// A path accessor only computes a path; it creates nothing, so merely asking
    /// where something lives has no side effect.
    #[test]
    fn accessors_do_not_create_directories() {
        let root =
            std::env::temp_dir().join(format!("bitarchive-b5-app-paths-{}", std::process::id()));

        let _ = std::fs::remove_dir_all(&root);

        let paths = AppPaths::below(&root);

        assert!(!paths.components().exists());
        assert!(!paths.staging().exists());
        assert!(!root.exists());
    }

    /// The standard locations follow the documented macOS layout, so a production
    /// run does not put application data somewhere the documents do not describe.
    #[test]
    fn the_default_paths_follow_the_documented_macos_layout() {
        let paths = AppPaths::default();

        assert!(
            paths
                .application_support()
                .ends_with("Library/Application Support/BitArchive"),
            "unexpected application support root: {}",
            paths.application_support().display()
        );
        assert!(
            paths.caches().ends_with("Library/Caches/BitArchive"),
            "unexpected cache root: {}",
            paths.caches().display()
        );
        assert!(
            paths.components().ends_with("BitArchive/components"),
            "the component store root is below the application support root"
        );
    }
}
