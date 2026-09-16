//! Reading an application bundle out of an Apple disk image.
//!
//! The official modern macOS RetroArch distribution is a `.dmg`, and this module
//! is BitArchive's bridge to the platform's disk-image service. It implements
//! [`DiskImageExtractor`], so the installer never learns how a disk image is
//! mounted, and the platform crate never learns what an installation is.
//!
//! ```text
//! Artifact (verified .dmg)
//!       ↓   mount read-only
//! /Volumes/<image>/
//!       ├── RetroArch.app/      ← copied out
//!       └── Applications@       ← a symlink to /Applications, not copied
//!       ↓   detach, always
//! staged payload/RetroArch.app
//! ```
//!
//! # Why a mounted image and not an archive format
//!
//! The official build host publishes the macOS build as a disk image and nothing
//! else for this platform, so "extract the `.dmg`" is the only available route
//! that stays on official artifacts (Issue #19). A pure-Rust disk image reader was
//! rejected: it would mean reimplementing compression, the UDIF container, and a
//! file system for one artifact whose correctness a signature already covers.
//!
//! # No shell
//!
//! The platform's own `/usr/bin/hdiutil` is started through
//! [`std::process::Command`] with an absolute path and one argument per value.
//! Nothing is joined into a command line, no shell runs in between, and no
//! `sh -c` exists anywhere on this path (ARCHITECTURE.md §20.3, Issue #19). The
//! executable's absolute path is used rather than a `PATH` lookup, so a tampered
//! `PATH` cannot substitute a different program.
//!
//! The copy itself is done in Rust with [`std::fs`], not by a second external
//! tool. Symbolic links are reproduced as symbolic links rather than followed, so
//! the `Applications` link inside the image is copied as a link — and the bundle
//! is read out of the bundle we asked for, never from wherever a link points.
//!
//! # Mount points and cleanup
//!
//! `/Volumes` is not writable for a normal user on macOS, so the mount point is
//! created in the per-process temporary directory and only falls back to
//! `/Volumes` when that is writable. The name is unique per process and per mount,
//! so two concurrent reads never share a mount point.
//!
//! The image is always detached again — on success and on failure — and a detach
//! that fails transiently is retried before it is reported. The mount point
//! directory is removed only after the volume is gone: removing the directory of a
//! still mounted volume would leave a mount that can no longer be detached.

use std::path::{Path, PathBuf};
use std::process::Command;

use bitarchive_application::managed_runtime::{DiskImageExtractor, RuntimeStoreError};
use bitarchive_domain::runtime::{ArtifactKind, RuntimeDefinition};

/// The absolute path of the platform's disk-image tool.
///
/// An absolute path rather than a `PATH` lookup, so the tool that runs is the one
/// the operating system ships at this location.
const HDIUTIL: &str = "/usr/bin/hdiutil";

/// The name of the directory the mount point is created in.
const MOUNT_POINT_PREFIX: &str = "bitarchive-dmg-";

/// The directory a mounted image is preferred to appear in when it is writable.
///
/// On macOS `/Volumes` belongs to root and a normal user cannot create a directory
/// in it, so this is a fallback and not the primary location.
const VOLUMES: &str = "/Volumes";

/// How often a detach is attempted before it is reported as a failure.
///
/// Detaching can fail transiently while the system is still releasing the volume,
/// and a mount point that outlives an attempt is a real leak, so a short retry is
/// worth more than an immediate report.
const DETACH_ATTEMPTS: u32 = 3;

/// How long to wait between detach attempts.
const DETACH_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(250);

/// Counts mount points within this process, so two concurrent reads use different
/// directories.
static MOUNT_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Reads an application bundle out of an Apple disk image.
///
/// The extractor is stateless; every call mounts, copies, and detaches.
///
/// ```
/// use bitarchive_platform::DmgBundleExtractor;
///
/// let extractor = DmgBundleExtractor::new();
///
/// assert_eq!(extractor.mount_point_prefix(), "bitarchive-dmg-");
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DmgBundleExtractor;

impl DmgBundleExtractor {
    /// Creates the extractor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns the prefix used for the temporary mount point.
    #[must_use]
    pub const fn mount_point_prefix(&self) -> &'static str {
        MOUNT_POINT_PREFIX
    }
}

