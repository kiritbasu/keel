//! Typed directed edges, and the direction rules that make them mean anything.
//!
//! # Read this before changing anything here
//!
//! The first draft of the spec had **both** graph traversals inverted. This is
//! the most dangerous bug class in Keel, because an inverted traversal returns
//! an empty result set that is indistinguishable from a legitimate "nothing is
//! linked here". It fails silently, plausibly, and in a direction that makes
//! the product look calm and correct while it quietly loses data.
//!
//! Three defences live in this module:
//!
//! 1. [`Relation::reads_as`] states each relation's direction in English, and
//!    the tests assert the sentence matches the traversal.
//! 2. [`Relation::normalise`] collapses `depends_on` into `blocks` on write,
//!    so exactly one direction is ever stored (D-11).
//! 3. [`Direction`] is an enum rather than a bool. `neighbours(id, true, …)`
//!    is unreadable at a call site and unreviewable in a diff.
//!
//! SPEC §3.3 has the normative table. It is the only authority. Read it every
//! time.

use crate::{Audit, EntityId, EntityType, Error, LinkId, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A typed relationship between two artifacts.
///
/// The canonical reading is always **from → to**: `from` *does the verb to*
/// `to`. A task `implements` a spec, so the task is `from`. A decision
/// `supersedes` an older decision, so the newer one is `from`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// `from` implements `to`. Example: task → spec `REQ-4`.
    Implements,
    /// `from` blocks `to`: `from` must finish first. Example: task A → task B.
    Blocks,
    /// `from` depends on `to`. The inverse of [`Relation::Blocks`], and
    /// **never stored** — [`Relation::normalise`] rewrites it. Accepted as
    /// input because it is often the more natural way to say it.
    DependsOn,
    /// `from` supersedes `to`. Example: decision v2 → decision v1.
    Supersedes,
    /// `from` derives from `to`. Example: spec → feedback.
    DerivedFrom,
    /// `from` resolves `to`. Example: decision → question, PR → task.
    Resolves,
    /// `from` references `to`. The catch-all; anything → anything.
    References,
    /// `from` duplicates `to`. Example: task → task.
    Duplicates,
    /// `from` informs `to`. Example: feedback → spec.
    Informs,
}

impl Relation {
    /// Every relation, including the one that is never stored.
    pub const ALL: [Relation; 9] = [
        Relation::Implements,
        Relation::Blocks,
        Relation::DependsOn,
        Relation::Supersedes,
        Relation::DerivedFrom,
        Relation::Resolves,
        Relation::References,
        Relation::Duplicates,
        Relation::Informs,
    ];

    /// Every relation that can appear in the `links` table.
    ///
    /// `depends_on` is absent by construction. A traversal that filters on it
    /// would match nothing, which is why no query anywhere names it.
    pub const STORED: [Relation; 8] = [
        Relation::Implements,
        Relation::Blocks,
        Relation::Supersedes,
        Relation::DerivedFrom,
        Relation::Resolves,
        Relation::References,
        Relation::Duplicates,
        Relation::Informs,
    ];

