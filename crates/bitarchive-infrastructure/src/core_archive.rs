//! Taking the pinned library out of a verified libretro core archive.
//!
//! This is the concrete [`CoreArchiveExtractor`] of the curated core path
//! (Issue #21). It answers the one question the installer asks — "write the
//! library this definition pins into this directory" — and it answers it with a
//! deliberately narrow rule:
//!
//! ```text
//! artifact (SHA-256 already verified by the downloader)
//!     ↓  find entries whose name is exactly the pinned member
//! none        → ArchiveMemberMissing
//! more than one → ArchiveMemberDuplicated
//! exactly one  → it must be a regular file, and its name must be the pinned
//!                relative member path → otherwise ArchiveMemberUnsafe
//!     ↓  stream it to <working directory>/<pinned library path>
//! the staged library
//! ```
//!
//! # The archive never decides a destination
//!
//! Nothing here unpacks "everything into a directory". The destination is derived
//! from the definition's [`RelativePath`](bitarchive_domain::component::RelativePath),
//! which cannot be absolute and cannot contain `..`, and the archive's own entry
//! name is only ever *compared* against the pinned member. An entry named
//! `../../etc/passwd`, an absolute name, a name with a NUL byte, a directory, and
//! a symbolic link are all refused before anything is written
//! (ARCHITECTURE.md §11, invariant 20).
//!
//! # There is no second compression backend
//!
//! The official libretro build host publishes a core as one DEFLATE member inside
//! a ZIP archive. The `zip` dependency is therefore built with `default-features =
//! false` and only the pure-Rust `zlib-rs` DEFLATE backend: no C toolchain, no
//! system zlib, and no bzip2, LZMA, zstd, PPMd, AES, or ZIP64 machinery that a
//! core archive does not use. An archive that needs any of those is
//! [`CoreStoreError::ArchiveUnreadable`], not a reason to add another decoder.
//!
//! # What this module does not do
//!
//! It does not verify the artifact: the SHA-256 check happened in the downloader,
//! before these bytes were trusted. It does not load the library: a core is never
//! opened into the BitArchive process, not even to validate it. And it does not
//! decide where a build is installed: it writes into the working directory it is
//! given, and the store owns everything above that.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;

use bitarchive_application::managed_core::{CoreArchiveExtractor, CoreStoreError};
use bitarchive_application::managed_runtime::{Artifact, RuntimeStoreError};
use bitarchive_domain::managed_core::{CoreArtifactKind, CoreDefinition};
use zip::ZipArchive;
use zip::read::ZipFile;
use zip::result::ZipError;

use crate::store_layout::below;

/// The buffer a member is streamed through, so a library is never held in memory.
const COPY_BUFFER_SIZE: usize = 64 * 1024;

/// Reads a verified libretro core archive and writes the one pinned library.
///
/// The extractor is stateless and cheap to construct; it is generic over nothing
/// because the archive format is the core archive format.
///
/// ```
/// use bitarchive_infrastructure::ZipCoreArchiveExtractor;
///
/// assert_eq!(
///     ZipCoreArchiveExtractor::ARTIFACT_KIND,
///     "libretro-core-archive"
/// );
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ZipCoreArchiveExtractor;

impl ZipCoreArchiveExtractor {
    /// The artifact kind this adapter handles.
    pub const ARTIFACT_KIND: &'static str = "libretro-core-archive";

    /// The largest library, in bytes, this extractor will write.
    ///
    /// The pinned digest is the real guard against a wrong archive, so this is not
    /// a security boundary; it is the point at which "this member is not a core
    /// library" becomes a structured refusal instead of filling the disk. The
    /// largest libretro cores are a few tens of megabytes, so the limit is
    /// generous by an order of magnitude.
    pub const MAX_LIBRARY_SIZE: u64 = 256 * 1024 * 1024;

    /// Creates the extractor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl CoreArchiveExtractor for ZipCoreArchiveExtractor {
    fn unpack(
        &self,
        definition: &CoreDefinition,
        artifact: &Artifact,
        working_directory: &Path,
    ) -> Result<(), CoreStoreError> {
        match definition.kind() {
            CoreArtifactKind::LibretroCoreArchive { .. } => {
                self.unpack_member(definition, artifact.path(), working_directory)
            }
        }
    }
}

impl ZipCoreArchiveExtractor {
    /// Finds the pinned member in `artifact` and writes it into
    /// `working_directory`.
    fn unpack_member(
        &self,
        definition: &CoreDefinition,
        artifact: &Path,
        working_directory: &Path,
    ) -> Result<(), CoreStoreError> {
        let member = definition.expected_member();

        let file = File::open(artifact).map_err(|cause| unreadable(artifact, cause))?;

        let mut archive =
            ZipArchive::new(file).map_err(|cause| unreadable(artifact, zip_cause(cause)))?;

        let mut occurrences = 0usize;
        let mut matching = None;
        let mut found = Vec::with_capacity(archive.len());

        // The reader keys its entries by raw name, so an archive that carried the
        // same name twice arrives here as *one* entry: `zip` resolves that ambiguity
        // itself, and which of the two survives is its decision and not this code's.
        // The count therefore stays in place as the extractor's own rule — an
        // ambiguous archive is refused rather than resolved — even though the
        // current backend cannot deliver that input.
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|cause| unreadable(artifact, zip_cause(cause)))?;

            let name = entry.name().to_owned();

            if name == member {
                occurrences += 1;
                matching = Some(index);
            }

