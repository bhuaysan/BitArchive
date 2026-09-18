//! Integration tests for managed component acquisition.
//!
//! This module holds the helpers both component classes' tests share — a temporary
//! component root and a fake downloader — and the runtime store tests. The core
//! store and the core archive extractor are tested in [`core_acquisition`].
//!
//! Every test here runs against a temporary component root and a fake downloader
//! and extractor, or against a loopback HTTP server. None of them downloads a real
//! RetroArch artifact or a real libretro core, so the normal test suite never
//! depends on the public network or on the official build host being reachable
//! (ARCHITECTURE.md §45.4, Issue #19, Issue #21).
//!
//! The fakes are deliberately *not* the real adapters with the network switched
//! off: they are separate implementations of the same ports, so a test can make
//! one step fail and observe what the store does with the previously active
//! runtime.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::ComponentStore;
use bitarchive_application::managed_runtime::{
    Artifact, ArtifactDownloader, ArtifactExtractor, ArtifactRequest, ArtifactSourceKind,
    InstalledRuntime, RuntimeInstaller, RuntimeStoreError,
};
use bitarchive_domain::runtime::{
    ArtifactKind, ArtifactSource, LicenseIdentifier, RelativePath, RuntimeAttribution,
    RuntimeDefinition, RuntimeId, RuntimeParts, RuntimePlatform, RuntimeSource, RuntimeVersion,
    Sha256Digest,
};

mod core_acquisition;

/// The executable path inside a modern macOS RetroArch build, as the official
/// universal artifact lays it out.
const EXECUTABLE_IN_BUNDLE: &str = "RetroArch.app/Contents/MacOS/RetroArch";

/// The bytes a fake download writes when it is asked for the "correct" artifact.
const FIXTURE: &[u8] = b"BitArchive managed runtime artifact fixture";

/// The SHA-256 of [`FIXTURE`].
///
/// The store tests use a fake downloader, so this value is not re-derived at test
/// time; it is the real digest of the fixture bytes, so the definition a test
/// installs is a plausible one.
const FIXTURE_DIGEST: &str = "9972f9716254c031b4b7f03709834096b72c00d2b2e785eb1a36ec86d74c84d2";

/// A real, temporary filesystem on a RAM disk, used to produce a cross-device
/// move on purpose.
///
/// `EXDEV` cannot be produced inside one temporary directory, which is why this
/// helper exists: a disk image is a genuinely separate filesystem, so a `rename`
/// from the store staging area to it fails with `EXDEV` exactly as it would on a
/// misconfigured installation. It is used only by the `#[ignore]`d cross-device
/// test, so the normal suite still runs entirely inside one temporary directory.
#[cfg(target_os = "macos")]
struct RamDisk {
    device: PathBuf,
    mount_point: PathBuf,
}

