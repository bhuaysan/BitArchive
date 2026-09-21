//! Reading which firmware files are available to a core.
//!
//! This is the concrete [`FirmwareChecker`] of the launch-readiness path. It answers
//! one question — are these specific file names present in the one firmware folder?
//! — and it answers it *read-only*:
//!
//! ```text
//! AppPaths::firmware()
//!         ↓
//! list the directory (one level, no recursion)
//!         ↓
//! intersect with the names the core asked for
//!         ↓
//! those names
//! ```
//!
//! # What it never does
//!
//! Firmware is a user-owned external resource (`AGENTS.md` §4, `ARCHITECTURE.md`
//! §25.3). Nothing here copies, moves, renames, repairs, deletes, downloads, or even
//! opens a firmware file: the only question is whether a name is present, and the
//! only information that leaves this module is that list of names. No bytes are read,
//! so no hash is computed and no firmware content is interpreted.
//!
//! # Why the caller decides the folder
//!
//! The folder is passed in, not looked up here. A launch resolves its paths through
//! [`AppPaths`](bitarchive_platform::AppPaths), and keeping the decision at that end
//! means this adapter contains no platform or layout knowledge at all — which is also
//! what lets its tests run against a temporary directory.
//!
//! # A missing folder is not an error
//!
//! A firmware folder that does not exist is the normal state of a fresh installation,
//! so it reports "none of these names are available" rather than an error. Only a
//! folder that exists and cannot be listed is an error, because that is a condition
//! the caller should see instead of having it silently become "no firmware".

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bitarchive_application::FirmwareChecker;

/// Answers firmware questions from one folder on disk.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FilesystemFirmwareChecker {
    root: PathBuf,
}

impl FilesystemFirmwareChecker {
    /// Creates a checker that reads `root`.
    ///
    /// The directory is not created, read, or validated here: constructing a checker
    /// performs no I/O, so a readiness check is the only thing that touches the disk.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the firmware folder this checker reads.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the names of the files directly inside the firmware folder.
    ///
    /// # Errors
    ///
    /// Returns the [`io::Error`] of a folder that exists but cannot be listed.
    fn file_names(&self) -> io::Result<Vec<String>> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            // No firmware folder is the normal state of a fresh installation, not a
            // failure: it simply provides nothing.
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };

        let mut names = Vec::new();

        for entry in entries {
            let entry = entry?;

            // Only the file name is read, and only for a regular file: directories,
            // symlinks, and anything else the platform reports are not firmware and
            // are not followed.
            if !entry.file_type()?.is_file() {
                continue;
            }

            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_owned());
            }
        }

        // The order of a directory listing is not part of the contract, so it is
        // normalized here instead of being passed on.
        names.sort();

        Ok(names)
    }
}

