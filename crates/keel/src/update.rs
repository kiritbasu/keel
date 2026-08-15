//! Replacing the installed binaries with a newer release.
//!
//! # What this is allowed to decide on its own
//!
//! The split is on **schema version, not on how the update arrives**. Two
//! releases that agree about the shape of the stored data are interchangeable
//! as far as anybody's store is concerned, so applying one is a file move and
//! needs no permission. A release that moves the schema is going to rewrite
//! somebody's data on next open, and that is a person's decision every time —
//! not least because a migrated store cannot be un-migrated, so the rollback
//! below would not undo it.
//!
//! # What "verified" means here, exactly
//!
//! The SHA-256 in the release manifest, and nothing else. B-73 has the full
//! reasoning: the repository is private (B-72) and GitHub issues no build
//! provenance for a user-owned private repository, so there is no attestation
//! on any release that exists. The manifest is therefore the trust root, and it
//! travels the same authenticated path as the artifact it describes. A missing
//! manifest is a hard failure rather than a reason to skip the check.
//!
//! That guarantee is real but narrower than provenance: it catches a corrupt,
//! truncated or substituted artifact, and it does not independently establish
//! that GitHub built those bytes from this commit.
//!
//! # Why it shells out to `gh`
//!
//! KEEL-221 found by testing it that a private repository's
//! `releases/download/…` URL returns 404 with a valid token as readily as
//! without one — the bytes are only served from
//! `api.github.com/repos/OWNER/REPO/releases/assets/{id}`, after looking the
//! asset id up by name. `plugin/scripts/setup.sh` already goes through
//! `gh release download` for that reason, and reusing it keeps credential
//! handling out of this process entirely.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The two executables a release installs. Both are replaced together.
///
/// Updating one and not the other is the drift this task exists to stop: they
/// share a store whose shape they each believe something about, and a mismatch
/// between them is not a state anything else in the system checks for.
const BINARIES: [&str; 2] = ["keel", "keel-daemon"];

/// What a release says about itself, as published beside its artifacts.
///
/// Deserialised rather than constructed, so the fields are the contract with
/// `keel release-manifest` and the release job that merges checksums into it.
#[derive(Debug, Deserialize)]
pub struct ReleaseManifest {
    /// The release's own version, as in `0.1.2`.
    pub version: String,
    /// The shape of the store this release believes in. The field the decision
    /// actually turns on.
    pub schema_version: i32,
    /// SHA-256 by artifact filename. Absent on a manifest published before the
    /// release job learned to merge them in, which is a refusal rather than a
    /// warning — see [`verify`].
    #[serde(default)]
    pub artifacts: BTreeMap<String, String>,
}

/// What should happen about a candidate release.
///
/// A value rather than a branch taken inline, so the decision can be tested
/// without a network, a GitHub account or a release existing.
#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    /// The installed version is the published one.
    UpToDate,
    /// The installed version is *newer* than the published one.
    ///
    /// Reachable in ordinary use: `gh release download` with no tag resolves to
    /// the latest non-prerelease, so anybody running an `-rc` is ahead of it.
    /// Treated as its own outcome because applying it would be a silent
    /// downgrade, which looks exactly like a successful update.
    Ahead {
        /// The version that is published.
        published: String,
    },
    /// Safe to apply without asking: same schema, higher version.
    Apply {
        /// The version to install.
        version: String,
        /// The archive to fetch.
        artifact: String,
    },
    /// The schema moves, so a person decides.
    NeedsAPerson {
        /// The version that is published.
        version: String,
        /// The schema the installed binaries believe in.
        from: i32,
        /// The schema the candidate believes in.
        to: i32,
    },
}

