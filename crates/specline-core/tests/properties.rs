//! Property tests for the two things examples cannot cover.
//!
//! The standing contract has asked for these since Phase 0 and `proptest` has
//! been a declared dependency for as long, with nothing using it. Both
//! invariants here are the kind where a handful of hand-picked graphs and a
//! hand-written sequence of writes prove very little:
//!
//! - **Graph traversal.** The recursive CTE has to terminate on a cycle,
//!   return each node once, and report the *shortest* number of hops. A
//!   hand-drawn graph tends to be a tree, and a tree exercises none of that.
//!   Worse, the failure mode is an empty or short result that reads exactly
//!   like "nothing is linked here" — which is why `product/CLAUDE.md` calls
//!   this the most dangerous bug class in the codebase.
//!
//! - **The revision chain.** Versions must be contiguous from 1, exactly one
//!   revision is current, and it is the newest. Every example test writes two
//!   or three revisions in a tidy order; the invariant has to hold for any
//!   sequence, including the repeats that the body-hash check collapses.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use specline_core::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// How many nodes the generated graphs have.
///
/// Small on purpose. Each case builds a real store on disk, and the invariants
/// break on shapes — a cycle, a diamond, a node reachable by two path lengths —
/// which all fit inside six nodes. A hundred-node graph would cost seconds per
/// case and find nothing a six-node one does not.
const NODES: usize = 6;

fn store_with_project() -> (Store, EntityId, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
    let project = store
        .create(
            Project::new("props", "Properties").into(),
            &Provenance::anonymous(Actor::Claude),
        )
        .unwrap()
        .entity
        .id()
        .clone();
    (store, project, dir)
}

/// An arbitrary set of directed edges over `NODES` nodes.
///
/// Deliberately unconstrained: self-edges and back-edges are both allowed, so
/// the generator produces cycles without being asked to. A generator that
/// filtered them out would be testing the shape the implementation finds easy.
fn edges() -> impl Strategy<Value = Vec<(usize, usize)>> {
    prop::collection::vec((0..NODES, 0..NODES), 0..14)
}

/// Shortest-path distance from `root` to every node reachable in `≤ depth`
/// hops, computed the obvious way so it shares no code with the CTE under test.
///
/// The root is excluded even when a cycle leads back to it. That is the
/// traversal's contract rather than an oversight: the query seeds its path with
/// the root, so the root can never be yielded, and "what is linked to this"
/// answering with the thing you asked about would be a strange thing for a
/// caller to have to filter out. Discovered by this test disagreeing with it on
/// a two-node cycle, which is the sort of shape nobody draws by hand.
fn reachable(
    adjacency: &BTreeMap<usize, BTreeSet<usize>>,
    root: usize,
    depth: u8,
) -> BTreeMap<usize, u8> {
    let mut seen: BTreeMap<usize, u8> = BTreeMap::new();
    let mut queue = VecDeque::from([(root, 0u8)]);
    let mut visited = BTreeSet::from([root]);
    while let Some((node, hops)) = queue.pop_front() {
        if hops == depth {
            continue;
        }
        for &next in adjacency.get(&node).into_iter().flatten() {
            let at = hops + 1;
            seen.entry(next).or_insert(at);
            if visited.insert(next) {
                queue.push_back((next, at));
            }
        }
    }
    seen.remove(&root);
    seen
}

