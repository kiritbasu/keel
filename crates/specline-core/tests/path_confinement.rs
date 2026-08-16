//! A generated file cannot land outside the project.
//!
//! The review's top security finding. `mirror_path`, `status_path` and
//! `decisions_path` are joined onto the repository root and written to, and
//! they were free-form strings with no validation — so a write could point one
//! at `~/.zshenv` and `specline generate` would put a document body there, to be
//! executed by the next shell.
//!
//! What makes it a real finding rather than a theoretical one is where the
//! values come from. They are set by a model that can be prompt-injected by
//! anything it reads, and `POST /api/generate` performs the write unattended,
//! outside whatever file-approval gate the harness around that model provides.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Map, json};
use specline_core::{
    Actor, EntityId, EntityStore, Project, Provenance, Spec, Store, generate, safe_path,
};

fn prov() -> Provenance {
    Provenance::anonymous(Actor::Claude).with_session("ses_paths")
}

fn fixture() -> (tempfile::TempDir, Store, EntityId) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
    let project = store
        .create(Project::new("specline", "Specline").into(), &prov())
        .unwrap()
        .entity
        .id()
        .clone();
    (dir, store, project)
}

/// The two shapes the finding names, refused on the way in.
#[test]
fn an_escaping_mirror_path_is_refused_at_write_time() {
    let (_d, mut store, project) = fixture();

    for attack in [
        "/etc/evil",
        "../../.zshenv",
        "product/../../../.zshenv",
        "~/.zshenv",
    ] {
        let mut spec = Spec::new(project.clone(), format!("A spec pointed at {attack}"));
        spec.mirror_path = Some(attack.to_owned());

        let err = store
            .create(spec.into(), &prov())
            .expect_err("`{attack}` should have been refused");
        assert!(
            err.to_string().contains("mirror_path"),
            "the message should name the field that was wrong: {err}"
        );
    }
}

/// A create is one door. An update is the other, and a rule enforced on one of
/// two doors is a rule with a door.
#[test]
fn an_escaping_mirror_path_is_refused_on_update_too() {
    let (_d, mut store, project) = fixture();
    let spec = store
        .create(
            Spec::new(project.clone(), "An ordinary spec").into(),
            &prov(),
        )
        .unwrap()
        .entity;

    let mut changes = Map::new();
    changes.insert("mirror_path".to_owned(), json!("../../.zshenv"));
    let err = store
        .update(spec.id(), spec.audit().version, &changes, &prov())
        .expect_err("an update should be refused the same way a create is");
    assert!(err.to_string().contains("mirror_path"), "{err}");
}

#[test]
fn an_escaping_status_or_decisions_path_is_refused() {
    let (_d, mut store, project) = fixture();
    let stored = store.get(&project).unwrap().unwrap();

    for field in ["status_path", "decisions_path"] {
        let mut changes = Map::new();
        changes.insert(field.to_owned(), json!("/tmp/keel-escape.md"));
        let err = store
            .update(&project, stored.audit().version, &changes, &prov())
            .expect_err("an absolute path should be refused");
        assert!(err.to_string().contains(field), "{err}");
    }
}

/// A relative `root_path` resolves against wherever the daemon was started, so
/// which repository it names depends on how it was launched.
#[test]
fn a_relative_root_path_is_refused_but_an_absolute_one_is_not() {
    let (_d, mut store, project) = fixture();
    let stored = store.get(&project).unwrap().unwrap();

    let mut bad = Map::new();
    bad.insert("root_path".to_owned(), json!("development/specline"));
    assert!(
        store
            .update(&project, stored.audit().version, &bad, &prov())
            .is_err()
    );

    let mut good = Map::new();
    good.insert(
        "root_path".to_owned(),
        json!("/Users/kb/development/specline"),
    );
    store
        .update(&project, stored.audit().version, &good, &prov())
        .expect("an absolute checkout path is what this field is for");
}

/// The second layer. A value already in the store — written before the
/// validator existed, or by `specline import` — still cannot escape, because the
/// check runs again where the path becomes a write.
#[test]
fn generate_writes_nothing_outside_the_repo_root_even_from_a_stored_bad_path() {
    let (_d, mut store, project) = fixture();
    let repo = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("stolen.md");

    let spec = store
        .create(
            Spec::new(
                project.clone(),
                "A spec with designs on your home directory",
            )
            .into(),
            &prov(),
        )
        .unwrap()
        .entity;
    store
        .write_revision(
            specline_core::Document::first(
                specline_core::EntityType::Spec,
                spec.id().clone(),
                Some(project.clone()),
                "A spec",
                "Content that must not leave the repository.",
                Actor::Claude,
                chrono::Utc::now(),
            )
            .unwrap(),
        )
        .unwrap();

    // Behind the validator's back, the way a row written by an older binary
    // would look. This is exactly what the fsck tests do to corrupt state.
    let escape = format!(
        "../{}/stolen.md",
        outside.path().file_name().unwrap().to_string_lossy()
    );
    store
        .connection()
        .execute(
            "UPDATE specs SET mirror_path = ?1 WHERE id = ?2",
            rusqlite::params![escape, spec.id().as_str()],
        )
        .unwrap();

    let result = generate::all(&store, &project, repo.path(), generate::Mode::Write);

    assert!(
        result.is_err(),
        "generate should refuse a stored path that escapes the repository"
    );
    assert!(
        !target.exists(),
        "generate wrote {} — outside the repository root it was given",
        target.display()
    );
}

/// The case lexical rules cannot catch: a directory inside the repository that
/// is a symlink out of it.
#[cfg(unix)]
#[test]
fn a_symlinked_directory_out_of_the_repo_is_refused_at_join_time() {
    let repo = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), repo.path().join("product")).unwrap();

    let err = safe_path::confine(repo.path(), "product/SPEC.md")
        .expect_err("a symlink leaving the repository should be refused");
    assert!(err.to_string().contains("outside the project"), "{err}");
}

/// The ordinary case still works, which matters more than it sounds: a
/// confinement check that refuses every legitimate path would be discovered
/// immediately, but one that refuses only temporary directories would pass
/// review and fail in tests forever.
#[test]
fn ordinary_generation_still_writes_its_files() {
    let (_d, mut store, project) = fixture();
    let repo = tempfile::tempdir().unwrap();

    let stored = store.get(&project).unwrap().unwrap();
    let mut changes = Map::new();
    changes.insert("status_path".to_owned(), json!("product/STATUS.md"));
    store
        .update(&project, stored.audit().version, &changes, &prov())
        .unwrap();

    let report = generate::all(&store, &project, repo.path(), generate::Mode::Write).unwrap();
    assert!(
        report.written.iter().any(|p| p == "product/STATUS.md"),
        "the tracker should have been written: {:?}",
        report.written
    );
    assert!(repo.path().join("product/STATUS.md").is_file());
}