impl DiskImageExtractor for DmgBundleExtractor {
    fn extract_bundle(
        &self,
        definition: &RuntimeDefinition,
        image: &Path,
        destination: &Path,
    ) -> Result<(), RuntimeStoreError> {
        let ArtifactKind::AppleDiskImage { bundle } = definition.kind();

        if !cfg!(target_os = "macos") {
            return Err(RuntimeStoreError::DiskImageFailed {
                image: image.to_path_buf(),
                cause: String::from("Apple disk images can only be read on macOS"),
            });
        }

        let mount_point = create_mount_point(image)?;

        // From here on the volume must be detached again, on every path.
        let outcome = with_mounted_image(image, &mount_point, || {
            let source = mount_point.join(bundle);

            if !source.is_dir() {
                return Err(RuntimeStoreError::UnexpectedArtifact {
                    expected: format!("a disk image containing a {bundle} bundle"),
                    found: describe_volume(&mount_point),
                });
            }

            let target = destination.join(bundle);

            copy_tree(&source, &target).map_err(|cause| RuntimeStoreError::UnpackFailed {
                kind: definition.kind().as_str().to_owned(),
                cause,
            })
        });

        let detached = detach(image, &mount_point);

        if detached.is_ok() {
            // Only once the volume is gone. Removing the directory of a still
            // mounted volume would leave a mount point that can no longer be
            // detached.
            let _ = std::fs::remove_dir(&mount_point);
        }

        match (outcome, detached) {
            (Ok(()), Ok(())) => Ok(()),
            // A failed detach after a successful copy is reported rather than
            // hidden: a mounted volume that outlives the attempt is a real problem.
            (Ok(()), Err(cause)) => Err(cause),
            (Err(error), _) => Err(error),
        }
    }
}

/// Runs `operation` with the image at `image` mounted read-only at `mount_point`.
///
/// A mount that never happened leaves no mount point behind: on a failure the
/// directory this code created is removed again, because nothing is mounted on it
/// and the caller should not have to clean up after a rejected image.
fn with_mounted_image(
    image: &Path,
    mount_point: &Path,
    operation: impl FnOnce() -> Result<(), RuntimeStoreError>,
) -> Result<(), RuntimeStoreError> {
    let attach = Command::new(HDIUTIL)
        .arg("attach")
        .arg(image)
        .arg("-nobrowse")
        .arg("-readonly")
        .arg("-noverify")
        .arg("-noautofsck")
        .arg("-mountpoint")
        .arg(mount_point)
        .output()
        .map_err(|cause| RuntimeStoreError::DiskImageFailed {
            image: image.to_path_buf(),
            cause: format!("starting {HDIUTIL} failed: {cause}"),
        })?;

    if !attach.status.success() {
        let _ = std::fs::remove_dir(mount_point);

        return Err(RuntimeStoreError::DiskImageFailed {
            image: image.to_path_buf(),
            cause: format!(
                "mounting the image failed with {}: {}",
                attach.status,
                stderr_of(&attach.stderr)
            ),
        });
    }

    operation()
}

/// Detaches the image mounted at `mount_point`, retrying a transient failure.
///
/// `image` is only used to name the artifact in a report; the tool is addressed
/// with the mount point, because that is what is actually mounted.
fn detach(image: &Path, mount_point: &Path) -> Result<(), RuntimeStoreError> {
    let mut last = String::new();

    for attempt in 0..DETACH_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(DETACH_RETRY_DELAY);
        }

        let detach = Command::new(HDIUTIL)
            .arg("detach")
            .arg(mount_point)
            .arg("-force")
            .output()
            .map_err(|cause| RuntimeStoreError::DiskImageFailed {
                image: image.to_path_buf(),
                cause: format!("starting {HDIUTIL} failed: {cause}"),
            })?;

        if detach.status.success() {
            return Ok(());
        }

        last = format!(
            "detaching the image failed with {}: {}",
            detach.status,
            stderr_of(&detach.stderr)
        );
    }

    Err(RuntimeStoreError::DiskImageFailed {
        image: image.to_path_buf(),
        cause: last,
    })
}

/// Creates an empty directory the image can be mounted at.
///
/// The per-process temporary directory is tried first, because `/Volumes` belongs
/// to root and a normal user cannot create a directory in it. `/Volumes` stays as
/// a fallback for an environment where it is writable, since that is where a user
/// would expect to find a mounted image.
fn create_mount_point(image: &Path) -> Result<PathBuf, RuntimeStoreError> {
    let name = image
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("image"));

    let sequence = MOUNT_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let directory = format!(
        "{MOUNT_POINT_PREFIX}{}-{sequence}-{name}",
        std::process::id()
    );

    let mut refusals = Vec::new();

    for parent in [std::env::temp_dir(), PathBuf::from(VOLUMES)] {
        let candidate = parent.join(&directory);

        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(cause) if cause.kind() == std::io::ErrorKind::AlreadyExists => {
                // A leftover mount point from an interrupted run. It is only a
                // container, so it is removed and recreated rather than reused.
                if std::fs::remove_dir_all(&candidate).is_ok()
                    && std::fs::create_dir(&candidate).is_ok()
                {
                    return Ok(candidate);
                }

                refusals.push(format!("{} already exists", candidate.display()));
            }
            Err(cause) => refusals.push(format!("{}: {cause}", candidate.display())),
        }
    }

    Err(RuntimeStoreError::DiskImageFailed {
        image: image.to_path_buf(),
        cause: format!(
            "no writable mount point could be created ({})",
            refusals.join("; ")
        ),
    })
}