#[cfg(target_os = "macos")]
impl RamDisk {
    /// Creates and mounts a small RAM disk.
    fn mount(label: &str) -> Self {
        /// 8 MiB of 512-byte sectors: far more than the test payload needs.
        const SECTORS: u32 = 16_384;

        let device = run(
            "/usr/bin/hdiutil",
            &["attach", "-nomount", &format!("ram://{SECTORS}")],
        );
        let device = PathBuf::from(device.trim());

        run(
            "/sbin/newfs_hfs",
            &["-v", "BATest", &device.to_string_lossy()],
        );

        let mount_point = std::env::temp_dir().join(format!(
            "bitarchive-b5-ramdisk-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&mount_point);
        fs::create_dir_all(&mount_point).expect("a mount point directory");

        run(
            "/sbin/mount",
            &[
                "-t",
                "hfs",
                &device.to_string_lossy(),
                &mount_point.to_string_lossy(),
            ],
        );

        Self {
            device,
            mount_point,
        }
    }

    /// Returns the mounted path.
    fn path(&self) -> &Path {
        &self.mount_point
    }
}

#[cfg(target_os = "macos")]
impl Drop for RamDisk {
    fn drop(&mut self) {
        let _ = run_status("/sbin/umount", &[&self.mount_point.to_string_lossy()]);
        let _ = run_status(
            "/usr/bin/hdiutil",
            &["detach", &self.device.to_string_lossy()],
        );
        let _ = fs::remove_dir_all(&self.mount_point);
    }
}

/// Runs a tool and returns its standard output, panicking on failure.
#[cfg(target_os = "macos")]
fn run(program: &str, arguments: &[&str]) -> String {
    let output = std::process::Command::new(program)
        .args(arguments)
        .output()
        .unwrap_or_else(|cause| panic!("{program} could not be started: {cause}"));

    assert!(
        output.status.success(),
        "{program} {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );

    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Runs a tool and returns whether it succeeded.
#[cfg(target_os = "macos")]
fn run_status(program: &str, arguments: &[&str]) -> bool {
    std::process::Command::new(program)
        .args(arguments)
        .status()
        .is_ok_and(|status| status.success())
}

/// A temporary component root that removes itself.
struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "bitarchive-store-test-{label}-{}",
            std::process::id()
        ));

        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a temporary component root");

        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A downloader that writes deterministic bytes and reports a chosen digest.
///
/// The fake is what lets a test produce a digest mismatch, an empty download, or a
/// transport failure without a network.
struct FakeDownloader {
    outcome: RefCell<FakeOutcome>,
}

/// What the fake downloader should do on the next call.
#[derive(Clone)]
enum FakeOutcome {
    /// Write these bytes and report the digest the definition pinned.
    Bytes(Vec<u8>),
    /// Write these bytes but report a different digest, as a tampered artifact
    /// would.
    WrongDigest(Vec<u8>),
    /// Fail like an unreachable host.
    Unreachable,
}

impl FakeDownloader {
    fn writing(bytes: &[u8]) -> Self {
        Self {
            outcome: RefCell::new(FakeOutcome::Bytes(bytes.to_vec())),
        }
    }

    fn reporting_a_wrong_digest(bytes: &[u8]) -> Self {
        Self {
            outcome: RefCell::new(FakeOutcome::WrongDigest(bytes.to_vec())),
        }
    }

    fn unreachable() -> Self {
        Self {
            outcome: RefCell::new(FakeOutcome::Unreachable),
        }
    }
}

impl ArtifactDownloader for FakeDownloader {
    fn download(
        &self,
        request: &ArtifactRequest,
        target: &Path,
    ) -> Result<Artifact, RuntimeStoreError> {
        // The real downloader creates the artifact directory; the fake does the
        // same so that a test exercises the same store behaviour.
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("the fake creates the artifact directory");
        }

        match self.outcome.borrow().clone() {
            FakeOutcome::Unreachable => Err(RuntimeStoreError::DownloadFailed {
                url: request.source().as_str().to_owned(),
                cause: std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "the fake host is unreachable",
                ),
            }),
            FakeOutcome::WrongDigest(bytes) => {
                fs::write(target, &bytes).expect("the fake writes its bytes");

                Err(RuntimeStoreError::DigestMismatch {
                    expected: request.digest(),
                    actual: Sha256Digest::from_bytes([0xff; 32]),
                })
            }
            FakeOutcome::Bytes(bytes) => {
                fs::write(target, &bytes).expect("the fake writes its bytes");

                Ok(
                    Artifact::new(target, request.digest(), ArtifactSourceKind::Local)
                        .with_size(bytes.len() as u64),
                )
            }
        }
    }
}

/// What the fake extractor should produce.
#[derive(Clone)]
enum FakeLayout {
    /// Create the executable the definition pins.
    WithExecutable,
    /// Create a bundle without the executable.
    WithoutExecutable,
    /// Create the executable without an executable bit.
    NonExecutableFile,
    /// Report an unpack failure.
    Failing,
}

/// An extractor that materialises a tiny, deterministic payload.
struct FakeExtractor {
    layout: FakeLayout,
}

impl FakeExtractor {
    fn producing(layout: FakeLayout) -> Self {
        Self { layout }
    }
}

impl ArtifactExtractor for FakeExtractor {
    fn unpack(
        &self,
        definition: &RuntimeDefinition,
        _artifact: &Artifact,
        working_directory: &Path,
    ) -> Result<(), RuntimeStoreError> {
        let ArtifactKind::AppleDiskImage { bundle } = definition.kind();

        match self.layout {
            FakeLayout::Failing => Err(RuntimeStoreError::UnpackFailed {
                kind: definition.kind().as_str().to_owned(),
                cause: std::io::Error::other("the fake image could not be mounted"),
            }),
            FakeLayout::WithExecutable | FakeLayout::NonExecutableFile => {
                let executable = working_directory
                    .join(bundle)
                    .join("Contents/MacOS/RetroArch");

                fs::create_dir_all(executable.parent().expect("a parent directory"))
                    .expect("the fake creates the bundle");

                fs::write(&executable, b"fake retroarch").expect("the fake writes the executable");

                // A real extractor copies the bundle, and the bundle's executable
                // is executable. The fake reproduces that explicitly, because
                // `fs::write` alone produces a data file.
                let mode = if matches!(self.layout, FakeLayout::NonExecutableFile) {
                    0o644
                } else {
                    0o755
                };

                set_mode(&executable, mode);

                Ok(())
            }
            FakeLayout::WithoutExecutable => {
                let marker = working_directory.join(bundle).join("Contents/Info.plist");

                fs::create_dir_all(marker.parent().expect("a parent directory"))
                    .expect("the fake creates the bundle");

                fs::write(&marker, b"<plist/>").expect("the fake writes a plist");

                Ok(())
            }
        }
    }
}

