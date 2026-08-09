//! Type-prefixed ULIDs.
//!
//! Every identifier carries its own type: `tsk_01H8XK…` is self-evidently a
//! task. That matters more here than in most systems because `links` is
//! polymorphic — an edge stores a bare id and a type string, and if the two
//! ever disagree the graph quietly returns wrong answers. Parsing the type out
//! of the id means the two can be cross-checked on every write, which is what
//! [`EntityId::entity_type`] exists for.
//!
//! ULIDs are also lexicographically sortable by creation time, which is why
//! "everything that changed since T" over the event log is a range scan rather
//! than a timestamp index (SPEC §3.4).

use crate::{EntityType, Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// The number of characters in a Crockford base-32 ULID.
const ULID_LEN: usize = 26;

/// The process-wide monotonic ULID source.
///
/// **This must not be replaced with `Ulid::new()`.** A plain ULID re-randomises
/// its 80 low bits on every call, so two ids minted in the same millisecond
/// sort arbitrarily with respect to each other. SPEC §3.4 relies on ULID order
/// *being* chronological order so that "everything that changed since T" is a
/// range scan over `events.id` — and a burst of writes inside one millisecond
/// is the normal case for an agent, not an edge case. Non-monotonic ids would
/// make an event-cursor query silently skip or repeat rows.
///
/// A `Mutex` is the right shape here rather than over-engineering: the daemon
/// owns the single write path (D-5), so contention is nil, and the alternative
/// — ordering every query by `(created_at, id)` — pushes the problem into
/// every call site instead of solving it once.
static GENERATOR: std::sync::LazyLock<std::sync::Mutex<ulid::Generator>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(ulid::Generator::new()));

/// Mint the next monotonically increasing ULID.
///
/// Both fallbacks are unreachable in practice and neither justifies failing a
/// write: a poisoned mutex means another thread panicked mid-mint, and
/// `Overflow` needs 2^80 ids inside one millisecond. Degrading to a
/// non-monotonic id keeps the write alive at the cost of ordering *within that
/// single millisecond*, which is strictly better than losing the write.
fn next_ulid() -> ulid::Ulid {
    let mut guard = match GENERATOR.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.generate().unwrap_or_else(|_| ulid::Ulid::new())
}

/// Generate a fresh prefixed identifier.
fn mint(prefix: &str) -> String {
    format!("{prefix}_{}", next_ulid())
}

/// Validate the `<prefix>_<ulid>` shape and return the two halves.
fn split(supplied: &str, expected_shape: &str) -> Result<(String, String)> {
    let malformed = |problem: String| Error::MalformedId {
        supplied: supplied.to_owned(),
        problem,
        expected: expected_shape.to_owned(),
    };

    let Some((prefix, body)) = supplied.split_once('_') else {
        return Err(malformed(
            "no `_` separating the type prefix from the ULID".into(),
        ));
    };
    if prefix.is_empty() {
        return Err(malformed("the type prefix is empty".into()));
    }
    if body.len() != ULID_LEN {
        return Err(malformed(format!(
            "the part after `_` is {} characters, not {ULID_LEN}",
            body.len()
        )));
    }
    if ulid::Ulid::from_string(body).is_err() {
        return Err(malformed("the part after `_` is not a valid ULID".into()));
    }
    Ok((prefix.to_owned(), body.to_owned()))
}

/// The identifier of one of the thirteen artifact types.
///
/// Construct with [`EntityId::generate`], or parse an existing one with
/// [`EntityId::parse`]. There is deliberately no `From<String>`: an
/// unvalidated identifier reaching the links table is the failure this type
/// exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityId(String);

impl EntityId {
    /// Mint a new identifier for `entity_type`.
    pub fn generate(entity_type: EntityType) -> Self {
        EntityId(mint(entity_type.prefix()))
    }