/// A version as three numbers plus whether it is a final release.
///
/// Deliberately not a semver dependency. The only comparison this makes is
/// "strictly newer than what is installed", over versions this project itself
/// produces, and the tag filter in `release.yml` already constrains those to
/// `MAJOR.MINOR.PATCH` with an optional suffix.
///
/// A prerelease sorts *below* the release with the same numbers, which is the
/// one rule that stops `0.2.0` being considered older than `0.2.0-rc.1`.
fn parse_version(raw: &str) -> Option<(u64, u64, u64, bool)> {
    let raw = raw.trim().trim_start_matches('v');
    let (core, is_release) = match raw.split_once('-') {
        Some((core, _suffix)) => (core, false),
        None => (raw, true),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, is_release))
}

/// Decide what to do, given what is installed and what is published.
///
/// Pure, and the whole of the policy. Every refusal in here is a refusal to do
/// something that would look like it worked.
pub fn plan(
    installed_version: &str,
    installed_schema: i32,
    manifest: &ReleaseManifest,
    target: &str,
) -> Result<Plan> {
    let installed = parse_version(installed_version).with_context(|| {
        format!("cannot read the installed version {installed_version:?} as MAJOR.MINOR.PATCH")
    })?;
    let published = parse_version(&manifest.version).with_context(|| {
        format!(
            "the release manifest's version {:?} is not MAJOR.MINOR.PATCH, so there is no way to \
             tell whether it is newer than {installed_version}",
            manifest.version
        )
    })?;

    if published == installed {
        return Ok(Plan::UpToDate);
    }
    if published < installed {
        return Ok(Plan::Ahead {
            published: manifest.version.clone(),
        });
    }
    if manifest.schema_version != installed_schema {
        return Ok(Plan::NeedsAPerson {
            version: manifest.version.clone(),
            from: installed_schema,
            to: manifest.schema_version,
        });
    }
    Ok(Plan::Apply {
        version: manifest.version.clone(),
        artifact: archive_name(target),
    })
}

/// The archive filename for a target, as `dist` names it.
fn archive_name(target: &str) -> String {
    format!("keel-{target}.tar.xz")
}

/// The repository releases come from.
///
/// Same name and same default as `plugin/scripts/setup.sh`, so a scratch
/// install and its updates cannot end up pointed at different repositories.
fn repo() -> String {
    std::env::var("KEEL_REPO").unwrap_or_else(|_| "kiritbasu/keel".to_owned())
}

/// Pull one asset out of the latest release.
///
/// `--pattern` is not optional even though it looks like it: with no tag
/// argument `gh release download` refuses without one, and the refusal is an
/// exit 1 whose message reads exactly like a permissions failure. KEEL-221 lost
/// a debugging cycle to that.
fn download(dir: &Path, pattern: &str) -> Result<PathBuf> {
    let repo = repo();
    let output = Command::new("gh")
        .args(["release", "download", "--repo", &repo, "--dir"])
        .arg(dir)
        .args(["--pattern", pattern, "--clobber"])
        .output()
        .context(
            "could not run `gh`, which is how Keel fetches a release from a private repository \
             (B-73).\n\nInstall it and sign in:\n    brew install gh\n    gh auth login\n\nThen \
             try again: keel update",
        )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`gh release download --repo {repo} --pattern {pattern}` failed:\n{}\n\nThis is also \
             what a signed-out `gh` looks like, and what no access to {repo} looks like. Check \
             with:\n    gh auth status\n    gh release view --repo {repo}",
            stderr.trim()
        );
    }

    let path = dir.join(pattern);
    if !path.is_file() {
        bail!(
            "`gh` reported success but {pattern} is not in the download directory. The release \
             may not carry that asset — check with: gh release view --repo {repo}"
        );
    }
    Ok(path)
}

/// Read the latest release's manifest.
pub fn fetch_manifest(dir: &Path) -> Result<ReleaseManifest> {
    let path = download(dir, "keel-release.json")?;
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("reading the release manifest at {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| {
        format!(
            "the release manifest at {} is not the JSON `keel release-manifest` produces",
            path.display()
        )
    })
}

