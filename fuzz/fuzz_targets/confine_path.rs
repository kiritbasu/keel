//! Path confinement, against arbitrary bytes.
//!
//! `confine` decides whether a relative path stays inside a repository root,
//! and it is the only thing standing between a `mirror_path` recorded in the
//! store and a write anywhere on the filesystem. Its inputs are strings written
//! by a model, so "what happens on input nobody thought of" is the question.
//!
//! The property is not "it returns Ok" — most of these should be refused. It is
//! that a refusal is a refusal, an acceptance stays under the root, and neither
//! panics.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    let Ok(relative) = std::str::from_utf8(data) else {
        return;
    };
    let root = Path::new("/tmp/keel-fuzz-root");

    if let Ok(joined) = keel_core::safe_path::confine(root, relative) {
        assert!(
            joined.starts_with(root),
            "confine accepted `{relative}` and produced {joined:?}, which is outside the root"
        );
    }
});
