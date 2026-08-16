//! The FTS5 query builder, against arbitrary text.
//!
//! This is the one function in the crate that takes a raw human string and
//! produces something another language parses. Getting it wrong does not
//! produce a crash, it produces `no such column: first` from a search for
//! `local-first` — an error naming a word the user typed, which reads as a
//! schema bug.
//!
//! The property: whatever comes out, every double quote in it is either the
//! delimiter of a term or doubled, which is what makes nothing a caller types
//! reach FTS5 as syntax.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    if let Some(query) = specline_core::store::search::fts_match(text) {
        assert!(
            !query.is_empty(),
            "a Some() answer that is empty would be a query matching everything"
        );
        // Terms are `"…"` joined by ` OR `, and any embedded quote is doubled,
        // so the count is always even.
        assert_eq!(
            query.matches('"').count() % 2,
            0,
            "unbalanced quoting in `{query}` — some of `{text}` would reach FTS5 as syntax"
        );
    }
});