proptest! {
    // Each case opens a store, so the default 256 would make this the slowest
    // test in the workspace for no extra coverage. Recorded here rather than
    // left to be wondered about.
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Traversal returns exactly what is reachable, at the right depth, once.
    ///
    /// Four claims in one, because they share an expensive fixture: the set is
    /// right, the hop count is the *shortest* one, nothing repeats, and the
    /// root is never its own neighbour even when it is on a cycle.
    #[test]
    fn outbound_traversal_matches_a_breadth_first_walk(
        raw in edges(),
        root in 0..NODES,
        depth in 1u8..4,
    ) {
        let (mut store, project, _dir) = store_with_project();
        let prov = Provenance::anonymous(Actor::Claude);

        let ids: Vec<EntityId> = (0..NODES)
            .map(|i| {
                store
                    .create(
                        Task::new(project.clone(), format!("node {i}"), "A generated row.").into(),
                        &prov,
                    )
                    .unwrap()
                    .entity
                    .id()
                    .clone()
            })
            .collect();

        // `references` because it is stored exactly as given. `blocks` and
        // `depends_on` are inverses and only one of them is ever stored, so a
        // generated mix of them would be testing the endpoint swap rather than
        // the walk — which `graph_direction.rs` already does, one relation at a
        // time and on purpose.
        let mut adjacency: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        for (from, to) in raw {
            if from == to {
                // A self-edge is refused by the store, so it cannot be part of
                // the expected answer either.
                continue;
            }
            if adjacency.entry(from).or_default().insert(to) {
                store
                    .link(
                        NewLink::new(ids[from].clone(), Relation::References, ids[to].clone()),
                        &prov,
                    )
                    .unwrap();
            }
        }

        let found = store
            .neighbours(&ids[root], Direction::Outbound, &[], depth)
            .unwrap();

        let expected = reachable(&adjacency, root, depth);

        let mut actual: BTreeMap<usize, u8> = BTreeMap::new();
        for neighbour in &found {
            let index = ids.iter().position(|id| *id == neighbour.id).unwrap();
            prop_assert!(
                actual.insert(index, neighbour.depth).is_none(),
                "node {index} was returned twice; a cycle has been walked more than once"
            );
        }

        prop_assert_eq!(
            &actual,
            &expected,
            "traversal disagrees with a breadth-first walk of the same edges"
        );
        prop_assert!(
            !found.iter().any(|n| n.id == ids[root]),
            "the root is not one of its own neighbours, even on a cycle"
        );
    }

    /// Inbound is outbound over the reversed edges, and nothing else.
    ///
    /// The direction bug this project fears most does not show up as an error;
    /// it shows up as the wrong set, and often as an empty one. Asserting
    /// inbound against a reversed adjacency is the only check that fails when
    /// the two are swapped.
    #[test]
    fn inbound_traversal_is_the_reverse_walk(
        raw in edges(),
        root in 0..NODES,
        depth in 1u8..4,
    ) {
        let (mut store, project, _dir) = store_with_project();
        let prov = Provenance::anonymous(Actor::Claude);

        let ids: Vec<EntityId> = (0..NODES)
            .map(|i| {
                store
                    .create(
                        Task::new(project.clone(), format!("node {i}"), "A generated row.").into(),
                        &prov,
                    )
                    .unwrap()
                    .entity
                    .id()
                    .clone()
            })
            .collect();

        let mut forward: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        let mut reverse: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        for (from, to) in raw {
            if from == to || !forward.entry(from).or_default().insert(to) {
                continue;
            }
            reverse.entry(to).or_default().insert(from);
            store
                .link(
                    NewLink::new(ids[from].clone(), Relation::References, ids[to].clone()),
                    &prov,
                )
                .unwrap();
        }

        let found = store
            .neighbours(&ids[root], Direction::Inbound, &[], depth)
            .unwrap();

        let mut actual: BTreeMap<usize, u8> = BTreeMap::new();
        for neighbour in &found {
            let index = ids.iter().position(|id| *id == neighbour.id).unwrap();
            actual.insert(index, neighbour.depth);
        }
        prop_assert_eq!(actual, reachable(&reverse, root, depth));
    }

    /// A revision chain is contiguous from 1, with exactly one current.
    ///
    /// The bodies are generated from a tiny alphabet so that repeats happen
    /// often. A repeat is the interesting case: the store collapses a write
    /// whose body matches the current revision, so "one write, one version" is
    /// *not* the invariant and a test that assumed it would pass while the
    /// real rule went unchecked.
    #[test]
    fn a_revision_chain_is_contiguous_with_one_current(bodies in prop::collection::vec(0u8..4, 1..12)) {
        let (mut store, project, _dir) = store_with_project();
        let prov = Provenance::anonymous(Actor::Claude);

        let spec = store
            .create(Spec::new(project.clone(), "Generated").into(), &prov)
            .unwrap()
            .entity
            .id()
            .clone();

        let mut expected_versions = 0;
        let mut previous: Option<u8> = None;
        for body in &bodies {
            if previous != Some(*body) {
                expected_versions += 1;
            }
            previous = Some(*body);

            let document = Document::first(
                EntityType::Spec,
                spec.clone(),
                Some(project.clone()),
                "Generated",
                format!("body variant {body}\n"),
                Actor::Claude,
                chrono::Utc::now(),
            )
            .unwrap();
            store.write_revision(document).unwrap();

            // Asserted after *every* write, not once at the end. A chain that
            // is briefly wrong and right again by the last write is still a
            // chain a reader could have caught mid-flight.
            let all = store.revisions(&spec).unwrap();
            prop_assert_eq!(all.len(), expected_versions);

            let versions: Vec<i32> = all.iter().map(|d| d.version).collect();
            prop_assert_eq!(
                &versions,
                &(1..=expected_versions as i32).collect::<Vec<_>>(),
                "versions must be contiguous from 1, oldest first"
            );

            let current: Vec<&Document> = all
                .iter()
                .filter(|d| d.status == DocStatus::Current)
                .collect();
            prop_assert_eq!(current.len(), 1, "exactly one revision is current");
            prop_assert_eq!(
                current[0].version,
                expected_versions as i32,
                "the current revision is the newest one"
            );

            let head = store.revision(&spec, None).unwrap().unwrap();
            prop_assert_eq!(head.version, current[0].version);
            prop_assert_eq!(&head.body, &current[0].body);
        }
    }

    /// Every revision stays readable by its number, and each names its parent.
    ///
    /// The chain is the audit trail. A version that cannot be fetched is a
    /// revision that has effectively been deleted, which is the one thing this
    /// store is not allowed to do.
    #[test]
    fn every_version_remains_fetchable_and_linked_to_its_parent(
        bodies in prop::collection::vec(0u8..6, 1..10),
    ) {
        let (mut store, project, _dir) = store_with_project();
        let prov = Provenance::anonymous(Actor::Claude);

        let spec = store
            .create(Spec::new(project.clone(), "Generated").into(), &prov)
            .unwrap()
            .entity
            .id()
            .clone();

        for body in &bodies {
            let document = Document::first(
                EntityType::Spec,
                spec.clone(),
                Some(project.clone()),
                "Generated",
                format!("body variant {body}\n"),
                Actor::Claude,
                chrono::Utc::now(),
            )
            .unwrap();
            store.write_revision(document).unwrap();
        }

        let all = store.revisions(&spec).unwrap();
        for (index, document) in all.iter().enumerate() {
            let fetched = store
                .revision(&spec, Some(document.version))
                .unwrap()
                .expect("every version in the chain is fetchable by number");
            prop_assert_eq!(&fetched.body, &document.body);

            let expected_parent = if index == 0 {
                None
            } else {
                Some(document.version - 1)
            };
            prop_assert_eq!(
                document.parent_version,
                expected_parent,
                "the first revision has no parent and every later one names the version before it"
            );
        }
    }
}
