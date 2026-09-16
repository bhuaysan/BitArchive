//! BitArchive domain layer.
//!
//! This crate owns BitArchive's domain identities and the minimal model of the
//! launch path: [`Game`], [`Release`], and [`Content`], plus the [`SystemId`]
//! and [`CoreId`] identities that later resolvers need.
//!
//! It also owns the first fully deterministic launch rule, the core selection
//! policy in [`core`] (ARCHITECTURE.md §20.4), and the pinned description of a
//! managed runtime component in [`runtime`] (ARCHITECTURE.md §23).
//!
//! It is deliberately small. It carries no titles, descriptions, regions,
//! languages, release types, hashes, paths, scanner or scraping state, save
//! states, or configuration. Those are added by the Issues that actually need
//! them, not in advance.
//!
//! # Boundary
//!
//! Dependencies point inwards (ARCHITECTURE.md §2.4). This crate depends on
//! Rust and the UUID crate only, and must stay free of:
//!
//! - Slint
//! - SQLite / rusqlite
//! - Tokio
//! - HTTP clients such as reqwest
//! - OS APIs
//! - RetroArch process, config, or path details
//! - filesystem adapters
//!
//! Consequently there are no ROM paths, core library paths, RetroArch
//! executable paths, `.cfg` paths, environment variables, or launch arguments
//! in any domain type.
//!
//! [`runtime`] is the one place where a domain type names a location, and it does
//! so without touching the filesystem: an [`ArtifactSource`] is a reviewed,
//! validated URL string and a [`RelativePath`] is a path *below* an installation
//! directory that cannot contain `..` or an absolute component. Nothing in this
//! crate opens, reads, or writes anything.
//!
//! # Model
//!
//! ```text
//! Game
//!   1 ─── * Release
//!             1 ─── * Content
//! ```
//!
//! Every child stores the identity of its parent, so the relationships are
//! traversable from the child side. `Game != Release != Content`
//! (ARCHITECTURE.md §53.1).

mod content;
pub mod core;
mod game;
mod id;
mod release;
pub mod runtime;

pub use content::Content;
pub use core::{CoreSelectionSource, ResolvedCore, resolve_core};
pub use game::Game;
pub use id::{ContentId, CoreId, GameId, ReleaseId, SystemId};
pub use release::Release;
pub use runtime::{
    ArtifactKind, ArtifactSource, LicenseIdentifier, LoopbackSource, RelativePath,
    RuntimeAttribution, RuntimeDefinition, RuntimeId, RuntimeParts, RuntimePlatform, RuntimeSource,
    RuntimeVersion, Sha256Digest,
};