/// Describes the top level of a mounted volume, for an unexpected-artifact report.
fn describe_volume(mount_point: &Path) -> String {
    let Ok(entries) = std::fs::read_dir(mount_point) else {
        return String::from("a volume that could not be listed");
    };

    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .collect();

    names.sort();

    if names.is_empty() {
        String::from("an empty volume")
    } else {
        format!("a volume containing {}", names.join(", "))
    }
}

/// Renders the standard error of a tool, bounded and trimmed.
fn stderr_of(stderr: &[u8]) -> String {
    const LIMIT: usize = 512;

    let text = String::from_utf8_lossy(stderr);
    let trimmed = text.trim();

    if trimmed.len() <= LIMIT {
        return trimmed.to_owned();
    }

    let mut truncated = trimmed.chars().take(LIMIT).collect::<String>();
    truncated.push('…');

    truncated
}

/// Copies a directory tree, preserving permissions and reproducing symbolic links
/// as links.
fn copy_tree(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;

    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&from)?;

        if metadata.is_dir() {
            copy_tree(&from, &to)?;
        } else if metadata.file_type().is_symlink() {
            let destination = std::fs::read_link(&from)?;
            symlink(&destination, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }

    Ok(())
}

/// Creates a symbolic link.
#[cfg(unix)]
fn symlink(destination: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(destination, link)
}

