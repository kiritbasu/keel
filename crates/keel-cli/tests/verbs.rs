//! The work verbs, driven as a person drives them.
//!
//! `ready`, `claim`, `close` and `generate` are the loop this project is built
//! around, and none of them had a test at the CLI layer. The functions
//! underneath are covered; what was not is everything between a shell and them
//! — argument wiring, the daemon-versus-direct decision, exit codes, and
//! whether the output says anything a person could act on.
//!
//! The binary is invoked directly rather than through `assert_cmd`, which the
//! task suggested. Cargo already hands the path over at compile time in
//! `CARGO_BIN_EXE_keel`, so the dependency would buy assertion sugar and
//! nothing else, and scale discipline says a crate has to earn its place.
//!
//! # Two flags on every invocation, and why
//!
//! `--daemon` points at a closed port and every write carries `--force`.
//! Without both, these tests would find whatever daemon the developer running
//! them happens to have up — reading and writing *their* store instead of the
//! temporary one. That is not a theoretical tidiness: the daemon answers reads
//! for a different home entirely, so the failure would be a test that passes
//! against the wrong data.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

/// A port nothing is listening on, so every command takes the direct path.
const NO_DAEMON: &str = "http://127.0.0.1:1";

struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
}

impl Run {
    fn expect_ok(self, what: &str) -> String {
        assert!(
            self.ok,
            "{what} failed\nstdout: {}\nstderr: {}",
            self.stdout, self.stderr
        );
        self.stdout
    }

    fn expect_failure(self, what: &str) -> String {
        assert!(
            !self.ok,
            "{what} should have failed but succeeded\nstdout: {}",
            self.stdout
        );
        format!("{}{}", self.stdout, self.stderr)
    }
}

