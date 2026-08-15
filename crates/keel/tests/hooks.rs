//! The session hooks, executed.
//!
//! This file is the point of KEEL-206. The hooks were 317 lines of bash that
//! **nothing in the workspace or in CI had ever run** — not once. KEEL-192 was
//! a real bug in them, found by reading and fixed by reading, and the fix was
//! guarded by nothing at all. Every other surface in this phase describes
//! itself and is tested; the hooks did neither.
//!
//! So these drive the real binary, over a real socket, with the payload on
//! stdin and the JSON read back off stdout — the same way Claude Code invokes
//! them. Unit tests in `hook.rs` cover the decisions; these cover the wiring,
//! which is where the bash version's dependencies were hiding.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};

/// A daemon that answers `/api/context` and `/api/activity` with fixed bodies.
///
/// Serves in a loop on a background thread: the Stop hook makes two calls, and
/// a one-shot stub would make the second one fail — which is a silent path, so
/// the test would pass for the wrong reason.
fn stub_daemon(context: &'static str, activity: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut socket) = stream else { return };
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            // Drain the headers so the client is not left writing into a
            // socket nobody is reading.
            loop {
                let mut header = String::new();
                match reader.read_line(&mut header) {
                    Ok(0) => break,
                    Ok(_) if header.trim().is_empty() => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }

            let body = if request_line.contains("/api/activity") {
                activity
            } else {
                context
            };
            let _ = socket.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        }
    });

    base
}

/// Run a hook exactly as Claude Code would: payload in, JSON out.
///
/// `TMPDIR` is redirected so the Stop hook's once-per-session marker lands in
/// the test's own directory. Without it, a marker from one test would silence
/// another, and the suite would pass by accident.
fn run_hook(which: &str, daemon: &str, payload: &str, tmpdir: &std::path::Path) -> (String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_keel"))
        .args(["hook", which, "--daemon", daemon])
        .env("TMPDIR", tmpdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the keel binary runs");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary directory")
}

const MATCHED: &str = r#"{"summary":"Keel (keel)\nstatus: active\n\n## Next\n- do the thing","data":{"project":{"slug":"keel"}}}"#;
const UNMATCHED: &str = r#"{"summary":"keel_context matched nothing for this checkout\n\nAcme Corp\n\nWidgets Ltd","data":{"project":null}}"#;
const NO_EVENTS: &str = r#"{"data":{"events":[]}}"#;
const WROTE: &str = r#"{"data":{"events":[{"session_id":"ses_abc123"}]}}"#;

// --- session-start ----------------------------------------------------------

#[test]
fn session_start_injects_the_digest_and_pins_the_session_id() {
    let dir = scratch();
    let daemon = stub_daemon(MATCHED, NO_EVENTS);

    let (stdout, code) = run_hook(
        "session-start",
        &daemon,
        r#"{"cwd":"/tmp/x","session_id":"abc123","source":"startup"}"#,
        dir.path(),
    );

    assert_eq!(code, 0);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON on stdout");
    let context = parsed["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is a string");
    assert!(context.contains("do the thing"), "{context}");
    assert!(context.contains("ses_abc123"), "{context}");
    assert_eq!(
        parsed["hookSpecificOutput"]["hookEventName"], "SessionStart",
        "the event name is what tells Claude Code where this belongs"
    );
}

/// A compaction is not a session start, and re-injecting there spends the most
/// context at the moment there is least.
#[test]
fn session_start_says_nothing_on_a_compaction() {
    let dir = scratch();
    let daemon = stub_daemon(MATCHED, NO_EVENTS);

    let (stdout, code) = run_hook(
        "session-start",
        &daemon,
        r#"{"cwd":"/tmp/x","session_id":"abc123","source":"compact"}"#,
        dir.path(),
    );

    assert_eq!(code, 0);
    assert!(stdout.trim().is_empty(), "{stdout}");
}

