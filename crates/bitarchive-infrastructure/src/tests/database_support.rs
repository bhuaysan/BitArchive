//! A sibling test module using the shared isolated-database support.
//!
//! The helper lives in [`crate::tests::support::database`] so that a test module
//! added later can open a temporary database without depending on the fixture
//! migrations of [`crate::tests::database`]. This module is the small proof that
//! it actually works that way: it is a test module of its own, it declares no
//! fixture, and it reaches the helper the same way a future repository test would.
//!
//! It is deliberately short. Its subject is the *support*, not the foundation —
//! what the foundation does is asserted in [`crate::tests::database`], which is
//! also where the fixture migrations belong.

use super::support::database::TempDatabase;
use crate::database::SchemaVersion;

/// A sibling test module opens its own isolated database, outside user data.
///
/// Two things are checked, and they are the two the helper exists for. It is
/// reachable from a module that declares nothing of its own, and the database it
/// hands out is a real one that lives in the system temporary directory — so a
/// test written in a module like this one cannot read, migrate, or remove
/// anything of a user's (ARCHITECTURE.md §45.2).
#[test]
fn a_sibling_test_module_opens_its_own_isolated_database() {
    let temporary = TempDatabase::new();
    let database = temporary.open().expect("a new database must open");

    assert_eq!(
        database
            .schema_version()
            .expect("the version must be readable"),
        SchemaVersion::NONE,
        "a sibling module starts from the foundation and nothing else"
    );

    assert!(
        temporary.directory().starts_with(std::env::temp_dir()),
        "the database must live in the system temporary directory, not below a \
         user's application data: {}",
        temporary.directory().display()
    );

    assert!(
        temporary.file().is_file(),
        "the helper must hand out a database that was really created"
    );

    drop(database);
}
