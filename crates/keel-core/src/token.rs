//! The secret that separates "a person did this" from "a web page did this".
//!
//! # What it is for
//!
//! The daemon listens on loopback, and loopback is not a trust boundary. Every
//! process on the machine can reach it, and so can every page the browser has
//! open — the `Origin` check turns away an ordinary cross-origin request, but
//! it cannot help against DNS rebinding, where the attacker's page arrives
//! looking same-origin, and it says nothing at all about a local process.
//!
//! So a mutating endpoint cannot tell, from the request alone, whether somebody
//! clicked something. This is what lets it: a secret that is readable only by
//! the user who owns the store, sent in a header that a form post and an image
//! tag cannot set.
//!
//! # Why it is minted per daemon, not stored
//!
//! A new one every start. That bounds the damage of a leak to the life of one
//! process, and it means there is no long-lived secret in the home directory to
//! be backed up, synced or committed by accident. The cost is that a client
//! holding an old token gets a refusal and has to read the file again — which
//! is the correct behaviour when the daemon it was talking to is gone.
//!
//! # How each caller gets it
//!
//! - **The CLI** reads the file. It already relies on being able to read the
//!   store beside it, so this is no new trust.
//! - **The interface** is handed it by the daemon, which serves the interface.
//!   A page on any other origin cannot read that response, so the token stays
//!   with the page the daemon itself rendered. That is the property that makes
//!   rebinding harmless: the request may arrive, but the reply it needs to
//!   steal is unreadable.
//!
//! Both of those are outside this module — it only mints, writes and reads.

use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// The file, beside the store it protects.
pub fn path(home: &Path) -> PathBuf {
    home.join("token")
}

/// Mint a token, write it, and return it.
///
/// 256 bits from the operating system. Overwrites whatever was there, because
/// the previous daemon's token is not something anybody should still be using.
///
/// The file is created with mode 0600 **before** anything is written to it, not
/// afterwards: creating it readable and tightening it later leaves a window in
/// which the secret is on disk and world-readable, and a window is all this
/// kind of bug ever needs.
pub fn mint(home: &Path) -> Result<String> {
    let mut bytes = [0u8; 32];
    // `rand::fill` rather than `rand::rng().fill_bytes(…)`, which is what this
    // said until rand 0.10 renamed `RngCore` to `Rng` and `Rng` to `RngExt`.
    //
    // **The generator is the same one**, which is the part worth checking
    // rather than assuming: `rand::fill` draws from `rand::rng()`, still a
    // ChaCha12 `ThreadRng`, still marked as cryptographically secure. A bump
    // that quietly swapped in a non-CSPRNG here would produce a token that
    // looks identical and is guessable, and nothing downstream would notice.
    rand::fill(&mut bytes);
    let token = crate::hex::encode(&bytes);

    let file = path(home);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::Io {
            context: format!("create {} for the daemon's token", parent.display()),
            source: e,
        })?;
    }

    write_private(&file, &token)?;
    Ok(token)
}

/// Read the token a running daemon minted, or `None` if there is not one.
///
/// `None` rather than an error for a missing file: no daemon has run against
/// this home yet, which is an ordinary state and not a failure. A file that
/// exists and cannot be read *is* an error, because that is a permissions
/// problem somebody needs to hear about rather than a reason to carry on
/// unauthenticated.
pub fn read(home: &Path) -> Result<Option<String>> {
    let file = path(home);
    match std::fs::read_to_string(&file) {
        Ok(raw) => {
            let token = raw.trim().to_owned();
            Ok(if token.is_empty() { None } else { Some(token) })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io {
            context: format!("read the daemon's token at {}", file.display()),
            source: e,
        }),
    }
}

/// Compare two tokens without leaking how far they matched.
///
/// The threat this closes is thin — an attacker who can time loopback requests
/// precisely enough to walk a 256-bit secret has easier options — but the cost
/// is four lines, and a short-circuiting `==` on a secret is the kind of thing
/// that is correct until the secret gets shorter or moves somewhere slower.
pub fn matches(expected: &str, offered: &str) -> bool {
    if expected.len() != offered.len() {
        return false;
    }
    expected
        .bytes()
        .zip(offered.bytes())
        .fold(0u8, |differences, (a, b)| differences | (a ^ b))
        == 0
}

#[cfg(unix)]
fn write_private(file: &Path, token: &str) -> Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt;

    let mut handle = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(file)
        .map_err(|e| Error::Io {
            context: format!("create the daemon's token at {} as 0600", file.display()),
            source: e,
        })?;
    handle.write_all(token.as_bytes()).map_err(|e| Error::Io {
        context: format!("write the daemon's token to {}", file.display()),
        source: e,
    })?;

    // `.mode()` applies only when the file is created, so an existing file
    // keeps whatever it had. Set it explicitly as well: a token file left
    // readable by a previous version, or by a person, must not survive a
    // restart still readable.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600)).map_err(|e| Error::Io {
        context: format!("restrict {} to its owner", file.display()),
        source: e,
    })
}

#[cfg(not(unix))]
fn write_private(file: &Path, token: &str) -> Result<()> {
    // No Windows target exists (dist-workspace.toml), so this is here to keep
    // the crate compiling rather than as a supported path. It writes the token
    // with the platform's default permissions, which is *not* equivalent, and
    // says so rather than pretending.
    tracing::warn!(
        path = %file.display(),
        "the daemon's token is written with default permissions on this platform, \
         not restricted to its owner"
    );
    std::fs::write(file, token).map_err(|e| Error::Io {
        context: format!("write the daemon's token to {}", file.display()),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn a_minted_token_is_readable_and_long_enough_to_be_a_secret() {
        let home = tempfile::tempdir().unwrap();
        let token = mint(home.path()).unwrap();

        assert_eq!(token.len(), 64, "256 bits, hex encoded");
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(read(home.path()).unwrap().as_deref(), Some(token.as_str()));
    }

    #[test]
    fn two_mints_do_not_produce_the_same_token() {
        let home = tempfile::tempdir().unwrap();
        let first = mint(home.path()).unwrap();
        let second = mint(home.path()).unwrap();
        assert_ne!(first, second, "a token is per daemon, not per machine");
        assert_eq!(
            read(home.path()).unwrap().as_deref(),
            Some(second.as_str()),
            "and the newest one is what a reader gets"
        );
    }

    /// No daemon has ever run here. Ordinary, and not an error — the caller
    /// decides what to do about it.
    #[test]
    fn a_home_with_no_token_reads_as_none() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(read(home.path()).unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let home = tempfile::tempdir().unwrap();
        mint(home.path()).unwrap();

        let mode = std::fs::metadata(path(home.path()))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "a secret every user can read is not a secret");
    }

    /// The case that matters: a file left readable by something else must not
    /// stay that way across a restart.
    #[cfg(unix)]
    #[test]
    fn minting_over_a_world_readable_file_tightens_it() {
        use std::os::unix::fs::PermissionsExt;

        let home = tempfile::tempdir().unwrap();
        let file = path(home.path());
        std::fs::write(&file, "someone else's token").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();

        mint(home.path()).unwrap();

        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn matching_is_exact() {
        assert!(matches("abc", "abc"));
        assert!(!matches("abc", "abd"));
        assert!(!matches("abc", "ab"), "a prefix is not a match");
        assert!(!matches("abc", "abcd"));
        assert!(!matches("abc", ""), "and neither is nothing");
    }
}
