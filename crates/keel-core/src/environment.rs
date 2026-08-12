//! Where the store is sitting, and whether that is somewhere safe.
//!
//! SQLite is a local database in a strong sense: it is three files that must be
//! written and read as one — `keel.sqlite`, `-wal` and `-shm` — with locking
//! that assumes a filesystem which implements it properly. Two ordinary,
//! well-intentioned setups break that assumption, and neither announces itself.
//!
//! **A sync client.** Dropbox, iCloud Drive, Google Drive and OneDrive all
//! upload files independently as they notice them change. Copying the database
//! at one instant and its write-ahead log at another produces a pair that do
//! not correspond, and restoring that pair — or having the client push it back
//! down over a working store — is the classic way a SQLite database is
//! corrupted. Nothing errors at the time. The damage surfaces later as a read
//! failure in an unrelated place.
//!
//! **A network drive.** SQLite's locking relies on the filesystem honouring
//! advisory locks, and over SMB, NFS and AFP that ranges from unreliable to
//! absent. Two processes then believe they hold the write lock simultaneously.
//!
//! # Why this is a warning and not a refusal
//!
//! Because the detection is a heuristic and a wrong refusal is worse than a
//! wrong warning. A directory called `Dropbox` is not necessarily synced, and a
//! synced directory is not necessarily called that. Refusing to start would
//! make Keel unusable for someone whose setup is fine, on the strength of a
//! path component; saying so loudly costs them one line in a log.

use std::path::{Component, Path};

/// A reason a store's location is worth mentioning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hazard {
    /// The path runs through a directory a sync client owns.
    SyncedFolder {
        /// Which one, as the path component that gave it away.
        service: String,
    },
    /// The path is somewhere that may not be a local disk.
    PossiblyRemote {
        /// What about the path suggested it.
        reason: String,
    },
}

impl Hazard {
    /// One sentence saying what was found.
    pub fn detail(&self) -> String {
        match self {
            Hazard::SyncedFolder { service } => format!(
                "the store is inside a {service} folder, which syncs files independently as it \
                 notices them change"
            ),
            Hazard::PossiblyRemote { reason } => format!(
                "the store may not be on a local disk ({reason}), and SQLite's locking needs a \
                 filesystem that implements advisory locks properly"
            ),
        }
    }

    /// What to do about it.
    pub fn remedy(&self) -> String {
        match self {
            Hazard::SyncedFolder { service } => format!(
                "move the store off the synced tree, or exclude it in {service}'s settings. A \
                 database, its -wal and its -shm have to be copied at the same instant, and a \
                 sync client copies them when it notices each one — which is how a SQLite store \
                 gets corrupted without anything failing at the time. Run `keel backup` first: \
                 a backup is one consistent snapshot and is safe to sync"
            ),
            Hazard::PossiblyRemote { .. } => "keep the store on a local disk. If this is an \
                 external drive rather than a network share, there is nothing wrong — the check \
                 cannot tell them apart from the path alone"
                .to_owned(),
        }
    }
}

/// Directory names that mean a sync client owns everything beneath them.
///
/// Matched on a whole path component, never as a substring: a project called
/// `dropbox-exporter` is not a Dropbox folder, and telling someone their store
/// is at risk when it is not is how a warning gets ignored the next time.
const SYNC_ROOTS: &[(&str, &str)] = &[
    ("Dropbox", "Dropbox"),
    (".dropbox", "Dropbox"),
    ("Mobile Documents", "iCloud Drive"),
    ("CloudStorage", "a cloud storage provider"),
    ("Google Drive", "Google Drive"),
    ("GoogleDrive", "Google Drive"),
    ("OneDrive", "OneDrive"),
    ("Nextcloud", "Nextcloud"),
    ("ownCloud", "ownCloud"),
    ("pCloud Drive", "pCloud"),
    ("Sync", "Sync.com"),
    ("Creative Cloud Files", "Creative Cloud"),
    ("Box Sync", "Box"),
];