/// Sets the permission bits of a file.
fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).expect("the file exists").permissions();
        permissions.set_mode(mode);
        fs::set_permissions(path, permissions).expect("the mode is set");
    }

    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
}

/// Parses the fixture digest.
fn fixture_digest() -> Sha256Digest {
    Sha256Digest::from_str(FIXTURE_DIGEST).expect("the fixture digest is valid")
}

/// Builds a pinned definition for RetroArch `version`, downloading from the
/// official host.
fn definition(version: &str, digest: Sha256Digest) -> RuntimeDefinition {
    RuntimeDefinition::new(RuntimeParts {
        identity: (
            RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity"),
            RuntimeVersion::from_str(version).expect("a valid version"),
            RuntimePlatform::MacOsUniversal,
        ),
        artifact: (
            RuntimeSource::official(
                ArtifactSource::new(&format!(
                    "https://buildbot.libretro.com/stable/{version}/apple/osx/universal/\
                         RetroArch_Metal.dmg"
                ))
                .expect("the official source is valid"),
            ),
            digest,
        ),
        layout: (
            ArtifactKind::AppleDiskImage {
                bundle: String::from("RetroArch.app"),
            },
            RelativePath::from_str(EXECUTABLE_IN_BUNDLE).expect("a valid relative path"),
        ),
        attribution: RuntimeAttribution {
            component: String::from("RetroArch"),
            upstream_project: String::from("libretro/RetroArch"),
            upstream_url: String::from("https://github.com/libretro/RetroArch"),
            license: LicenseIdentifier::from_str(LicenseIdentifier::GPL_3_0_OR_LATER)
                .expect("a valid license identifier"),
        },
    })
}

/// Builds a store over a fresh root with a succeeding downloader and a payload
/// that contains the pinned executable.
fn working_store(root: &TempRoot) -> ComponentStore<FakeDownloader, FakeExtractor> {
    ComponentStore::new(
        root.path(),
        FakeDownloader::writing(FIXTURE),
        FakeExtractor::producing(FakeLayout::WithExecutable),
    )
}

/// The version directory an install of `version` must create.
fn version_directory(root: &TempRoot, version: &str) -> PathBuf {
    root.path()
        .join("runtime/retroarch/macos-universal")
        .join(version)
}

// ---------------------------------------------------------------------------
// Successful acquisition
// ---------------------------------------------------------------------------

/// A successful install produces the versioned layout the architecture pins, puts
/// the executable where the definition says, and activates exactly that version.
#[test]
fn a_successful_install_produces_the_versioned_store_and_activates_the_version() {
    let root = TempRoot::new("success");
    let store = working_store(&root);
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");

    let installed = store
        .install(&definition("1.22.2", fixture_digest()))
        .expect("a valid install succeeds");

    // Versioned directory layout: components/runtime/<platform>/<version>/.
    let directory = version_directory(&root, "1.22.2");
    assert!(directory.is_dir(), "the versioned directory exists");
    assert_eq!(installed.version().as_str(), "1.22.2");
    assert_eq!(installed.directory(), directory);

    // Executable resolution: definition-relative, inside the version directory.
    let expected_executable = directory.join(EXECUTABLE_IN_BUNDLE);
    assert!(expected_executable.is_file());
    assert_eq!(installed.executable_path(), expected_executable);
    assert!(
        installed
            .executable_path()
            .starts_with(installed.directory())
    );

    // Activation.
    let active = store
        .active_runtime(&id)
        .expect("the record is readable")
        .expect("a runtime is active");

    assert_eq!(active.version().as_str(), "1.22.2");
    assert_eq!(active.executable_path(), expected_executable);

    // The installed set is derived from the store, not from a database.
    assert_eq!(
        store
            .installed_versions(&id)
            .expect("the store is readable"),
        vec![RuntimeVersion::from_str("1.22.2").expect("a valid version")]
    );

    // Staging is transient and gone.
    assert!(
        !store.staging_directory().exists()
            || fs::read_dir(store.staging_directory())
                .expect("the staging directory is readable")
                .next()
                .is_none(),
        "no staging directory survives a successful install"
    );
}

