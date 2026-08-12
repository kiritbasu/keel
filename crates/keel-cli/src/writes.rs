//! The one door to a direct store write, and the probe that guards it.
//!
//! Hard constraint 1 says the daemon owns the single write path. Under DuckDB
//! the engine enforced that — a second process could not open the store
//! read-write at all. SQLite in WAL mode will let it, happily and silently, so
//! the constraint is now Keel's to keep.
//!
//! Two holes were open before this module existed.
//!
//! **The fallback was too generous.** `keel generate` asked the daemon first
//! and fell back to opening the store directly on *any* transport error —
//! including the 30-second timeout you get from a daemon that is alive and
//! busy. So the one case most likely to produce a second writer, a slow
//! generate against a working daemon, was the case that produced one.
//!
//! **Several commands never asked.** `note`, `archive` and `task add` opened
//! the store unconditionally. Nothing went wrong, because SQLite serialises
//! writers correctly — but a write that goes round the daemon skips validation
//! provenance, the event, the revision, the embedding and the index, which is
//! six of the seven steps and the entire reason the single write path exists.
//!
//! # Why connection-refused is the only safe signal
//!
//! It is the only answer that means *nothing is listening*. A timeout means the
//! opposite: something accepted the connection and has not replied yet, which
//! is a daemon under load. So the probe is a TCP connect rather than an HTTP
//! request — the question is "is anything holding this port", and an HTTP
//! request answers a harder question and can fail for reasons that are not that
//! one.

use anyhow::{Context, Result, bail};
use keel_core::Store;
use serde_json::Value;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

/// Where the local daemon listens, when nothing says otherwise.
pub const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:7654";

/// The daemon URL for a command that has no `--daemon` flag of its own.
///
/// The write commands take no such flag — they are not *talking* to the daemon,
/// they are checking whether one exists — so they read the same environment
/// variable the flagged commands default from. One place, so the probe cannot
/// look at a different daemon than the one the rest of the CLI talks to.
pub fn daemon_url() -> String {
    std::env::var("KEEL_DAEMON_URL").unwrap_or_else(|_| DEFAULT_DAEMON_URL.to_owned())
}

/// How long to wait for the port to answer. A local daemon accepts a connection
/// in microseconds; a second is already three orders of magnitude of slack.
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// What the probe found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Daemon {
    /// Something is listening. It may be busy — that is still a daemon, and
    /// still a reason not to open the store yourself.
    Listening,
    /// Connection refused: nothing is holding the port. The only answer that
    /// makes a direct write safe.
    NotRunning,
    /// Neither — a bad address, a DNS failure, a firewall. Fails closed,
    /// because "I could not tell" and "nobody is there" are different answers
    /// and only one of them permits a second writer.
    Unknown(String),
}

/// Ask whether a daemon holds `base`.
pub fn probe(base: &str) -> Daemon {
    let addr = match socket_addr(base) {
        Some(a) => a,
        None => return Daemon::Unknown(format!("`{base}` is not an address with a port")),
    };

    match TcpStream::connect_timeout(&addr, PROBE_TIMEOUT) {
        Ok(_) => Daemon::Listening,
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => Daemon::NotRunning,
        Err(e) => Daemon::Unknown(e.to_string()),
    }
}

/// Turn `http://127.0.0.1:7171` into a socket address.
fn socket_addr(base: &str) -> Option<SocketAddr> {
    let rest = base
        .strip_prefix("http://")
        .or_else(|| base.strip_prefix("https://"))
        .unwrap_or(base);
    let authority = rest.split('/').next()?;
    authority.to_socket_addrs().ok()?.next()
}

/// Open the store for writing from a process that is not the daemon.
///
/// Every CLI command that writes goes through here. It refuses when a daemon is
/// listening, which is the whole point: the refusal is the enforcement that the
/// engine stopped providing.
///
/// `force` is the deliberate override, for the cases where a person knows
/// better than the probe — a wedged daemon, or a store being repaired. It is a
/// flag rather than an environment variable so that it appears in the shell
/// history of whoever used it.
pub fn open_for_write(home: &Path, daemon: &str, force: bool, what: &str) -> Result<Store> {
    refuse_if_daemon_is_running(daemon, force, what)?;
    let path = keel_core::store_path(home);
    Store::open(&path).with_context(|| format!("open the store at {}", path.display()))
}