    /// The stored string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Relation::Implements => "implements",
            Relation::Blocks => "blocks",
            Relation::DependsOn => "depends_on",
            Relation::Supersedes => "supersedes",
            Relation::DerivedFrom => "derived_from",
            Relation::Resolves => "resolves",
            Relation::References => "references",
            Relation::Duplicates => "duplicates",
            Relation::Informs => "informs",
        }
    }

    /// The direction in English, for error messages, the UI and — mostly —
    /// for the tests that assert traversal matches intent.
    pub const fn reads_as(self) -> &'static str {
        match self {
            Relation::Implements => "from implements to",
            Relation::Blocks => "from blocks to",
            Relation::DependsOn => "from depends on to",
            Relation::Supersedes => "from supersedes to",
            Relation::DerivedFrom => "from derives from to",
            Relation::Resolves => "from resolves to",
            Relation::References => "from references to",
            Relation::Duplicates => "from duplicates to",
            Relation::Informs => "from informs to",
        }
    }

    /// Whether this relation is ever written to the `links` table.
    pub const fn is_stored(self) -> bool {
        !matches!(self, Relation::DependsOn)
    }

    /// Parse a relation name.
    pub fn parse(s: &str) -> Result<Self> {
        Relation::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| Error::MalformedId {
                supplied: s.to_owned(),
                problem: format!("`{s}` is not a Keel relation"),
                expected: Relation::ALL
                    .into_iter()
                    .map(Relation::as_str)
                    .collect::<Vec<_>>()
                    .join(" | "),
            })
    }

    /// Collapse a requested edge into its stored form.
    ///
    /// `A depends_on B` and `B blocks A` are the same fact. Storing both would
    /// mean every traversal has to consider two relations and two directions,
    /// and the first query that forgets one returns a plausible wrong answer.
    /// So exactly one is stored, and this is the single place the swap
    /// happens (D-11).
    ///
    /// Returns `(from, rel, to)` as they should be written.
    pub fn normalise(
        from: EntityId,
        rel: Relation,
        to: EntityId,
    ) -> (EntityId, Relation, EntityId) {
        match rel {
            Relation::DependsOn => (to, Relation::Blocks, from),
            other => (from, other, to),
        }
    }
}

impl fmt::Display for Relation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which way to walk an edge.
///
/// An enum rather than a boolean because the whole point is that the reader of
/// a call site can tell which way it goes without opening another file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Follow edges *away from* the root: match `from_id = root`, yield
    /// `to_id`.
    ///
    /// Outbound from a task on `implements` answers "what does this task
    /// implement" — it returns specs.
    Outbound,
    /// Follow edges *into* the root: match `to_id = root`, yield `from_id`.
    ///
    /// Inbound to a spec on `implements` answers "what implements this spec" —
    /// it returns tasks. This is the direction UC-7 needs, and the one the
    /// first draft got backwards.
    Inbound,
    /// Both at once, unioned.
    Both,
}

impl Direction {
    /// The column an edge is matched on when walking this way.
    pub const fn match_column(self) -> &'static str {
        match self {
            Direction::Outbound => "from_id",
            // `Both` is expanded into a union before reaching SQL; naming a
            // column for it would be meaningless, so it borrows Inbound's.
            Direction::Inbound | Direction::Both => "to_id",
        }
    }

    /// The column yielded as the neighbour when walking this way.
    pub const fn yield_column(self) -> &'static str {
        match self {
            Direction::Outbound => "to_id",
            Direction::Inbound | Direction::Both => "from_id",
        }
    }

    /// The opposite direction. `Both` is its own opposite.
    pub const fn inverse(self) -> Direction {
        match self {
            Direction::Outbound => Direction::Inbound,
            Direction::Inbound => Direction::Outbound,
            Direction::Both => Direction::Both,
        }
    }

    /// Parse a direction name.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "outbound" => Ok(Direction::Outbound),
            "inbound" => Ok(Direction::Inbound),
            "both" => Ok(Direction::Both),
            other => Err(Error::MalformedId {
                supplied: other.to_owned(),
                problem: format!("`{other}` is not a traversal direction"),
                expected: "outbound | inbound | both".to_owned(),
            }),
        }
    }

    /// The stored/wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Direction::Outbound => "outbound",
            Direction::Inbound => "inbound",
            Direction::Both => "both",
        }
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The default traversal depth (REQ-5).
pub const DEFAULT_DEPTH: u8 = 6;
/// The hard cap on traversal depth (REQ-5). Requests above this are clamped,
/// and the response says so rather than silently returning less.
pub const MAX_DEPTH: u8 = 16;

