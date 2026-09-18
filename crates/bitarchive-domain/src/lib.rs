//! BitArchive domain layer.
//!
//! This crate owns BitArchive's domain identities and the minimal model of the
//! launch path: [`Game`], [`Release`], and [`Content`], plus the [`SystemId`]
//! and [`CoreId`] identities that later resolvers need.
//!
//! It also owns the first fully deterministic launch rule, the core selection
//! policy in [`core`] (ARCHITECTURE.md §20.4), the pinned description of a
//! managed runtime component in [`runtime`] (ARCHITECTURE.md §23), the pinned
//! description of a curated core component in [`core`]'s sibling [`managed_core`],
//! and the value types those two descriptions genuinely share in [`component`].
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
//! - archive libraries
//!
//! Consequently there are no ROM paths, core library paths, RetroArch
//! executable paths, `.cfg` paths, environment variables, or launch arguments
//! in any domain type.
//!
//! [`runtime`] and [`component`] are the only places where a domain type names a
//! location, and they do so without touching the filesystem: an
//! [`ArtifactSource`] is a reviewed, validated URL
//! string and a [`RelativePath`] is a path *below* an
//! installation directory that cannot contain `..` or an absolute component.
//! Nothing in this crate opens, reads, writes, decompresses, or hashes anything.
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
//!
//! # Three different "core" notions
//!
//! They are separate on purpose and must not be conflated:
//!
//! ```text
//! CoreId                fachliche Identität (UUIDv7) einer Core-Zuordnung
//! CoreComponentId       Distributions-/Komponenten-Identität, z. B. "mgba"
//! libretro core name    wie RetroArch den Core anspricht, z. B. "mgba_libretro"
//! ```
//!
//! [`CoreId`] is what a system default or a game override stores;
//! [`CoreComponentId`] is what BitArchive
//! *installs*. They are different types because they are different things, and no
//! conversion between them exists yet.

pub mod component;
mod content;
pub mod core;
mod game;
mod id;
pub mod managed_core;
mod release;
pub mod runtime;

pub use component::{
    ArtifactReference, ArtifactSource, ComponentArtifactSource, ComponentAttribution, ComponentId,
    LicenseIdentifier, LoopbackSource, RelativePath, Sha256Digest,
};
pub use content::Content;
pub use core::{CoreSelectionSource, ResolvedCore, resolve_core};
pub use game::Game;
pub use id::{ContentId, CoreId, GameId, ReleaseId, SystemId};
pub use managed_core::{
    CoreBuildId, CoreComponentId, CoreDefinition, CoreParts, CorePlatform, CoreProvenance,
    MAX_CORE_BUILD_ID_LENGTH,
};
pub use release::Release;
pub use runtime::{
    ArtifactKind, RuntimeAttribution, RuntimeDefinition, RuntimeId, RuntimeParts, RuntimePlatform,
    RuntimeSource, RuntimeVersion,
};