/// Check a downloaded file against the hash the manifest states for it.
///
/// The absence of an entry is a failure, not a skip. A manifest that does not
/// mention the artifact cannot vouch for it, and proceeding anyway is the
/// unverified fallback B-73 rules out.
pub fn verify(path: &Path, artifact: &str, manifest: &ReleaseManifest) -> Result<()> {
    use sha2::{Digest, Sha256};

    let Some(expected) = manifest.artifacts.get(artifact) else {
        bail!(
            "the release manifest states no checksum for {artifact}, so there is nothing to \
             verify it against. Refusing to install it. This is what a release published before \
             the manifest carried checksums looks like."
        );
    };

    let bytes = std::fs::read(path)
        .with_context(|| format!("reading {} to check its hash", path.display()))?;
    let actual = format!("{:x}", Sha256::digest(&bytes));

    if &actual != expected {
        bail!(
            "{artifact} does not match the checksum in the release manifest.\n  expected \
             {expected}\n  got      {actual}\n\nNothing has been installed. A truncated download \
             is the usual cause; a repeat is worth investigating rather than retrying."
        );
    }
    Ok(())
}

/// Where the running executables live.
fn install_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("finding the running executable's own path")?;
    let dir = exe
        .parent()
        .context("the running executable has no parent directory")?;
    Ok(dir.to_path_buf())
}

/// Put the new binaries in place, keeping the ones they replace.
///
/// Renaming over a running executable is safe on Unix — the running process
/// holds its inode and carries on with the bytes it started from — which is why
/// this can replace `keel` while `keel` is the thing doing the replacing.
///
/// The previous copies are kept as `<name>.previous` for [`rollback`]. One
/// generation, not a history: the case it exists for is "the release I just
/// took is bad", and keeping more would mostly ensure that the copy somebody
/// eventually reaches for is one nobody has run.
pub fn install_from(unpacked: &Path, into: &Path) -> Result<()> {
    for name in BINARIES {
        let fresh = unpacked.join(name);
        if !fresh.is_file() {
            bail!(
                "the release archive does not contain {name}. Refusing to install a partial \
                 release — {} and {} have to move together.",
                BINARIES[0],
                BINARIES[1]
            );
        }
    }

    for name in BINARIES {
        let fresh = unpacked.join(name);
        let live = into.join(name);
        let kept = into.join(format!("{name}.previous"));

        if live.exists() {
            std::fs::rename(&live, &kept).with_context(|| {
                format!(
                    "keeping the current {name} as {} before replacing it",
                    kept.display()
                )
            })?;
        }
        std::fs::copy(&fresh, &live)
            .with_context(|| format!("installing {name} to {}", live.display()))?;
        make_executable(&live)?;
    }
    Ok(())
}

/// Restore the binaries kept by the last [`install_from`].
pub fn rollback(dir: &Path) -> Result<()> {
    for name in BINARIES {
        if !dir.join(format!("{name}.previous")).is_file() {
            bail!(
                "there is no {name}.previous in {}, so there is no earlier version to go back \
                 to. Only an update taken by `keel update` leaves one.",
                dir.display()
            );
        }
    }

    for name in BINARIES {
        let live = dir.join(name);
        let kept = dir.join(format!("{name}.previous"));
        std::fs::rename(&kept, &live)
            .with_context(|| format!("restoring {} from {}", live.display(), kept.display()))?;
        make_executable(&live)?;
    }
    Ok(())
}

/// Give a freshly written binary the execute bit.
///
/// `fs::copy` carries the mode on Unix, so this is belt and braces for the
/// case where the source came out of an archive that did not record one.
fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .with_context(|| format!("reading the mode of {}", path.display()))?
            .permissions();
        perms.set_mode(perms.mode() | 0o755);
        std::fs::set_permissions(path, perms)
            .with_context(|| format!("making {} executable", path.display()))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Unpack a release archive and return the directory holding its binaries.