/// The probe half of [`open_for_write`], for the one caller that cannot open a
/// store to do its work.
///
/// `keel migrate` needs the same refusal and cannot go through `open_for_write`
/// to get it: the store it is about to migrate is one `Store::open` declines to
/// open, which is the whole reason the command exists.
pub fn refuse_if_daemon_is_running(daemon: &str, force: bool, what: &str) -> Result<()> {
    if force {
        return Ok(());
    }
    match probe(daemon) {
        Daemon::NotRunning => Ok(()),
        Daemon::Listening => bail!(
            "a daemon is running at {daemon}, and it owns the single write path.\n\n\
             Writing to the store from here would skip validation, provenance, the event, \
             the revision, the embedding and the index — six of the seven steps in a Keel \
             write. Nothing would fail; the row would simply be poorer than every other row.\n\n\
             Ask the daemon instead, or stop it and retry. To write anyway — a wedged \
             daemon, a store being repaired — pass --force."
        ),
        Daemon::Unknown(why) => bail!(
            "could not tell whether a daemon is running at {daemon}: {why}\n\n\
             Refusing to {what}, because a second writer against a live store skips most \
             of what a Keel write does. Pass --force if you know no daemon is running."
        ),
    }
}

/// Refuse to write through a daemon that is older than this binary.
///
/// The pair to the guard in `Store::open`. That one stops a newer CLI changing
/// the schema under a running older daemon; this one stops the same pair of
/// processes doing the quieter version of the same thing — the CLI asking for a
/// write in terms the daemon does not have, and the daemon writing something
/// close enough to look fine.
///
/// The comparison is the schema number, not the package version. A CLI a patch
/// release ahead of the daemon is nothing to stop; a CLI a migration ahead is.
///
/// Silent when nothing answers, because that is the normal case and the caller
/// has its own handling for it: this is a check on a daemon that is there, not
/// a second probe for whether one is.
pub fn refuse_if_daemon_is_older(base: &str) -> Result<()> {
    let Ok(response) = ureq::get(&format!("{base}/api/health"))
        .timeout(PROBE_TIMEOUT)
        .call()
    else {
        return Ok(());
    };
    let Ok(body) = response.into_json::<serde_json::Value>() else {
        return Ok(());
    };
    // A daemon predating the field reports nothing, and that is itself the
    // answer: it was built before schema numbers were compared, so it is older
    // than any binary that knows to ask.
    let theirs = body.get("schema").and_then(serde_json::Value::as_i64);
    let ours = i64::from(keel_core::shipped_schema_version());
    match theirs {
        Some(theirs) if theirs >= ours => Ok(()),
        _ => {
            let theirs = theirs
                .map(|n| n.to_string())
                .unwrap_or_else(|| "an unreported schema".to_owned());
            bail!(
                "the daemon at {base} is at schema {theirs} and this binary ships {ours}, so it \
                 is older than this command.\n\n\
                 It would accept the write and store it in the shape it knows, which is not the \
                 shape this binary would read back. Nothing errors; the row is simply wrong in a \
                 way that surfaces later and somewhere else.\n\n\
                 Restart the daemon from a current build: `./plugin/install.sh`, then stop and \
                 start it. `keel migrate` brings the store up with the daemon stopped."
            )
        }
    }
}

/// Whether a *read* may fall back to opening the store directly.
///
/// Looser than [`open_for_write`], deliberately: a second reader is safe in WAL
/// mode and always was. What this rules out is falling back because the daemon
/// was slow — which for a read costs only a stale snapshot, but for `generate`
/// means writing files from a store the daemon is mid-write on.
pub fn may_read_directly(daemon: &str) -> Result<()> {
    match probe(daemon) {
        Daemon::NotRunning => Ok(()),
        Daemon::Listening => bail!(
            "the daemon at {daemon} is listening but did not answer in time.\n\n\
             Not falling back to the store directly: it is alive and possibly mid-write, so \
             going round it would read a snapshot that is about to be wrong. Retry, or check \
             `keel serve`'s logs."
        ),
        Daemon::Unknown(why) => bail!("could not reach the daemon at {daemon}: {why}"),
    }
}

