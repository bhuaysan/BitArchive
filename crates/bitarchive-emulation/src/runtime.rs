//! The RetroArch runtime BitArchive manages, pinned to one exact version.
//!
//! This module is the definition the whole acquisition path is verified against,
//! and the only place that names a RetroArch version, URL, or digest. It is
//! deliberately a function and not a lookup: the definition cannot change because
//! a server said so, and reading it never depends on the network
//! (Issue #19).
//!
//! ```text
//! pinned_retroarch_runtime()
//! ├── id           retroarch
//! ├── version      1.22.2
//! ├── platform     macos-universal
//! ├── source       https://buildbot.libretro.com/stable/1.22.2/apple/osx/universal/
//! │                RetroArch_Metal.dmg
//! ├── digest       81b79121ba26d539064ae13b4d0419a120c3d165afbe656cf5f5412b15fdb434
//! ├── kind         apple-disk-image { bundle: "RetroArch.app" }
//! ├── executable   RetroArch.app/Contents/MacOS/RetroArch
//! └── attribution  RetroArch · libretro/RetroArch · GPL-3.0-only
//! ```
//!
//! # Why this version and this artifact
//!
//! - **Version `1.22.2`** is the newest stable release on the official stable
//!   channel at the time this pin was written.
//! - **`apple/osx/universal/RetroArch_Metal.dmg`** is the artifact the official
//!   download page offers for modern macOS. The universal image contains both the
//!   Apple Silicon and the Intel slice, so one artifact serves both, and the
//!   `Metal` variant is the current official macOS build.
//! - The digest is the SHA-256 of that exact artifact as published on the official
//!   build host. It is recorded here so that verification compares the download
//!   against a value BitArchive reviewed, never against the artifact's own claims.
//!
//! Verified structure of the artifact, which is why the executable path is what it
//! is: the image contains a `RetroArch.app` bundle beside a symlink to
//! `/Applications`, and the executable is a universal Mach-O binary at
//! `RetroArch.app/Contents/MacOS/RetroArch`. The bundle also carries its
//! frameworks, its `assets.zip`, and its embedded code signature, so the whole
//! bundle is installed and never flattened.
//!
//! # What this module does not do
//!
//! - **No "latest" resolution.** Nothing here asks a server which version exists;
//!   a version is chosen by a human and recorded here.
//! - **No core assumption.** A runtime and a core are separate component classes
//!   (ARCHITECTURE.md §23.4). This artifact contains no libretro cores and nothing
//!   here assumes it does; core distribution is a later Issue with its own
//!   allowlist and license review.
//! - **No signature verification.** ARCHITECTURE.md §24 describes signed
//!   distribution manifests and that step is not implemented yet, so the pinned
//!   SHA-256 is the trust anchor. See
//!   `docs/decisions/0001-managed-runtime-acquisition.md`.
//! - **No user path.** There is no fallback to an installed RetroArch and no way to
//!   point BitArchive at one: `PRODUCT.md` §16.1 states that an existing user
//!   installation is not required for normal operation.

use std::str::FromStr;

use bitarchive_domain::runtime::{
    ArtifactKind, ArtifactSource, LicenseIdentifier, RelativePath, RuntimeAttribution,
    RuntimeDefinition, RuntimeId, RuntimeParts, RuntimePlatform, RuntimeSource, RuntimeVersion,
    Sha256Digest,
};

/// The RetroArch version BitArchive pins.
///
/// Bumping this constant is a deliberate act: a new version also needs a new URL
/// and a new digest, and both must be reviewed together.
pub const PINNED_RETROARCH_VERSION: &str = "1.22.2";

/// The exact official URL the pinned artifact is downloaded from.
///
/// The official stable channel of the libretro build host. There is no mirror, no
/// redirect, and no `latest` path in this URL.
pub const PINNED_RETROARCH_ARTIFACT_URL: &str =
    "https://buildbot.libretro.com/stable/1.22.2/apple/osx/universal/RetroArch_Metal.dmg";

/// The file name the pinned artifact URL ends in.
pub const PINNED_RETROARCH_ARTIFACT_FILE: &str = "RetroArch_Metal.dmg";

/// The SHA-256 of the pinned artifact as published on the official build host.
pub const PINNED_RETROARCH_ARTIFACT_SHA256: &str =
    "81b79121ba26d539064ae13b4d0419a120c3d165afbe656cf5f5412b15fdb434";

/// The application bundle inside the pinned disk image.
pub const PINNED_RETROARCH_BUNDLE: &str = "RetroArch.app";

/// The executable inside the installed bundle.
pub const PINNED_RETROARCH_EXECUTABLE: &str = "RetroArch.app/Contents/MacOS/RetroArch";

/// The upstream project the runtime comes from.
pub const PINNED_RETROARCH_UPSTREAM_PROJECT: &str = "libretro/RetroArch";

/// The canonical URL of the upstream project.
pub const PINNED_RETROARCH_UPSTREAM_URL: &str = "https://github.com/libretro/RetroArch";

/// The license RetroArch is distributed under.
///
/// RetroArch is GPLv3 ("GPL-3.0-only" in SPDX terms), which is why the runtime is
/// recorded as a redistributed third-party component with its origin and license
/// rather than as an anonymous download. Anyone redistributing a BitArchive build
/// has to keep those obligations satisfiable, and this is where the facts live.
pub const PINNED_RETROARCH_LICENSE: &str = "GPL-3.0-only";

