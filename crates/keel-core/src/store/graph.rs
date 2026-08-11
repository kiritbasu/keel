//! Graph traversal over `links`, in SQLite.
//!
//! Every walk is one recursive CTE, and that was the capability the engine had
//! to have: without it a traversal becomes a loop in Rust issuing one query per
//! hop, which is both slower and — far worse — a second place where the
//! direction can be got wrong.
//!
//! # Direction
//!
//! `product/SPEC.md` §3.3 is the normative table and it says the same thing for
//! all nine relations: the `from` endpoint does the verb and the `to` endpoint
//! has it done to it. So:
//!
//! - **Outbound** follows edges *away from* the root — match `from_id`, yield
//!   `to_id`. Outbound from a task on `implements` answers "what does this task
//!   implement", and returns specs.
//! - **Inbound** follows edges *into* the root — match `to_id`, yield
//!   `from_id`. Inbound to a spec on `implements` answers "what implements this
//!   spec", and returns tasks. This is the direction UC-7 needs and the one the
//!   first draft of the spec had backwards.
//!
//! Getting it the wrong way round does not raise anything. It returns an empty
//! result set indistinguishable from a legitimate "nothing is linked here",
//! which is why the tests below assert both directions for every relation and
//! also assert the *negative* — that the opposite walk finds nothing.
//!
//! `depends_on` is never stored. It is the inverse of `blocks` and
//! [`Relation::normalise`] swaps the endpoints on write, so there is exactly
//! one direction to reason about and a traversal that filters on `depends_on`
//! alone honestly matches nothing.
//!
//! # Why the path is a string
//!
//! SQLite has no array type and no `list_contains`, so a traversal carries the
//! path it has walked as a delimited string — `|root|a|b|` — and the cycle
//! guard is `instr`. The delimiters on both ends of every element are what make
//! that an exact match rather than a prefix match; ids are fixed-shape prefixed
//! ULIDs, so no id can appear inside another between two bars.

use super::Store;
use super::rows::read_audit;
use crate::store::{GraphStore, Neighbour};
use crate::{Direction, EntityId, EntityType, Error, Link, LinkId, MAX_DEPTH, Relation, Result};
use rusqlite::types::Value;
use rusqlite::{Row, params_from_iter};

/// The bar that brackets every id in a stored path.
///
/// A path is `|root|a|b|` rather than `root,a,b` so that the cycle guard can
/// look for `|a|` and never match the tail of some other id.
const BAR: &str = "|";

