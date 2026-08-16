//! The real binary, over a real socket.
//!
//! Every other test in this crate builds the router in-process, which is right
//! for almost everything and skips four things that only exist in the binary:
//! argument parsing, the socket bind, the signal wiring, and the order in which
//! those happen. A daemon that serves perfectly under `axum::serve` in a test
//! and refuses to start from a shell is a daemon nobody can run.
//!
//! One process, shared by the tests below, because starting it is the expensive
//! part and none of these tests writes anything the others read.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A `specline-daemon` process and the address it actually bound.
struct Daemon {
    child: Child,
    base: String,
    home: tempfile::TempDir,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Daemon {
    /// Start the binary and wait until it answers.
    ///
    /// Port 0 so the operating system picks one — a fixed port makes a test
    /// suite fail for whoever happens to have that port in use, and makes two
    /// runs on one machine collide.
    fn start() -> Daemon {
        let home = tempfile::tempdir().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_specline-daemon"))
            .arg("--home")
            .arg(home.path())
            .args(["--bind", "127.0.0.1:0"])
            .env_remove("SPECLINE_HOME")
            .env_remove("SPECLINE_BIND")
            .env("RUST_LOG", "specline_daemon=info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn specline-daemon");

        // The port comes out of the daemon's own startup line rather than being
        // guessed. Reading it is also the readiness signal: the line is logged
        // after the bind succeeds.
        let stdout = child.stdout.take().expect("piped stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        let base = loop {
            assert!(
                Instant::now() < deadline,
                "the daemon never reported an address it had bound"
            );
            line.clear();
            let read = reader.read_line(&mut line).unwrap_or(0);
            if read == 0 {
                let mut rest = String::new();
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_string(&mut rest);
                }
                panic!("the daemon exited before binding. stderr: {rest}");
            }
            if let Some(at) = line.find("MCP endpoint  http://") {
                let url = line[at + "MCP endpoint  ".len()..].trim();
                break url.trim_end_matches("/mcp").to_owned();
            }
        };

        // Keep draining stdout, or the daemon blocks on a full pipe once its
        // log fills the buffer — which would look like a hang under load and be
        // this test's fault rather than the daemon's.
        std::thread::spawn(move || {
            let mut sink = String::new();
            let _ = reader.read_to_string(&mut sink);
        });

        Daemon { child, base, home }
    }
}

impl Daemon {
    /// The token this daemon minted, read the way the CLI reads it.
    ///
    /// Not a test convenience: a mutating request needs it, and reading it from
    /// the daemon's home is the whole of how a real caller gets one. A test
    /// that skipped it would be exercising a path nobody has (KEEL-238).
    fn token(&self) -> String {
        specline_core::token::read(self.home.path())
            .expect("read the daemon's token")
            .expect("a running daemon has minted one")
    }
}

fn daemon() -> Daemon {
    Daemon::start()
}

fn rpc(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

#[test]
fn the_binary_binds_a_port_and_answers_health() {
    let d = daemon();

    let body: Value = ureq::get(&format!("{}/api/health", d.base))
        .call()
        .expect("health over a real socket")
        .into_json()
        .unwrap();

    assert_eq!(body["status"], "ok");
    assert!(body["protocol"].is_string());
    assert!(body["schema"].is_i64());
}

#[test]
fn a_tool_call_works_over_the_real_transport() {
    let d = daemon();

    let response: Value = ureq::post(&format!("{}/mcp", d.base))
        .send_json(rpc(1, "tools/list", json!({})))
        .expect("tools/list")
        .into_json()
        .unwrap();

    let tools = response
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("no tools in {response}"));
    assert_eq!(tools.len(), 13);
}

#[test]
fn the_event_stream_opens_and_says_something() {
    let d = daemon();

    let response = ureq::get(&format!("{}/api/events", d.base))
        .call()
        .expect("open the stream");
    assert!(
        response
            .header("content-type")
            .unwrap_or_default()
            .starts_with("text/event-stream")
    );

    // Just the first chunk. The stream never ends by design, so reading it to
    // completion is a hang rather than a test.
    let mut buffer = [0u8; 64];
    let read = response.into_reader().read(&mut buffer).unwrap();
    assert!(read > 0, "the stream sent no bytes at all");
    assert!(
        String::from_utf8_lossy(&buffer[..read]).contains("specline"),
        "the opening comment is what stops a buffering proxy holding the headers"
    );
}

#[test]
fn generate_runs_over_the_real_transport() {
    let d = daemon();
    let repo = tempfile::tempdir().unwrap();

    ureq::post(&format!("{}/mcp", d.base))
        .send_json(rpc(
            1,
            "tools/call",
            json!({"name": "specline_create",
                   "arguments": {"type": "project", "title": "E2E", "slug": "e2e"}}),
        ))
        .expect("create a project")
        .into_string()
        .unwrap();

    let response: Value = ureq::post(&format!("{}/api/generate", d.base))
        .set("x-specline-token", &d.token())
        .send_json(json!({"project": "e2e", "repo": repo.path()}))
        .expect("generate")
        .into_json()
        .unwrap();

    assert!(
        response
            .pointer("/data/written")
            .and_then(Value::as_array)
            .is_some_and(|w| !w.is_empty()),
        "generate wrote nothing: {response}"
    );
    assert!(repo.path().join(".specline/manifest.json").is_file());
}

#[test]
fn a_malformed_body_over_the_wire_still_comes_back_as_json_rpc() {
    let d = daemon();

    let error = ureq::post(&format!("{}/mcp", d.base))
        .set("content-type", "application/json")
        .send_string("{ not json")
        .expect_err("a broken body is a client error");

    let ureq::Error::Status(status, response) = error else {
        panic!("the daemon dropped the connection instead of answering");
    };
    assert!((400..500).contains(&status), "got {status}");

    let body: Value = response.into_json().expect("the refusal must be JSON");
    assert_eq!(body["jsonrpc"], "2.0");
    assert!(body.pointer("/error/message").is_some(), "{body}");
}

/// The limiter fires, and says how long to wait.
///
/// The burst is 300 with a 50/second refill, so this has to spend more than
/// that faster than it refills. Sequential requests over loopback manage it
/// comfortably; if they ever stop managing it, the assertion says the limiter
/// never fired rather than failing on a count.
#[test]
fn the_rate_limiter_answers_429_with_a_retry_after() {
    let d = daemon();
    let body = rpc(1, "tools/list", json!({}));

    let mut limited = None;
    for _ in 0..1_200 {
        match ureq::post(&format!("{}/mcp", d.base)).send_json(body.clone()) {
            Ok(_) => continue,
            Err(ureq::Error::Status(429, response)) => {
                limited = Some(response);
                break;
            }
            Err(e) => panic!("unexpected error before the limit: {e}"),
        }
    }

    let response = limited.expect(
        "1,200 calls in a row did not trip a 300-burst, 50-per-second limiter — either the \
         limiter is not wired up or the machine is slower than the refill rate",
    );
    assert!(
        response.header("retry-after").is_some(),
        "a 429 without retry-after leaves a client guessing"
    );

    let payload: Value = response.into_json().unwrap();
    assert_eq!(payload["jsonrpc"], "2.0");
    assert!(
        payload
            .pointer("/error/message")
            .and_then(Value::as_str)
            .is_some_and(|m| m.contains("Retry in")),
        "the message should say when: {payload}"
    );
}