/// An unrelated repository must not be handed a roll-up of other projects.
#[test]
fn session_start_in_an_unknown_directory_injects_one_paragraph() {
    let dir = scratch();
    let daemon = stub_daemon(UNMATCHED, NO_EVENTS);

    let (stdout, _) = run_hook(
        "session-start",
        &daemon,
        r#"{"cwd":"/tmp/elsewhere","session_id":"abc123","source":"startup"}"#,
        dir.path(),
    );

    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let context = parsed["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("matched nothing"), "{context}");
    assert!(
        !context.contains("Acme Corp"),
        "another project's name must not reach an unrelated session: {context}"
    );
}

/// The constraint that matters most: this runs before the human's first word.
#[test]
fn session_start_is_silent_and_successful_when_no_daemon_answers() {
    let dir = scratch();
    let (stdout, code) = run_hook(
        "session-start",
        "http://127.0.0.1:1",
        r#"{"cwd":"/tmp/x","session_id":"abc123","source":"startup"}"#,
        dir.path(),
    );

    assert_eq!(code, 0, "a hook must never fail a session start");
    assert!(stdout.trim().is_empty(), "{stdout}");
}

#[test]
fn session_start_survives_a_payload_it_cannot_parse() {
    let dir = scratch();
    let daemon = stub_daemon(MATCHED, NO_EVENTS);
    let (_, code) = run_hook("session-start", &daemon, "this is not json", dir.path());
    assert_eq!(code, 0);
}

// --- stop -------------------------------------------------------------------

#[test]
fn stop_asks_when_a_session_in_a_keel_project_recorded_nothing() {
    let dir = scratch();
    let daemon = stub_daemon(MATCHED, NO_EVENTS);

    let (stdout, code) = run_hook(
        "stop",
        &daemon,
        r#"{"cwd":"/tmp/x","session_id":"abc123","stop_hook_active":false}"#,
        dir.path(),
    );

    assert_eq!(code, 0);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON on stdout");
    assert_eq!(parsed["decision"], "block");
    assert!(
        parsed["reason"]
            .as_str()
            .unwrap()
            .contains("Nothing from this session reached Keel"),
        "{stdout}"
    );
}

/// Seven of ten sessions already write unprompted. A forcing function that
/// fires on correct behaviour is one a user disables within a week.
#[test]
fn stop_is_silent_for_a_session_that_already_wrote() {
    let dir = scratch();
    let daemon = stub_daemon(MATCHED, WROTE);

    let (stdout, code) = run_hook(
        "stop",
        &daemon,
        r#"{"cwd":"/tmp/x","session_id":"abc123","stop_hook_active":false}"#,
        dir.path(),
    );

    assert_eq!(code, 0);
    assert!(stdout.trim().is_empty(), "{stdout}");
}

/// **KEEL-192, and the reason this file exists.** The activity check is global,
/// so a session in an unrelated repository has no events and was nagged for not
/// filing notes about a project that does not exist. This behaviour was fixed
/// by reading and, until now, guarded by nothing.
#[test]
fn stop_is_silent_in_a_directory_keel_has_never_heard_of() {
    let dir = scratch();
    let daemon = stub_daemon(UNMATCHED, NO_EVENTS);

    let (stdout, code) = run_hook(
        "stop",
        &daemon,
        r#"{"cwd":"/tmp/somebody-elses-repo","session_id":"abc123","stop_hook_active":false}"#,
        dir.path(),
    );

    assert_eq!(code, 0);
    assert!(
        stdout.trim().is_empty(),
        "a session in a directory Keel does not know must not be nagged: {stdout}"
    );
}

/// Without this the hook blocks its own continuation, for ever.
#[test]
fn stop_does_not_block_a_continuation_it_caused() {
    let dir = scratch();
    let daemon = stub_daemon(MATCHED, NO_EVENTS);

    let (stdout, code) = run_hook(
        "stop",
        &daemon,
        r#"{"cwd":"/tmp/x","session_id":"abc123","stop_hook_active":true}"#,
        dir.path(),
    );

    assert_eq!(code, 0);
    assert!(stdout.trim().is_empty(), "{stdout}");
}

/// One nudge per session, held by a file rather than by trust.
#[test]
fn stop_asks_at_most_once_for_the_same_session() {
    let dir = scratch();
    let daemon = stub_daemon(MATCHED, NO_EVENTS);
    let payload = r#"{"cwd":"/tmp/x","session_id":"abc123","stop_hook_active":false}"#;

    let (first, _) = run_hook("stop", &daemon, payload, dir.path());
    assert!(first.contains("block"), "the first ask should happen");

    let (second, code) = run_hook("stop", &daemon, payload, dir.path());
    assert_eq!(code, 0);
    assert!(
        second.trim().is_empty(),
        "a session must not be asked twice: {second}"
    );
}

/// A false nudge on every session in a project whose daemon is down would make
/// this the most annoying thing in the toolchain.
#[test]
fn stop_is_silent_when_no_daemon_answers() {
    let dir = scratch();
    let (stdout, code) = run_hook(
        "stop",
        "http://127.0.0.1:1",
        r#"{"cwd":"/tmp/x","session_id":"abc123","stop_hook_active":false}"#,
        dir.path(),
    );

    assert_eq!(code, 0, "a hook must never stop a session from ending");
    assert!(stdout.trim().is_empty(), "{stdout}");
}

#[test]
fn stop_says_nothing_without_a_session_id() {
    let dir = scratch();
    let daemon = stub_daemon(MATCHED, NO_EVENTS);
    let (stdout, code) = run_hook("stop", &daemon, r#"{"cwd":"/tmp/x"}"#, dir.path());
    assert_eq!(code, 0);
    assert!(stdout.trim().is_empty(), "{stdout}");
}

/// The dependency the bash version had and never declared. `python3` is absent
/// on a Mac until the Xcode command line tools arrive, and every parse failure
/// exited 0 silently — so on a fresh machine the hooks did nothing and it
/// looked exactly like Keel being broken.
#[test]
fn the_hooks_need_neither_python_nor_curl() {
    let dir = scratch();
    let daemon = stub_daemon(MATCHED, NO_EVENTS);

    // `env -i` with a path holding nothing but the shell's own utilities: no
    // python3 on a stock Mac, and nothing the binary could shell out to.
    let mut child = Command::new("/usr/bin/env")
        .arg("-i")
        .arg("PATH=/nonexistent")
        .arg(format!("TMPDIR={}", dir.path().display()))
        .arg(env!("CARGO_BIN_EXE_keel"))
        .args(["hook", "session-start", "--daemon", &daemon])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the binary runs with nothing on PATH");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"cwd":"/tmp/x","session_id":"abc123","source":"startup"}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("do the thing"),
        "the hook must work with an empty PATH — that is the whole point of \
         moving it out of bash: {stdout}"
    );
}

// --- the shim ---------------------------------------------------------------
//
// Three lines of shell that cannot be moved into the binary, because their
// whole job is to speak when the binary is not there. Small, and every one of
// these cases was a real failure rather than a hypothetical.

fn shim() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/hooks/keel-hook.sh")
}

