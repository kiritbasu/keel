//! `specline-daemon` — the binary.
//!
//! It is three lines because it has to be in this package and nothing else
//! does. `dist` builds one installer per package that owns binaries, PHASE-10
//! §1 advertises a single `specline-installer.sh`, and
//! `scripts/verify-release-tier1.sh` checks that running it leaves both `specline`
//! and `specline-daemon` on disk — so both binaries belong to one package, and this
//! is the cheapest way for that to be true without moving the daemon itself.
//!
//! The daemon is still in `specline-daemon`; [`specline_daemon::run`] is its entry
//! point.

fn main() -> anyhow::Result<()> {
    specline_daemon::run()
}