/// One stored edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// The edge's own identifier.
    pub id: LinkId,
    /// The project the edge belongs to. Null for edges touching a global term.
    pub project_id: Option<EntityId>,
    /// The type of the `from` endpoint, denormalised so traversal never has to
    /// join thirteen tables to find out what it just reached.
    pub from_type: EntityType,
    /// The `from` endpoint — the one that *does the verb*.
    pub from_id: EntityId,
    /// The relation.
    pub rel: Relation,
    /// The type of the `to` endpoint.
    pub to_type: EntityType,
    /// The `to` endpoint — the one the verb is done *to*.
    pub to_id: EntityId,
    /// A sub-entity anchor such as `REQ-4`. Empty string means whole-entity.
    ///
    /// `NOT NULL` in the schema, and empty rather than null here for the same
    /// reason: the unique index on `(from_id, rel, to_id, anchor)` would not
    /// fire on nulls, and every ordinary edge would be distinct from every
    /// other one.
    pub anchor: String,
    /// A free-text note on why the edge exists.
    pub note: Option<String>,
    /// The audit block.
    pub audit: Audit,
}

impl Link {
    /// The endpoint reached when walking this edge in `direction` from `root`.
    ///
    /// Returns `None` if the edge does not touch `root` in that direction,
    /// which is how a caller can assert an edge is *not* reachable a given way
    /// — the negative half of every direction test.
    pub fn neighbour_of(&self, root: &EntityId, direction: Direction) -> Option<&EntityId> {
        match direction {
            Direction::Outbound if &self.from_id == root => Some(&self.to_id),
            Direction::Inbound if &self.to_id == root => Some(&self.from_id),
            Direction::Both if &self.from_id == root => Some(&self.to_id),
            Direction::Both if &self.to_id == root => Some(&self.from_id),
            _ => None,
        }
    }
}

/// A request to create an edge, before normalisation and validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewLink {
    /// The `from` endpoint as the caller stated it.
    pub from_id: EntityId,
    /// The relation as the caller stated it — may be `depends_on`.
    pub rel: Relation,
    /// The `to` endpoint as the caller stated it.
    pub to_id: EntityId,
    /// The anchor, if any. `None` becomes the empty string.
    pub anchor: Option<String>,
    /// Why the edge exists.
    pub note: Option<String>,
}

impl NewLink {
    /// A whole-entity edge with no note.
    pub fn new(from_id: EntityId, rel: Relation, to_id: EntityId) -> Self {
        NewLink {
            from_id,
            rel,
            to_id,
            anchor: None,
            note: None,
        }
    }

    /// Attach an anchor such as `REQ-4`.
    pub fn anchored(mut self, anchor: impl Into<String>) -> Self {
        self.anchor = Some(anchor.into());
        self
    }