            found.push(name);
        }

        match (occurrences, matching) {
            (0, _) => Err(CoreStoreError::ArchiveMemberMissing {
                artifact: artifact.to_path_buf(),
                expected: member.to_owned(),
                found,
            }),
            (1, Some(index)) => {
                self.write_member(definition, artifact, &mut archive, index, working_directory)
            }
            (occurrences, _) => Err(CoreStoreError::ArchiveMemberDuplicated {
                artifact: artifact.to_path_buf(),
                member: member.to_owned(),
                occurrences,
            }),
        }
    }

    /// Writes the one matching entry, after checking that it may be written at
    /// all.
    fn write_member(
        &self,
        definition: &CoreDefinition,
        artifact: &Path,
        archive: &mut ZipArchive<File>,
        index: usize,
        working_directory: &Path,
    ) -> Result<(), CoreStoreError> {
        let member = definition.expected_member();

        let mut entry = archive
            .by_index(index)
            .map_err(|cause| unreadable(artifact, zip_cause(cause)))?;

        self.check_member(artifact, &entry)?;

        let destination = below(working_directory, definition.library());

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|cause| {
                CoreStoreError::store(RuntimeStoreError::io(
                    "creating the staged library directory",
                    parent,
                    cause,
                ))
            })?;
        }

        let mut file = File::create(&destination).map_err(|cause| {
            CoreStoreError::store(RuntimeStoreError::io(
                "creating the staged core library",
                &destination,
                cause,
            ))
        })?;

        let written = copy_bounded(&mut entry, &mut file, artifact, member);

        if written.is_err() {
            // A refused or failed member must not leave a partial library behind,
            // so the installer's "the staged payload is what the definition pins"
            // assumption keeps holding.
            let _ = fs::remove_file(&destination);
        }

        written
    }

    /// Refuses an entry that may not become the installed library.
    ///
    /// The checks are the ones the port's contract names: the entry must be the
    /// pinned relative member path, it must be a regular file, and it must not
    /// claim to be larger than a core library can be.
    fn check_member(
        &self,
        artifact: &Path,
        entry: &ZipFile<'_, File>,
    ) -> Result<(), CoreStoreError> {
        let member = entry.name();

        match entry.enclosed_name() {
            Some(enclosed) if enclosed == Path::new(member) => {}
            Some(_) => {
                return Err(unsafe_member(
                    artifact,
                    member,
                    "the member name is not a plain relative path",
                ));
            }
            None => {
                return Err(unsafe_member(
                    artifact,
                    member,
                    "the member name is absolute, contains a NUL byte, or escapes the archive root",
                ));
            }
        }

        if !entry.is_file() {
            return Err(unsafe_member(
                artifact,
                member,
                "the member is a directory or a symbolic link, not a regular file",
            ));
        }

        if entry.size() > Self::MAX_LIBRARY_SIZE {
            return Err(unsafe_member(
                artifact,
                member,
                &format!(
                    "the member declares {} bytes, more than the {}-byte limit for a core library",
                    entry.size(),
                    Self::MAX_LIBRARY_SIZE
                ),
            ));
        }

        Ok(())
    }
}

/// Streams `entry` into `file`, refusing to write more than the library limit.
///
/// The declared size of a ZIP entry is metadata an archive supplies about itself,
/// so the limit is enforced on the bytes that are actually read as well.
fn copy_bounded(
    entry: &mut ZipFile<'_, File>,
    file: &mut File,
    artifact: &Path,
    member: &str,
) -> Result<(), CoreStoreError> {
    let mut buffer = vec![0u8; COPY_BUFFER_SIZE];
    let mut written = 0u64;

    loop {
        let read = match entry.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(cause) => {
                return Err(CoreStoreError::store(RuntimeStoreError::UnpackFailed {
                    kind: String::from(ZipCoreArchiveExtractor::ARTIFACT_KIND),
                    cause,
                }));
            }
        };

        written += read as u64;

        if written > ZipCoreArchiveExtractor::MAX_LIBRARY_SIZE {
            return Err(unsafe_member(
                artifact,
                member,
                &format!(
                    "the member expands to more than the {}-byte limit for a core library",
                    ZipCoreArchiveExtractor::MAX_LIBRARY_SIZE
                ),
            ));
        }

        file.write_all(&buffer[..read]).map_err(|cause| {
            CoreStoreError::store(RuntimeStoreError::io(
                "writing the staged core library",
                artifact,
                cause,
            ))
        })?;
    }

    Ok(())
}

/// Builds the "the archive could not be read" failure.
fn unreadable(artifact: &Path, cause: io::Error) -> CoreStoreError {
    CoreStoreError::ArchiveUnreadable {
        artifact: artifact.to_path_buf(),
        cause,
    }
}

/// Builds the "this member may not be installed" failure.
fn unsafe_member(artifact: &Path, member: &str, reason: &str) -> CoreStoreError {
    CoreStoreError::ArchiveMemberUnsafe {
        artifact: artifact.to_path_buf(),
        member: member.to_owned(),
        reason: reason.to_owned(),
    }
}

/// Converts a `zip` failure into the I/O failure the port reports.
///
/// A malformed archive is an invalid input, and an archive that needs a
/// compression method BitArchive does not build is reported as such instead of
/// being silently treated as empty.
fn zip_cause(cause: ZipError) -> io::Error {
    match cause {
        ZipError::Io(cause) => cause,
        cause => io::Error::new(io::ErrorKind::InvalidData, cause.to_string()),
    }
}
