//! The extractor adapter: from a verified artifact to a staged payload.
//!
//! The installer's [`ArtifactExtractor`] port asks a single question — "unpack the
//! artifact this definition pins into this directory" — and answers it without
//! knowing how any particular packaging works. This module is the adapter that
//! dispatches on the artifact kind and, for an Apple disk image, hands the work to
//! a [`DiskImageExtractor`], which is where the operating system's disk-image
//! service is reached.
//!
//! ```text
//! ArtifactExtractor::unpack           ← the installer calls this
//!         ↓  match definition.kind()
//! ArtifactKind::AppleDiskImage        → DiskImageExtractor::extract_bundle
//!         ↓
//! staged payload/RetroArch.app/…
//! ```
//!
//! # Why a wrapper instead of one trait
//!
//! The two ports answer different questions. [`ArtifactExtractor`] is about
//! packaging and belongs to the installer; [`DiskImageExtractor`] is about a
//! specific operating-system capability and belongs to the platform layer. Merging
//! them would put "what is a `.dmg`" into the installer's vocabulary, and would
//! force every future packaging format through a disk-image-shaped interface.
//! Keeping them apart costs one small adapter.
//!
//! # What this adapter does not do
//!
//! It does not verify anything. The artifact it receives has already been checked
//! against the pinned SHA-256 by the downloader, which is the only component that
//! may decide an artifact is trustworthy. It also does not validate the
//! executable: that check runs on the staged payload, in the installer.

use std::path::Path;

use bitarchive_application::managed_runtime::{
    Artifact, ArtifactExtractor, DiskImageExtractor, RuntimeStoreError,
};
use bitarchive_domain::runtime::{ArtifactKind, RuntimeDefinition};

/// Unpacks a verified artifact by dispatching on the kind the definition pins.
///
/// The adapter is generic over the disk-image capability so that a test can supply
/// a fake, and so that a future platform with a different disk-image service
/// plugs in without touching the installer.
///
/// ```
/// use bitarchive_infrastructure::AppleDiskImageExtractor;
///
/// // The adapter is selected by the artifact kind, once a capability is supplied.
/// fn assert_artifact_kind(kind: &str) {
///     assert_eq!(kind, AppleDiskImageExtractor::<()>::ARTIFACT_KIND);
/// }
///
/// assert_artifact_kind("apple-disk-image");
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct AppleDiskImageExtractor<D> {
    images: D,
}

impl<D> AppleDiskImageExtractor<D> {
    /// The artifact kind this adapter handles.
    pub const ARTIFACT_KIND: &'static str = "apple-disk-image";

    /// Creates the adapter over a disk-image capability.
    #[must_use]
    pub const fn new(images: D) -> Self {
        Self { images }
    }
}

impl<D> ArtifactExtractor for AppleDiskImageExtractor<D>
where
    D: DiskImageExtractor,
{
    fn unpack(
        &self,
        definition: &RuntimeDefinition,
        artifact: &Artifact,
        working_directory: &Path,
    ) -> Result<(), RuntimeStoreError> {
        match definition.kind() {
            ArtifactKind::AppleDiskImage { .. } => {
                self.images
                    .extract_bundle(definition, artifact.path(), working_directory)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bitarchive_domain::runtime::RuntimeParts;

    use std::cell::RefCell;
    use std::str::FromStr;

    use bitarchive_application::managed_runtime::ArtifactSourceKind;
    use bitarchive_domain::runtime::{
        ArtifactSource, LicenseIdentifier, RelativePath, RuntimeAttribution, RuntimeId,
        RuntimePlatform, RuntimeSource, RuntimeVersion, Sha256Digest,
    };

    use super::*;

    /// Records what the adapter asked the disk-image capability to do.
    #[derive(Default)]
    struct RecordingImages {
        calls: RefCell<Vec<(std::path::PathBuf, std::path::PathBuf)>>,
    }

    impl DiskImageExtractor for RecordingImages {
        fn extract_bundle(
            &self,
            _definition: &RuntimeDefinition,
            image: &Path,
            destination: &Path,
        ) -> Result<(), RuntimeStoreError> {
            self.calls
                .borrow_mut()
                .push((image.to_path_buf(), destination.to_path_buf()));

            Ok(())
        }
    }

    /// A definition whose artifact is an Apple disk image.
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

    /// An Apple disk image is delegated to the disk-image capability, with the
    /// verified artifact's path and the caller's destination.
    #[test]
    fn an_apple_disk_image_is_delegated_to_the_disk_image_capability() {
        let images = RecordingImages::default();
        let adapter = AppleDiskImageExtractor::new(images);
        let artifact = Artifact::new(
            "/caches/artifacts/retroarch-1.22.2-macos-universal.dmg",
            Sha256Digest::from_str(
                "81b79121ba26d539064ae13b4d0419a120c3d165afbe656cf5f5412b15fdb434",
            )
            .expect("a valid digest"),
            ArtifactSourceKind::Network,
        );

        adapter
            .unpack(&definition(), &artifact, Path::new("/staging/payload"))
            .expect("the delegation succeeds");

        let calls = adapter.images.calls.borrow();

        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].0,
            std::path::PathBuf::from("/caches/artifacts/retroarch-1.22.2-macos-universal.dmg")
        );
        assert_eq!(calls[0].1, std::path::PathBuf::from("/staging/payload"));
    }

    /// The adapter reports the kind it handles, so metadata and diagnostics can
    /// name it without a second source of truth.
    #[test]
    fn the_adapter_names_the_artifact_kind_it_handles() {
        let definition = definition();

        assert_eq!(
            AppleDiskImageExtractor::<()>::ARTIFACT_KIND,
            definition.kind().as_str()
        );
        assert_eq!(
            AppleDiskImageExtractor::<()>::ARTIFACT_KIND,
            "apple-disk-image"
        );
    }
}
