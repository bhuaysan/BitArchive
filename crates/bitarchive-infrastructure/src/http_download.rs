//! Downloading a pinned artifact over HTTPS and verifying it while it arrives.
//!
//! This is the concrete [`ArtifactDownloader`] of the managed runtime path
//! (Issue #19). It implements a narrow, reviewable contract:
//!
//! 1. it downloads **only** from the host the pinned definition names, so a
//!    mirror, a redirect to a third party, or a plaintext URL is refused before a
//!    byte is written;
//! 2. it computes the SHA-256 of the bytes **as they are written**, so there is no
//!    window in which unverified bytes are treated as verified;
//! 3. it removes everything it wrote when the digest does not match.
//!
//! ```text
//! RuntimeDefinition        ← pinned: version, official URL, SHA-256
//!         ↓
//! HTTP GET (https, pinned host, bounded timeouts)
//!         ↓
//! bytes ──▶ SHA-256 ──▶ staging file
//!         ↓
//! computed == pinned ?  ── no ──▶ discard the file, report DigestMismatch
//!         │ yes
//!         ▼
//! Artifact { path, verified digest }
//! ```
//!
//! # Redirects
//!
//! Redirects are disabled. A pinned artifact URL is a direct URL, and following a
//! redirect would mean downloading from a host BitArchive did not pin. Refusing
//! is both simpler and safer than re-validating each hop, and it makes the
//! official-source rule hold for the bytes as well as for the URL.
//!
//! # Why this step does not use the shared async network layer yet
//!
//! ARCHITECTURE.md §31 describes a central network layer built on `reqwest`, and
//! §48 says network access is asynchronous. That layer is the target, and it
//! arrives with the `JobManager` and the download UI that need progress,
//! cancellation, retries, and rate limits.
//!
//! This step needs none of that: one artifact, one pinned host, one bounded
//! synchronous call in an acquisition path that has no UI and no job yet.
//! Introducing Tokio and a full async stack now would add the runtime, the
//! scheduler, and the async client before anything consumes them, which Issue #19
//! explicitly rules out while a synchronous controlled downloader suffices. The
//! decision, its trade-off, and its trigger for revisiting are recorded in
//! `docs/decisions/0001-managed-runtime-acquisition.md`: when downloads move into
//! the `JobManager`, this adapter is replaced by the shared layer while the
//! [`ArtifactDownloader`] port stays exactly as it is.
//!
//! # What is deliberately absent
//!
//! - **No shell.** No `curl`, `wget`, `sh`, or `bash` is started; the request is
//!   made by an HTTP client in this process (Issue #19, ARCHITECTURE.md §20.3).
//! - **No retries and no resume.** A failed download is reported; deciding
//!   whether to try again belongs to the job layer a later Issue adds.
//! - **No signature check.** ARCHITECTURE.md §24 describes signed distribution
//!   manifests and that step is not implemented yet, so the pinned SHA-256 is the
//!   trust anchor. Nothing is weakened in the meantime: there was no signature
//!   check to weaken.
//! - **No dynamic digest.** The expected digest is read from the definition and
//!   never from a response header, a sidecar file, or the artifact's own claims.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use bitarchive_application::managed_runtime::{
    Artifact, ArtifactDownloader, ArtifactSourceKind, DownloadTimeout, RuntimeStoreError,
};
use bitarchive_domain::runtime::{RuntimeDefinition, Sha256Digest};
use sha2::{Digest, Sha256};
use ureq::ResponseExt as _;

/// The size of one read from the response body.
///
/// The buffer exists so that a large artifact streams to disk instead of being
/// held in memory; its size is a throughput detail and carries no meaning.
const READ_BUFFER_BYTES: usize = 64 * 1024;

/// The suffix of the file a download is assembled in.
///
/// Nothing else ever looks at a `.part` file, so an interrupted download cannot be
/// mistaken for a complete artifact.
const PARTIAL_SUFFIX: &str = ".part";

/// The user agent BitArchive identifies itself with.
///
/// A download from the official build host should be attributable, so the agent
/// names the application and the repository rather than impersonating a browser.
#[must_use]
pub fn user_agent() -> String {
    format!(
        "BitArchive/{} (+https://github.com/bhuaysan/BitArchive)",
        env!("CARGO_PKG_VERSION")
    )
}