/// Creates a symbolic link.
#[cfg(not(unix))]
fn symlink(_destination: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symbolic links are not supported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use bitarchive_domain::runtime::RuntimeParts;

    use std::str::FromStr;

    use bitarchive_domain::runtime::{
        ArtifactSource, LicenseIdentifier, RelativePath, RuntimeAttribution, RuntimeId,
        RuntimePlatform, RuntimeSource, RuntimeVersion, Sha256Digest,
    };

    use super::*;

    /// A definition for the pinned RetroArch runtime on macOS.
    fn definition() -> RuntimeDefinition {
        RuntimeDefinition::new(RuntimeParts {
            identity: (
                RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity"),
                RuntimeVersion::from_str("1.22.2").expect("a valid version"),
                RuntimePlatform::MacOsUniversal,
            ),
            artifact: (
                RuntimeSource::official(
                    ArtifactSource::new(
                        "https://buildbot.libretro.com/stable/1.22.2/apple/osx/universal/\
                                 RetroArch_Metal.dmg",
                    )
                    .expect("the official source is valid"),
                ),
                Sha256Digest::from_str(
                    "81b79121ba26d539064ae13b4d0419a120c3d165afbe656cf5f5412b15fdb434",
                )
                .expect("a valid digest"),
            ),
            layout: (
                ArtifactKind::AppleDiskImage {
                    bundle: String::from("RetroArch.app"),
                },
                RelativePath::from_str("RetroArch.app/Contents/MacOS/RetroArch")
                    .expect("a valid relative path"),
            ),
            attribution: RuntimeAttribution {
                component: String::from("RetroArch"),
                upstream_project: String::from("libretro/RetroArch"),
                upstream_url: String::from("https://github.com/libretro/RetroArch"),
                license: LicenseIdentifier::from_str(LicenseIdentifier::GPL_3_0_ONLY)
                    .expect("a valid license identifier"),
            },
        })
    }

    /// A temporary directory that removes itself.
    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(label: &str) -> Self {
            static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

            let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            let mut path = std::env::temp_dir();
            path.push(format!(
                "bitarchive-b5-dmg-{label}-{}-{sequence}",
                std::process::id()
            ));

            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a temporary directory");

            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The tool is addressed by an absolute path, so a tampered `PATH` cannot
    /// substitute a different program.
    #[test]
    fn the_disk_image_tool_is_addressed_absolutely() {
        assert!(HDIUTIL.starts_with('/'));
        assert_eq!(HDIUTIL, "/usr/bin/hdiutil");
    }

    /// The mount point is recognisable as BitArchive's own, so a later cleanup can
    /// tell its own temporary directories from the user's.
    #[test]
    fn the_mount_point_is_marked_as_bitarchive_owned() {
        let extractor = DmgBundleExtractor::new();

        assert_eq!(extractor.mount_point_prefix(), MOUNT_POINT_PREFIX);
        assert!(MOUNT_POINT_PREFIX.starts_with("bitarchive-"));
    }

    /// A non-existent image is reported as a disk-image failure with the tool's
    /// own reason, and nothing is left mounted.
    #[test]
    fn a_missing_image_is_reported_and_leaves_nothing_mounted() {
        if !cfg!(target_os = "macos") {
            return;
        }

        let root = TempRoot::new("missing-image");
        let image = root.path().join("does-not-exist.dmg");
        let destination = root.path().join("payload");
        std::fs::create_dir_all(&destination).expect("the destination exists");

        let error = DmgBundleExtractor::new()
            .extract_bundle(&definition(), &image, &destination)
            .expect_err("a missing image cannot be mounted");

        match error {
            RuntimeStoreError::DiskImageFailed {
                image: reported, ..
            } => {
                assert_eq!(reported, image);
            }
            other => panic!("expected a disk image failure, got {other}"),
        }

        assert!(
            !destination.join("RetroArch.app").exists(),
            "nothing is copied out of an image that could not be mounted"
        );

        // A failed mount leaves no mount point behind.
        let leftovers: Vec<_> = std::fs::read_dir(std::env::temp_dir())
            .expect("the temporary directory is readable")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                name.starts_with(&format!("{MOUNT_POINT_PREFIX}{}-", std::process::id()))
            })
            .collect();

        assert!(
            leftovers.is_empty(),
            "a rejected image must not leave a mount point behind: {leftovers:?}"
        );
    }

    /// A file that is not a disk image is reported rather than copied.
    #[test]
    fn a_file_that_is_not_a_disk_image_is_refused() {
        if !cfg!(target_os = "macos") {
            return;
        }

        let root = TempRoot::new("not-an-image");
        let image = root.path().join("not-an-image.dmg");
        std::fs::write(&image, b"this is not a disk image").expect("the fixture is written");

        let destination = root.path().join("payload");
        std::fs::create_dir_all(&destination).expect("the destination exists");

        let error = DmgBundleExtractor::new()
            .extract_bundle(&definition(), &image, &destination)
            .expect_err("a text file is not a disk image");

        assert!(matches!(error, RuntimeStoreError::DiskImageFailed { .. }));
        assert!(!destination.join("RetroArch.app").exists());
    }

    /// The volume description names what was actually found, so an unexpected
    /// artifact can be diagnosed.
    #[test]
    fn a_volume_description_lists_its_top_level_entries() {
        let root = TempRoot::new("describe");
        std::fs::create_dir_all(root.path().join("Some.app")).expect("a bundle");
        std::fs::create_dir_all(root.path().join("Other")).expect("a directory");
        std::fs::create_dir_all(root.path().join(".hidden")).expect("a hidden directory");

        let description = describe_volume(root.path());

        assert!(description.contains("Some.app"));
        assert!(description.contains("Other"));
        assert!(
            !description.contains(".hidden"),
            "hidden entries are noise in a diagnostic"
        );
    }

    /// A volume with nothing in it is described as empty rather than as an empty
    /// list.
    #[test]
    fn an_empty_volume_is_described_as_empty() {
        let root = TempRoot::new("describe-empty");

        assert_eq!(describe_volume(root.path()), "an empty volume");
    }

    /// A tree is copied with its structure, its file contents, and its symbolic
    /// links, and a link is reproduced as a link rather than followed.
    #[test]
    fn copying_reproduces_structure_content_and_links() {
        let root = TempRoot::new("copy-tree");
        let source = root.path().join("source");
        let target = root.path().join("target");

        std::fs::create_dir_all(source.join("RetroArch.app/Contents/MacOS"))
            .expect("the source tree");

        let executable = source.join("RetroArch.app/Contents/MacOS/RetroArch");
        std::fs::write(&executable, b"binary").expect("the executable");

        #[cfg(unix)]
        std::os::unix::fs::symlink("/Applications", source.join("Applications"))
            .expect("the source link");

        copy_tree(&source, &target).expect("the tree is copied");

        assert_eq!(
            std::fs::read(target.join("RetroArch.app/Contents/MacOS/RetroArch"))
                .expect("the executable was copied"),
            b"binary"
        );

        #[cfg(unix)]
        {
            let copied = std::fs::symlink_metadata(target.join("Applications"))
                .expect("the link was copied");

            assert!(
                copied.file_type().is_symlink(),
                "a link is reproduced as a link, never followed"
            );
            assert_eq!(
                std::fs::read_link(target.join("Applications")).expect("the link target"),
                Path::new("/Applications")
            );
        }
    }

    /// The tool's diagnostics are trimmed and bounded, so an error message cannot
    /// grow without limit.
    #[test]
    fn tool_diagnostics_are_bounded() {
        assert_eq!(
            stderr_of(b"  hdiutil: attach failed  \n"),
            "hdiutil: attach failed"
        );

        let long = vec![b'x'; 4096];
        let rendered = stderr_of(&long);

        assert!(
            rendered.len() < long.len(),
            "the rendered reason is bounded, but was {} bytes",
            rendered.len()
        );
        assert!(rendered.ends_with('…'));
    }
}
