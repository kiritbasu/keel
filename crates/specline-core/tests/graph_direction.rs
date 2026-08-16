//! Graph direction: one test per relation, asserting **both** directions.
//!
//! This file is non-negotiable, and it is the reason it is so repetitive.
//!
//! The first draft of the spec had both graph traversals inverted. An inverted
//! traversal returns an empty result set that is indistinguishable from a
//! legitimate "nothing is linked here" — it fails silently, plausibly, and in
//! a direction that makes the product look calm while it quietly loses data.
//!
//! Every test below therefore asserts two things, and the second matters more
//! than the first:
//!
//! 1. Walking the *correct* way finds the neighbour.
//! 2. Walking the *wrong* way finds **nothing**.
//!
//! A test that only checks (1) passes just as happily against an
//! implementation that returns every edge regardless of direction.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::*;

fn store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("keel.sqlite")).unwrap();
    (store, dir)
}

fn prov() -> Provenance {
    Provenance::anonymous(Actor::Claude)
}

/// A store with a project, plus a helper to make entities in it.
struct Fixture {
    store: Store,
    project_id: EntityId,
    _dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let (mut store, dir) = store();
        let project_id = store
            .create(Project::new("specline", "Specline").into(), &prov())
            .unwrap()
            .entity
            .id()
            .clone();
        Fixture {
            store,
            project_id,
            _dir: dir,
        }
    }

    /// Create an entity of `entity_type` with a unique label.
    fn make(&mut self, entity_type: EntityType, label: &str) -> EntityId {
        let p = self.project_id.clone();
        let entity: Entity = match entity_type {
            EntityType::Task => Task::new(p, label, "A row this test needs in the store.").into(),
            EntityType::Spec => Spec::new(p, label).into(),
            EntityType::Decision => Decision::new(p, label).into(),
            EntityType::Question => Question::new(p, label).into(),
            EntityType::Feedback => Feedback::new(p, label).into(),
            EntityType::Milestone => {
                Milestone::new(p, label, "A phase, for a graph-direction test.").into()
            }
            EntityType::Design => Design::new(p, label).into(),
            EntityType::Artifact => Artifact::new(p, label).into(),
            other => panic!("fixture does not make {other}"),
        };
        self.store
            .create(entity, &prov())
            .unwrap()
            .entity
            .id()
            .clone()
    }

    fn link(&mut self, from: &EntityId, rel: Relation, to: &EntityId) {
        self.store
            .link(NewLink::new(from.clone(), rel, to.clone()), &prov())
            .unwrap_or_else(|e| panic!("link {from} {rel} {to}: {e}"));
    }

    fn reachable(&self, root: &EntityId, direction: Direction, rel: Relation) -> Vec<EntityId> {
        self.store
            .neighbours(root, direction, &[rel], DEFAULT_DEPTH)
            .unwrap_or_else(|e| panic!("traverse {direction} on {rel}: {e}"))
            .into_iter()
            .map(|n| n.id)
            .collect()
    }
}

/// Assert that `from --rel--> to` is found walking outbound from `from` and
/// inbound to `to`, and found by neither of the two inversions.
fn assert_direction(f: &Fixture, from: &EntityId, rel: Relation, to: &EntityId) {
    let out = f.reachable(from, Direction::Outbound, rel);
    assert!(
        out.contains(to),
        "`{}`: outbound from the `from` end should reach the `to` end, but reached {out:?}",
        rel.reads_as()
    );

    let inb = f.reachable(to, Direction::Inbound, rel);
    assert!(
        inb.contains(from),
        "`{}`: inbound to the `to` end should reach the `from` end, but reached {inb:?}",
        rel.reads_as()
    );

    // The two inversions. These are the assertions that actually catch the bug.
    assert!(
        f.reachable(to, Direction::Outbound, rel).is_empty(),
        "`{}`: the `to` end must have no outbound {rel} edge — finding one means \
         the endpoints were stored backwards",
        rel.reads_as()
    );
    assert!(
        f.reachable(from, Direction::Inbound, rel).is_empty(),
        "`{}`: the `from` end must have no inbound {rel} edge — finding one means \
         the endpoints were stored backwards",
        rel.reads_as()
    );

    // `Both` finds it from either end.
    assert!(f.reachable(from, Direction::Both, rel).contains(to));
    assert!(f.reachable(to, Direction::Both, rel).contains(from));
}

