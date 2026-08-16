//! The id parser, against arbitrary bytes.
//!
//! Every identifier in the store arrives as a string from somewhere — a tool
//! call, a JSON body, a manifest — and `EntityId::parse` is the only thing that
//! decides whether it is one. It is also the function whose failure mode is
//! quiet rather than loud: an id that parses when it should not reports itself
//! as an artifact, because that is what an unrecognised prefix falls back to.
//!
//! The property: anything that parses has a recognised prefix and round-trips
//! through its own string form.

#![no_main]

use libfuzzer_sys::fuzz_target;
use specline_core::EntityId;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    if let Ok(id) = EntityId::parse(text) {
        assert_eq!(id.as_str(), text, "parsing changed the identifier");
        let prefix = id.entity_type().prefix();
        assert!(
            text.starts_with(&format!("{prefix}_")),
            "`{text}` parsed but reports the type `{prefix}`, which is not its prefix"
        );
        // And it survives the JSON door, which is the one that used to skip
        // validation entirely.
        let json = serde_json::to_string(&id).unwrap_or_default();
        assert!(serde_json::from_str::<EntityId>(&json).is_ok());
    }
});
