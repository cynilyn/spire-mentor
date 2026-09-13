//! Stamp a content hash of this crate's own source into `STS2_CORE_SRC_HASH`,
//! exposed by the crate as `sts2_core::SRC_HASH`. Part of the content-addressed
//! engine-identity fix (issue #15, bead `sts2-rs-cg5ci`): a stamp compiled INTO
//! each engine crate lets the parity harness prove the loaded `.so` was built
//! from the source it claims to test, catching a stale dependency rlib that an
//! mtime / git-SHA gate cannot see.
//!
//! Deliberately emits NO `rerun-if-changed`: with no rerun directives cargo
//! falls back to re-running this script whenever ANY file in the package
//! changes, so the stamp tracks every source edit without per-file directives.

fn main() {
    let crate_dir = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo"),
    );
    let hash = sts2_srchash::hash_crate(&crate_dir, &[]);
    println!("cargo:rustc-env=STS2_CORE_SRC_HASH={hash}");
}