fn keel_with_session(home: &Path, session: Option<&str>, args: &[&str]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_keel"));
    command
        .arg("--home")
        .arg(home)
        .args(args)
        .env_remove("KEEL_DAEMON_URL")
        .env_remove("KEEL_HOME");
    match session {
        Some(s) => command.env("KEEL_SESSION", s),
        None => command.env_remove("KEEL_SESSION"),
    };
    let output = command.output().expect("run the keel binary");
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn keel(home: &Path, args: &[&str]) -> Run {
    keel_with_session(home, Some("ses_cli_verbs"), args)
}

/// A home holding the fixture corpus, plus one task of this test's own.
///
/// The fixture rather than a hand-built store, because it is the corpus the CLI
/// is meant to be usable against and because there is no `create project` verb
/// — which is itself worth knowing: every project in existence arrives through
/// MCP, `bootstrap` or `fixture`.
fn seeded() -> (tempfile::TempDir, String) {
    let home = tempfile::tempdir().unwrap();
    keel(home.path(), &["--force", "fixture"]).expect_ok("load the fixture");

    let created = keel(
        home.path(),
        &[
            "--force",
            "--json",
            "task",
            "--project",
            "harbour",
            "Wire up the thing",
            "--body",
            "A task the CLI verb tests move through its whole lifecycle.",
        ],
    )
    .expect_ok("create a task");

    let value: serde_json::Value = serde_json::from_str(&created).expect("--json should be json");
    let id = value
        .pointer("/entity/id")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no id in the create output: {created}"))
        .to_owned();

    (home, id)
}

#[test]
fn ready_lists_work_and_says_why_it_is_first() {
    let (home, _task) = seeded();

    let out = keel(
        home.path(),
        &["ready", "harbour", "--limit", "50", "--daemon", NO_DAEMON],
    )
    .expect_ok("keel ready");

    assert!(
        out.contains("Wire up the thing"),
        "ready should name the work: {out}"
    );
    assert!(
        out.contains("nothing is blocking it"),
        "and say why it is where it is, which is the whole point of the ordering: {out}"
    );
}

/// A cut list says it was cut, with a total. Hard constraint 4, at the surface
/// a person actually reads.
#[test]
fn a_truncated_ready_list_says_how_much_it_left_out() {
    let (home, _task) = seeded();

    let out = keel(
        home.path(),
        &["ready", "harbour", "--limit", "3", "--daemon", NO_DAEMON],
    )
    .expect_ok("keel ready --limit 3");

    assert!(
        out.contains("3 of 16 shown"),
        "a cut list that does not say how much it left out reads as the whole list: {out}"
    );
}

#[test]
fn ready_against_a_project_that_does_not_exist_says_so() {
    let (home, _task) = seeded();

    let out = keel(home.path(), &["ready", "nope", "--daemon", NO_DAEMON])
        .expect_failure("keel ready against a missing project");

    assert!(
        out.contains("nope"),
        "the refusal should name what was asked for: {out}"
    );
}

/// A claim has to name a session and Keel never invents one, so the failure
/// case here is the more interesting half.
#[test]
fn claim_records_the_session_and_refuses_without_one() {
    let (home, task) = seeded();

    let out = keel(
        home.path(),
        &["--force", "claim", &task, "--daemon", NO_DAEMON],
    )
    .expect_ok("keel claim with KEEL_SESSION set");
    assert!(out.contains("claimed"), "{out}");

    // `KEEL_SESSION` removed rather than blanked: an empty variable and an
    // absent one are different arguments to clap, and only one of them is what
    // a fresh shell looks like.
    let out = keel_with_session(
        home.path(),
        None,
        &["--force", "claim", &task, "--daemon", NO_DAEMON],
    )
    .expect_failure("a claim naming nobody");
    assert!(
        out.contains("--session") || out.contains("KEEL_SESSION"),
        "and say how to fix it: {out}"
    );
}

/// `done` needs evidence. The storage layer enforces it; this asserts the CLI
/// carries the refusal through rather than swallowing it into an exit code
/// nobody can read.
#[test]
fn close_needs_evidence_for_done_and_takes_it_when_given() {
    let (home, task) = seeded();

    let refused = keel(
        home.path(),
        &[
            "--force",
            "close",
            &task,
            "--reason",
            "done",
            "-m",
            "Finished it.",
            "--daemon",
            NO_DAEMON,
        ],
    )
    .expect_failure("close as done with no evidence");
    assert!(
        refused.contains("evidence"),
        "the refusal has to say what is missing: {refused}"
    );

    let closed = keel(
        home.path(),
        &[
            "--force",
            "close",
            &task,
            "--reason",
            "done",
            "-m",
            "Wired it up and tested it.",
            "--evidence",
            "commit:0000000",
            "--daemon",
            NO_DAEMON,
        ],
    )
    .expect_ok("close as done with evidence");
    assert!(closed.contains("done"), "{closed}");

    // And it is actually closed, not merely reported as such.
    let ready = keel(
        home.path(),
        &["ready", "harbour", "--limit", "50", "--daemon", NO_DAEMON],
    )
    .expect_ok("keel ready after the close");
    assert!(
        !ready.contains("Wire up the thing"),
        "a closed task should not still be offered as work: {ready}"
    );
}

/// An unknown reason is refused with the real ones listed.
///
/// This is the case `product/CLAUDE.md` records going wrong from the other
/// side: the contract said to use a status that never existed, and a session
/// following it literally got an enum rejection listing five values, none of
/// them the word it had been told to use.
#[test]
fn close_with_a_reason_that_is_not_one_lists_the_real_ones() {
    let (home, task) = seeded();

    let out = keel(
        home.path(),
        &[
            "--force",
            "close",
            &task,
            "--reason",
            "dropped",
            "-m",
            "Not doing it.",
            "--daemon",
            NO_DAEMON,
        ],
    )
    .expect_failure("close with an invented reason");

    assert!(
        out.contains("wont_do") && out.contains("superseded"),
        "the refusal should list what is valid: {out}"
    );
}

#[test]
fn generate_writes_the_mirror_and_check_agrees_afterwards() {
    let (home, _task) = seeded();
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().to_str().unwrap().to_owned();

    let out = keel(
        home.path(),
        &[
            "generate", "harbour", "--repo", &repo_path, "--daemon", NO_DAEMON,
        ],
    )
    .expect_ok("keel generate");
    assert!(
        out.contains("wrote") || out.contains("written") || out.contains("file"),
        "generate should say what it did: {out}"
    );
    assert!(
        repo.path().join(".keel/manifest.json").is_file(),
        "generate should have written the mirror manifest"
    );

    // The second run has nothing to do, and `--check` agrees. A generator whose
    // output is not stable across two runs makes the pre-commit hook cry wolf,
    // which is how a check ends up bypassed with --no-verify.
    keel(
        home.path(),
        &[
            "generate", "harbour", "--repo", &repo_path, "--check", "--daemon", NO_DAEMON,
        ],
    )
    .expect_ok("keel generate --check straight after a generate");
}

/// `--check` against a hand-edited file fails, which is the whole reason the
/// pre-commit hook can be trusted.
#[test]
fn generate_check_notices_a_hand_edit() {
    let (home, _task) = seeded();
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().to_str().unwrap().to_owned();
    let args = [
        "generate", "harbour", "--repo", &repo_path, "--daemon", NO_DAEMON,
    ];
    keel(home.path(), &args).expect_ok("keel generate");

    let glossary = repo.path().join(".keel/glossary.md");
    assert!(glossary.is_file(), "the fixture should produce a glossary");
    std::fs::write(&glossary, "I edited this by hand.\n").unwrap();

    let mut check: Vec<&str> = args.to_vec();
    check.push("--check");
    let out = keel(home.path(), &check).expect_failure("--check over a hand-edited file");
    assert!(
        out.contains("glossary"),
        "the report should name the file that differs: {out}"
    );
}

/// Every command that takes `--daemon` defaults to the same place.
///
/// `keel migrate` shipped with `7171` where everything else has `7654`, which
/// is worse than a cosmetic slip: a migrate pointed at a port nothing is
/// listening on concludes no daemon is running and changes the schema under one
/// that is. That is the exact failure the command exists to prevent, arriving
/// through a typo in its own default.
#[test]
fn every_daemon_flag_points_at_the_same_daemon() {
    let source = include_str!("../src/main.rs");

    let mut defaults: Vec<&str> = source
        .match_indices("default_value = \"http://")
        .filter_map(|(at, _)| {
            let rest = &source[at + "default_value = \"".len()..];
            rest.find('"').map(|end| &rest[..end])
        })
        .collect();
    defaults.sort_unstable();
    defaults.dedup();

    assert_eq!(
        defaults.len(),
        1,
        "the CLI has more than one idea of where the daemon lives: {defaults:?}"
    );
}