/// Everything worth saying about where this store lives.
///
/// Path inspection only: no `statfs`, no mount table, nothing that needs the
/// path to exist. That keeps it testable against strings and keeps `keel-core`
/// free of a libc dependency it has no other use for — at the cost of being a
/// heuristic, which the return type is honest about.
pub fn hazards(path: &Path) -> Vec<Hazard> {
    let mut found = Vec::new();

    for component in path.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        let name = name.to_string_lossy();
        if let Some((_, service)) = SYNC_ROOTS.iter().find(|(dir, _)| *dir == name)
            && !found
                .iter()
                .any(|h| matches!(h, Hazard::SyncedFolder { service: s } if s == service))
        {
            found.push(Hazard::SyncedFolder {
                service: (*service).to_owned(),
            });
        }
    }

    // A UNC path is unambiguously a network share. Everything else here is a
    // guess, and says so.
    let display = path.to_string_lossy();
    if display.starts_with("\\\\") {
        found.push(Hazard::PossiblyRemote {
            reason: "it is a UNC network path".to_owned(),
        });
    } else if path.starts_with("/Volumes/") {
        found.push(Hazard::PossiblyRemote {
            reason: "it is under /Volumes, where macOS mounts external and network drives"
                .to_owned(),
        });
    } else if path.starts_with("/net/") || path.starts_with("/mnt/") {
        found.push(Hazard::PossiblyRemote {
            reason: format!(
                "it is under {}",
                if path.starts_with("/net/") {
                    "/net"
                } else {
                    "/mnt"
                }
            ),
        });
    }

    found
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn services(path: &str) -> Vec<String> {
        hazards(&PathBuf::from(path))
            .into_iter()
            .filter_map(|h| match h {
                Hazard::SyncedFolder { service } => Some(service),
                Hazard::PossiblyRemote { .. } => None,
            })
            .collect()
    }

    #[test]
    fn an_ordinary_home_is_fine() {
        assert!(hazards(&PathBuf::from("/Users/kb/.keel")).is_empty());
        assert!(hazards(&PathBuf::from("/home/kb/.keel")).is_empty());
    }

    #[test]
    fn the_sync_clients_are_recognised() {
        assert_eq!(services("/Users/kb/Dropbox/.keel"), ["Dropbox"]);
        assert_eq!(
            services("/Users/kb/Library/Mobile Documents/com~apple~CloudDocs/.keel"),
            ["iCloud Drive"]
        );
        assert_eq!(services("/Users/kb/OneDrive/keel/.keel"), ["OneDrive"]);
        assert_eq!(services("/Users/kb/Google Drive/.keel"), ["Google Drive"]);
    }

    /// Matched on a whole component, never as a substring. A wrong warning is
    /// how the next right one gets ignored.
    #[test]
    fn a_name_that_merely_contains_one_is_not_one() {
        assert!(services("/Users/kb/dropbox-exporter/.keel").is_empty());
        assert!(services("/Users/kb/my-onedrive-notes/.keel").is_empty());
        assert!(services("/Users/kb/NextcloudBackups/.keel").is_empty());
    }

    #[test]
    fn a_service_is_reported_once_however_many_times_it_appears() {
        assert_eq!(
            services("/Users/kb/Dropbox/work/Dropbox/.keel"),
            ["Dropbox"]
        );
    }

    #[test]
    fn network_shaped_paths_are_flagged_as_a_guess() {
        for path in [
            "\\\\server\\share\\keel",
            "/Volumes/team/.keel",
            "/net/nas/.keel",
        ] {
            let found = hazards(&PathBuf::from(path));
            assert!(
                found
                    .iter()
                    .any(|h| matches!(h, Hazard::PossiblyRemote { .. })),
                "{path} should be flagged"
            );
        }
    }

    /// The remedy has to be actionable, which for the sync case means naming
    /// the service and saying to back up first.
    #[test]
    fn the_remedy_says_what_to_do() {
        let hazard = Hazard::SyncedFolder {
            service: "Dropbox".to_owned(),
        };
        let remedy = hazard.remedy();
        assert!(remedy.contains("Dropbox"), "{remedy}");
        assert!(remedy.contains("keel backup"), "{remedy}");
    }
}
