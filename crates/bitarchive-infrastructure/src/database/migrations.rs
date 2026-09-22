//! The migrations this build carries.
//!
//! A migration is compiled into the binary rather than discovered at run time, so
//! a packaged application applies exactly the migrations it was built with —
//! never a file that happens to sit next to it, and never one a build did not
//! review (ARCHITECTURE.md §9).
//!
//! # Schema v1 is migration 1
//!
//! The catalogue begins with the initial schema, which is the whole of schema v1:
//! every persisted object `DATA_MODEL.md` §20.1 lists, in one migration. It is not
//! split, because it is not an upgrade — there is no earlier BitArchive table it
//! could be a sequence of corrections to. A database therefore reports schema
//! version 0 when it carries only the ledger (the foundation's own state) and
//! schema version 1 once this migration has run, so "schema version 1" means
//! exactly "schema v1 is applied".
//!
//! The statements themselves live in `migrations/0001_initial_schema.sql` and are
//! compiled in with `include_str!`, which keeps them reviewable as SQL while
//! keeping the catalogue the only thing that can decide what runs. The checksum
//! the ledger records covers the file's contents, so editing the file after a
//! release is detected on the next open rather than silently changing what a
//! recorded migration means.

use super::migration::Migration;

/// The initial schema — the whole of schema v1.
///
/// `DATA_MODEL.md` §20.1 is the inventory this migration implements, and its §6–§17
/// are the per-object definitions the statements follow.
const INITIAL_SCHEMA: &str = include_str!("../../migrations/0001_initial_schema.sql");

/// Every migration this build knows, in ascending version order.
///
/// The order is the array order and nothing else; `MigrationRunner` checks on
/// construction that the versions are `1..=n`. Migrations are only ever appended
/// here: a released migration is not edited, and a correction is a new migration
/// (DATA_MODEL.md §18.2 rule 7).
pub(crate) const BITARCHIVE_MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "initial schema: the whole of schema v1 (DATA_MODEL.md §20.1)",
    sql: INITIAL_SCHEMA,
}];
