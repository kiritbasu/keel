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
    if !force {
        match probe(daemon) {
            Daemon::NotRunning => {}
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

    let path = keel_core::store_path(home);
    Store::open(&path).with_context(|| format!("open the store at {}", path.display()))
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
}