/// GET one path on the daemon, returning `Ok(None)` when nothing is listening.
///
/// One copy, in the module that already owns "is the daemon there and may I
/// talk to it". There were two — `work::read_daemon` and `main::read_via_daemon`
/// — differing only in their timeout and identical in every judgement that
/// actually matters: that a 404 means the daemon predates this binary rather
/// than declining, that an error body's `error.message` is what a person should
/// see, and that a connection failure is a normal absence rather than a
/// problem. Three judgements maintained twice is three chances for the copies
/// to disagree about what "no daemon" means.
///
/// `None` is a normal state: nothing is holding the store, so opening it
/// directly is correct and safe. A daemon that answers with an *error* is a
/// different thing and is returned as one, because falling back silently would
/// hide a real failure behind a conflicting-lock error a moment later.
pub fn read(base: &str, path: &str, timeout: std::time::Duration) -> Result<Option<Value>> {
    let response = match ureq::get(&format!("{base}{path}")).timeout(timeout).call() {
        Ok(r) => r,
        // A 404 is not the daemon declining — it is a daemon that predates the
        // endpoint, which means it is older than this binary. Falling back
        // would open the store it is holding and fail with a lock error that
        // names none of this.
        Err(ureq::Error::Status(404, _)) => bail!(
            "the daemon at {base} does not know {path}, so it is older than this binary.\n\n\
             Restart it from a current build: `./plugin/install.sh` then `keel-daemon`.\n\
             Until then this command can only run with the daemon stopped."
        ),
        Err(ureq::Error::Status(code, r)) => {
            let text = r.into_string().unwrap_or_default();
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or(text);
            bail!("the daemon at {base} refused {path} ({code}): {message}");
        }
        Err(_) => return Ok(None),
    };
    let body: Value = response
        .into_json()
        .with_context(|| format!("read the daemon's response to {path}"))?;
    Ok(Some(body.get("data").cloned().unwrap_or(body)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_base_url_resolves_to_its_port() {
        let addr = socket_addr("http://127.0.0.1:7171").unwrap();
        assert_eq!(addr.port(), 7171);
        assert_eq!(socket_addr("127.0.0.1:9").unwrap().port(), 9);
        assert_eq!(
            socket_addr("http://127.0.0.1:7171/api").unwrap().port(),
            7171
        );
    }

    #[test]
    fn an_address_with_no_port_is_unknown_not_absent() {
        // Fails closed. Guessing 80 here would mean probing a port Keel does
        // not use, finding nothing, and concluding it is safe to write.
        assert!(socket_addr("http://127.0.0.1").is_none());
        assert!(matches!(probe("not an address"), Daemon::Unknown(_)));
    }

    #[test]
    fn a_closed_port_reads_as_not_running() {
        // Port 1 on loopback: privileged, and nothing binds it.
        assert_eq!(probe("http://127.0.0.1:1"), Daemon::NotRunning);
    }

    #[test]
    fn a_listening_port_reads_as_running_even_with_nothing_behind_it() {
        // The probe asks whether the port is held, not whether an HTTP server
        // is well. A daemon too busy to answer is still a daemon.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_eq!(
            probe(&format!("http://127.0.0.1:{port}")),
            Daemon::Listening
        );
    }

    #[test]
    fn a_direct_write_is_refused_while_a_daemon_listens() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();

        let err = open_for_write(
            dir.path(),
            &format!("http://127.0.0.1:{port}"),
            false,
            "test",
        )
        .expect_err("a listening daemon should refuse a direct write");
        assert!(err.to_string().contains("--force"), "{err}");

        // And --force is a real escape, not decoration.
        open_for_write(
            dir.path(),
            &format!("http://127.0.0.1:{port}"),
            true,
            "test",
        )
        .expect("--force should open the store anyway");
    }

    #[test]
    fn a_direct_write_is_allowed_when_nothing_is_listening() {
        let dir = tempfile::tempdir().unwrap();
        open_for_write(dir.path(), "http://127.0.0.1:1", false, "test")
            .expect("no daemon means the write is unambiguous");
    }

    /// A stand-in daemon that answers one `/api/health` with whatever body the
    /// test wants, so the version comparison can be driven from both sides
    /// without building two versions of the daemon.
    fn health_server(body: &'static str) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf);
            let _ = socket.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        });
        (base, thread)
    }

    #[test]
    fn a_daemon_at_our_schema_is_written_to() {
        let body: &'static str = Box::leak(
            format!(
                r#"{{"status":"ok","schema":{}}}"#,
                keel_core::shipped_schema_version()
            )
            .into_boxed_str(),
        );
        let (base, thread) = health_server(body);

        refuse_if_daemon_is_older(&base).expect("a daemon at our schema is fine to write through");
        thread.join().unwrap();
    }

    #[test]
    fn an_older_daemon_is_refused_before_the_write() {
        let (base, thread) = health_server(r#"{"status":"ok","schema":0}"#);

        let err = refuse_if_daemon_is_older(&base)
            .expect_err("a daemon behind this binary's schema must not be written through");
        let message = err.to_string();
        assert!(
            message.contains("older than this command"),
            "the refusal should say which way round it is: {message}"
        );
        assert!(
            message.contains("keel migrate"),
            "and how to fix it: {message}"
        );
        thread.join().unwrap();
    }

    /// A daemon built before the field existed reports nothing, and that is the
    /// answer rather than a reason to shrug: it predates schema comparison, so
    /// it predates this binary.
    #[test]
    fn a_daemon_that_does_not_report_a_schema_is_treated_as_older() {
        let (base, thread) = health_server(r#"{"status":"ok","version":"0.1.0"}"#);

        let err = refuse_if_daemon_is_older(&base).expect_err("an unreported schema is not a pass");
        assert!(err.to_string().contains("an unreported schema"), "{err}");
        thread.join().unwrap();
    }

    /// Nothing listening is not a version mismatch. The caller has its own
    /// handling for an absent daemon, and turning silence into a refusal would
    /// break every offline command.
    #[test]
    fn nothing_listening_is_not_a_refusal() {
        refuse_if_daemon_is_older("http://127.0.0.1:1")
            .expect("an absent daemon is the caller's problem, not this check's");
    }
}