fn run_shim(event: &str, keel_bin: &str, payload: &str, tmpdir: &std::path::Path) -> (String, i32) {
    let mut child = Command::new("/bin/sh")
        .arg(shim())
        .arg(event)
        .env("KEEL_BIN", keel_bin)
        .env("TMPDIR", tmpdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the shim runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

/// With a binary, the shim is a pass-through.
#[test]
fn the_shim_hands_off_to_the_binary() {
    let dir = scratch();
    let daemon = stub_daemon(MATCHED, NO_EVENTS);
    // The shim does not take `--daemon`, so the address arrives by environment,
    // which is the same route Claude Code would use.
    let mut child = Command::new("/bin/sh")
        .arg(shim())
        .arg("session-start")
        .env("KEEL_BIN", env!("CARGO_BIN_EXE_keel"))
        .env("KEEL_DAEMON_URL", &daemon)
        .env("TMPDIR", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"cwd":"/tmp/x","session_id":"abc123","source":"startup"}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("do the thing"), "{stdout}");
}

/// Without one, it says so — and only at session start. This is the reason the
/// shim exists: a hook that *is* the binary cannot report the binary's absence.
#[test]
fn the_shim_reports_a_missing_binary_at_session_start() {
    let dir = scratch();
    let (stdout, code) = run_shim(
        "session-start",
        "/nonexistent/keel",
        r#"{"session_id":"abc"}"#,
        dir.path(),
    );

    assert_eq!(code, 0);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let context = parsed["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("/keel:setup"), "{context}");
}

/// A session that is ending is the wrong moment to be told about installation,
/// and Stop output would block it.
#[test]
fn the_shim_says_nothing_about_a_missing_binary_at_stop() {
    let dir = scratch();
    let (stdout, code) = run_shim(
        "stop",
        "/nonexistent/keel",
        r#"{"session_id":"abc"}"#,
        dir.path(),
    );
    assert_eq!(code, 0);
    assert!(stdout.trim().is_empty(), "{stdout}");
}

/// The upgrade path, and a real failure rather than a hypothetical: between
/// updating the plugin and updating the binary, `keel` exists but has no `hook`
/// subcommand. With `exec`, clap's "unrecognized subcommand" went straight to
/// Claude Code — and a non-zero Stop hook means *block, using stderr as the
/// reason*, so a stale binary would have injected a usage message as a
/// blocking instruction.
#[test]
fn a_binary_too_old_to_know_hook_is_silent_rather_than_blocking() {
    let dir = scratch();
    let fake = dir.path().join("old-keel");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho 'error: unrecognized subcommand' >&2\nexit 2\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    for event in ["session-start", "stop"] {
        let (stdout, code) = run_shim(
            event,
            fake.to_str().unwrap(),
            r#"{"session_id":"abc"}"#,
            dir.path(),
        );
        assert_eq!(code, 0, "{event} must exit 0 with a stale binary");
        assert!(
            stdout.trim().is_empty(),
            "{event} must say nothing rather than pass through a usage message: {stdout}"
        );
    }
}
