//! Build integration for the Slint presentation sources.
//!
//! `slint-build` compiles `ui/app.slint` into Rust inside `OUT_DIR`, so no
//! generated Slint code is checked into the repository.

fn main() {
    println!("cargo:rerun-if-changed=ui/app.slint");
    slint_build::compile("ui/app.slint").expect("failed to compile ui/app.slint");
}
