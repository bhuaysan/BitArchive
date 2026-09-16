//! Reading the real official macOS runtime artifact.
//!
//! These tests need an actual RetroArch disk image, which normal CI deliberately
//! does not download (ARCHITECTURE.md §45.4, Issue #19). They are therefore
//! `#[ignore]`d and only run when someone asks for them **and** points
//! `BITARCHIVE_TEST_RETROARCH_DMG` at an image:
//!
//! ```bash
//! BITARCHIVE_TEST_RETROARCH_DMG=/path/to/RetroArch_Metal.dmg \
//!     cargo test -p bitarchive-platform --test live_dmg -- --ignored --nocapture
//! ```
//!
//! The image can be produced by the opt-in developer command:
//!
//! ```bash
//! cargo run -p bitarchive-desktop -- acquire-retroarch-runtime --root /tmp/bitarchive-live
//! ```
//!
//! then taken from `<root>/artifacts/retroarch-1.22.2-macos-universal`.
//!
//! # What this proves
//!
//! Mounting, the bundle layout the pin assumes, the executable's universal
//! binary structure, and that the OS-provided bundle signature survives being
//! read out of the image. That is everything the pinned definition claims about
//! the artifact, checked against the artifact itself.

use std::path::{Path, PathBuf};
use std::process::Command;

use bitarchive_application::managed_runtime::DiskImageExtractor;
use bitarchive_emulation::pinned_retroarch_runtime;
use bitarchive_platform::DmgBundleExtractor;

/// The environment variable that points at a real RetroArch disk image.
const IMAGE_ENVIRONMENT: &str = "BITARCHIVE_TEST_RETROARCH_DMG";

/// Returns the image to test against, or [`None`] when none was pointed at.
fn image() -> Option<PathBuf> {
    let path = std::env::var_os(IMAGE_ENVIRONMENT).map(PathBuf::from)?;

    if path.is_file() { Some(path) } else { None }
}

/// A temporary directory that removes itself.
struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "bitarchive-b5-live-dmg-{label}-{}",
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

/// The real artifact contains the bundle the pin names, and the bundle contains
/// the executable the pin names.
#[test]
#[ignore = "needs a real RetroArch disk image; set BITARCHIVE_TEST_RETROARCH_DMG"]
fn the_pinned_artifact_contains_the_pinned_bundle_and_executable() {
    let Some(image) = image() else {
        panic!("{IMAGE_ENVIRONMENT} must point at a RetroArch disk image");
    };

    let root = TempRoot::new("extract");
    let destination = root.path().join("payload");
    std::fs::create_dir_all(&destination).expect("the destination exists");

    let definition = pinned_retroarch_runtime();

    DmgBundleExtractor::new()
        .extract_bundle(&definition, &image, &destination)
        .expect("the official artifact can be read");

    let expected = destination.join("RetroArch.app/Contents/MacOS/RetroArch");
    let metadata = std::fs::metadata(&expected).expect("the pinned executable was copied out");

    assert!(metadata.is_file());
    println!("extracted executable: {}", expected.display());
    println!("size: {} bytes", metadata.len());
}

/// Nothing the extractor mounted is still mounted afterwards.
#[test]
#[ignore = "needs a real RetroArch disk image; set BITARCHIVE_TEST_RETROARCH_DMG"]
fn no_mount_outlives_the_extraction() {
    let Some(image) = image() else {
        panic!("{IMAGE_ENVIRONMENT} must point at a RetroArch disk image");
    };

    let root = TempRoot::new("detached");
    let destination = root.path().join("payload");
    std::fs::create_dir_all(&destination).expect("the destination exists");

    DmgBundleExtractor::new()
        .extract_bundle(&pinned_retroarch_runtime(), &image, &destination)
        .expect("the official artifact can be read");

    let mounted = Command::new("/usr/bin/hdiutil")
        .arg("info")
        .output()
        .expect("hdiutil info runs");
    let rendered = String::from_utf8_lossy(&mounted.stdout);

    assert!(
        !rendered.contains("bitarchive-dmg-"),
        "a BitArchive mount point is still mounted:\n{rendered}"
    );
}

/// The copied bundle is a universal binary, which is what "macos-universal"
/// claims about the artifact.
#[test]
#[ignore = "needs a real RetroArch disk image; set BITARCHIVE_TEST_RETROARCH_DMG"]
fn the_extracted_executable_is_a_universal_binary() {
    let Some(image) = image() else {
        panic!("{IMAGE_ENVIRONMENT} must point at a RetroArch disk image");
    };

    let root = TempRoot::new("universal");
    let destination = root.path().join("payload");
    std::fs::create_dir_all(&destination).expect("the destination exists");

    DmgBundleExtractor::new()
        .extract_bundle(&pinned_retroarch_runtime(), &image, &destination)
        .expect("the official artifact can be read");

    let executable = destination.join("RetroArch.app/Contents/MacOS/RetroArch");

    let lipo = Command::new("/usr/bin/lipo")
        .arg("-archs")
        .arg(&executable)
        .output()
        .expect("lipo runs");

    let architectures = String::from_utf8_lossy(&lipo.stdout);
    println!("architectures: {}", architectures.trim());

    assert!(architectures.contains("arm64"));
    assert!(architectures.contains("x86_64"));
}

/// The OS sees the copied bundle as a valid, notarized app, so reading it out of
/// the image did not damage its signature.
#[test]
#[ignore = "needs a real RetroArch disk image; set BITARCHIVE_TEST_RETROARCH_DMG"]
fn the_copied_bundle_keeps_a_valid_signature() {
    let Some(image) = image() else {
        panic!("{IMAGE_ENVIRONMENT} must point at a RetroArch disk image");
    };

    let root = TempRoot::new("signature");
    let destination = root.path().join("payload");
    std::fs::create_dir_all(&destination).expect("the destination exists");

    DmgBundleExtractor::new()
        .extract_bundle(&pinned_retroarch_runtime(), &image, &destination)
        .expect("the official artifact can be read");

    let bundle = destination.join("RetroArch.app");

    let verification = Command::new("/usr/bin/codesign")
        .arg("--verify")
        .arg("--deep")
        .arg("--strict")
        .arg(&bundle)
        .output()
        .expect("codesign runs");

    println!(
        "codesign status: {}\n{}",
        verification.status,
        String::from_utf8_lossy(&verification.stderr).trim()
    );

    assert!(
        verification.status.success(),
        "the copied bundle must keep a valid code signature"
    );
}
