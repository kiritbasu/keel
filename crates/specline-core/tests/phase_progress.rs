//! How far a phase has got, which is what the roadmap says instead of a date.
//!
//! `milestone_progress` used to be `milestone_states` and returned the derived
//! state alone — the conclusion, with the counts it was drawn from thrown away.
//! So `render_status` recounted the tasks itself and the roadmap screen, having
//! no counts at all, fell back to `target_date`: a field reachable only through
//! an undocumented bag that nobody had ever set. Seven of fifteen phases showed
//! "no target" and the other four showed the day the store was seeded
//! (KEEL-332).
//!
//! These tests hold the tally and the activity time to the same standard as the
//! state, because the reason they went missing is that nothing asked for them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{
    Actor, Close, CloseReason, EntityId, EntityStore, Milestone, MilestoneState, Project,
    Provenance, Store, Task, TaskStatus, close,
};

struct Fixture {
    store: Store,
    project: EntityId,
    _dir: tempfile::TempDir,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("specline.sqlite")).unwrap();
    let project = store
        .create(
            Project::new("demo", "Demo").into(),
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap()
        .entity
        .id()
        .clone();
    Fixture {
        store,
        project,
        _dir: dir,
    }
}

impl Fixture {
    fn phase(&mut self, name: &str) -> EntityId {
        self.store
            .create(
                Milestone::new(self.project.clone(), name, "A phase this test needs.").into(),
                &Provenance::anonymous(Actor::Human),
            )
            .unwrap()
            .entity
            .id()
            .clone()
    }

    fn task_in(&mut self, phase: &EntityId, title: &str) -> EntityId {
        let mut task = Task::new(
            self.project.clone(),
            title,
            "A row this test needs in the store.",
        );
        task.milestone_id = Some(phase.clone());
        self.store
            .create(task.into(), &Provenance::anonymous(Actor::Human))
            .unwrap()
            .entity
            .id()
            .clone()
    }

    fn start(&mut self, task: &EntityId) {
        let version = self.store.get(task).unwrap().unwrap().audit().version;
        let mut changes = serde_json::Map::new();
        changes.insert("status".to_owned(), TaskStatus::InProgress.as_str().into());
        self.store
            .update(
                task,
                version,
                &changes,
                &Provenance::anonymous(Actor::Human),
            )
            .unwrap();
    }

    fn finish(&mut self, task: &EntityId) {
        close(
            &mut self.store,
            task,
            &Close {
                reason: CloseReason::Done,
                message: "Finished, as this test needs it to be.".to_owned(),
                evidence: vec!["commit:abc1234".to_owned()],
                other: None,
            },
            &Provenance::anonymous(Actor::Claude).with_session("ses_alpha"),
        )
        .unwrap();
    }

    fn progress_of(&self, phase: &EntityId) -> specline_core::MilestoneProgress {
        *self
            .store
            .milestone_progress(&self.project)
            .unwrap()
            .get(phase)
            .expect("the phase is in its own project's progress map")
    }
}

#[test]
fn the_tally_comes_back_with_the_state_it_was_derived_from() {
    let mut f = setup();
    let phase = f.phase("Phase 1 — Spine");
    let done = f.task_in(&phase, "One that is finished");
    let started = f.task_in(&phase, "One that is under way");
    let _todo = f.task_in(&phase, "One nobody has picked up");
    f.finish(&done);
    f.start(&started);

    let progress = f.progress_of(&phase);
    assert_eq!(progress.state, MilestoneState::Active);
    assert_eq!(progress.tally.total, 3);
    assert_eq!(progress.tally.closed, 1);
    assert_eq!(progress.tally.started, 1);
}

#[test]
fn a_phase_with_no_tasks_reports_zero_rather_than_being_absent() {
    let mut f = setup();
    let phase = f.phase("Phase 2 — Named but not scoped");

    // The case the roadmap renders as "not scoped". It has to come back as a
    // row with an empty tally, not as a missing key: a caller that treats
    // absence as zero and a caller that skips the row would disagree about the
    // same phase, and the roadmap is the caller that must not skip it.
    let progress = f.progress_of(&phase);
    assert_eq!(progress.state, MilestoneState::Planned);
    assert_eq!(progress.tally.total, 0);
    assert_eq!(progress.tally.closed, 0);
    assert!(progress.last_activity.is_none());
}

#[test]
fn last_activity_tracks_the_tasks_rather_than_the_phase_row() {
    let mut f = setup();
    let phase = f.phase("Phase 3 — Desktop");
    let task = f.task_in(&phase, "Something that will move");

    let created = f
        .progress_of(&phase)
        .last_activity
        .expect("creating a task under a phase is the phase moving");

    f.finish(&task);
    let after = f
        .progress_of(&phase)
        .last_activity
        .expect("closing a task under a phase is the phase moving");

    assert!(
        after >= created,
        "closing a task should not move the phase's activity time backwards: {after} < {created}"
    );
}