#[test]
fn implements_runs_task_to_spec() {
    // SPEC §3.3: "from implements to", task → spec REQ-4.
    // UC-7 asks "what implements this spec", which is the *inbound* traversal.
    let mut f = Fixture::new();
    let task = f.make(EntityType::Task, "Build rate limiting");
    let spec = f.make(EntityType::Spec, "Rate limiting spec");
    f.link(&task, Relation::Implements, &spec);
    assert_direction(&f, &task, Relation::Implements, &spec);
}

#[test]
fn blocks_runs_blocker_to_blocked() {
    // "from blocks to": A must finish before B.
    let mut f = Fixture::new();
    let a = f.make(EntityType::Task, "Design the schema");
    let b = f.make(EntityType::Task, "Write the migration");
    f.link(&a, Relation::Blocks, &b);
    assert_direction(&f, &a, Relation::Blocks, &b);
}

#[test]
fn supersedes_runs_newer_to_older() {
    let mut f = Fixture::new();
    let v2 = f.make(EntityType::Decision, "Use DuckDB v2");
    let v1 = f.make(EntityType::Decision, "Use SQLite v1");
    f.link(&v2, Relation::Supersedes, &v1);
    assert_direction(&f, &v2, Relation::Supersedes, &v1);
}

#[test]
fn derived_from_runs_spec_to_feedback() {
    let mut f = Fixture::new();
    let spec = f.make(EntityType::Spec, "Onboarding redesign");
    let feedback = f.make(EntityType::Feedback, "Onboarding felt slow");
    f.link(&spec, Relation::DerivedFrom, &feedback);
    assert_direction(&f, &spec, Relation::DerivedFrom, &feedback);
}

#[test]
fn resolves_runs_decision_to_question() {
    let mut f = Fixture::new();
    let decision = f.make(EntityType::Decision, "Store lives in ~/.specline");
    let question = f.make(EntityType::Question, "Where does the store live?");
    f.link(&decision, Relation::Resolves, &question);
    assert_direction(&f, &decision, Relation::Resolves, &question);
}

#[test]
fn references_runs_source_to_target() {
    let mut f = Fixture::new();
    let a = f.make(EntityType::Spec, "Storage spec");
    let b = f.make(EntityType::Artifact, "DuckDB Lance docs");
    f.link(&a, Relation::References, &b);
    assert_direction(&f, &a, Relation::References, &b);
}

#[test]
fn duplicates_runs_copy_to_original() {
    let mut f = Fixture::new();
    let copy = f.make(EntityType::Task, "Add login page again");
    let original = f.make(EntityType::Task, "Add login page");
    f.link(&copy, Relation::Duplicates, &original);
    assert_direction(&f, &copy, Relation::Duplicates, &original);
}

#[test]
fn informs_runs_feedback_to_spec() {
    let mut f = Fixture::new();
    let feedback = f.make(EntityType::Feedback, "Customers want SSO");
    let spec = f.make(EntityType::Spec, "SSO spec");
    f.link(&feedback, Relation::Informs, &spec);
    assert_direction(&f, &feedback, Relation::Informs, &spec);
}

