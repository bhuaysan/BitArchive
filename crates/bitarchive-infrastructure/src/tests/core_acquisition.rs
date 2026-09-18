//! Integration tests for the curated core acquisition path (Issue #21).
//!
//! Two layers are exercised here, and both run entirely offline:
//!
//! - the [`ZipCoreArchiveExtractor`] against ZIP archives this module builds
//!   itself, including the archives it must refuse;
//! - the [`CoreStore`] against a fake downloader and against a loopback HTTP
//!   server, so download, digest verification, extraction, installation, and
//!   resolution are proven together without the public network
//!   (ARCHITECTURE.md §45.4).
//!
//! No test here downloads a real mGBA artifact, and none needs the official build
//! host to be reachable. The one test that does download a real artifact is the
//! ignored live test of the composition root, which is opt-in.

use std::fs;
use std::io::{Cursor, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::thread;

use bitarchive_application::managed_core::{CoreArchiveExtractor, CoreInstaller, CoreStoreError};
use bitarchive_application::managed_runtime::{Artifact, ArtifactSourceKind};
use bitarchive_domain::component::{
    ArtifactReference, ComponentArtifactSource, ComponentAttribution, LicenseIdentifier,
    LoopbackSource, RelativePath, Sha256Digest,
};
use bitarchive_domain::managed_core::{
    CoreArtifactKind, CoreBuildId, CoreComponentId, CoreDefinition, CoreParts, CorePlatform,
    CoreProvenance, host_core_platform, mgba_bootstrap,
};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::{FakeDownloader, TempRoot};
use crate::core_archive::ZipCoreArchiveExtractor;
use crate::core_store::CoreStore;
use crate::http_download::HttpArtifactDownloader;

/// The library the curated mGBA definition pins.
const LIBRARY: &str = "mgba_libretro.dylib";

/// The published file name of the curated mGBA artifact.
const ARTIFACT_FILE: &str = "mgba_libretro.dylib.zip";

/// A second, synthetic build identity, so two builds can be installed side by
/// side without inventing a second curated core.
const SECOND_BUILD: &str = "mgba-0.10-1-abcdef0";

/// A synthetic build identity for definitions that exist only to reach a case the
/// curated allowlist cannot express — a non-canonical member name, or a directory
/// entry with the member's name.
const SYNTHETIC_BUILD: &str = "mgba-0.0-synthetic-0000000";

/// A loopback port for definitions that are never actually fetched.
///
/// The fake downloader ignores the source entirely, so the port is only there
/// because a definition carries a full URL. Nothing connects to it.
const UNCONTACTED_PORT: u16 = 9;

/// Returns the platform these tests install for.
fn host_platform() -> CorePlatform {
    host_core_platform().expect("the test suite runs on a host with a managed-core platform")
}

/// Returns the SHA-256 of `bytes`.
fn digest_of(bytes: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(bytes);

    Sha256Digest::from_bytes(hasher.finalize().into())
}

/// Builds a ZIP archive out of `members`, compressed the way the official build
/// host compresses a core.
fn zip_of(members: &[(&str, &[u8])]) -> Vec<u8> {
    zip_with(|writer| {
        for (name, content) in members {
            writer
                .start_file(*name, options())
                .expect("the fixture member starts");

            writer
                .write_all(content)
                .expect("the fixture member writes");
        }
    })
}

/// Builds a ZIP archive with full control over the entries.
fn zip_with(build: impl FnOnce(&mut ZipWriter<Cursor<Vec<u8>>>)) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));

    build(&mut writer);

    writer
        .finish()
        .expect("the fixture archive finishes")
        .into_inner()
}

/// The entry options of a fixture member: DEFLATE, as the build host uses.
fn options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

/// Builds a component source that points at a loopback server.
fn loopback_source(
    platform: CorePlatform,
    port: u16,
    published_file_name: &str,
) -> ComponentArtifactSource {
    ComponentArtifactSource::from(
        LoopbackSource::new(
            port,
            &format!(
                "/nightly/apple/osx/{}/latest/{published_file_name}",
                platform.build_host_architecture()
            ),
        )
        .expect("a valid loopback source"),
    )
}