///
/// `tar` rather than a crate: it is present on both platforms this ships to,
/// and the alternative is two dependencies (tar and xz) to do what one process
/// already does. The archive's internal directory is named for the target,
/// which is a published layout — checked against the real v0.1.1 asset rather
/// than assumed.
pub fn unpack(archive: &Path, into: &Path, target: &str) -> Result<PathBuf> {
    let status = Command::new("tar")
        .arg("-xJf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .status()
        .with_context(|| format!("running tar to unpack {}", archive.display()))?;
    if !status.success() {
        bail!(
            "tar could not unpack {}, so the download is not a usable release archive",
            archive.display()
        );
    }

    let dir = into.join(format!("keel-{target}"));
    if !dir.is_dir() {
        bail!(
            "the archive did not contain the directory keel-{target}. Its layout has changed, and \
             installing from a guess about where the binaries are is not worth doing."
        );
    }
    Ok(dir)
}

/// The triple this binary was built for, recorded by `build.rs`.
fn target() -> Result<&'static str> {
    let target = env!("KEEL_TARGET");
    if target.is_empty() {
        bail!(
            "this binary was built without a target triple recorded, so there is no way to know \
             which release archive belongs to it. Rebuild with a normal `cargo build`."
        );
    }
    Ok(target)
}

/// `keel update`.
///
/// One line when there is nothing to do, one line when something is done, and
/// a paragraph only when a person has to decide something. When the daemon
/// eventually runs this check on its own schedule — the open half of KEEL-203 —
/// it will report through a log nobody reads, so the terminal is where any of
/// this is legible and the wording is worth the care.
pub fn run(check_only: bool, rollback_requested: bool, json: bool) -> Result<()> {
    let dir = install_dir()?;

    if rollback_requested {
        rollback(&dir)?;
        if json {
            println!("{}", serde_json::json!({ "rolled_back": true }));
        } else {
            println!(
                "Put the previous binaries back in {}. Restart the daemon to run them.",
                dir.display()
            );
        }
        return Ok(());
    }

    let target = target()?;
    let work = tempfile::tempdir().context("making a scratch directory for the download")?;
    let manifest = fetch_manifest(work.path())?;

    let installed_version = env!("CARGO_PKG_VERSION");
    let installed_schema = keel_core::shipped_schema_version();
    let plan = plan(installed_version, installed_schema, &manifest, target)?;

    if json {
        println!("{}", serde_json::to_string(&describe(&plan))?);
    }

    match &plan {
        Plan::UpToDate => {
            if !json {
                println!("Keel {installed_version} is the current release.");
            }
            Ok(())
        }
        Plan::Ahead { published } => {
            if !json {
                println!(
                    "Keel {installed_version} is newer than the published release ({published}). \
                     Leaving it alone."
                );
            }
            Ok(())
        }
        Plan::NeedsAPerson { version, from, to } => {
            if !json {
                println!(
                    "Keel {version} is available and changes the store's shape (schema {from} → \
                     {to}), so it is not applied automatically.\n\nA migration rewrites your \
                     store and cannot be undone — `keel update --rollback` puts the binaries \
                     back, not the data. Take it deliberately, with the daemon stopped:\n\n    \
                     keel backup <dir>\n    # install {version} the way you installed Keel the \
                     first time\n    keel migrate\n\nThere is no flag here that will do it for \
                     you. That is the point of this refusal, not a gap in it."
                );
            }
            Ok(())
        }
        Plan::Apply { version, artifact } => {
            if check_only {
                if !json {
                    println!(
                        "Keel {version} is available and safe to apply (same store shape). Run \
                         `keel update` to take it."
                    );
                }
                return Ok(());
            }

            let archive = download(work.path(), artifact)?;
            verify(&archive, artifact, &manifest)?;
            let unpacked = unpack(&archive, work.path(), target)?;
            install_from(&unpacked, &dir)?;

            if !json {
                println!(
                    "Updated Keel {installed_version} → {version}. Restart the daemon to run it; \
                     `keel update --rollback` undoes this."
                );
            }
            Ok(())
        }
    }
}

