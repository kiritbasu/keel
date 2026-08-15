//! Records the target triple this binary was built for.
//!
//! The updater needs it to name the release archive it should fetch —
//! `keel-aarch64-apple-darwin.tar.xz` — and Rust has no way to ask at runtime
//! what it was compiled for. Cargo tells a build script, and only a build
//! script, so this hands it forward as an environment variable that `env!`
//! reads at compile time.
//!
//! Deriving it from `std::env::consts::ARCH` and `OS` instead would work for
//! the targets that exist today and break silently on the first one where the
//! triple is not simply `{arch}-{vendor}-{os}` — which is most of them. The
//! failure would be a 404 on an asset name nobody had ever typed.

fn main() {
    // Cargo always sets this for a build script. `unwrap_or_default` rather
    // than a panic keeps the lints happy; the empty case is caught at runtime
    // with a message that says what is wrong, which a panic here would not.
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=KEEL_TARGET={target}");
    println!("cargo:rerun-if-changed=build.rs");
}