#[test]
fn one_phases_tasks_do_not_count_towards_another() {
    let mut f = setup();
    let first = f.phase("Phase 4 — Integrations");
    let second = f.phase("Phase 5 — Remote");
    let a = f.task_in(&first, "Belongs to the first");
    f.task_in(&second, "Belongs to the second");
    f.finish(&a);

    // The failure this guards is a `GROUP BY` that loses its grouping, which
    // does not error — it reports every phase as finished, which reads like
    // good news.
    assert_eq!(f.progress_of(&first).tally.closed, 1);
    assert_eq!(f.progress_of(&second).tally.closed, 0);
    assert_eq!(f.progress_of(&second).tally.total, 1);
}

#[test]
fn a_task_with_no_phase_is_counted_against_none_of_them() {
    let mut f = setup();
    let phase = f.phase("Phase 6 — Make the tracker real");
    f.task_in(&phase, "Filed under the phase");
    f.store
        .create(
            Task::new(f.project.clone(), "Filed under nothing", "A loose row.").into(),
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap();

    assert_eq!(f.progress_of(&phase).tally.total, 1);
}

#[test]
fn archiving_the_last_task_leaves_the_phase_with_activity_but_no_tally() {
    let mut f = setup();
    let phase = f.phase("Phase 7 — Clean up the footprint");
    let task = f.task_in(&phase, "A row that gets archived");
    let version = f.store.get(&task).unwrap().unwrap().audit().version;
    f.store
        .archive(&task, version, &Provenance::anonymous(Actor::Human))
        .unwrap();

    // The two halves answer different questions on purpose. The tally is about
    // live work, so an archived row leaves it — otherwise the roadmap's bar
    // would count rows nobody can open. The activity time is about whether
    // anything is happening, and archiving a task is something happening.
    let progress = f.progress_of(&phase);
    assert_eq!(progress.tally.total, 0, "an archived task is not live work");
    assert!(
        progress.last_activity.is_some(),
        "archiving a task is still the phase moving"
    );
}

/// The rendered tracker, which is the same numbers in the file a human opens.
mod tracker {
    use super::*;
    use specline_core::{MilestoneKind, render_status};

    fn release(f: &mut Fixture, name: &str, version: &str, shipped: &str) {
        let mut m = Milestone::new(f.project.clone(), name, "A version that went out.");
        m.kind = MilestoneKind::Release;
        m.version_string = Some(version.to_owned());
        m.shipped_at = Some(
            chrono::DateTime::parse_from_rfc3339(shipped)
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        m.status = specline_core::MilestoneStatus::Shipped;
        f.store
            .create(m.into(), &Provenance::anonymous(Actor::Human))
            .unwrap();
    }

    #[test]
    fn the_phase_table_says_when_it_last_moved_instead_of_a_target() {
        let mut f = setup();
        let phase = f.phase("Phase 1 — Spine");
        let task = f.task_in(&phase, "Something to close");
        f.finish(&task);

        let markdown = render_status::render(&f.store, &f.project).unwrap();
        assert!(
            markdown.contains("| Tasks done | Last activity |"),
            "the phase table should carry activity, not a target:\n{markdown}"
        );
        assert!(
            !markdown.contains("| Target |"),
            "the target column is gone:\n{markdown}"
        );
    }

    #[test]
    fn releases_get_their_own_table_and_stay_out_of_the_phases() {
        let mut f = setup();
        f.phase("Phase 10 — Release, distribution and install");
        release(
            &mut f,
            "0.1.0 — the first one",
            "0.1.0",
            "2026-08-15T07:58:25Z",
        );
        release(
            &mut f,
            "0.3.0 — the latest",
            "0.3.0",
            "2026-08-18T09:26:23Z",
        );

        let markdown = render_status::render(&f.store, &f.project).unwrap();
        let (phases, released) = markdown
            .split_once("## Released")
            .expect("a project with releases renders a Released section");

        // The failure this guards is the one the backfill created: ten rows of
        // `planned  0 / 0  —` in the table a reader opens to see how the plan
        // is going, because a release carries no tasks.
        assert!(
            !phases.contains("0.1.0"),
            "a release must not appear in the phase table:\n{phases}"
        );
        assert!(phases.contains("Phase 10"), "the phase is still listed");
        assert!(released.contains("2026-08-15"), "the shipped date is shown");
        assert!(
            released.find("0.1.0") < released.find("0.3.0"),
            "releases run oldest first:\n{released}"
        );
    }

    #[test]
    fn a_project_with_no_releases_renders_no_release_section() {
        let mut f = setup();
        f.phase("Phase 1 — Spine");

        let markdown = render_status::render(&f.store, &f.project).unwrap();
        assert!(!markdown.contains("## Released"), "{markdown}");
    }
}
