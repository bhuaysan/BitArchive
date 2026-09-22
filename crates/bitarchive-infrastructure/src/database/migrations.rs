//! The migrations this build carries.
//!
//! A migration is compiled into the binary rather than discovered at run time, so
//! a packaged application applies exactly the migrations it was built with —
//! never a file that happens to sit next to it, and never one a build did not
//! review (ARCHITECTURE.md §9).
//!
//! # The catalogue is empty on purpose
//!
//! The Issue that establishes the foundation is not the Issue that defines the
//! schema. There is no BitArchive table here: `games`, `releases`, `contents`,
//! their locations and fingerprints, and the FTS5 projection belong to schema v1,
//! which is Issue #109 and arrives as the first entry of this catalogue.
//!
//! A database the current build creates on its own therefore carries only the
//! migration ledger and reports schema version 0 — no migration has been applied.
//! When #109 adds migration 1, "schema version 1" means exactly "schema v1 is
//! applied", and a database created before it upgrades by applying that one
//! migration.

use super::migration::Migration;

/// Every migration this build knows, in ascending version order.
///
/// The order is the array order and nothing else; `MigrationRunner` checks on
/// construction that the versions are `1..=n`. Migrations are only ever appended
/// here: a released migration is not edited, and a correction is a new migration
/// (DATA_MODEL.md §18.2 rule 7).
pub(crate) const BITARCHIVE_MIGRATIONS: &[Migration] = &[];