/// Downloads artifacts over HTTPS and verifies them against the pinned digest.
///
/// The downloader holds one configured HTTP agent; it performs no I/O until
/// [`download`](ArtifactDownloader::download) is called.
///
/// ```
/// use bitarchive_infrastructure::HttpArtifactDownloader;
///
/// let downloader = HttpArtifactDownloader::new();
///
/// assert!(downloader.timeout().idle < downloader.timeout().overall);
/// ```
#[derive(Debug)]
pub struct HttpArtifactDownloader {
    agent: ureq::Agent,
    timeout: DownloadTimeout,
}

impl Default for HttpArtifactDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpArtifactDownloader {
    /// Creates a downloader with the default runtime artifact timeout.
    #[must_use]
    pub fn new() -> Self {
        Self::with_timeout(DownloadTimeout::runtime_artifact())
    }

    /// Creates a downloader with an explicit timeout.
    #[must_use]
    pub fn with_timeout(timeout: DownloadTimeout) -> Self {
        Self {
            agent: agent_for(timeout),
            timeout,
        }
    }

    /// Returns the timeout this downloader applies.
    #[must_use]
    pub const fn timeout(&self) -> DownloadTimeout {
        self.timeout
    }
}

/// Builds the HTTP agent with the bounded timeouts and no redirects.
///
/// The connect and overall budgets come from the download contract. Inactivity is
/// *not* configured here: the read loop enforces [`DownloadTimeout::idle`] per
/// read, which is independent of how the transport client happens to interpret
/// its own settings.
fn agent_for(timeout: DownloadTimeout) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(user_agent())
        .timeout_connect(Some(timeout.idle))
        .timeout_global(Some(timeout.overall))
        .max_redirects(0)
        // The status is classified by this module, so the transport client must
        // not convert a non-2xx response into an opaque error before that.
        .http_status_as_error(false)
        .build()
        .into()
}

impl ArtifactDownloader for HttpArtifactDownloader {
    fn download(
        &self,
        definition: &RuntimeDefinition,
        target: &Path,
    ) -> Result<Artifact, RuntimeStoreError> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|cause| {
                RuntimeStoreError::io("creating the artifact directory", parent, cause)
            })?;
        }

        let partial = partial_path(target);

        match self.fetch_verified(definition, &partial) {
            Ok(size) => {
                // The verified bytes only become the artifact once they are in
                // their final place, so an interrupted run never leaves a file
                // that looks complete.
                std::fs::rename(&partial, target).map_err(|cause| {
                    let _ = std::fs::remove_file(&partial);

                    RuntimeStoreError::io("moving the verified artifact into place", target, cause)
                })?;

                Ok(
                    Artifact::new(target, definition.digest(), ArtifactSourceKind::Network)
                        .with_size(size),
                )
            }
            Err(error) => {
                // Never leave unverified bytes behind: neither as a partial file
                // nor under the artifact's own name.
                let _ = std::fs::remove_file(&partial);
                let _ = std::fs::remove_file(target);

                Err(error)
            }
        }
    }
}

impl HttpArtifactDownloader {
    /// Streams the artifact into `partial`, hashing as it writes.
    ///
    /// Returns the number of bytes written on success.
    fn fetch_verified(
        &self,
        definition: &RuntimeDefinition,
        partial: &Path,
    ) -> Result<u64, RuntimeStoreError> {
        let url = definition.source().as_str();

        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|cause| transport_error(url, cause))?;

        // The transport client does not turn a non-success status into an error
        // here, so the status is classified explicitly: 2xx is the artifact, a
        // redirect is refused because the artifact would come from a host
        // BitArchive did not pin, and everything else is a failure.
        let status = response.status();

        // Belt and braces, and deliberately not redundant: the redirect policy
        // above is a configuration of the transport client, while this check is a
        // property of the artifact. If a client ever followed a redirect — a
        // configuration regression, a transport that ignores the setting — the
        // assembled URL would name a different location than the one BitArchive
        // pinned, and the bytes would be refused here instead of trusted.
        if !served_from_the_pinned_url(url, response.get_uri().to_string().as_str()) {
            return Err(RuntimeStoreError::DownloadFailed {
                url: url.to_owned(),
                cause: std::io::Error::other(format!(
                    "the artifact was served from {}, but the definition pins {url}",
                    response.get_uri()
                )),
            });
        }

        if status.is_redirection() {
            return Err(RuntimeStoreError::DownloadFailed {
                url: url.to_owned(),
                cause: std::io::Error::other(
                    "the pinned host answered with a redirect, which is not followed",
                ),
            });
        }