/// The curated mGBA definition with its artifact redirected to `artifact`.
///
/// Only the artifact changes: identity, build, layout, membership, provenance, and
/// attribution stay exactly as the allowlist pins them, which is what a test of
/// the real store should exercise.
fn curated_definition(platform: CorePlatform, artifact: &[u8], port: u16) -> CoreDefinition {
    mgba_bootstrap(platform).with_artifact(
        ArtifactReference::with_file_name(
            loopback_source(platform, port, ARTIFACT_FILE),
            digest_of(artifact),
            ARTIFACT_FILE,
        )
        .expect("the published file name is a plain file name"),
    )
}

/// Builds a definition with a chosen build identity, archive member, library
/// path, published name, and artifact digest.
///
/// This is how the tests reach the cases the curated allowlist cannot express —
/// a second build of the same core, or a member name that is not canonical — while
/// keeping everything else about the definition pinned.
fn definition_with(
    platform: CorePlatform,
    build_id: &str,
    member: &str,
    library: &str,
    published_file_name: &str,
    digest: Sha256Digest,
    port: u16,
) -> CoreDefinition {
    CoreDefinition::new(CoreParts {
        identity: (
            CoreComponentId::from_str(CoreComponentId::MGBA).expect("a valid component identity"),
            String::from("mGBA"),
            String::from("mgba_libretro"),
            CoreBuildId::from_str(build_id).expect("a valid build identity"),
            platform,
        ),
        artifact: ArtifactReference::with_file_name(
            loopback_source(platform, port, published_file_name),
            digest,
            published_file_name,
        )
        .expect("the published file name is a plain file name"),
        layout: (
            CoreArtifactKind::LibretroCoreArchive {
                member: String::from(member),
            },
            RelativePath::from_str(library).expect("a valid relative library path"),
        ),
        // Synthetic provenance: this definition describes a fixture, not a
        // reviewed upstream build, and the values say so instead of repeating a
        // pin that would go stale with the next one.
        provenance: CoreProvenance {
            version: String::from("0.0-synthetic-0000000"),
            revision: String::from("0000000000000000000000000000000000000000"),
            revision_short: String::from("0000000"),
            channel_date: String::from("1970-01-01"),
            channel_crc32: String::from("00000000"),
        },
        attribution: ComponentAttribution {
            component: String::from("mGBA"),
            upstream_project: String::from("mgba-emu/mgba"),
            upstream_url: String::from("https://github.com/mgba-emu/mgba"),
            license: LicenseIdentifier::from_str(LicenseIdentifier::MPL_2_0)
                .expect("a valid license identifier"),
        },
    })
}

/// Writes a fixture artifact into the store's artifact directory.
fn write_artifact(root: &TempRoot, name: &str, bytes: &[u8]) -> PathBuf {
    let directory = root.path().join("artifacts");

    fs::create_dir_all(&directory).expect("the artifact directory");

    let path = directory.join(name);
    fs::write(&path, bytes).expect("the fixture artifact");

    path
}

/// Builds an artifact value for a file that exists on disk.
fn artifact_at(path: &Path, bytes: &[u8]) -> Artifact {
    Artifact::new(path, digest_of(bytes), ArtifactSourceKind::Local)
}

/// Builds a store over a fresh root with the given downloader.
fn store_with(
    root: &TempRoot,
    downloader: FakeDownloader,
) -> CoreStore<FakeDownloader, ZipCoreArchiveExtractor> {
    CoreStore::new(root.path(), downloader, ZipCoreArchiveExtractor::new())
}

/// Builds a store whose downloader serves `artifact`.
fn store_serving(
    root: &TempRoot,
    artifact: &[u8],
) -> CoreStore<FakeDownloader, ZipCoreArchiveExtractor> {
    store_with(root, FakeDownloader::writing(artifact))
}

/// A one-shot loopback HTTP server that answers any GET with a core archive.
///
/// The download path speaks to a real socket here, so request handling, streaming,
/// hashing, and the file bookkeeping around them are exercised end to end without
/// an external service.
struct FixtureServer {
    port: u16,
}

impl FixtureServer {
    /// Starts a server that answers one request with `body`.
    fn serving(body: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a local address").port();

        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);

                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\
                     Content-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                    body.len()
                );

                stream.write_all(head.as_bytes()).ok();
                stream.write_all(&body).ok();
                stream.flush().ok();
            }
        });

        Self { port }
    }
}

// ---------------------------------------------------------------------------
// The archive extractor
// ---------------------------------------------------------------------------