/// The `rel IN (…)` clause for a requested relation set.
///
/// Interpolated rather than bound because the values come from a closed enum —
/// there is no caller-supplied string anywhere in the output — and a bound `IN`
/// list would need a placeholder count that varies with the argument.
///
/// An empty request means every stored relation. A request naming only
/// `depends_on` collapses to `FALSE`: it is never written, so the truthful
/// answer is "no edges match" and not "all of them", which is what dropping an
/// empty filter would have said.
fn rel_filter(rels: &[Relation]) -> String {
    let effective: Vec<Relation> = if rels.is_empty() {
        Relation::STORED.to_vec()
    } else {
        rels.iter().copied().filter(|r| r.is_stored()).collect()
    };
    if effective.is_empty() {
        return "0".to_owned();
    }
    let list = effective
        .iter()
        .map(|r| format!("'{}'", r.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("l.rel IN ({list})")
}

/// The columns a traversal matches on and yields, for one direction.
///
/// `Both` is expanded into two walks before reaching SQL, so it never arrives
/// here; it borrows `Inbound`'s columns to keep the match exhaustive without a
/// panic.
fn columns(direction: Direction) -> (&'static str, &'static str, &'static str) {
    match direction {
        Direction::Outbound => ("from_id", "to_id", "to_type"),
        Direction::Inbound | Direction::Both => ("to_id", "from_id", "from_type"),
    }
}

/// Rebuild one traversal row.
fn read_neighbour(row: &Row<'_>) -> Result<Neighbour> {
    let e = |c: &'static str| Error::storage(format!("read column `{c}` of a traversal row"));

    let id = EntityId::parse(&row.get::<_, String>("id").map_err(e("id"))?)?;
    let entity_type = EntityType::parse(
        &row.get::<_, String>("entity_type")
            .map_err(e("entity_type"))?,
    )?;
    let rel = Relation::parse(&row.get::<_, String>("rel").map_err(e("rel"))?)?;
    let anchor = row
        .get::<_, Option<String>>("anchor")
        .map_err(e("anchor"))?
        .unwrap_or_default();
    let depth: i64 = row.get::<_, i64>("depth").map_err(e("depth"))?;

    // The stored path is `|root|a|b|`; the empty fragments either side of the
    // outer bars are what the filter drops.
    let raw_path = row
        .get::<_, Option<String>>("path")
        .map_err(e("path"))?
        .unwrap_or_default();
    let mut path = Vec::new();
    for step in raw_path.split(BAR).filter(|s| !s.is_empty()) {
        path.push(EntityId::parse(step)?);
    }

    // A `LEFT JOIN`, so an edge pointing at a row that no longer resolves
    // yields an empty label rather than vanishing. Dropping it would hide the
    // exact breakage `fsck`'s dangling-link check exists to report.
    let label = row
        .get::<_, Option<String>>("label")
        .map_err(e("label"))?
        .unwrap_or_default();

    Ok(Neighbour {
        id,
        entity_type,
        label,
        rel,
        anchor,
        depth: u8::try_from(depth).unwrap_or(MAX_DEPTH),
        path,
    })
}

/// Rebuild an edge from a `links` row.
fn read_link(row: &Row<'_>) -> Result<Link> {
    let e = |c: &'static str| Error::storage(format!("read column `{c}` of `links`"));
    Ok(Link {
        id: LinkId::parse(&row.get::<_, String>("id").map_err(e("id"))?)?,
        project_id: match row
            .get::<_, Option<String>>("project_id")
            .map_err(e("project_id"))?
        {
            Some(p) => Some(EntityId::parse(&p)?),
            None => None,
        },
        from_type: EntityType::parse(&row.get::<_, String>("from_type").map_err(e("from_type"))?)?,
        from_id: EntityId::parse(&row.get::<_, String>("from_id").map_err(e("from_id"))?)?,
        rel: Relation::parse(&row.get::<_, String>("rel").map_err(e("rel"))?)?,
        to_type: EntityType::parse(&row.get::<_, String>("to_type").map_err(e("to_type"))?)?,
        to_id: EntityId::parse(&row.get::<_, String>("to_id").map_err(e("to_id"))?)?,
        anchor: row
            .get::<_, Option<String>>("anchor")
            .map_err(e("anchor"))?
            .unwrap_or_default(),
        note: row.get::<_, Option<String>>("note").map_err(e("note"))?,
        audit: read_audit(row, "links")?,
    })
}

impl GraphStore for Store {
    fn neighbours(
        &self,
        root: &EntityId,
        direction: Direction,
        rels: &[Relation],
        depth: u8,
    ) -> Result<Vec<Neighbour>> {
        let depth = depth.clamp(1, MAX_DEPTH);

        if direction == Direction::Both {
            // Run each way and merge, keeping the shallower reach when both
            // find the same node. Expressing `Both` as one query would put the
            // union inside the recursive term, where a node reached outbound
            // could then be walked inbound — which is not "both directions", it
            // is an undirected walk, and it would return things that stand in
            // no stated relation to the root at all.
            let mut out = self.neighbours(root, Direction::Outbound, rels, depth)?;
            for n in self.neighbours(root, Direction::Inbound, rels, depth)? {
                match out.iter_mut().find(|e| e.id == n.id) {
                    Some(existing) if existing.depth > n.depth => *existing = n,
                    Some(_) => {}
                    None => out.push(n),
                }
            }
            out.sort_by_key(|n| (n.depth, n.id.as_str().to_owned()));
            return Ok(out);
        }

        let (match_col, yield_col, yield_type) = columns(direction);
        let filter = rel_filter(rels);

        let sql = format!(
            "WITH RECURSIVE walk(id, entity_type, rel, anchor, depth, path) AS (
                 SELECT l.{yield_col}, l.{yield_type}, l.rel, l.anchor,
                        1,
                        '{BAR}' || ?1 || '{BAR}' || l.{yield_col} || '{BAR}'
                 FROM links l
                 WHERE l.{match_col} = ?1 AND l.archived_at IS NULL AND {filter}
               UNION ALL
                 SELECT l.{yield_col}, l.{yield_type}, l.rel, l.anchor,
                        w.depth + 1,
                        w.path || l.{yield_col} || '{BAR}'
                 FROM links l
                 JOIN walk w ON l.{match_col} = w.id
                 WHERE l.archived_at IS NULL AND {filter}
                   AND w.depth < ?2
                   AND instr(w.path, '{BAR}' || l.{yield_col} || '{BAR}') = 0
             )
             SELECT w.id AS id, w.entity_type AS entity_type, w.rel AS rel,
                    w.anchor AS anchor, w.depth AS depth, w.path AS path,
                    v.label AS label
             FROM walk w LEFT JOIN v_entities v ON v.id = w.id
             ORDER BY w.depth, w.id"
        );

        let mut stmt = self
            .connection()
            .prepare(&sql)
            .map_err(Error::storage(format!(
                "prepare a {direction} traversal from {root}"
            )))?;
        let mut rows = stmt
            .query(params_from_iter(vec![
                Value::Text(root.as_str().to_owned()),
                Value::Integer(i64::from(depth)),
            ]))
            .map_err(Error::storage(format!(
                "run a {direction} traversal from {root}"
            )))?;

        let mut out: Vec<Neighbour> = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(Error::storage("read a traversal row"))?
        {
            // A node reachable by two paths appears twice. Keep the shorter,
            // because "how far is this from the root" has one answer and it is
            // the nearest one; `ORDER BY depth` means the first seen wins.
            let neighbour = read_neighbour(row)?;
            match out.iter().position(|n| n.id == neighbour.id) {
                Some(i) if out[i].depth > neighbour.depth => out[i] = neighbour,
                Some(_) => {}
                None => out.push(neighbour),
            }
        }
        Ok(out)
    }

    fn links_of(&self, id: &EntityId, direction: Direction) -> Result<Vec<Link>> {
        let clause = match direction {
            Direction::Outbound => "from_id = ?1",
            Direction::Inbound => "to_id = ?1",
            Direction::Both => "from_id = ?1 OR to_id = ?1",
        };

        let sql = format!(
            "SELECT id, project_id, from_type, from_id, rel, to_type, to_id, anchor, note, \
             created_at, updated_at, version, created_by, updated_by, session_id, surface, \
             archived_at FROM links WHERE ({clause}) AND archived_at IS NULL ORDER BY id"
        );
        let mut stmt = self
            .connection()
            .prepare(&sql)
            .map_err(Error::storage(format!(
                "prepare the {direction} links of {id}"
            )))?;
        let mut rows = stmt
            .query(params_from_iter(vec![Value::Text(id.as_str().to_owned())]))
            .map_err(Error::storage(format!("run the {direction} links of {id}")))?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(Error::storage("read a link row"))? {
            out.push(read_link(row)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// A timestamp in the format the store writes. Fixed rather than `now()`
    /// so a failure is never about the clock.
    const AT: &str = "2026-08-11T09:14:36.524000Z";

    /// The project every fixture row belongs to.
    ///
    /// A real prefixed ULID and not a readable placeholder, because
    /// `links.project_id` is parsed back into an [`EntityId`] on read — the
    /// first draft used `prj_test` and only `links_of` noticed.
    const PROJECT: &str = "prj_01H8XK4RPVBQ2N7DZM9C3FGTWY";

    /// Insert a minimally valid entity row and return its id.
    ///
    /// Direct SQL because `EntityStore for Store` is being written in
    /// parallel; these tests are about the traversal, and coupling them to
    /// another module's progress would mean neither could be finished first.
    fn node(store: &Store, ty: EntityType, label: &str) -> EntityId {
        let id = EntityId::generate(ty);
        let (table, label_col) = match ty {
            EntityType::Task => ("tasks", "title"),
            EntityType::Spec => ("specs", "title"),
            EntityType::Decision => ("decisions", "title"),
            EntityType::Question => ("questions", "title"),
            EntityType::Feedback => ("feedback", "summary"),
            EntityType::Design => ("design_artifacts", "name"),
            EntityType::Environment => ("environments", "name"),
            EntityType::Milestone => ("milestones", "name"),
            EntityType::Metric => ("metrics", "name"),
            EntityType::Artifact => ("artifacts", "name"),
            other => panic!("the test helper cannot insert a {other}"),
        };
        let sql = format!(
            "INSERT INTO {table} (id, project_id, {label_col}, idempotency_key,
                                  created_at, updated_at, created_by, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, 'claude', 'claude')"
        );
        store
            .connection()
            .execute(
                &sql,
                rusqlite::params![id.as_str(), PROJECT, label, id.as_str(), AT],
            )
            .unwrap();
        id
    }

    /// Draw an edge. `archived` writes it soft-deleted, which is the only way
    /// an edge ever leaves the table.
    fn link_with(
        store: &Store,
        from: &EntityId,
        rel: Relation,
        to: &EntityId,
        anchor: &str,
        archived: bool,
    ) {
        store
            .connection()
            .execute(
                "INSERT INTO links (id, project_id, from_id, from_type, to_id, to_type, rel,
                                    anchor, created_at, updated_at, created_by, updated_by,
                                    archived_at)
                 VALUES (?1, ?10, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 'claude', 'claude', ?9)",
                rusqlite::params![
                    LinkId::generate().as_str(),
                    from.as_str(),
                    from.entity_type().as_str(),
                    to.as_str(),
                    to.entity_type().as_str(),
                    rel.as_str(),
                    anchor,
                    AT,
                    if archived { Some(AT) } else { None },
                    PROJECT,
                ],
            )
            .unwrap();
    }

    fn link(store: &Store, from: &EntityId, rel: Relation, to: &EntityId) {
        link_with(store, from, rel, to, "", false);
    }

    fn ids(found: &[Neighbour]) -> Vec<String> {
        found.iter().map(|n| n.id.as_str().to_owned()).collect()
    }

    /// The shared body of all nine direction tests.
    ///
    /// Asserts four things, and the last two are the point: that walking the
    /// *wrong* way finds nothing. An implementation with the columns swapped
    /// passes the first two by accident, because both endpoints exist and both
    /// walks return one row — it is only the negative that distinguishes them.
    fn assert_direction(rel: Relation, from_type: EntityType, to_type: EntityType) {
        let store = Store::in_memory().unwrap();
        let from = node(&store, from_type, "the doer");
        let to = node(&store, to_type, "the done-to");
        link(&store, &from, rel, &to);

        let out = store
            .neighbours(&from, Direction::Outbound, &[rel], 6)
            .unwrap();
        assert_eq!(
            ids(&out),
            vec![to.as_str().to_owned()],
            "outbound on `{rel}` ({}) should reach the `to` endpoint",
            rel.reads_as()
        );
        assert_eq!(out[0].label, "the done-to", "the label should be resolved");
        assert_eq!(out[0].entity_type, to_type);
        assert_eq!(out[0].depth, 1);
        assert_eq!(out[0].path, vec![from.clone(), to.clone()]);

        let inn = store
            .neighbours(&to, Direction::Inbound, &[rel], 6)
            .unwrap();
        assert_eq!(
            ids(&inn),
            vec![from.as_str().to_owned()],
            "inbound on `{rel}` ({}) should reach the `from` endpoint",
            rel.reads_as()
        );
        assert_eq!(inn[0].label, "the doer");
        assert_eq!(inn[0].entity_type, from_type);
        assert_eq!(inn[0].path, vec![to.clone(), from.clone()]);

        assert!(
            store
                .neighbours(&from, Direction::Inbound, &[rel], 6)
                .unwrap()
                .is_empty(),
            "nothing points *into* the `from` endpoint of a `{rel}` edge"
        );
        assert!(
            store
                .neighbours(&to, Direction::Outbound, &[rel], 6)
                .unwrap()
                .is_empty(),
            "nothing leads *out of* the `to` endpoint of a `{rel}` edge"
        );
    }

    // ---- One test per relation in SPEC §3.3 -----------------------------

    #[test]
    fn implements_runs_task_to_spec() {
        assert_direction(Relation::Implements, EntityType::Task, EntityType::Spec);
    }

    #[test]
    fn blocks_runs_blocker_to_blocked() {
        assert_direction(Relation::Blocks, EntityType::Task, EntityType::Task);
    }

    /// `depends_on` is the ninth relation and the one with no rows.
    ///
    /// It is the inverse of `blocks` and is swapped on write, so the only
    /// honest assertions are that filtering on it alone matches nothing in
    /// either direction, and that the `blocks` edge it normalises to is found
    /// the way §3.3 says. A filter that quietly fell back to "every relation"
    /// would look like it worked and would return the whole graph.
    #[test]
    fn depends_on_is_never_stored_and_matches_nothing() {
        let store = Store::in_memory().unwrap();
        let blocker = node(&store, EntityType::Task, "must finish first");
        let blocked = node(&store, EntityType::Task, "waits");

        let (from, rel, to) =
            Relation::normalise(blocked.clone(), Relation::DependsOn, blocker.clone());
        assert_eq!(rel, Relation::Blocks, "`depends_on` normalises to `blocks`");
        assert_eq!(from, blocker, "and the endpoints swap");
        assert_eq!(to, blocked);
        link(&store, &from, rel, &to);

        for direction in [Direction::Outbound, Direction::Inbound, Direction::Both] {
            for root in [&blocker, &blocked] {
                assert!(
                    store
                        .neighbours(root, direction, &[Relation::DependsOn], 6)
                        .unwrap()
                        .is_empty(),
                    "a `depends_on` filter must match nothing, not everything"
                );
            }
        }

        assert_eq!(
            ids(&store
                .neighbours(&blocker, Direction::Outbound, &[Relation::Blocks], 6)
                .unwrap()),
            vec![blocked.as_str().to_owned()],
            "outbound from the blocker reaches what it blocks"
        );
        assert_eq!(
            ids(&store
                .neighbours(&blocked, Direction::Inbound, &[Relation::Blocks], 6)
                .unwrap()),
            vec![blocker.as_str().to_owned()],
            "inbound to the blocked task reaches its blocker"
        );
    }

    #[test]
    fn supersedes_runs_new_decision_to_old() {
        assert_direction(
            Relation::Supersedes,
            EntityType::Decision,
            EntityType::Decision,
        );
    }

    #[test]
    fn derived_from_runs_spec_to_feedback() {
        assert_direction(
            Relation::DerivedFrom,
            EntityType::Spec,
            EntityType::Feedback,
        );
    }

    #[test]
    fn resolves_runs_decision_to_question() {
        assert_direction(
            Relation::Resolves,
            EntityType::Decision,
            EntityType::Question,
        );
    }

    #[test]
    fn references_runs_anything_to_anything() {
        assert_direction(Relation::References, EntityType::Artifact, EntityType::Task);
    }

    #[test]
    fn duplicates_runs_task_to_task() {
        assert_direction(Relation::Duplicates, EntityType::Task, EntityType::Task);
    }

    #[test]
    fn informs_runs_feedback_to_spec() {
        assert_direction(Relation::Informs, EntityType::Feedback, EntityType::Spec);
    }

    // ---- The traversal itself -------------------------------------------

    /// Depth and path together, because an off-by-one in either is invisible
    /// on a one-hop walk and both are what a caller uses to explain a result.
    #[test]
    fn a_multi_hop_walk_reports_depth_and_the_whole_path() {
        let store = Store::in_memory().unwrap();
        let a = node(&store, EntityType::Task, "a");
        let b = node(&store, EntityType::Task, "b");
        let c = node(&store, EntityType::Task, "c");
        link(&store, &a, Relation::Blocks, &b);
        link(&store, &b, Relation::Blocks, &c);

        let found = store.neighbours(&a, Direction::Outbound, &[], 6).unwrap();
        assert_eq!(found.len(), 2);

        let first = found.iter().find(|n| n.id == b).unwrap();
        assert_eq!(first.depth, 1, "a direct neighbour is at depth 1");
        assert_eq!(
            first.path,
            vec![a.clone(), b.clone()],
            "the path is inclusive of both ends"
        );

        let second = found.iter().find(|n| n.id == c).unwrap();
        assert_eq!(second.depth, 2);
        assert_eq!(second.path, vec![a.clone(), b.clone(), c.clone()]);

        // The same chain walked the other way, so a path built by appending in
        // the wrong place cannot pass by symmetry.
        let back = store.neighbours(&c, Direction::Inbound, &[], 6).unwrap();
        let root_ward = back.iter().find(|n| n.id == a).unwrap();
        assert_eq!(root_ward.depth, 2);
        assert_eq!(root_ward.path, vec![c, b, a]);
    }

    /// A chain longer than the cap, asked for with the largest depth there is.
    #[test]
    fn depth_is_clamped_to_max_depth() {
        let store = Store::in_memory().unwrap();
        let chain: Vec<EntityId> = (0..usize::from(MAX_DEPTH) + 4)
            .map(|i| node(&store, EntityType::Task, &format!("step {i}")))
            .collect();
        for pair in chain.windows(2) {
            link(&store, &pair[0], Relation::Blocks, &pair[1]);
        }

        let found = store
            .neighbours(&chain[0], Direction::Outbound, &[], u8::MAX)
            .unwrap();
        assert_eq!(
            found.len(),
            usize::from(MAX_DEPTH),
            "a walk should stop at MAX_DEPTH hops however deep it was asked to go"
        );
        assert_eq!(
            found.iter().map(|n| n.depth).max(),
            Some(MAX_DEPTH),
            "and should reach exactly that depth"
        );

        // A depth below the cap is honoured as asked, so the clamp cannot be
        // hiding a walk that always runs to sixteen.
        let shallow = store
            .neighbours(&chain[0], Direction::Outbound, &[], 2)
            .unwrap();
        assert_eq!(shallow.len(), 2);
    }

    /// A recursive CTE with no cycle guard does not return on a cyclic graph;
    /// this test either passes quickly or hangs, and hanging is the failure.
    #[test]
    fn a_cycle_terminates() {
        let store = Store::in_memory().unwrap();
        let a = node(&store, EntityType::Task, "a");
        let b = node(&store, EntityType::Task, "b");
        link(&store, &a, Relation::Blocks, &b);
        link(&store, &b, Relation::Blocks, &a);

        let found = store.neighbours(&a, Direction::Outbound, &[], 16).unwrap();
        assert_eq!(
            ids(&found),
            vec![b.as_str().to_owned()],
            "the walk should reach b and stop, not come back round to the root"
        );

        // Both directions, since the guard lives in the recursive term and a
        // walk that only guards one way would still hang the other.
        assert_eq!(
            store
                .neighbours(&a, Direction::Both, &[], 16)
                .unwrap()
                .len(),
            1
        );
    }

    /// Soft delete is the only delete there is, so an archived edge has to stop
    /// being traversable without leaving the table.
    #[test]
    fn an_archived_edge_is_not_walked() {
        let store = Store::in_memory().unwrap();
        let a = node(&store, EntityType::Task, "a");
        let b = node(&store, EntityType::Task, "b");
        let c = node(&store, EntityType::Task, "c");
        link(&store, &a, Relation::Blocks, &b);
        link_with(&store, &b, Relation::Blocks, &c, "", true);

        let found = store.neighbours(&a, Direction::Outbound, &[], 6).unwrap();
        assert_eq!(
            ids(&found),
            vec![b.as_str().to_owned()],
            "an archived edge should not be followed, so c is unreachable"
        );

        assert!(
            store.links_of(&b, Direction::Outbound).unwrap().is_empty(),
            "and it should not be listed either"
        );
    }

    #[test]
    fn a_rels_filter_returns_only_what_was_asked_for() {
        let store = Store::in_memory().unwrap();
        let task = node(&store, EntityType::Task, "the task");
        let spec = node(&store, EntityType::Spec, "the spec");
        let other = node(&store, EntityType::Task, "the twin");
        let noted = node(&store, EntityType::Question, "the question");
        link(&store, &task, Relation::Implements, &spec);
        link(&store, &task, Relation::Duplicates, &other);
        link(&store, &task, Relation::References, &noted);

        let only_implements = store
            .neighbours(&task, Direction::Outbound, &[Relation::Implements], 6)
            .unwrap();
        assert_eq!(ids(&only_implements), vec![spec.as_str().to_owned()]);
        assert_eq!(only_implements[0].rel, Relation::Implements);

        let two = store
            .neighbours(
                &task,
                Direction::Outbound,
                &[Relation::Implements, Relation::References],
                6,
            )
            .unwrap();
        assert_eq!(two.len(), 2);
        assert!(
            !two.iter().any(|n| n.id == other),
            "duplicates was not asked for"
        );

        let everything = store
            .neighbours(&task, Direction::Outbound, &[], 6)
            .unwrap();
        assert_eq!(everything.len(), 3, "an empty filter means every relation");
    }

    /// An edge pointing at an id with no row must still be returned, labelled
    /// empty. Dropping it would make a broken graph look tidy, and `fsck`'s
    /// dangling-link report is the thing that is supposed to notice.
    #[test]
    fn a_dangling_edge_is_returned_with_an_empty_label() {
        let store = Store::in_memory().unwrap();
        let task = node(&store, EntityType::Task, "the task");
        let missing = EntityId::generate(EntityType::Spec);
        link(&store, &task, Relation::Implements, &missing);

        let found = store
            .neighbours(&task, Direction::Outbound, &[], 6)
            .unwrap();
        assert_eq!(
            ids(&found),
            vec![missing.as_str().to_owned()],
            "the edge should survive even though its target does not"
        );
        assert_eq!(found[0].label, "", "with no label to resolve");
        assert_eq!(
            found[0].entity_type,
            EntityType::Spec,
            "the type is on the edge"
        );
    }

    /// `Both` is a union of two walks, not an undirected one — a node reached
    /// outbound must not then be walked inbound.
    #[test]
    fn both_unions_the_two_walks_without_wandering() {
        let store = Store::in_memory().unwrap();
        let root = node(&store, EntityType::Task, "root");
        let downstream = node(&store, EntityType::Task, "downstream");
        let upstream = node(&store, EntityType::Task, "upstream");
        let stranger = node(&store, EntityType::Task, "unrelated");
        link(&store, &root, Relation::Blocks, &downstream);
        link(&store, &upstream, Relation::Blocks, &root);
        // Reachable from `downstream` only by walking backwards, which `Both`
        // must not do once it has stepped forwards.
        link(&store, &stranger, Relation::Blocks, &downstream);

        let found = store.neighbours(&root, Direction::Both, &[], 6).unwrap();
        let mut got = ids(&found);
        got.sort();
        let mut want = vec![downstream.as_str().to_owned(), upstream.as_str().to_owned()];
        want.sort();
        assert_eq!(
            got, want,
            "`Both` is two directed walks, not an undirected one"
        );
    }

    // ---- links_of --------------------------------------------------------

    #[test]
    fn links_of_reports_the_edges_touching_an_entity() {
        let store = Store::in_memory().unwrap();
        let task = node(&store, EntityType::Task, "the task");
        let spec = node(&store, EntityType::Spec, "the spec");
        let blocker = node(&store, EntityType::Task, "the blocker");
        link_with(&store, &task, Relation::Implements, &spec, "REQ-4", false);
        link(&store, &blocker, Relation::Blocks, &task);

        let out = store.links_of(&task, Direction::Outbound).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to_id, spec);
        assert_eq!(out[0].rel, Relation::Implements);
        assert_eq!(out[0].anchor, "REQ-4", "the anchor round-trips");
        assert_eq!(out[0].from_type, EntityType::Task);
        assert_eq!(out[0].to_type, EntityType::Spec);
        assert_eq!(out[0].audit.version, 1, "the audit block is read back");

        let inn = store.links_of(&task, Direction::Inbound).unwrap();
        assert_eq!(inn.len(), 1);
        assert_eq!(inn[0].from_id, blocker);

        assert_eq!(store.links_of(&task, Direction::Both).unwrap().len(), 2);
    }

    /// The failure case for the reader: a row whose `rel` is not a Keel
    /// relation is a corrupted store, and it should say so rather than be
    /// skipped.
    #[test]
    fn a_link_row_with_an_unknown_relation_is_an_error() {
        let store = Store::in_memory().unwrap();
        let a = node(&store, EntityType::Task, "a");
        let b = node(&store, EntityType::Task, "b");
        store
            .connection()
            .execute(
                "INSERT INTO links (id, project_id, from_id, from_type, to_id, to_type, rel,
                                    anchor, created_at, updated_at, created_by, updated_by)
                 VALUES (?1, ?5, ?2, 'task', ?3, 'task', 'relates_to', '',
                         ?4, ?4, 'claude', 'claude')",
                rusqlite::params![
                    LinkId::generate().as_str(),
                    a.as_str(),
                    b.as_str(),
                    AT,
                    PROJECT
                ],
            )
            .unwrap();

        let err = store.links_of(&a, Direction::Outbound).unwrap_err();
        assert!(
            err.to_string().contains("relates_to"),
            "the error should name the value it could not read, got: {err}"
        );
    }
}