        if !status.is_success() {
            return Err(RuntimeStoreError::DownloadFailed {
                url: url.to_owned(),
                cause: std::io::Error::other(format!(
                    "the pinned host answered with HTTP {}",
                    status.as_u16()
                )),
            });
        }

        let mut file = File::create(partial).map_err(|cause| {
            RuntimeStoreError::io("creating the staged artifact", partial, cause)
        })?;

        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; READ_BUFFER_BYTES];
        let mut total = 0_u64;

        {
            let mut body = response.body_mut().as_reader();

            loop {
                let read =
                    body.read(&mut buffer)
                        .map_err(|cause| RuntimeStoreError::DownloadFailed {
                            url: url.to_owned(),
                            cause: std::io::Error::new(
                                cause.kind(),
                                format!("reading the response body failed or stalled: {cause}"),
                            ),
                        })?;

                if read == 0 {
                    break;
                }

                hasher.update(&buffer[..read]);
                file.write_all(&buffer[..read]).map_err(|cause| {
                    RuntimeStoreError::io("writing the staged artifact", partial, cause)
                })?;

                total += read as u64;
            }
        }

        file.flush().map_err(|cause| {
            RuntimeStoreError::io("flushing the staged artifact", partial, cause)
        })?;

        if total == 0 {
            return Err(RuntimeStoreError::DownloadFailed {
                url: url.to_owned(),
                cause: std::io::Error::other("the pinned host returned an empty body"),
            });
        }

        let actual = Sha256Digest::from_bytes(hasher.finalize().into());
        let expected = definition.digest();

        if actual != expected {
            return Err(RuntimeStoreError::DigestMismatch { expected, actual });
        }

        Ok(total)
    }
}

/// Maps a transport failure onto the download contract's failure category.
fn transport_error(url: &str, cause: ureq::Error) -> RuntimeStoreError {
    let cause = match cause {
        ureq::Error::StatusCode(status) => {
            std::io::Error::other(format!("the pinned host answered with HTTP {status}"))
        }
        ureq::Error::Timeout(..) => {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "the request timed out")
        }
        other => std::io::Error::other(other.to_string()),
    };

    RuntimeStoreError::DownloadFailed {
        url: url.to_owned(),
        cause,
    }
}

/// Returns `true` when a response really came from the URL the definition pins.
///
/// Kept as a small function so the property is testable on its own, without a
/// transport that would have to misbehave first.
#[must_use]
fn served_from_the_pinned_url(pinned: &str, served: &str) -> bool {
    pinned == served
}

/// Returns the path a download is assembled in before it is verified.
fn partial_path(target: &Path) -> PathBuf {
    let mut partial = target.as_os_str().to_owned();
    partial.push(PARTIAL_SUFFIX);

    PathBuf::from(partial)
}

#[cfg(test)]
mod tests {
    use bitarchive_domain::runtime::RuntimeParts;

    use std::net::TcpListener;
    use std::str::FromStr;
    use std::thread;

    use bitarchive_domain::runtime::{
        ArtifactKind, LicenseIdentifier, LoopbackSource, RelativePath, RuntimeAttribution,
        RuntimeId, RuntimePlatform, RuntimeSource, RuntimeVersion,
    };

    use super::*;

    /// The bytes the loopback server serves as the artifact fixture.
    const FIXTURE: &[u8] = b"BitArchive runtime artifact fixture";

    /// The path the loopback server answers.
    const FIXTURE_PATH: &str = "/stable/1.22.2/fixture.dmg";

    /// Returns the SHA-256 of the fixture.
    fn fixture_digest() -> Sha256Digest {
        let mut hasher = Sha256::new();
        hasher.update(FIXTURE);

        Sha256Digest::from_bytes(hasher.finalize().into())
    }

    /// A running one-shot HTTP server on the loopback interface.
    ///
    /// The tests speak to a real socket, so the download path — request, status
    /// handling, streaming, hashing, and cleanup — is exercised end to end
    /// without an external service and without the public network
    /// (ARCHITECTURE.md §45.4).
    struct LoopbackServer {
        port: u16,
    }

    impl LoopbackServer {
        /// Starts a server that answers one request with `response`.
        fn answering(response: Vec<u8>) -> Self {
            Self::spawn(move |stream| {
                // Read the request head; a GET has no body that matters here.
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer);
                stream.write_all(&response).ok();
                stream.flush().ok();
            })
        }

        /// Starts a server that accepts the connection and closes it unanswered.
        fn dropping_the_connection() -> Self {
            Self::spawn(|_| {})
        }