impl FirmwareChecker for FilesystemFirmwareChecker {
    /// Returns the subset of `expected` that the firmware folder contains.
    ///
    /// The comparison is an exact file-name match. It is deliberately not
    /// case-insensitive and does not consult a hash: `ARCHITECTURE.md` §25 models
    /// `FirmwareWrongFilename` and `FirmwareWrongContent` as separate findings, and
    /// BitArchive has no reviewed digest source for a commercial BIOS, so matching a
    /// wrong file by guessing would be worse than reporting the expected name as
    /// missing.
    ///
    /// The returned names keep the order of `expected`, so a result is deterministic
    /// and independent of how the directory happened to be listed.
    fn available(&self, expected: &[String]) -> io::Result<Vec<String>> {
        let present = self.file_names()?;

        Ok(expected
            .iter()
            .filter(|name| present.iter().any(|candidate| candidate == *name))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A uniquely named directory that removes itself afterwards.
    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        /// Creates an empty directory below the system temporary directory.
        fn create(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos());
            let path = env::temp_dir().join(format!(
                "bitarchive-firmware-{label}-{}-{nanos}",
                std::process::id()
            ));

            fs::create_dir_all(&path).expect("the temporary directory can be created");

            Self { path }
        }

        /// Returns the directory path.
        fn path(&self) -> &Path {
            &self.path
        }

        /// Writes a file with `name` and a single byte of content.
        fn write(&self, name: &str) {
            fs::write(self.path.join(name), b"x").expect("the firmware file can be written");
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// A checker that was never pointed at a directory reports every requested name
    /// as unavailable, and does so without an error: a fresh installation has no
    /// firmware folder, which is normal rather than broken.
    #[test]
    fn a_missing_firmware_folder_reports_nothing_available() {
        let root = TempRoot::create("missing");
        let checker = FilesystemFirmwareChecker::new(root.path().join("does-not-exist"));

        let available = checker
            .available(&[String::from("gba_bios.bin")])
            .expect("a folder that does not exist is not an error");

        assert!(available.is_empty());
    }

    /// A folder that exists but holds nothing reports nothing, which is the same
    /// answer as a folder that does not exist.
    #[test]
    fn an_empty_firmware_folder_reports_nothing_available() {
        let root = TempRoot::create("empty");
        let checker = FilesystemFirmwareChecker::new(root.path());

        assert!(
            checker
                .available(&[String::from("gba_bios.bin")])
                .expect("an empty folder can be listed")
                .is_empty()
        );
    }

    /// A present file is reported, and only the names that were asked for: the
    /// checker never enumerates the folder's whole content to its caller.
    #[test]
    fn only_the_requested_names_are_reported() {
        let root = TempRoot::create("requested");
        root.write("gba_bios.bin");
        root.write("unrelated.bin");

        let checker = FilesystemFirmwareChecker::new(root.path());

        assert_eq!(
            checker
                .available(&[String::from("gba_bios.bin")])
                .expect("the folder can be listed"),
            [String::from("gba_bios.bin")]
        );
        assert!(
            checker
                .available(&[String::from("absent.bin")])
                .expect("the folder can be listed")
                .is_empty(),
            "a file that is not there is never reported"
        );
    }

    /// The reported order follows the requested order, so the answer does not depend
    /// on how the operating system listed the directory.
    #[test]
    fn the_reported_order_follows_the_requested_order() {
        let root = TempRoot::create("order");
        root.write("second.bin");
        root.write("first.bin");

        let checker = FilesystemFirmwareChecker::new(root.path());

        let available = checker
            .available(&[
                String::from("second.bin"),
                String::from("first.bin"),
                String::from("absent.bin"),
            ])
            .expect("the folder can be listed");

        assert_eq!(
            available,
            [String::from("second.bin"), String::from("first.bin")]
        );
    }

    /// The file name comparison is exact: a differently cased or differently spelled
    /// file is not the file the core asked for.
    #[test]
    fn the_file_name_comparison_is_exact() {
        let root = TempRoot::create("exact");
        root.write("GBA_BIOS.BIN");

        let checker = FilesystemFirmwareChecker::new(root.path());

        assert!(
            checker
                .available(&[String::from("gba_bios.bin")])
                .expect("the folder can be listed")
                .is_empty(),
            "a differently cased name is not reported as the expected file"
        );
    }

    /// A directory is not a firmware file, even when its name matches.
    #[test]
    fn a_directory_is_not_a_firmware_file() {
        let root = TempRoot::create("directory");
        fs::create_dir_all(root.path().join("gba_bios.bin")).expect("the directory can be created");

        let checker = FilesystemFirmwareChecker::new(root.path());

        assert!(
            checker
                .available(&[String::from("gba_bios.bin")])
                .expect("the folder can be listed")
                .is_empty(),
            "a directory named like firmware does not satisfy a firmware requirement"
        );
    }

    /// Constructing a checker reads nothing, so asking where firmware lives has no
    /// side effect and creates no directory.
    #[test]
    fn constructing_a_checker_creates_nothing() {
        let root = TempRoot::create("constructing");
        let missing = root.path().join("firmware");

        let checker = FilesystemFirmwareChecker::new(&missing);

        assert_eq!(checker.root(), missing.as_path());
        assert!(
            !missing.exists(),
            "a checker never creates the firmware folder"
        );
    }
}