    /// Attach a note.
    pub fn noted(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Validate and normalise into the endpoints that will actually be stored.
    ///
    /// Rejects self-edges: they are never meaningful for any of the nine
    /// relations, and a task that blocks itself makes the blocker traversal
    /// non-terminating in a way the cycle guard would mask rather than report.
    pub fn normalised(self) -> Result<(EntityId, Relation, EntityId, String, Option<String>)> {
        if self.from_id == self.to_id {
            return Err(Error::Invariant {
                operation: format!("link {} {} {}", self.from_id, self.rel.as_str(), self.to_id),
                problem: "an entity cannot be linked to itself".to_owned(),
            });
        }
        let (from, rel, to) = Relation::normalise(self.from_id, self.rel, self.to_id);
        Ok((from, rel, to, self.anchor.unwrap_or_default(), self.note))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn id(t: EntityType) -> EntityId {
        EntityId::generate(t)
    }

    #[test]
    fn relation_names_round_trip() {
        for r in Relation::ALL {
            assert_eq!(Relation::parse(r.as_str()).unwrap(), r);
        }
        assert!(Relation::parse("relates_to").is_err());
    }

    #[test]
    fn depends_on_is_the_only_unstored_relation() {
        let unstored: Vec<_> = Relation::ALL
            .into_iter()
            .filter(|r| !r.is_stored())
            .collect();
        assert_eq!(unstored, vec![Relation::DependsOn]);
        assert_eq!(Relation::STORED.len(), 8);
        assert!(!Relation::STORED.contains(&Relation::DependsOn));
    }

    #[test]
    fn depends_on_normalises_to_blocks_with_swapped_endpoints() {
        // "A depends on B" is the same fact as "B blocks A".
        let a = id(EntityType::Task);
        let b = id(EntityType::Task);

        let (from, rel, to) = Relation::normalise(a.clone(), Relation::DependsOn, b.clone());
        assert_eq!(rel, Relation::Blocks);
        assert_eq!(from, b, "the blocker is the thing depended upon");
        assert_eq!(to, a, "the blocked thing is the one that depends");
    }

    #[test]
    fn normalising_is_idempotent_for_every_other_relation() {
        let a = id(EntityType::Task);
        let b = id(EntityType::Spec);
        for r in Relation::STORED {
            let (from, rel, to) = Relation::normalise(a.clone(), r, b.clone());
            assert_eq!(
                (from, rel, to),
                (a.clone(), r, b.clone()),
                "{r} was rewritten"
            );
        }
    }

    #[test]
    fn normalising_depends_on_twice_returns_to_the_start() {
        // Guards against a future "normalise on read as well as write", which
        // would silently invert every dependency in the store.
        let a = id(EntityType::Task);
        let b = id(EntityType::Task);
        let (f1, r1, t1) = Relation::normalise(a.clone(), Relation::DependsOn, b.clone());
        let (f2, r2, t2) = Relation::normalise(f1, r1, t1);
        assert_eq!((f2, r2, t2), (b, Relation::Blocks, a));
    }

    #[test]
    fn directions_use_opposite_columns() {
        assert_eq!(Direction::Outbound.match_column(), "from_id");
        assert_eq!(Direction::Outbound.yield_column(), "to_id");
        assert_eq!(Direction::Inbound.match_column(), "to_id");
        assert_eq!(Direction::Inbound.yield_column(), "from_id");
        assert_ne!(
            Direction::Outbound.match_column(),
            Direction::Inbound.match_column()
        );
        assert_eq!(Direction::Outbound.inverse(), Direction::Inbound);
        assert_eq!(Direction::Inbound.inverse(), Direction::Outbound);
        assert_eq!(Direction::Both.inverse(), Direction::Both);
    }

    #[test]
    fn neighbour_of_respects_direction() {
        let task = id(EntityType::Task);
        let spec = id(EntityType::Spec);
        let link = Link {
            id: LinkId::generate(),
            project_id: None,
            from_type: EntityType::Task,
            from_id: task.clone(),
            rel: Relation::Implements,
            to_type: EntityType::Spec,
            to_id: spec.clone(),
            anchor: String::new(),
            note: None,
            audit: Audit::new(
                &crate::Provenance::anonymous(crate::Actor::System),
                chrono::Utc::now(),
            ),
        };

        // "What does this task implement?" — outbound from the task.
        assert_eq!(link.neighbour_of(&task, Direction::Outbound), Some(&spec));
        // "What implements this spec?" — inbound to the spec.
        assert_eq!(link.neighbour_of(&spec, Direction::Inbound), Some(&task));

        // The inversions that would silently return nothing.
        assert_eq!(
            link.neighbour_of(&spec, Direction::Outbound),
            None,
            "a spec has no outbound `implements` edge; getting this wrong is the empty-set bug"
        );
        assert_eq!(link.neighbour_of(&task, Direction::Inbound), None);

        // Both finds it from either end.
        assert_eq!(link.neighbour_of(&task, Direction::Both), Some(&spec));
        assert_eq!(link.neighbour_of(&spec, Direction::Both), Some(&task));
    }

    #[test]
    fn self_links_are_rejected() {
        let a = id(EntityType::Task);
        let err = NewLink::new(a.clone(), Relation::Blocks, a)
            .normalised()
            .unwrap_err();
        assert!(err.to_string().contains("cannot be linked to itself"));
    }

    #[test]
    fn a_missing_anchor_becomes_the_empty_string_not_null() {
        let (_, _, _, anchor, _) = NewLink::new(
            id(EntityType::Task),
            Relation::Implements,
            id(EntityType::Spec),
        )
        .normalised()
        .unwrap();
        assert_eq!(anchor, "", "null anchors would defeat the unique index");
    }

    #[test]
    fn depth_bounds_match_the_requirement() {
        // REQ-5: default 6, hard cap 16.
        assert_eq!(DEFAULT_DEPTH, 6);
        assert_eq!(MAX_DEPTH, 16);
    }
}