/// The pinned member is written to the pinned library path, and an unrelated
/// member of the same archive is not installed.
#[test]
fn the_pinned_member_is_written_and_unrelated_members_are_ignored() {
    let root = TempRoot::new("core-extract");
    let work = root.path().join("work");
    fs::create_dir_all(&work).expect("a working directory");

    let archive = zip_of(&[
        ("readme.txt", b"not a core"),
        (LIBRARY, b"the pinned library"),
        ("other_libretro.dylib", b"another core"),
    ]);

    let artifact_path = write_artifact(&root, "core.zip", &archive);
    let definition = curated_definition(host_platform(), &archive, UNCONTACTED_PORT);
    let artifact = artifact_at(&artifact_path, &archive);

    ZipCoreArchiveExtractor::new()
        .unpack(&definition, &artifact, &work)
        .expect("the pinned member is extracted");

    assert_eq!(
        fs::read(work.join(LIBRARY)).expect("the extracted library"),
        b"the pinned library"
    );
    assert!(!work.join("readme.txt").exists());
    assert!(!work.join("other_libretro.dylib").exists());
}

/// An archive without the pinned member is refused, and the refusal reports what
/// the archive did contain.
#[test]
fn an_archive_without_the_pinned_member_is_refused() {
    let root = TempRoot::new("core-member-missing");
    let work = root.path().join("work");
    fs::create_dir_all(&work).expect("a working directory");

    let archive = zip_of(&[("snes9x_libretro.dylib", b"another core")]);

    let artifact_path = write_artifact(&root, "core.zip", &archive);
    let definition = curated_definition(host_platform(), &archive, UNCONTACTED_PORT);

    let error = ZipCoreArchiveExtractor::new()
        .unpack(&definition, &artifact_at(&artifact_path, &archive), &work)
        .expect_err("a missing member is refused");

    match error {
        CoreStoreError::ArchiveMemberMissing {
            expected, found, ..
        } => {
            assert_eq!(expected, LIBRARY);
            assert_eq!(found, vec![String::from("snes9x_libretro.dylib")]);
        }
        other => panic!("expected a missing member, got {other}"),
    }

    assert!(!work.join(LIBRARY).exists());
}

/// A symbolic link is never installed as the library, whatever it points at.
#[test]
fn a_symbolic_link_is_never_installed_as_the_library() {
    let root = TempRoot::new("core-member-symlink");
    let work = root.path().join("work");
    fs::create_dir_all(&work).expect("a working directory");

    let archive = zip_with(|writer| {
        writer
            .add_symlink(LIBRARY, "/etc/passwd", options())
            .expect("the symlink member is written");
    });

    let artifact_path = write_artifact(&root, "core.zip", &archive);
    let definition = curated_definition(host_platform(), &archive, UNCONTACTED_PORT);

    let error = ZipCoreArchiveExtractor::new()
        .unpack(&definition, &artifact_at(&artifact_path, &archive), &work)
        .expect_err("a symbolic link is refused");

    assert!(
        matches!(error, CoreStoreError::ArchiveMemberUnsafe { .. }),
        "expected an unsafe member, got {error}"
    );
    assert!(!work.join(LIBRARY).exists());
}

/// A directory entry is not a library, even when its name is the pinned member
/// name written the way the format writes a directory.
#[test]
fn a_directory_is_never_installed_as_the_library() {
    let root = TempRoot::new("core-member-directory");
    let work = root.path().join("work");
    fs::create_dir_all(&work).expect("a working directory");

    // A ZIP directory entry carries a trailing separator in its name, so the
    // pinned member is spelled with it here: that is the only spelling under which
    // the entry is the member at all, and the extractor must still refuse it as a
    // library.
    let member = "mgba_libretro.dylib/";
    let archive = zip_with(|writer| {
        writer
            .add_directory(member, options())
            .expect("the directory member is written");
    });

    let artifact_path = write_artifact(&root, "core.zip", &archive);
    let definition = definition_with(
        host_platform(),
        SYNTHETIC_BUILD,
        member,
        LIBRARY,
        ARTIFACT_FILE,
        digest_of(&archive),
        UNCONTACTED_PORT,
    );

    let error = ZipCoreArchiveExtractor::new()
        .unpack(&definition, &artifact_at(&artifact_path, &archive), &work)
        .expect_err("a directory is refused");

    assert!(
        matches!(error, CoreStoreError::ArchiveMemberUnsafe { .. }),
        "expected an unsafe member, got {error}"
    );
    assert!(!work.join(LIBRARY).exists());
}

