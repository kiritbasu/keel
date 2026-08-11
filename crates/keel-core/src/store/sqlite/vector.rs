//! Registering `sqlite-vec`, and the only `unsafe` in the workspace.
//!
//! # Why this file has an unsafe block
//!
//! SQLite loads an extension by being handed a function pointer through
//! `sqlite3_auto_extension`, which it stores and calls on every connection
//! opened afterwards. There is no safe wrapper for that in `rusqlite` and there
//! cannot be a useful one: the safety obligation is that the pointer really is
//! an init function with the right signature and that it outlives every
//! connection, and neither is something the type system can be shown.
//!
//! The workspace lint was `unsafe_code = "forbid"` before Phase 9. `forbid`
//! exists precisely so an `#[allow]` cannot override it, so this was a choice
//! between relaxing the lint to `deny` with one exception or shipping no vector
//! search at all. The lint is now `deny`, the reasoning is in the root
//! `Cargo.toml`, and this is the one exception.
//!
//! # Why the obligations hold here
//!
//! `sqlite_vec::sqlite3_vec_init` is a `'static` item in a statically linked
//! crate, so the pointer is valid for the whole process and cannot dangle. The
//! transmute is from a Rust `fn` item to the C ABI pointer type SQLite expects,
//! which is the signature `sqlite-vec` documents for exactly this call.
//!
//! # Why registration is idempotent
//!
//! `register` can be called from anywhere a store is opened, including from
//! several tests at once. `Once` makes the hook install exactly one time.
//! Installing it twice would not be unsound, but SQLite would then run the
//! initialiser twice per connection, and the second run reports an error nobody
//! is in a position to handle.

use std::sync::Once;

static REGISTERED: Once = Once::new();

/// Make `vec0` available to every connection opened after this call.
///
/// Call it before opening a connection, not after: SQLite runs the registered
/// initialisers at open time, so a connection created first never sees the
/// extension and fails on `CREATE VIRTUAL TABLE … USING vec0` with a message
/// about an unknown module.
pub fn register() {
    REGISTERED.call_once(|| {
        // SAFETY: `sqlite3_vec_init` is a `'static` function item in a
        // statically linked crate, so the pointer is valid for the lifetime of
        // the process and outlives every connection SQLite will call it for.
        // The transmute converts a Rust `fn` item to the `extern "C"` pointer
        // type `sqlite3_auto_extension` takes, which is the call `sqlite-vec`
        // documents. `Once` guarantees this runs at most once.
        #[allow(unsafe_code)]
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut rusqlite::ffi::sqlite3,
                    *mut *mut i8,
                    *const rusqlite::ffi::sqlite3_api_routines,
                ) -> i32,
            >(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use rusqlite::Connection;

    /// The registration works and the module is reachable. If this fails,
    /// nothing else in semantic search can work either — so it is worth
    /// asserting on its own rather than discovering it through a search test.
    #[test]
    fn vec0_is_available_after_registering() {
        super::register();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE VIRTUAL TABLE t USING vec0(doc_id TEXT PRIMARY KEY, embedding float[4]);",
        )
        .unwrap();

        let one = bytes(&[1.0, 0.0, 0.0, 0.0]);
        let two = bytes(&[0.0, 1.0, 0.0, 0.0]);
        db.execute(
            "INSERT INTO t(doc_id, embedding) VALUES ('a', ?1), ('b', ?2)",
            rusqlite::params![one, two],
        )
        .unwrap();

        // A probe vector all but identical to 'a'. The nearest neighbour must
        // be 'a' — a search that returned 'b' would be a distance metric
        // pointing the wrong way, which ranks plausibly and is wrong.
        let probe = bytes(&[0.99, 0.01, 0.0, 0.0]);
        let mut stmt = db
            .prepare("SELECT doc_id FROM t WHERE embedding MATCH ?1 AND k = 2 ORDER BY distance")
            .unwrap();
        let order: Vec<String> = stmt
            .query_map(rusqlite::params![probe], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();

        assert_eq!(order, vec!["a", "b"], "nearest neighbour came back wrong");
    }

    /// Calling it repeatedly must stay harmless — every store open calls it.
    #[test]
    fn registering_twice_is_harmless() {
        super::register();
        super::register();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE VIRTUAL TABLE t USING vec0(id TEXT PRIMARY KEY, e float[2]);")
            .unwrap();
    }

    fn bytes(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|f| f.to_le_bytes()).collect()
    }
}
