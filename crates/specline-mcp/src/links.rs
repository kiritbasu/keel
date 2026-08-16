//! Where the interface is, so a tool result can point at it.
//!
//! # Why the daemon mints these and the model does not
//!
//! A URL into the interface is three things a model would have to guess: the
//! address the daemon actually bound, the project's slug, and whether the type
//! in hand has a screen at all. Get the first wrong and the link is dead on any
//! daemon that is not on 7654; get the third wrong and it opens an empty page,
//! which is worse than no link because it looks like the interface is broken.
//!
//! Returned as data instead, so a wrong link is a bug with a test rather than a
//! hallucination with a plausible shape.
//!
//! # Why this is process-global
//!
//! One daemon serves one address for its lifetime, and every tool call it
//! answers belongs to that daemon. Threading the address through thirteen tools
//! and every response shape would be a parameter that is the same value every
//! time — the kind of plumbing that gets added and then quietly defaulted at
//! one call site.
//!
//! Set once, after the bind succeeds, by the process that did the binding. A
//! caller with no daemon — the CLI reading a store directly — never sets it,
//! and every artifact simply comes back without a `url`. That is the honest
//! answer there: with nothing serving the interface, there is nothing to open.

use std::sync::OnceLock;

static INTERFACE: OnceLock<String> = OnceLock::new();

/// Record where the interface is being served, as an origin: `http://host:port`.
///
/// Called once, by the daemon, after it knows the address it actually bound
/// rather than the one it was asked for — the two differ whenever the port was
/// `0`, which is every test and any second daemon.
///
/// Later calls are ignored rather than being an error. The value describes the
/// process, and a process binds once.
pub fn set_interface(base: &str) {
    let _ = INTERFACE.set(base.trim_end_matches('/').to_owned());
}

/// The interface's origin, if anything is serving one.
pub fn interface() -> Option<&'static str> {
    INTERFACE.get().map(String::as_str)
}

/// The link to a task, by its readable reference.
///
/// The app addresses tasks by `KEEL-42` rather than by ULID, so this is the
/// address a person would also arrive at by clicking, and it stays valid if the
/// row is renumbered — which it cannot be, but the reasoning is why the route
/// is shaped this way.
pub fn task(base: &str, slug: &str, reference: &str) -> String {
    format!("{base}/#/projects/{slug}/tasks/{reference}")
}

/// The link to a document — a spec, decision, question, feedback or design.
///
/// By id, because these have no readable reference and the library screen
/// selects on the id it was given.
pub fn document(base: &str, slug: &str, id: &str) -> String {
    format!("{base}/#/projects/{slug}/documents/{id}")
}

/// The link to a project's overview.
pub fn project(base: &str, slug: &str) -> String {
    format!("{base}/#/projects/{slug}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// The routes are the app's, and this is the copy of them that has to stay
    /// true. If `apps/desktop/src/lib/router.ts` moves a pattern, these are the
    /// strings that go stale — and a link to a route that no longer exists
    /// opens the app's fallback screen rather than failing, which is exactly
    /// the silent kind of wrong.
    #[test]
    fn the_shapes_match_the_apps_routes() {
        assert_eq!(
            task("http://127.0.0.1:7654", "specline", "KEEL-42"),
            "http://127.0.0.1:7654/#/projects/specline/tasks/KEEL-42"
        );
        assert_eq!(
            document("http://127.0.0.1:7654", "specline", "spc_01H8"),
            "http://127.0.0.1:7654/#/projects/specline/documents/spc_01H8"
        );
        assert_eq!(
            project("http://127.0.0.1:7654", "specline"),
            "http://127.0.0.1:7654/#/projects/specline"
        );
    }

    /// A daemon on a port that is not 7654 is the case a model composing URLs
    /// from a template gets wrong, and the reason these are minted rather than
    /// described.
    #[test]
    fn a_daemon_on_another_port_produces_links_to_that_port() {
        assert_eq!(
            task("http://127.0.0.1:9999", "demo", "DEMO-1"),
            "http://127.0.0.1:9999/#/projects/demo/tasks/DEMO-1"
        );
    }

    #[test]
    fn a_trailing_slash_does_not_become_a_double_one() {
        set_interface("http://127.0.0.1:7654/");
        assert_eq!(interface(), Some("http://127.0.0.1:7654"));
    }
}