/// An archive that also carries entries with escaping names cannot write outside
/// the working directory: only the pinned member is ever written, and it is
/// written where the definition says.
#[test]
fn an_archive_cannot_write_outside_the_working_directory() {
    let root = TempRoot::new("core-no-escape");
    let work = root.path().join("work");
    fs::create_dir_all(&work).expect("a working directory");

    let archive = zip_of(&[
        ("../escaped.dylib", b"escape attempt"),
        ("/absolute.dylib", b"absolute attempt"),
        ("nested/../../escaped-twice.dylib", b"nested escape attempt"),
        (LIBRARY, b"the pinned library"),
    ]);

    let artifact_path = write_artifact(&root, "core.zip", &archive);
    let definition = curated_definition(host_platform(), &archive, UNCONTACTED_PORT);

    ZipCoreArchiveExtractor::new()
        .unpack(&definition, &artifact_at(&artifact_path, &archive), &work)
        .expect("the pinned member is extracted");

    assert_eq!(
        fs::read(work.join(LIBRARY)).expect("the extracted library"),
        b"the pinned library"
    );
    assert_eq!(
        fs::read_dir(&work)
            .expect("the working directory is readable")
            .count(),
        1,
        "only the pinned member is written"
    );

    for escaped in ["escaped.dylib", "absolute.dylib", "escaped-twice.dylib"] {
        assert!(
            !root.path().join(escaped).exists(),
            "{escaped} must not be written next to the working directory"
        );
    }
}

/// A member whose name is not the pinned relative member path is refused, even
/// when it would resolve to the same file.
#[test]
fn a_member_whose_name_is_not_the_pinned_relative_path_is_refused() {
    let root = TempRoot::new("core-member-not-canonical");
    let work = root.path().join("work");
    fs::create_dir_all(&work).expect("a working directory");

    let member = "a/../mgba_libretro.dylib";
    let archive = zip_of(&[(member, b"a library under a non-canonical name")]);

    let artifact_path = write_artifact(&root, "core.zip", &archive);
    let definition = definition_with(
        host_platform(),
        SYNTHETIC_BUILD,
        member,
        LIBRARY,
        ARTIFACT_FILE,
        digest_of(&archive),
        UNCONTACTED_PORT,
    );

    let error = ZipCoreArchiveExtractor::new()
        .unpack(&definition, &artifact_at(&artifact_path, &archive), &work)
        .expect_err("a non-canonical member name is refused");

    assert!(
        matches!(error, CoreStoreError::ArchiveMemberUnsafe { .. }),
        "expected an unsafe member, got {error}"
    );
}

/// A member that tries to escape the archive root is refused before anything is
/// written.
#[test]
fn a_member_that_escapes_the_archive_root_is_refused() {
    let root = TempRoot::new("core-member-escapes");
    let work = root.path().join("work");
    fs::create_dir_all(&work).expect("a working directory");

    let member = "a/../../mgba_libretro.dylib";
    let archive = zip_of(&[(member, b"an escaping member")]);

    let artifact_path = write_artifact(&root, "core.zip", &archive);
    let definition = definition_with(
        host_platform(),
        SYNTHETIC_BUILD,
        member,
        LIBRARY,
        ARTIFACT_FILE,
        digest_of(&archive),
        UNCONTACTED_PORT,
    );

    let error = ZipCoreArchiveExtractor::new()
        .unpack(&definition, &artifact_at(&artifact_path, &archive), &work)
        .expect_err("an escaping member name is refused");

    assert!(
        matches!(error, CoreStoreError::ArchiveMemberUnsafe { .. }),
        "expected an unsafe member, got {error}"
    );
}