    /// Parse and validate an identifier, recovering its type from the prefix.
    pub fn parse(supplied: &str) -> Result<Self> {
        let (prefix, _) = split(
            supplied,
            "a prefixed ULID such as `tsk_01H8XK4RPVBQ2N7DZM9C3FGTWY`",
        )?;
        if EntityType::from_prefix(&prefix).is_none() {
            return Err(Error::MalformedId {
                supplied: supplied.to_owned(),
                problem: format!("`{prefix}` is not a known type prefix"),
                expected: format!(
                    "one of: {}",
                    EntityType::ALL
                        .into_iter()
                        .map(|t| format!("{}_ ({t})", t.prefix()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
        Ok(EntityId(supplied.to_owned()))
    }

    /// Parse an identifier and require it to be of a particular type.
    ///
    /// Used wherever an argument is typed by position rather than by name — a
    /// `milestone_id` field that receives `spc_…` is a caller bug worth
    /// rejecting immediately rather than storing and puzzling over later.
    pub fn parse_as(supplied: &str, expected: EntityType) -> Result<Self> {
        let id = Self::parse(supplied)?;
        let actual = id.entity_type();
        if actual != expected {
            return Err(Error::MalformedId {
                supplied: supplied.to_owned(),
                problem: format!("this is a {actual} id, but a {expected} id is required here"),
                expected: format!("an id beginning `{}_`", expected.prefix()),
            });
        }
        Ok(id)
    }

    /// The type this identifier belongs to.
    ///
    /// Infallible because the prefix was validated at construction. The
    /// fallback is unreachable but is a `System` panic-free path rather than
    /// an `unwrap`, per the definition of done.
    pub fn entity_type(&self) -> EntityType {
        self.0
            .split_once('_')
            .and_then(|(p, _)| EntityType::from_prefix(p))
            .unwrap_or(EntityType::Artifact)
    }

    /// The full identifier, prefix included.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the underlying string, for binding to SQL.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for EntityId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Generates a newtype identifier for one of the connective structures, which
/// have a fixed prefix and no `EntityType`.
macro_rules! simple_id {
    ($name:ident, $prefix:literal, $what:literal) => {
        #[doc = concat!("The identifier of ", $what, ", prefixed `", $prefix, "_`.")]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[doc = concat!("Mint a new ", $what, " identifier.")]
            pub fn generate() -> Self {
                $name(mint($prefix))
            }

            #[doc = concat!("Parse and validate a ", $what, " identifier.")]
            pub fn parse(supplied: &str) -> Result<Self> {
                let (prefix, _) = split(supplied, concat!("an id beginning `", $prefix, "_`"))?;
                if prefix != $prefix {
                    return Err(Error::MalformedId {
                        supplied: supplied.to_owned(),
                        problem: format!(
                            concat!("prefix `{}` is not `", $prefix, "`, so this is not ", $what),
                            prefix
                        ),
                        expected: concat!("an id beginning `", $prefix, "_`").to_owned(),
                    });
                }
                Ok($name(supplied.to_owned()))
            }

            /// The full identifier, prefix included.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consume into the underlying string, for binding to SQL.
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

simple_id!(LinkId, "lnk", "a link");
simple_id!(EventId, "evt", "an event");
simple_id!(DocId, "doc", "a document revision");
simple_id!(BlobId, "blb", "a stored blob");

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_carry_their_type() {
        for t in EntityType::ALL {
            let id = EntityId::generate(t);
            assert_eq!(id.entity_type(), t);
            assert!(id.as_str().starts_with(t.prefix()));
            assert_eq!(EntityId::parse(id.as_str()).unwrap(), id);
        }
    }

    #[test]
    fn ids_sort_chronologically() {
        // The event log's "since T" range scan depends on this holding.
        let mut ids: Vec<_> = (0..50)
            .map(|_| EntityId::generate(EntityType::Task))
            .collect();
        let sorted = {
            let mut c = ids.clone();
            c.sort();
            c
        };
        assert_eq!(ids, sorted, "ULIDs minted in order must already be sorted");
        ids.dedup();
        assert_eq!(ids.len(), 50, "no collisions");
    }

    #[test]
    fn parse_as_rejects_the_wrong_type() {
        let spec = EntityId::generate(EntityType::Spec);
        let err = EntityId::parse_as(spec.as_str(), EntityType::Milestone).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("spec id"), "{msg}");
        assert!(
            msg.contains("mst_"),
            "should say what would be valid: {msg}"
        );

        assert!(EntityId::parse_as(spec.as_str(), EntityType::Spec).is_ok());
    }

    #[test]
    fn malformed_ids_are_rejected_with_a_reason() {
        let cases = [
            ("", "no `_`"),
            ("tsk", "no `_`"),
            ("_01H8XK4RPVBQ2N7DZM9C3FGTWY", "prefix is empty"),
            ("tsk_short", "not 26"),
            ("tsk_IIIIIIIIIIIIIIIIIIIIIIIIII", "not a valid ULID"),
            ("zzz_01H8XK4RPVBQ2N7DZM9C3FGTWY", "unknown prefix"),
        ];
        for (bad, why) in cases {
            assert!(
                EntityId::parse(bad).is_err(),
                "`{bad}` should be rejected ({why})"
            );
        }
    }

    #[test]
    fn connective_ids_do_not_accept_each_others_prefixes() {
        let link = LinkId::generate();
        assert!(EventId::parse(link.as_str()).is_err());
        assert!(DocId::parse(link.as_str()).is_err());
        assert!(LinkId::parse(link.as_str()).is_ok());
        // And an entity id is not a link id.
        let task = EntityId::generate(EntityType::Task);
        assert!(LinkId::parse(task.as_str()).is_err());
    }
}