/// Returns the pinned definition of the RetroArch runtime BitArchive manages.
///
/// # Panics
///
/// Never in practice: every value is a compile-time constant that is validated by
/// the tests in this module, so a malformed pin fails the test suite rather than a
/// user's installation.
#[must_use]
pub fn pinned_retroarch_runtime() -> RuntimeDefinition {
    RuntimeDefinition::new(RuntimeParts {
        identity: (
            RuntimeId::from_str(RuntimeId::RETROARCH).expect("the pinned identity is valid"),
            RuntimeVersion::from_str(PINNED_RETROARCH_VERSION)
                .expect("the pinned version is valid"),
            RuntimePlatform::MacOsUniversal,
        ),
        artifact: (
            RuntimeSource::official(
                ArtifactSource::new(PINNED_RETROARCH_ARTIFACT_URL)
                    .expect("the pinned artifact URL is an official https URL"),
            ),
            Sha256Digest::from_str(PINNED_RETROARCH_ARTIFACT_SHA256)
                .expect("the pinned digest is a SHA-256 digest"),
        ),
        layout: (
            ArtifactKind::AppleDiskImage {
                bundle: String::from(PINNED_RETROARCH_BUNDLE),
            },
            RelativePath::from_str(PINNED_RETROARCH_EXECUTABLE)
                .expect("the pinned executable path is relative and canonical"),
        ),
        attribution: RuntimeAttribution {
            component: String::from("RetroArch"),
            upstream_project: String::from(PINNED_RETROARCH_UPSTREAM_PROJECT),
            upstream_url: String::from(PINNED_RETROARCH_UPSTREAM_URL),
            license: LicenseIdentifier::from_str(PINNED_RETROARCH_LICENSE)
                .expect("the pinned license identifier is valid"),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pin is internally consistent: every constant describes the same
    /// release, so a partial edit cannot produce a definition that mixes two
    /// versions.
    #[test]
    fn the_pin_describes_one_consistent_release() {
        let definition = pinned_retroarch_runtime();

        assert_eq!(definition.id().as_str(), RuntimeId::RETROARCH);
        assert_eq!(definition.version().as_str(), PINNED_RETROARCH_VERSION);
        assert_eq!(definition.platform(), RuntimePlatform::MacOsUniversal);

        assert!(
            definition
                .source()
                .as_str()
                .contains(&format!("/stable/{PINNED_RETROARCH_VERSION}/")),
            "the artifact URL must name the pinned version: {}",
            definition.source()
        );
        assert!(
            definition
                .source()
                .as_str()
                .ends_with(PINNED_RETROARCH_ARTIFACT_FILE),
            "the artifact URL must name the pinned artifact"
        );

        let ArtifactKind::AppleDiskImage { bundle } = definition.kind();
        assert_eq!(bundle, PINNED_RETROARCH_BUNDLE);
        assert_eq!(
            definition.executable().to_string(),
            PINNED_RETROARCH_EXECUTABLE
        );
        assert!(definition.executable().to_string().starts_with(bundle));
    }

    /// The source is the official host over https, so a mirror cannot be pinned by
    /// accident and the download cannot be plaintext.
    #[test]
    fn the_pin_uses_the_official_https_source_only() {
        let definition = pinned_retroarch_runtime();

        assert!(definition.source().is_official());
        assert_eq!(
            definition.source().host(),
            bitarchive_domain::runtime::OFFICIAL_RUNTIME_HOST
        );
        assert!(definition.source().as_str().starts_with("https://"));
        assert!(
            !definition.source().as_str().contains("latest"),
            "a pin must not follow a moving 'latest' link"
        );
    }

    /// The digest is a real SHA-256 and is not derived from anything at run time.
    #[test]
    fn the_pin_carries_a_well_formed_digest() {
        let definition = pinned_retroarch_runtime();
        let expected = Sha256Digest::from_str(PINNED_RETROARCH_ARTIFACT_SHA256)
            .expect("the pinned digest parses");

        assert_eq!(definition.digest(), expected);
        assert_eq!(definition.digest().as_str().len(), 64);
        assert_eq!(
            definition.digest().as_str(),
            PINNED_RETROARCH_ARTIFACT_SHA256,
            "the rendered digest is the pinned spelling"
        );
    }

    /// The license and upstream metadata are recorded, so redistribution stays
    /// traceable (Issue #19).
    #[test]
    fn the_pin_records_upstream_and_license_metadata() {
        let definition = pinned_retroarch_runtime();
        let attribution = definition.attribution();

        assert_eq!(attribution.component, "RetroArch");
        assert_eq!(
            attribution.upstream_project,
            PINNED_RETROARCH_UPSTREAM_PROJECT
        );
        assert_eq!(attribution.upstream_url, PINNED_RETROARCH_UPSTREAM_URL);
        assert_eq!(attribution.license.as_str(), PINNED_RETROARCH_LICENSE);
        assert_eq!(attribution.license.as_str(), "GPL-3.0-only");
    }

    /// Reading the pin twice yields the same definition: it is a value, not a
    /// cached lookup that could drift.
    #[test]
    fn the_pin_is_deterministic() {
        assert_eq!(pinned_retroarch_runtime(), pinned_retroarch_runtime());
    }

    /// The executable the pin names is the one the artifact actually contains,
    /// which is what the installer validates before activating a version.
    #[test]
    fn the_pin_names_an_executable_inside_the_installed_bundle() {
        let definition = pinned_retroarch_runtime();
        let components = definition.executable().components();

        assert_eq!(
            components,
            ["RetroArch.app", "Contents", "MacOS", "RetroArch"],
            "the executable is the bundle's Mach-O binary"
        );
    }
}