/// A file that is not a ZIP archive at all is reported as unreadable, not as a
/// missing member.
#[test]
fn an_archive_that_is_not_a_zip_is_refused() {
    let root = TempRoot::new("core-not-a-zip");
    let work = root.path().join("work");
    fs::create_dir_all(&work).expect("a working directory");

    let bytes = b"this is not a zip archive";

    let artifact_path = write_artifact(&root, "core.zip", bytes);
    let definition = curated_definition(host_platform(), bytes, UNCONTACTED_PORT);

    let error = ZipCoreArchiveExtractor::new()
        .unpack(&definition, &artifact_at(&artifact_path, bytes), &work)
        .expect_err("a file that is not an archive is refused");

    assert!(
        matches!(error, CoreStoreError::ArchiveUnreadable { .. }),
        "expected an unreadable archive, got {error}"
    );
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

/// A curated core installs into a platform- and build-specific directory, and
/// nothing anywhere in the store is activated.
#[test]
fn a_curated_core_installs_into_a_platform_and_build_specific_directory() {
    let root = TempRoot::new("core-install");
    let platform = host_platform();
    let archive = zip_of(&[(LIBRARY, b"the pinned library")]);
    let definition = curated_definition(platform, &archive, UNCONTACTED_PORT);
    let store = store_serving(&root, &archive);

    let installed = store
        .install(&definition)
        .expect("the curated core installs");

    let directory = root
        .path()
        .join("cores")
        .join("mgba")
        .join(platform.as_str())
        .join(definition.build_id().as_str());

    assert_eq!(installed.directory(), directory);
    assert_eq!(installed.library_path(), directory.join(LIBRARY));
    assert_eq!(
        fs::read(installed.library_path()).expect("the installed library"),
        b"the pinned library"
    );
    assert_eq!(
        fs::read_dir(&directory)
            .expect("the build directory is readable")
            .count(),
        1,
        "only the pinned library is installed"
    );

    assert!(
        store
            .is_installed(definition.component_id(), platform, definition.build_id())
            .expect("the store is readable"),
        "the installed build is reported as installed"
    );

    assert_eq!(
        store
            .installed_builds(definition.component_id(), platform)
            .expect("the store is readable"),
        vec![definition.build_id().clone()]
    );

    let resolved = store.resolve(&definition).expect("the build resolves");

    assert_eq!(resolved.library_path(), installed.library_path());
    assert_eq!(resolved.core_name(), "mgba_libretro");

    // There is no activation record, no "current build", and no default core:
    // core selection stays with the resolution policy, which this store never
    // writes to.
    for forbidden in ["active", "current", "default"] {
        assert!(
            !root
                .path()
                .join("cores")
                .join("mgba")
                .join(forbidden)
                .exists(),
            "a core store must not contain a {forbidden} record"
        );
    }

    assert_eq!(
        fs::read_dir(root.path().join("staging"))
            .expect("the staging directory is readable")
            .count(),
        0,
        "a successful install leaves no staging state"
    );
}

/// An installed build is immutable: a second install of the same build is
/// refused, and the installed bytes are untouched.
#[test]
fn an_installed_build_is_immutable() {
    let root = TempRoot::new("core-immutable");
    let platform = host_platform();
    let archive = zip_of(&[(LIBRARY, b"the pinned library")]);
    let definition = curated_definition(platform, &archive, UNCONTACTED_PORT);
    let store = store_serving(&root, &archive);

    let installed = store
        .install(&definition)
        .expect("the curated core installs");

    let error = store
        .install(&definition)
        .expect_err("a second install of the same build is refused");

    match error {
        CoreStoreError::AlreadyInstalled {
            component_id,
            platform: refused_platform,
            build_id,
            directory,
        } => {
            assert_eq!(&component_id, definition.component_id());
            assert_eq!(refused_platform, platform);
            assert_eq!(&build_id, definition.build_id());
            assert_eq!(directory, installed.directory());
        }
        other => panic!("expected an already installed build, got {other}"),
    }

    assert_eq!(
        fs::read(installed.library_path()).expect("the installed library"),
        b"the pinned library"
    );
}

/// Two builds of one core are installed side by side and listed, because a core
/// has no active version that a second install would have to replace.
#[test]
fn two_builds_of_one_core_are_installed_side_by_side_and_listed() {
    let root = TempRoot::new("core-two-builds");
    let platform = host_platform();
    let archive = zip_of(&[(LIBRARY, b"the pinned library")]);
    let first = curated_definition(platform, &archive, UNCONTACTED_PORT);
    let second = definition_with(
        platform,
        SECOND_BUILD,
        LIBRARY,
        LIBRARY,
        ARTIFACT_FILE,
        digest_of(&archive),
        UNCONTACTED_PORT,
    );

    let store = store_serving(&root, &archive);

    let first_installed = store.install(&first).expect("the first build installs");
    let second_installed = store.install(&second).expect("the second build installs");

    assert_ne!(first_installed.directory(), second_installed.directory());
    assert!(first_installed.library_path().exists());
    assert!(second_installed.library_path().exists());

    let mut expected = vec![first.build_id().clone(), second.build_id().clone()];
    expected.sort();

    assert_eq!(
        store
            .installed_builds(first.component_id(), platform)
            .expect("the store is readable"),
        expected
    );

    assert!(
        !store
            .is_installed(
                first.component_id(),
                platform,
                &CoreBuildId::from_str("mgba-0.9-0-0000000").expect("a valid build identity")
            )
            .expect("the store is readable"),
        "a build that is not installed is not reported as installed"
    );
}

/// A definition for another architecture is refused instead of being served this
/// machine's build.
#[test]
fn a_definition_for_another_architecture_is_refused() {
    let root = TempRoot::new("core-other-platform");
    let host = host_core_platform();

    let other = match host {
        Some(CorePlatform::MacOsArm64) => CorePlatform::MacOsX86_64,
        _ => CorePlatform::MacOsArm64,
    };

    let archive = zip_of(&[(LIBRARY, b"the pinned library")]);
    let definition = curated_definition(other, &archive, UNCONTACTED_PORT);
    let store = store_serving(&root, &archive);

    let error = store
        .install(&definition)
        .expect_err("another architecture is refused");

    match error {
        CoreStoreError::UnsupportedPlatform {
            platform,
            host: refused_host,
        } => {
            assert_eq!(platform, other);
            assert_eq!(refused_host, host);
        }
        other => panic!("expected an unsupported platform, got {other}"),
    }

    assert!(
        !root.path().join("cores").exists(),
        "nothing is created for a platform this build cannot install"
    );
}

/// A digest mismatch aborts the acquisition and leaves neither a build directory
/// nor staging state behind.
#[test]
fn a_digest_mismatch_leaves_no_build_and_no_staging_state() {
    let root = TempRoot::new("core-digest-mismatch");
    let platform = host_platform();
    let archive = zip_of(&[(LIBRARY, b"the pinned library")]);
    let definition = curated_definition(platform, &archive, UNCONTACTED_PORT);
    let store = store_with(&root, FakeDownloader::reporting_a_wrong_digest(&archive));

    let error = store
        .install(&definition)
        .expect_err("a tampered artifact is refused");

    assert!(
        matches!(error, CoreStoreError::Store { .. }),
        "the shared download failure keeps its spelling, got {error}"
    );
    assert!(!root.path().join("cores").exists());
    assert_eq!(
        fs::read_dir(root.path().join("staging"))
            .expect("the staging directory is readable")
            .count(),
        0
    );
}

/// An archive that does not contain the pinned library never becomes a build
/// directory, and no staging state survives it.
#[test]
fn an_archive_without_the_pinned_library_leaves_no_build_directory() {
    let root = TempRoot::new("core-wrong-member-install");
    let platform = host_platform();
    let archive = zip_of(&[("snes9x_libretro.dylib", b"another core")]);
    let definition = curated_definition(platform, &archive, UNCONTACTED_PORT);
    let store = store_serving(&root, &archive);

    let error = store
        .install(&definition)
        .expect_err("an archive without the pinned library is refused");

    assert!(
        matches!(error, CoreStoreError::ArchiveMemberMissing { .. }),
        "expected a missing member, got {error}"
    );
    assert!(!root.path().join("cores").exists());
    assert_eq!(
        fs::read_dir(root.path().join("staging"))
            .expect("the staging directory is readable")
            .count(),
        0
    );
}

/// A definition whose recorded published name disagrees with its URL is refused
/// before anything is downloaded.
#[test]
fn an_inconsistent_definition_is_not_downloaded() {
    let root = TempRoot::new("core-inconsistent");
    let platform = host_platform();
    let archive = zip_of(&[(LIBRARY, b"the pinned library")]);

    let definition = mgba_bootstrap(platform).with_artifact(
        ArtifactReference::with_file_name(
            loopback_source(platform, UNCONTACTED_PORT, ARTIFACT_FILE),
            digest_of(&archive),
            "mgba_libretro.dll.zip",
        )
        .expect("the published file name is a plain file name"),
    );

    // A downloader that fails on contact: reaching it at all would be the bug.
    let store = store_with(&root, FakeDownloader::unreachable());

    let error = store
        .install(&definition)
        .expect_err("an inconsistent definition is refused");

    assert!(
        matches!(error, CoreStoreError::InconsistentDefinition { .. }),
        "expected an inconsistent definition, got {error}"
    );
    assert!(
        !root.path().join("artifacts").exists(),
        "an inconsistent definition is never downloaded"
    );
}

/// Resolving a build that is not installed reports exactly that, and substitutes
/// neither another build nor another architecture.
#[test]
fn resolving_a_build_that_is_not_installed_reports_it_and_substitutes_nothing() {
    let root = TempRoot::new("core-not-installed");
    let platform = host_platform();
    let archive = zip_of(&[(LIBRARY, b"the pinned library")]);
    let definition = curated_definition(platform, &archive, UNCONTACTED_PORT);
    let store = store_serving(&root, &archive);

    let error = store
        .resolve(&definition)
        .expect_err("a build that is not installed does not resolve");

    match error {
        CoreStoreError::NotInstalled {
            component_id,
            platform: requested_platform,
            build_id,
            directory,
        } => {
            assert_eq!(&component_id, definition.component_id());
            assert_eq!(requested_platform, platform);
            assert_eq!(&build_id, definition.build_id());
            assert!(
                directory.ends_with(definition.build_id().as_str()),
                "the reported directory names the exact build"
            );
        }
        other => panic!("expected a not-installed build, got {other}"),
    }

    assert_eq!(
        store
            .installed_builds(definition.component_id(), platform)
            .expect("the store is readable"),
        Vec::new()
    );
}

/// A build directory that lost its library is reported as a broken installation
/// instead of resolving to a path that does not exist.
#[test]
fn a_build_whose_library_disappeared_is_reported() {
    let root = TempRoot::new("core-library-missing");
    let platform = host_platform();
    let archive = zip_of(&[(LIBRARY, b"the pinned library")]);
    let definition = curated_definition(platform, &archive, UNCONTACTED_PORT);
    let store = store_serving(&root, &archive);

    let installed = store
        .install(&definition)
        .expect("the curated core installs");

    fs::remove_file(installed.library_path()).expect("the library is removed");

    let error = store
        .resolve(&definition)
        .expect_err("a build without its library does not resolve");

    match error {
        CoreStoreError::LibraryMissing { expected } => {
            assert_eq!(expected, installed.library_path());
        }
        other => panic!("expected a missing library, got {other}"),
    }
}

/// Staging state an interrupted run left behind is removed, and nothing else is
/// touched.
#[test]
fn abandoned_staging_state_is_cleaned_up_and_nothing_else_is() {
    let root = TempRoot::new("core-staging-cleanup");
    let platform = host_platform();
    let archive = zip_of(&[(LIBRARY, b"the pinned library")]);
    let definition = curated_definition(platform, &archive, UNCONTACTED_PORT);
    let store = store_serving(&root, &archive);

    let installed = store
        .install(&definition)
        .expect("the curated core installs");

    let staging = root.path().join("staging");
    fs::create_dir_all(staging.join("install-mgba-abandoned")).expect("abandoned staging state");
    fs::create_dir_all(staging.join("not-ours")).expect("an unrelated directory");

    assert_eq!(store.cleanup_staging().expect("staging is cleaned up"), 1);

    assert!(!staging.join("install-mgba-abandoned").exists());
    assert!(staging.join("not-ours").exists());
    assert!(installed.library_path().exists());
}

/// Download, digest verification, extraction, installation, and resolution work
/// together over a real socket, without the public network.
#[test]
fn a_core_is_downloaded_verified_installed_and_resolved_over_loopback_http() {
    let root = TempRoot::new("core-loopback");
    let platform = host_platform();
    let archive = zip_of(&[(LIBRARY, b"the pinned library")]);
    let server = FixtureServer::serving(archive.clone());
    let definition = curated_definition(platform, &archive, server.port);

    let store = CoreStore::new(
        root.path(),
        HttpArtifactDownloader::new(),
        ZipCoreArchiveExtractor::new(),
    );

    let installed = store
        .install(&definition)
        .expect("the core is downloaded, verified, and installed");

    assert_eq!(
        fs::read(installed.library_path()).expect("the installed library"),
        b"the pinned library"
    );

    let resolved = store.resolve(&definition).expect("the build resolves");

    assert_eq!(resolved.library_path(), installed.library_path());
    assert_eq!(resolved.build_id(), definition.build_id());
    assert!(resolved.directory().is_dir());
}