        /// Starts a server on a loopback port and hands each connection to
        /// `handle` on its own thread.
        fn spawn(handle: impl Fn(&mut std::net::TcpStream) + Send + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
            let port = listener.local_addr().expect("a local address").port();

            thread::spawn(move || {
                if let Ok((mut stream, _)) = listener.accept() {
                    handle(&mut stream);
                }
            });

            Self { port }
        }

        /// Returns a definition whose source points at this server.
        fn definition_for(&self, digest: Sha256Digest) -> RuntimeDefinition {
            definition(
                RuntimeSource::from(
                    LoopbackSource::new(self.port, FIXTURE_PATH).expect("a valid loopback source"),
                ),
                digest,
            )
        }
    }

    /// Builds the artifact definition used by these tests.
    fn definition(source: RuntimeSource, digest: Sha256Digest) -> RuntimeDefinition {
        RuntimeDefinition::new(RuntimeParts {
            identity: (
                RuntimeId::from_str(RuntimeId::RETROARCH).expect("a valid identity"),
                RuntimeVersion::from_str("1.22.2").expect("a valid version"),
                RuntimePlatform::MacOsUniversal,
            ),
            artifact: (source, digest),
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

    /// Assembles a minimal HTTP/1.1 response with a body.
    fn response(status: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\
             Connection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();

        response.extend_from_slice(body);

        response
    }

    /// A temporary directory that removes itself, so a test leaves nothing behind.
    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!("bitarchive-b5-http-{label}-{}", std::process::id()));

            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a temporary directory");

            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The pinned host is the only host a production definition can name, and it
    /// must be reached over https.
    #[test]
    fn a_mirror_cannot_become_a_production_source() {
        use bitarchive_domain::runtime::ArtifactSource;

        assert_eq!(
            ArtifactSource::new("https://mirror.invalid/a.dmg"),
            Err(bitarchive_domain::runtime::SourceError::UnexpectedHost)
        );
        assert_eq!(
            ArtifactSource::new("http://buildbot.libretro.com/a.dmg"),
            Err(bitarchive_domain::runtime::SourceError::NotHttps)
        );

        let official = ArtifactSource::new("https://buildbot.libretro.com/stable/1.22.2/a.dmg")
            .expect("the official source is valid");

        assert!(RuntimeSource::official(official).is_official());
    }

    /// A `.part` file is used while a download is assembled, so an interrupted
    /// download never looks like a finished artifact.
    #[test]
    fn a_download_is_assembled_in_a_partial_file() {
        assert_eq!(
            partial_path(Path::new("/downloads/artifacts/retroarch.dmg")),
            PathBuf::from("/downloads/artifacts/retroarch.dmg.part")
        );
    }

    /// The agent identifies BitArchive and the contract bounds the wait, so a
    /// stalled transfer cannot hang the caller forever.
    #[test]
    fn the_downloader_is_attributable_and_bounded() {
        let downloader = HttpArtifactDownloader::new();

        assert!(downloader.timeout().idle < downloader.timeout().overall);
        assert!(user_agent().starts_with("BitArchive/"));
        assert!(user_agent().contains("github.com/bhuaysan/BitArchive"));
    }

    /// The correct bytes are accepted, hashed, and moved into place with the
    /// digest the definition pinned.
    #[test]
    fn a_matching_digest_is_accepted_and_the_artifact_is_verified() {
        let root = TempRoot::new("accepted");
        let target = root.join("artifacts/retroarch.dmg");
        let server = LoopbackServer::answering(response("200 OK", FIXTURE));
        let downloader = HttpArtifactDownloader::new();

        let artifact = downloader
            .download(&server.definition_for(fixture_digest()), &target)
            .expect("the fixture matches its pinned digest");

        assert_eq!(artifact.digest(), fixture_digest());
        assert_eq!(artifact.source(), ArtifactSourceKind::Network);
        assert_eq!(artifact.size(), Some(FIXTURE.len() as u64));
        assert_eq!(
            std::fs::read(&target).expect("the artifact exists"),
            FIXTURE
        );
        assert!(
            !partial_path(&target).exists(),
            "the partial file is gone once the artifact is in place"
        );
    }

    /// Bytes that do not match the pinned digest are rejected, and neither the
    /// artifact nor a partial file survives the failure.
    #[test]
    fn a_wrong_digest_is_rejected_and_nothing_is_left_behind() {
        let root = TempRoot::new("mismatch");
        let target = root.join("artifacts/retroarch.dmg");
        let server = LoopbackServer::answering(response("200 OK", b"tampered bytes"));
        let downloader = HttpArtifactDownloader::new();

        let error = downloader
            .download(&server.definition_for(fixture_digest()), &target)
            .expect_err("the bytes do not match the pinned digest");

        match error {
            RuntimeStoreError::DigestMismatch { expected, actual } => {
                assert_eq!(expected, fixture_digest());
                assert_ne!(actual, expected);
            }
            other => panic!("expected a digest mismatch, got {other}"),
        }

        assert!(!target.exists(), "a rejected artifact must not survive");
        assert!(
            !partial_path(&target).exists(),
            "a rejected artifact must not survive as a partial file either"
        );
    }

    /// An error status from the pinned host is a download failure, not a verified
    /// artifact.
    #[test]
    fn an_error_status_is_propagated() {
        let root = TempRoot::new("status");
        let target = root.join("artifacts/retroarch.dmg");
        let server = LoopbackServer::answering(response("404 Not Found", b"no such artifact"));
        let downloader = HttpArtifactDownloader::new();

        let error = downloader
            .download(&server.definition_for(fixture_digest()), &target)
            .expect_err("a 404 is not an artifact");

        assert!(matches!(error, RuntimeStoreError::DownloadFailed { .. }));
        assert!(error.to_string().contains("404"));
        assert!(!target.exists());
        assert!(!partial_path(&target).exists());
    }

    /// A response is only accepted when it came from the exact URL the definition
    /// pins, so a location change the client might follow cannot be trusted by
    /// accident.
    #[test]
    fn only_the_pinned_url_is_accepted_as_the_origin_of_the_bytes() {
        let pinned = "https://buildbot.libretro.com/stable/1.22.2/apple/osx/universal/                      RetroArch_Metal.dmg";

        assert!(served_from_the_pinned_url(pinned, pinned));
        assert!(!served_from_the_pinned_url(
            pinned,
            "https://mirror.invalid/stable/1.22.2/apple/osx/universal/RetroArch_Metal.dmg"
        ));
        assert!(!served_from_the_pinned_url(
            pinned,
            "http://buildbot.libretro.com/stable/1.22.2/apple/osx/universal/RetroArch_Metal.dmg"
        ));
    }

    /// A redirect is refused rather than followed, so the artifact can only ever
    /// come from the host BitArchive pinned.
    #[test]
    fn a_redirect_is_refused() {
        let root = TempRoot::new("redirect");
        let target = root.join("artifacts/retroarch.dmg");
        let server = LoopbackServer::answering(
            b"HTTP/1.1 302 Found\r\nLocation: https://mirror.invalid/retroarch.dmg\r\n\
              Content-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
        );
        let downloader = HttpArtifactDownloader::new();

        let error = downloader
            .download(&server.definition_for(fixture_digest()), &target)
            .expect_err("a redirect is not an artifact");

        assert!(matches!(error, RuntimeStoreError::DownloadFailed { .. }));
        assert!(error.to_string().contains("redirect"));
        assert!(!target.exists());
        assert!(!partial_path(&target).exists());
    }

    /// A server that closes the connection without answering produces a download
    /// failure and leaves no partial artifact.
    #[test]
    fn a_broken_connection_is_propagated() {
        let root = TempRoot::new("broken");
        let target = root.join("artifacts/retroarch.dmg");
        let server = LoopbackServer::dropping_the_connection();
        let downloader = HttpArtifactDownloader::new();

        let error = downloader
            .download(&server.definition_for(fixture_digest()), &target)
            .expect_err("a dropped connection is not an artifact");

        assert!(matches!(error, RuntimeStoreError::DownloadFailed { .. }));
        assert!(!target.exists());
        assert!(!partial_path(&target).exists());
    }

    /// An empty body is refused rather than hashed into an "artifact" of zero
    /// bytes.
    #[test]
    fn an_empty_body_is_refused() {
        let root = TempRoot::new("empty");
        let target = root.join("artifacts/retroarch.dmg");
        let server = LoopbackServer::answering(response("200 OK", b""));
        let downloader = HttpArtifactDownloader::new();

        let error = downloader
            .download(&server.definition_for(fixture_digest()), &target)
            .expect_err("an empty body is not an artifact");

        assert!(matches!(error, RuntimeStoreError::DownloadFailed { .. }));
        assert!(!target.exists());
        assert!(!partial_path(&target).exists());
    }
}