#[test]
fn depends_on_is_stored_as_blocks_with_the_endpoints_swapped() {
    // D-11. "A depends_on B" and "B blocks A" are the same fact, and only one
    // of them is ever written. Storing both is the single easiest way to make
    // the graph queries silently wrong.
    let mut f = Fixture::new();
    let a = f.make(EntityType::Task, "Write the migration");
    let b = f.make(EntityType::Task, "Design the schema");

    f.link(&a, Relation::DependsOn, &b);

    // Exactly one row, and it is a `blocks` row.
    let rows: i64 = f
        .store
        .connection()
        .query_row(
            "SELECT count(*) FROM links WHERE archived_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1, "one fact must produce one row, not two");

    let rel: String = f
        .store
        .connection()
        .query_row("SELECT rel FROM links", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rel, "blocks", "depends_on must never reach the table");

    // And the endpoints were swapped: B blocks A.
    assert_direction(&f, &b, Relation::Blocks, &a);

    // Asking for `depends_on` in a traversal filter matches nothing, because
    // nothing is stored under that name. This is the honest answer, and the
    // reason no query in the codebase ever names it.
    assert!(
        f.reachable(&a, Direction::Outbound, Relation::DependsOn)
            .is_empty()
    );
    assert!(
        f.reachable(&b, Direction::Outbound, Relation::DependsOn)
            .is_empty()
    );
}

#[test]
fn every_relation_in_the_spec_has_a_direction_test() {
    // A guard against a tenth relation being added without a direction test.
    // If this fails, add the test — do not extend the list.
    let covered = [
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
    for r in Relation::ALL {
        assert!(
            covered.contains(&r),
            "{r} has no direction test in this file"
        );
    }
    assert_eq!(covered.len(), Relation::ALL.len());
}

// --- Traversal behaviour -------------------------------------------------

#[test]
fn traversal_is_transitive_and_reports_depth() {
    let mut f = Fixture::new();
    let a = f.make(EntityType::Task, "A");
    let b = f.make(EntityType::Task, "B");
    let c = f.make(EntityType::Task, "C");
    f.link(&a, Relation::Blocks, &b);
    f.link(&b, Relation::Blocks, &c);

    let found = f
        .store
        .neighbours(&a, Direction::Outbound, &[Relation::Blocks], DEFAULT_DEPTH)
        .unwrap();
    assert_eq!(found.len(), 2);
    let depth_of = |id: &EntityId| found.iter().find(|n| &n.id == id).map(|n| n.depth);
    assert_eq!(depth_of(&b), Some(1));
    assert_eq!(depth_of(&c), Some(2));

    // And the whole chain is reachable inbound from the far end.
    let back = f
        .store
        .neighbours(&c, Direction::Inbound, &[Relation::Blocks], DEFAULT_DEPTH)
        .unwrap();
    assert_eq!(back.len(), 2);
}

#[test]
fn a_depth_limit_stops_the_walk() {
    let mut f = Fixture::new();
    let ids: Vec<EntityId> = (0..5)
        .map(|i| f.make(EntityType::Task, &format!("T{i}")))
        .collect();
    for pair in ids.windows(2) {
        f.link(&pair[0], Relation::Blocks, &pair[1]);
    }

    let two = f
        .store
        .neighbours(&ids[0], Direction::Outbound, &[Relation::Blocks], 2)
        .unwrap();
    assert_eq!(two.len(), 2, "depth 2 must reach exactly two hops");

    let all = f
        .store
        .neighbours(
            &ids[0],
            Direction::Outbound,
            &[Relation::Blocks],
            DEFAULT_DEPTH,
        )
        .unwrap();
    assert_eq!(all.len(), 4);
}

#[test]
fn a_cycle_terminates_instead_of_looping_forever() {
    let mut f = Fixture::new();
    let a = f.make(EntityType::Task, "A");
    let b = f.make(EntityType::Task, "B");
    let c = f.make(EntityType::Task, "C");
    f.link(&a, Relation::Blocks, &b);
    f.link(&b, Relation::Blocks, &c);
    f.link(&c, Relation::Blocks, &a);

    let found = f
        .store
        .neighbours(&a, Direction::Outbound, &[Relation::Blocks], MAX_DEPTH)
        .unwrap();
    // B, C and A itself — reached back round the cycle exactly once.
    assert!(
        found.len() <= 3,
        "the cycle guard did not stop the walk: {found:?}"
    );
    assert!(found.iter().any(|n| n.id == b));
    assert!(found.iter().any(|n| n.id == c));
}

#[test]
fn a_node_reachable_by_two_paths_appears_once_at_its_shortest_depth() {
    // SPEC §4 notes this: a task reachable by two paths of different lengths
    // would otherwise be returned twice.
    let mut f = Fixture::new();
    let root = f.make(EntityType::Task, "root");
    let mid = f.make(EntityType::Task, "mid");
    let leaf = f.make(EntityType::Task, "leaf");
    f.link(&root, Relation::Blocks, &mid);
    f.link(&mid, Relation::Blocks, &leaf);
    f.link(&root, Relation::Blocks, &leaf); // the short path

    let found = f
        .store
        .neighbours(
            &root,
            Direction::Outbound,
            &[Relation::Blocks],
            DEFAULT_DEPTH,
        )
        .unwrap();
    let leaves: Vec<_> = found.iter().filter(|n| n.id == leaf).collect();
    assert_eq!(leaves.len(), 1, "duplicated by path: {found:?}");
    assert_eq!(leaves[0].depth, 1, "should report the shorter path");
}

#[test]
fn an_empty_relation_filter_means_every_stored_relation() {
    let mut f = Fixture::new();
    let task = f.make(EntityType::Task, "T");
    let spec = f.make(EntityType::Spec, "S");
    let other = f.make(EntityType::Task, "U");
    f.link(&task, Relation::Implements, &spec);
    f.link(&task, Relation::Blocks, &other);

    let found = f
        .store
        .neighbours(&task, Direction::Outbound, &[], DEFAULT_DEPTH)
        .unwrap();
    assert_eq!(found.len(), 2);
}

#[test]
fn archived_links_disappear_from_traversal_but_not_from_the_table() {
    let mut f = Fixture::new();
    let task = f.make(EntityType::Task, "T");
    let spec = f.make(EntityType::Spec, "S");
    f.link(&task, Relation::Implements, &spec);

    f.store
        .unlink(&task, Relation::Implements, &spec, "", &prov())
        .unwrap();

    assert!(
        f.reachable(&task, Direction::Outbound, Relation::Implements)
            .is_empty()
    );
    let rows: i64 = f
        .store
        .connection()
        .query_row("SELECT count(*) FROM links", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1, "unlink is a soft delete (D-9), not a DELETE");
}

#[test]
fn archiving_an_entity_archives_its_links_but_not_its_neighbours() {
    let mut f = Fixture::new();
    let task = f.make(EntityType::Task, "T");
    let spec = f.make(EntityType::Spec, "S");
    f.link(&task, Relation::Implements, &spec);

    let version = f.store.get(&task).unwrap().unwrap().audit().version;
    f.store.archive(&task, version, &prov()).unwrap();

    assert!(
        f.reachable(&spec, Direction::Inbound, Relation::Implements)
            .is_empty(),
        "the edge should be archived with its endpoint"
    );
    let neighbour = f.store.get(&spec).unwrap().unwrap();
    assert!(
        !neighbour.audit().is_archived(),
        "archiving a parent must not cascade to children (SPEC §3.1)"
    );
}

#[test]
fn linking_to_a_nonexistent_entity_is_refused() {
    // The foreign key that cannot be declared: `links` is polymorphic across
    // thirteen tables, so the column holding the far end has no single table to
    // reference. Without this check a typo creates an edge to nothing, and the
    // traversal silently drops it.
    let mut f = Fixture::new();
    let task = f.make(EntityType::Task, "T");
    let ghost = EntityId::generate(EntityType::Spec);

    let err = f
        .store
        .link(
            NewLink::new(task, Relation::Implements, ghost.clone()),
            &prov(),
        )
        .unwrap_err();
    assert!(err.to_string().contains(ghost.as_str()), "{err}");
}

#[test]
fn re_linking_the_same_edge_returns_the_existing_one() {
    let mut f = Fixture::new();
    let task = f.make(EntityType::Task, "T");
    let spec = f.make(EntityType::Spec, "S");

    let first = f
        .store
        .link(
            NewLink::new(task.clone(), Relation::Implements, spec.clone()),
            &prov(),
        )
        .unwrap();
    let second = f
        .store
        .link(NewLink::new(task, Relation::Implements, spec), &prov())
        .unwrap();
    assert_eq!(
        first.id, second.id,
        "an agent re-asserting a true fact is not an error"
    );
}

#[test]
fn anchors_make_two_edges_between_the_same_pair_distinct() {
    let mut f = Fixture::new();
    let task = f.make(EntityType::Task, "T");
    let spec = f.make(EntityType::Spec, "S");

    f.store
        .link(
            NewLink::new(task.clone(), Relation::Implements, spec.clone()).anchored("REQ-4"),
            &prov(),
        )
        .unwrap();
    f.store
        .link(
            NewLink::new(task.clone(), Relation::Implements, spec.clone()).anchored("REQ-7"),
            &prov(),
        )
        .unwrap();

    let links = f.store.links_of(&task, Direction::Outbound).unwrap();
    assert_eq!(links.len(), 2);
    let mut anchors: Vec<&str> = links.iter().map(|l| l.anchor.as_str()).collect();
    anchors.sort_unstable();
    assert_eq!(anchors, vec!["REQ-4", "REQ-7"]);
}

#[test]
fn a_traceability_query_answers_what_implements_this_spec() {
    // UC-7 end to end: every task implementing a spec, found inbound.
    let mut f = Fixture::new();
    let spec = f.make(EntityType::Spec, "Storage spec");
    let t1 = f.make(EntityType::Task, "Schema");
    let t2 = f.make(EntityType::Task, "Migrations");
    let unrelated = f.make(EntityType::Task, "Something else");
    f.link(&t1, Relation::Implements, &spec);
    f.link(&t2, Relation::Implements, &spec);

    let implementers = f.reachable(&spec, Direction::Inbound, Relation::Implements);
    assert_eq!(implementers.len(), 2);
    assert!(implementers.contains(&t1));
    assert!(implementers.contains(&t2));
    assert!(!implementers.contains(&unrelated));
}
