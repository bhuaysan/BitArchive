//! Shared support for the crate's tests.
//!
//! Test-only and internal to the crate: nothing declared here is part of the
//! library, it is compiled only under `cfg(test)`, and it is deliberately
//! `pub(crate)` rather than `pub` so it cannot become public API by accident.
//!
//! Support lives here rather than beside the tests that happen to have grown it,
//! so a test module added later can use it without depending on another test
//! module's fixtures. Nothing in this tree decides what a *test* is about — it
//! only provides the scaffolding (a temporary directory, a database inside it) on
//! which a test builds its own scenario.

pub(crate) mod database;