/// The artifact is downloaded into the store's own artifact directory, whose name
/// is derived from the definition and never from the remote URL.
#[test]
fn the_artifact_is_named_from_the_definition_not_from_the_url() {
    let root = TempRoot::new("artifact-name");
    let store = working_store(&root);
    let artifact = store.artifact_path(&definition("1.22.2", fixture_digest()));

    assert_eq!(
        artifact.file_name().and_then(|name| name.to_str()),
        Some("retroarch-1.22.2-macos-universal")
    );
    assert!(artifact.starts_with(root.path().join("artifacts")));

    store
        .install(&definition("1.22.2", fixture_digest()))
        .expect("a valid install succeeds");

    assert!(artifact.is_file(), "the verified artifact is kept locally");
}

/// Two versions can be installed side by side, and each keeps its own executable,
/// so an update never has to overwrite the version it replaces.
#[test]
fn two_versions_are_installed_side_by_side() {
    let root = TempRoot::new("side-by-side");
    let store = working_store(&root);
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");

    let older = store
        .install(&definition("1.22.1", fixture_digest()))
        .expect("the first version installs");
    let newer = store
        .install(&definition("1.22.2", fixture_digest()))
        .expect("the second version installs");

    assert_ne!(older.directory(), newer.directory());
    assert!(older.executable_path().is_file());
    assert!(newer.executable_path().is_file());

    assert_eq!(
        store
            .installed_versions(&id)
            .expect("the store is readable"),
        vec![
            RuntimeVersion::from_str("1.22.1").expect("a valid version"),
            RuntimeVersion::from_str("1.22.2").expect("a valid version"),
        ],
        "installed versions are listed in order"
    );

    let active = store
        .active_runtime(&id)
        .expect("the record is readable")
        .expect("a runtime is active");

    assert_eq!(
        active.version().as_str(),
        "1.22.2",
        "activation moves to the version that was installed last"
    );
    assert_eq!(
        active.directory(),
        version_directory(&root, "1.22.2"),
        "the active runtime points at the newly installed version"
    );

    // The replaced version is untouched: installed versions are immutable.
    assert!(version_directory(&root, "1.22.1").is_dir());
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/// A digest mismatch aborts the install, discards the staged state, and — most
/// importantly — leaves the previously active runtime exactly as it was.
#[test]
fn a_digest_mismatch_leaves_the_active_runtime_untouched() {
    let root = TempRoot::new("mismatch");
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");

    // A first version is installed and active.
    let good = working_store(&root);

    let active_before = good
        .install(&definition("1.22.1", fixture_digest()))
        .expect("the first install succeeds");

    let record_before = fs::read(good.active_record(&id)).expect("the record exists");

    // A second version fails verification.
    let failing: ComponentStore<_, _> = ComponentStore::new(
        root.path(),
        FakeDownloader::reporting_a_wrong_digest(FIXTURE),
        FakeExtractor::producing(FakeLayout::WithExecutable),
    );

    let error = failing
        .install(&definition("1.22.2", fixture_digest()))
        .expect_err("a wrong digest must abort the install");

    assert!(
        matches!(error, RuntimeStoreError::DigestMismatch { .. }),
        "expected a digest mismatch, got {error}"
    );

    // Nothing of the failed version exists.
    assert!(!version_directory(&root, "1.22.2").exists());

    // Staging was discarded.
    assert!(
        !failing.staging_directory().exists()
            || fs::read_dir(failing.staging_directory())
                .expect("the staging directory is readable")
                .next()
                .is_none(),
        "staging is removed after a failed verification"
    );

    // The active runtime is unchanged: same record, same version, still usable.
    let record_after = fs::read(failing.active_record(&id)).expect("the record exists");

    assert_eq!(
        record_before, record_after,
        "the active record is untouched"
    );

    let active_after = failing
        .active_runtime(&id)
        .expect("the record is readable")
        .expect("the previous runtime is still active");

    assert_eq!(active_after, active_before);
    assert!(active_after.executable_path().is_file());
}

/// An unreachable host aborts the install and leaves no version directory and no
/// staging state behind.
#[test]
fn a_download_failure_leaves_no_version_and_no_staging_state() {
    let root = TempRoot::new("download-failure");
    let failing: ComponentStore<_, _> = ComponentStore::new(
        root.path(),
        FakeDownloader::unreachable(),
        FakeExtractor::producing(FakeLayout::WithExecutable),
    );

    let error = failing
        .install(&definition("1.22.2", fixture_digest()))
        .expect_err("an unreachable host must abort the install");

    assert!(matches!(error, RuntimeStoreError::DownloadFailed { .. }));
    assert!(!version_directory(&root, "1.22.2").exists());
    assert!(
        !failing.staging_directory().exists()
            || fs::read_dir(failing.staging_directory())
                .expect("the staging directory is readable")
                .next()
                .is_none()
    );

    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");

    assert!(
        failing
            .active_runtime(&id)
            .expect("no record means nothing is active")
            .is_none(),
        "a failed first install activates nothing"
    );
}

/// A payload that does not contain the pinned executable is refused before it is
/// installed, so a broken artifact never becomes a version directory.
#[test]
fn a_payload_without_the_pinned_executable_is_refused() {
    let root = TempRoot::new("no-executable");
    let store: ComponentStore<_, _> = ComponentStore::new(
        root.path(),
        FakeDownloader::writing(FIXTURE),
        FakeExtractor::producing(FakeLayout::WithoutExecutable),
    );

    let error = store
        .install(&definition("1.22.2", fixture_digest()))
        .expect_err("a payload without the executable must be refused");

    match error {
        RuntimeStoreError::ExecutableMissing { expected } => {
            assert!(
                expected.ends_with(EXECUTABLE_IN_BUNDLE),
                "the missing path names the pinned executable: {}",
                expected.display()
            );
        }
        other => panic!("expected a missing executable, got {other}"),
    }

    assert!(!version_directory(&root, "1.22.2").exists());
}

/// An executable without an executable bit is not usable, so it is refused too.
#[test]
fn a_non_executable_file_is_not_accepted_as_the_runtime() {
    let root = TempRoot::new("not-executable");
    let store: ComponentStore<_, _> = ComponentStore::new(
        root.path(),
        FakeDownloader::writing(FIXTURE),
        FakeExtractor::producing(FakeLayout::NonExecutableFile),
    );

    let error = store
        .install(&definition("1.22.2", fixture_digest()))
        .expect_err("a non-executable file must be refused");

    assert!(matches!(error, RuntimeStoreError::ExecutableMissing { .. }));
    assert!(!version_directory(&root, "1.22.2").exists());
}

/// A failing unpack leaves the store exactly as it was.
#[test]
fn an_unpack_failure_leaves_no_version_and_no_staging_state() {
    let root = TempRoot::new("unpack-failure");
    let store: ComponentStore<_, _> = ComponentStore::new(
        root.path(),
        FakeDownloader::writing(FIXTURE),
        FakeExtractor::producing(FakeLayout::Failing),
    );

    let error = store
        .install(&definition("1.22.2", fixture_digest()))
        .expect_err("a failing unpack must abort the install");

    assert!(matches!(error, RuntimeStoreError::UnpackFailed { .. }));
    assert!(!version_directory(&root, "1.22.2").exists());
    assert!(
        !store.staging_directory().exists()
            || fs::read_dir(store.staging_directory())
                .expect("the staging directory is readable")
                .next()
                .is_none()
    );
}

// ---------------------------------------------------------------------------
// Immutability
// ---------------------------------------------------------------------------

/// Re-installing a version that is already installed is refused, and the
/// installed bytes are not touched.
#[test]
fn an_installed_version_is_immutable() {
    let root = TempRoot::new("immutable");
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");

    // The first store writes a marker into the installed version, so a later
    // overwrite would be detectable.
    let store = working_store(&root);

    store
        .install(&definition("1.22.2", fixture_digest()))
        .expect("the first install succeeds");

    let marker = version_directory(&root, "1.22.2").join("installed-marker");
    fs::write(&marker, b"first install").expect("the marker is written");

    // A second store tries to install the same version with different content.
    let second: ComponentStore<_, _> = ComponentStore::new(
        root.path(),
        FakeDownloader::writing(b"different bytes"),
        FakeExtractor::producing(FakeLayout::WithExecutable),
    );

    let error = second
        .install(&definition("1.22.2", fixture_digest()))
        .expect_err("an installed version must not be replaced");

    match error {
        RuntimeStoreError::AlreadyInstalled {
            id: refused_id,
            version,
            directory,
        } => {
            assert_eq!(refused_id, id);
            assert_eq!(version.as_str(), "1.22.2");
            assert_eq!(directory, version_directory(&root, "1.22.2"));
        }
        other => panic!("expected an already-installed refusal, got {other}"),
    }

    // The refusal happened before anything was downloaded or written.
    assert_eq!(
        second.installed_versions(&id).expect("readable"),
        vec![RuntimeVersion::from_str("1.22.2").expect("a valid version")]
    );
    assert!(marker.is_file(), "the installed version is untouched");
    assert_eq!(
        fs::read_to_string(&marker).expect("the marker is readable"),
        "first install"
    );
}

/// A cross-filesystem move fails as a whole on the store it happened to: the
/// already active runtime of *that* store is untouched, no partial final version
/// directory appears, and staging is cleaned up.
///
/// Installation is one `rename`, and it stays one `rename` on every path. This
/// test is what holds that invariant with a real `EXDEV`: the destination store
/// lives on a RAM disk, so a genuinely separate filesystem is involved, and only
/// its staging area is pointed back at the first filesystem. No filesystem trait is
/// introduced and nothing is simulated.
///
/// The ordering matters and is deliberate: the first version is installed and
/// activated while staging and store still share a filesystem, so the store is in
/// the state a real installation leaves behind. Only then is the staging area moved
/// to the other filesystem, so only the second installation is cross-device.
///
/// Every assertion about the active runtime is made against the *same* store whose
/// installation failed. A test that checked a different store would only prove that
/// an uninvolved store stays unchanged.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "mounts a RAM disk to produce a real cross-device move; needs no network"]
fn a_cross_device_move_fails_without_leaving_a_partial_installation() {
    let root = TempRoot::new("cross-device");
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");

    let older = RuntimeVersion::from_str("1.22.1").expect("a valid version");
    let newer = RuntimeVersion::from_str("1.22.2").expect("a valid version");

    // The destination store, on its own filesystem.
    let ram_disk = RamDisk::mount("cross-device");
    let store: ComponentStore<_, _> = ComponentStore::new(
        ram_disk.path(),
        FakeDownloader::writing(FIXTURE),
        FakeExtractor::producing(FakeLayout::WithExecutable),
    );

    // While staging and store share a filesystem, an installation succeeds and
    // becomes active. This is the state the failed installation below must preserve.
    let active_before = store
        .install(&definition("1.22.1", fixture_digest()))
        .expect("the first install succeeds while the filesystem is shared");

    assert_eq!(active_before.version(), &older);
    assert!(active_before.executable_path().is_file());

    let record_before = fs::read(store.active_record(&id)).expect("the record exists");

    // The successful installation left its staging area empty, which is asserted
    // here because the redirection below replaces that directory.
    let staging = store.staging_directory();

    assert!(
        fs::read_dir(&staging)
            .expect("the staging directory is readable")
            .next()
            .is_none(),
        "a successful install leaves no staging state behind"
    );

    // Now the store's staging area is pointed at a different filesystem. That is the
    // misconfiguration the installation contract has to refuse: staging and store no
    // longer share a filesystem, so the one-`rename` installation cannot be honoured.
    fs::remove_dir(&staging).expect("the empty staging directory is removed");

    let elsewhere_staging = root.path().join("staging-on-the-other-filesystem");
    fs::create_dir_all(&elsewhere_staging).expect("the foreign staging directory");

    std::os::unix::fs::symlink(&elsewhere_staging, &staging)
        .expect("the staging area is moved to the other filesystem");

    let installation = store.installation_directory(&id, RuntimePlatform::MacOsUniversal, &newer);

    let outcome = store.install(&definition("1.22.2", fixture_digest()));

    // 1. No partial version directory exists at the final path. This is the
    //    invariant an in-place copy would break, because it would create the version
    //    directory first and fill it afterwards — and it is asserted before the
    //    outcome so that a non-atomic fallback is reported as the partial
    //    installation it is, not merely as an unexpected success.
    assert!(
        !installation.exists(),
        "a failed move must not create the final version directory"
    );

    // ...and the store lists exactly the version it had before, so nothing else
    // appeared anywhere below the versions directory either.
    assert_eq!(
        store
            .installed_versions(&id)
            .expect("the store is readable"),
        vec![older.clone()],
        "only the previously installed version may remain"
    );

    // 2. The attempt did not succeed either, so no caller can mistake a refused
    //    installation for a completed one.
    let error = match outcome {
        Ok(installed) => panic!(
            "a cross-device move must not report success, but installed {} at {}",
            installed.version(),
            installed.directory().display()
        ),
        Err(error) => error,
    };

    // 3. The failure is reported as the cross-device case, not as a generic I/O
    //    error, so a caller can tell what is actually wrong.
    match &error {
        RuntimeStoreError::CrossDeviceInstallation {
            staged_payload,
            installation: reported,
        } => {
            assert!(staged_payload.starts_with(store.staging_directory()));
            assert_eq!(*reported, installation);
        }
        other => panic!("expected a cross-device installation failure, got {other}"),
    }

    // 4. Staging was cleaned up, on the filesystem it actually lived on.
    assert!(
        fs::read_dir(&elsewhere_staging)
            .expect("the staging directory is readable")
            .next()
            .is_none(),
        "staging is removed after a failed move"
    );

    // 5. The active runtime of the same store is byte-for-byte unchanged, is still
    //    the previous version, and its executable is still present and resolvable.
    assert_eq!(
        fs::read(store.active_record(&id)).expect("the record exists"),
        record_before,
        "the active record is untouched by a failed installation"
    );

    let active_after = store
        .active_runtime(&id)
        .expect("the record is readable")
        .expect("the previously activated runtime is still active");

    assert_eq!(active_after, active_before);
    assert_eq!(active_after.version(), &older);
    assert!(active_after.executable_path().is_file());
    assert_ne!(
        active_after.executable_path(),
        installation.join(EXECUTABLE_IN_BUNDLE),
        "the active runtime must not point at the version that failed to install"
    );

    // 6. The refusal was the move and not the payload: with the staging area back on
    //    the store's own filesystem, the same store installs the same version.
    fs::remove_file(&staging).expect("the staging redirection is removed");

    let installed = store
        .install(&definition("1.22.2", fixture_digest()))
        .expect("the same version installs once staging and store share a filesystem");

    assert_eq!(installed.version(), &newer);
    assert_eq!(
        store
            .installed_versions(&id)
            .expect("the store is readable"),
        vec![older, newer],
    );
}

// ---------------------------------------------------------------------------
// Activation
// ---------------------------------------------------------------------------

/// With nothing ever activated there is no active runtime — and no fallback to a
/// user installation or a system path.
#[test]
fn nothing_is_active_until_something_is_installed() {
    let root = TempRoot::new("nothing-active");
    let store = working_store(&root);
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");

    assert!(
        store
            .active_runtime(&id)
            .expect("an absent record is not an error")
            .is_none()
    );
    assert!(
        store
            .installed_versions(&id)
            .expect("an absent store is not an error")
            .is_empty()
    );
}

/// An activation record that names a version which is not installed is reported
/// as a broken installation instead of silently resolving to something else.
#[test]
fn an_active_record_pointing_at_a_missing_version_is_reported() {
    let root = TempRoot::new("broken-record");
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");
    let store = working_store(&root);

    let record = store.active_record(&id);
    fs::create_dir_all(record.parent().expect("a parent directory"))
        .expect("the runtime directory is created");
    fs::write(
        &record,
        "version=1.22.2\nplatform=macos-universal\nexecutable=RetroArch.app\n",
    )
    .expect("the record is written");

    let error = store
        .active_runtime(&id)
        .expect_err("a record without an installation is broken");

    match error {
        RuntimeStoreError::ActiveRuntimeMissing {
            version, directory, ..
        } => {
            assert_eq!(version.as_str(), "1.22.2");
            assert_eq!(directory, version_directory(&root, "1.22.2"));
        }
        other => panic!("expected a missing active runtime, got {other}"),
    }
}

/// A record that cannot be parsed is reported with a reason rather than being
/// treated as "nothing is active".
#[test]
fn an_unreadable_active_record_is_reported() {
    let root = TempRoot::new("unreadable-record");
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");
    let store = working_store(&root);

    let record = store.active_record(&id);
    fs::create_dir_all(record.parent().expect("a parent directory"))
        .expect("the runtime directory is created");
    fs::write(&record, "this is not a record\n").expect("the record is written");

    let error = store
        .active_runtime(&id)
        .expect_err("an unparseable record must be reported");

    assert!(matches!(
        error,
        RuntimeStoreError::ActiveRecordUnreadable { .. }
    ));
    assert!(error.to_string().contains("key=value"));
}

/// An activation record never points outside the store, because the executable it
/// stores is a relative path.
#[test]
fn an_activation_record_cannot_escape_the_store() {
    let root = TempRoot::new("escaping-record");
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");
    let store = working_store(&root);

    let record = store.active_record(&id);
    fs::create_dir_all(record.parent().expect("a parent directory"))
        .expect("the runtime directory is created");
    fs::write(
        &record,
        "version=1.22.2\nplatform=macos-universal\nexecutable=../../../../Applications/\
         RetroArch.app/Contents/MacOS/RetroArch\n",
    )
    .expect("the record is written");

    let error = store
        .active_runtime(&id)
        .expect_err("an escaping path must be refused");

    assert!(matches!(
        error,
        RuntimeStoreError::ActiveRecordUnreadable { .. }
    ));
}

// ---------------------------------------------------------------------------
// Staging cleanup
// ---------------------------------------------------------------------------

/// Cleanup removes abandoned staging directories this store owns, and nothing
/// else — neither an installed version nor a file a user put next to it.
#[test]
fn staging_cleanup_is_limited_to_the_stores_own_staging_directories() {
    let root = TempRoot::new("cleanup");
    let store = working_store(&root);
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");

    store
        .install(&definition("1.22.2", fixture_digest()))
        .expect("the install succeeds");

    // Simulate an interrupted run, plus content that is not this store's business.
    let abandoned = store
        .staging_directory()
        .join("install-retroarch-1.22.3-macos-universal");
    fs::create_dir_all(abandoned.join("payload")).expect("the abandoned staging directory");

    let foreign = store.staging_directory().join("user-notes");
    fs::create_dir_all(&foreign).expect("a directory this store does not own");

    let removed = store.cleanup_staging().expect("cleanup succeeds");

    assert_eq!(
        removed, 1,
        "exactly the abandoned staging directory is removed"
    );
    assert!(!abandoned.exists());
    assert!(
        foreign.is_dir(),
        "cleanup never touches what it does not own"
    );
    assert!(
        version_directory(&root, "1.22.2").is_dir(),
        "cleanup never touches an installed version"
    );
    assert!(
        store
            .active_runtime(&id)
            .expect("the record is readable")
            .is_some(),
        "cleanup never deactivates a runtime"
    );
}

/// Cleanup over a store that has no staging directory at all is a no-op rather
/// than an error.
#[test]
fn staging_cleanup_without_a_staging_directory_is_a_no_op() {
    let root = TempRoot::new("cleanup-empty");
    let store = working_store(&root);

    assert_eq!(store.cleanup_staging().expect("cleanup succeeds"), 0);
}

// ---------------------------------------------------------------------------
// Whole-flow properties
// ---------------------------------------------------------------------------

/// Every definition a store installs produces the layout the architecture pins,
/// regardless of version.
#[test]
fn the_store_layout_is_derived_from_the_definition() {
    let root = TempRoot::new("layout");
    let store = working_store(&root);
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");

    for version in ["1.22.2", "1.22.10", "2.0.0-rc.1"] {
        store
            .install(&definition(version, fixture_digest()))
            .expect("each pinned version installs");

        let directory = version_directory(&root, version);

        assert!(directory.is_dir(), "{version} has its own directory");
        assert!(
            directory.join(EXECUTABLE_IN_BUNDLE).is_file(),
            "{version} has the pinned executable"
        );
        assert_eq!(
            store.installation_directory(
                &id,
                RuntimePlatform::MacOsUniversal,
                &RuntimeVersion::from_str(version).expect("a valid version"),
            ),
            directory
        );
    }
}

/// The definition that was installed is the one the activation record describes,
/// so a resolved executable always belongs to the pinned version.
#[test]
fn the_active_record_describes_the_pinned_definition() {
    let root = TempRoot::new("record-contents");
    let store = working_store(&root);
    let id = RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity");
    let definition = definition("1.22.2", fixture_digest());

    store.install(&definition).expect("the install succeeds");

    let record = fs::read_to_string(store.active_record(&id)).expect("the record exists");

    assert!(record.contains("version=1.22.2"));
    assert!(record.contains("platform=macos-universal"));
    assert!(record.contains(EXECUTABLE_IN_BUNDLE));

    let active: InstalledRuntime = store
        .active_runtime(&id)
        .expect("the record is readable")
        .expect("a runtime is active");

    assert_eq!(active.version(), definition.version());
    assert_eq!(active.executable(), definition.executable());
}