/// The plan as JSON, for `--json`.
///
/// Hand-written rather than derived: the wire shape is read by whatever watches
/// this, and deriving it would tie that contract to the enum's field names.
fn describe(plan: &Plan) -> serde_json::Value {
    match plan {
        Plan::UpToDate => serde_json::json!({ "action": "none", "reason": "up_to_date" }),
        Plan::Ahead { published } => {
            serde_json::json!({ "action": "none", "reason": "ahead", "published": published })
        }
        Plan::NeedsAPerson { version, from, to } => serde_json::json!({
            "action": "needs_a_person",
            "reason": "schema_change",
            "version": version,
            "schema_from": from,
            "schema_to": to,
        }),
        Plan::Apply { version, artifact } => serde_json::json!({
            "action": "apply",
            "version": version,
            "artifact": artifact,
        }),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn manifest(version: &str, schema: i32) -> ReleaseManifest {
        ReleaseManifest {
            version: version.to_owned(),
            schema_version: schema,
            artifacts: BTreeMap::new(),
        }
    }

    #[test]
    fn same_version_is_up_to_date() {
        let m = manifest("0.1.1", 7);
        assert_eq!(
            plan("0.1.1", 7, &m, "aarch64-apple-darwin").unwrap(),
            Plan::UpToDate
        );
    }

    #[test]
    fn a_newer_release_on_the_same_schema_applies() {
        let m = manifest("0.1.2", 7);
        assert_eq!(
            plan("0.1.1", 7, &m, "aarch64-apple-darwin").unwrap(),
            Plan::Apply {
                version: "0.1.2".to_owned(),
                artifact: "keel-aarch64-apple-darwin.tar.xz".to_owned(),
            }
        );
    }

    /// The whole point of the task: a schema move never applies itself.
    #[test]
    fn a_schema_change_waits_for_a_person() {
        let m = manifest("0.2.0", 8);
        assert_eq!(
            plan("0.1.1", 7, &m, "aarch64-apple-darwin").unwrap(),
            Plan::NeedsAPerson {
                version: "0.2.0".to_owned(),
                from: 7,
                to: 8,
            }
        );
    }

    /// `gh release download` with no tag resolves to the latest *non*-prerelease,
    /// so anybody on an rc is ahead of it. Applying that is a downgrade wearing
    /// a successful update's clothes.
    #[test]
    fn an_older_published_release_is_not_applied() {
        let m = manifest("0.1.1", 7);
        assert_eq!(
            plan("0.1.2", 7, &m, "aarch64-apple-darwin").unwrap(),
            Plan::Ahead {
                published: "0.1.1".to_owned()
            }
        );
    }

    #[test]
    fn a_prerelease_is_older_than_its_own_release() {
        // 0.2.0-rc.1 installed, 0.2.0 published: an upgrade, not a downgrade.
        let m = manifest("0.2.0", 7);
        assert_eq!(
            plan("0.2.0-rc.1", 7, &m, "aarch64-apple-darwin").unwrap(),
            Plan::Apply {
                version: "0.2.0".to_owned(),
                artifact: "keel-aarch64-apple-darwin.tar.xz".to_owned(),
            }
        );
    }

    #[test]
    fn an_unreadable_published_version_is_an_error_not_a_guess() {
        let m = manifest("latest", 7);
        let err = plan("0.1.1", 7, &m, "aarch64-apple-darwin").unwrap_err();
        assert!(
            err.to_string().contains("not MAJOR.MINOR.PATCH"),
            "unhelpful error: {err}"
        );
    }

    #[test]
    fn a_missing_checksum_refuses_rather_than_skipping() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("keel-x.tar.xz");
        std::fs::write(&file, b"whatever").unwrap();

        let err = verify(&file, "keel-x.tar.xz", &manifest("0.1.2", 7)).unwrap_err();
        assert!(
            err.to_string().contains("no checksum"),
            "unhelpful error: {err}"
        );
    }

    #[test]
    fn a_wrong_checksum_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("keel-x.tar.xz");
        std::fs::write(&file, b"whatever").unwrap();

        let mut m = manifest("0.1.2", 7);
        m.artifacts
            .insert("keel-x.tar.xz".to_owned(), "00".repeat(32));

        let err = verify(&file, "keel-x.tar.xz", &m).unwrap_err();
        assert!(
            err.to_string().contains("does not match"),
            "unhelpful error: {err}"
        );
        assert!(err.to_string().contains("Nothing has been installed"));
    }

    #[test]
    fn a_right_checksum_passes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("keel-x.tar.xz");
        std::fs::write(&file, b"whatever").unwrap();

        let mut m = manifest("0.1.2", 7);
        // sha256("whatever")
        m.artifacts.insert(
            "keel-x.tar.xz".to_owned(),
            "85738f8f9a7f1b04b5329c590ebcb9e425925c6d0984089c43a022de4f19c281".to_owned(),
        );

        verify(&file, "keel-x.tar.xz", &m).unwrap();
    }

    #[test]
    fn installing_keeps_the_previous_binaries() {
        let fresh = tempfile::tempdir().unwrap();
        let live = tempfile::tempdir().unwrap();
        for name in BINARIES {
            std::fs::write(fresh.path().join(name), b"new").unwrap();
            std::fs::write(live.path().join(name), b"old").unwrap();
        }

        install_from(fresh.path(), live.path()).unwrap();

        for name in BINARIES {
            assert_eq!(std::fs::read(live.path().join(name)).unwrap(), b"new");
            assert_eq!(
                std::fs::read(live.path().join(format!("{name}.previous"))).unwrap(),
                b"old"
            );
        }
    }

    /// Both binaries move together or neither does. A release archive missing
    /// one of them is the drift this task exists to prevent, arriving as an
    /// install rather than as a slow divergence.
    #[test]
    fn a_partial_archive_installs_nothing() {
        let fresh = tempfile::tempdir().unwrap();
        let live = tempfile::tempdir().unwrap();
        std::fs::write(fresh.path().join("keel"), b"new").unwrap();
        for name in BINARIES {
            std::fs::write(live.path().join(name), b"old").unwrap();
        }

        let err = install_from(fresh.path(), live.path()).unwrap_err();
        assert!(
            err.to_string().contains("keel-daemon"),
            "unhelpful error: {err}"
        );
        for name in BINARIES {
            assert_eq!(
                std::fs::read(live.path().join(name)).unwrap(),
                b"old",
                "{name} was touched despite the refusal"
            );
        }
    }

    #[test]
    fn rollback_puts_the_previous_binaries_back() {
        let fresh = tempfile::tempdir().unwrap();
        let live = tempfile::tempdir().unwrap();
        for name in BINARIES {
            std::fs::write(fresh.path().join(name), b"new").unwrap();
            std::fs::write(live.path().join(name), b"old").unwrap();
        }

        install_from(fresh.path(), live.path()).unwrap();
        rollback(live.path()).unwrap();

        for name in BINARIES {
            assert_eq!(std::fs::read(live.path().join(name)).unwrap(), b"old");
        }
    }

    #[test]
    fn rollback_with_nothing_kept_says_so() {
        let live = tempfile::tempdir().unwrap();
        for name in BINARIES {
            std::fs::write(live.path().join(name), b"only").unwrap();
        }

        let err = rollback(live.path()).unwrap_err();
        assert!(
            err.to_string().contains("no earlier version"),
            "unhelpful error: {err}"
        );
    }
}
