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
//! The SHA-256 in the release manifest, and — from the first release built
//! after 2026-08-15 — nothing more yet. The manifest is the trust root, so a
//! missing one is a hard failure rather than a reason to skip the check.
//!
//! That guarantee is real but narrower than provenance: it catches a corrupt,
//! truncated or substituted artifact, and it does not independently establish
//! that GitHub built those bytes from this commit.
//!
//! Provenance is now *available* — the repository went public on 2026-08-15, so
//! `release.yml`'s attestation step stops being skipped and releases cut from
//! here carry one. Checking it is not built yet and is the open half of B-73;
//! until it is, a release carrying an attestation and one not carrying it are
//! treated identically, which is the weakness worth naming rather than leaving
//! for somebody to infer from the absence of code.
//!
//! # How it fetches
//!
//! A plain unauthenticated GET of `releases/latest/download/<name>`. That is
//! what going public bought: no token, no `gh`, no asset-id lookup, and an
//! install path that works for somebody who is not the author. While the
//! repository was private that URL returned 404 with a valid token as readily
//! as without one (KEEL-221), and the only route was `api.github.com` with an
//! asset id — which is what B-73 first chose and what it no longer has to.

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
    /// Reachable in ordinary use: `releases/latest` resolves to the latest
    /// non-prerelease, so anybody running an `-rc` is ahead of it.
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

/// Fetch one asset from the latest release.
///
/// A plain unauthenticated GET, which is what the repository going public buys:
/// `releases/latest/download/<name>` is served to anybody, so the updater needs
/// no token, no `gh`, and no asset-id lookup. While the repository was private
/// this same URL returned 404 with a valid token as readily as without one
/// (KEEL-221), and the only route was the API — which is why B-73 originally
/// shelled out and why that is no longer the trade.
///
/// `latest` deliberately excludes prereleases, which is what makes
/// [`Plan::Ahead`] reachable rather than theoretical.
fn download(dir: &Path, name: &str) -> Result<PathBuf> {
    let repo = repo();
    let url = format!("https://github.com/{repo}/releases/latest/download/{name}");

    let response = match ureq::get(&url).call() {
        Ok(response) => response,
        Err(ureq::Error::Status(404, _)) => bail!(
            "the latest release of {repo} has no asset called {name}.\n\nA release published \
             before the job learned to attach it looks exactly like this. Check what it \
             carries:\n    https://github.com/{repo}/releases/latest"
        ),
        Err(ureq::Error::Status(code, _)) => bail!(
            "fetching {url} returned HTTP {code}, so there is nothing to install. Nothing has \
             been changed."
        ),
        Err(e) => bail!("could not reach {url}: {e}\n\nNothing has been changed."),
    };

    let path = dir.join(name);
    let mut file = std::fs::File::create(&path)
        .with_context(|| format!("creating {} for the download", path.display()))?;
    std::io::copy(&mut response.into_reader(), &mut file)
        .with_context(|| format!("writing the download to {}", path.display()))?;
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
pub fn install_dir() -> Result<PathBuf> {
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

/// Put the new binaries beside the current ones without taking effect.
///
/// The daemon's route, and the reason it differs from [`install_from`]: a
/// daemon replacing its own executable and then carrying on is running code
/// from a file that no longer exists at that path, so anything it re-reads at
/// runtime — and any crash handler that re-execs — sees a version it never
/// started. Staging keeps the swap to one moment, at a startup, where there is
/// no half-updated process to reason about.
///
/// Written to a scratch name and renamed into place, so a `.staged` file is
/// always complete. A partial one is what a `SIGKILL` mid-copy would otherwise
/// leave, and [`apply_staged`] cannot tell a truncated binary from a whole one.
pub fn stage(unpacked: &Path, into: &Path, version: &str) -> Result<()> {
    for name in BINARIES {
        if !unpacked.join(name).is_file() {
            bail!(
                "the release archive does not contain {name}. Refusing to stage a partial \
                 release — {} and {} have to move together.",
                BINARIES[0],
                BINARIES[1]
            );
        }
    }

    for name in BINARIES {
        let staged = into.join(format!("{name}.staged"));
        let partial = into.join(format!("{name}.staged.partial"));
        std::fs::copy(unpacked.join(name), &partial)
            .with_context(|| format!("staging {name} to {}", partial.display()))?;
        make_executable(&partial)?;
        std::fs::rename(&partial, &staged)
            .with_context(|| format!("putting {} in place", staged.display()))?;
    }

    // Last, and only once both binaries are whole. `apply_staged` keys off this
    // file, so writing it earlier would advertise an update that is still being
    // copied.
    std::fs::write(into.join(STAGED_VERSION), version)
        .with_context(|| format!("recording the staged version in {}", into.display()))?;
    Ok(())
}

/// The file naming what has been staged. Its presence is the signal.
const STAGED_VERSION: &str = ".keel-staged-version";

/// What is staged and waiting, if anything.
///
/// Reading without applying, which is the whole of the difference B-75 and
/// KEEL-225 introduced: the daemon used to call [`apply_staged`] at startup and
/// swap the binary under whoever was using it. Now it reports, and applying is
/// something a person agrees to — because agreeing means the daemon restarts.
pub fn staged_version(dir: &Path) -> Result<Option<String>> {
    let marker = dir.join(STAGED_VERSION);
    if !marker.is_file() {
        return Ok(None);
    }
    let version = std::fs::read_to_string(&marker)
        .with_context(|| format!("reading {}", marker.display()))?
        .trim()
        .to_owned();
    Ok(Some(version))
}

/// Swap in a staged release, if there is one. Returns the version applied.
///
/// Called at startup, before anything is served. Renaming over a running
/// executable is safe on Unix — the process holds its own inode — but this runs
/// early precisely so that nothing has happened yet that a version change could
/// be inconsistent with.
///
/// Leaves the previous binaries as `<name>.previous`, same as [`install_from`],
/// so `keel update --rollback` undoes an unattended update exactly as it undoes
/// a deliberate one.
pub fn apply_staged(dir: &Path) -> Result<Option<String>> {
    let marker = dir.join(STAGED_VERSION);
    if !marker.is_file() {
        return Ok(None);
    }

    // A marker without both binaries beside it means a staging run died between
    // the two. Clear it rather than half-applying: the next check will stage
    // again, and the alternative is a daemon that fails to start for good.
    for name in BINARIES {
        if !dir.join(format!("{name}.staged")).is_file() {
            let _ = std::fs::remove_file(&marker);
            bail!(
                "a staged update was recorded but {name}.staged is missing, so it was discarded \
                 rather than applied in half. The next check will stage it again."
            );
        }
    }

    let version = std::fs::read_to_string(&marker)
        .with_context(|| format!("reading {}", marker.display()))?
        .trim()
        .to_owned();

    for name in BINARIES {
        let live = dir.join(name);
        let staged = dir.join(format!("{name}.staged"));
        let kept = dir.join(format!("{name}.previous"));

        if live.exists() {
            std::fs::rename(&live, &kept)
                .with_context(|| format!("keeping the current {name} as {}", kept.display()))?;
        }
        std::fs::rename(&staged, &live)
            .with_context(|| format!("applying the staged {name} to {}", live.display()))?;
        make_executable(&live)?;
    }

    std::fs::remove_file(&marker)
        .with_context(|| format!("clearing {} after applying it", marker.display()))?;
    Ok(Some(version))
}

/// Whether the unattended check may run at all.
///
/// `KEEL_AUTO_UPDATE=0` turns it off. This is the smaller half of KEEL-204,
/// landed here rather than after it because the alternative is shipping a
/// daily outbound request from a local-first tool with no way to stop it —
/// which is the thing KEEL-204 exists to avoid, not a detail of how it is
/// announced. The prompt at setup time and `keel doctor` reporting it are
/// still that task's.
///
/// Anything other than `0` is on, including nonsense, because a typo in this
/// variable should not silently disable an update path.
pub fn auto_update_enabled() -> bool {
    enabled_from(std::env::var("KEEL_AUTO_UPDATE").ok().as_deref())
}

/// The rule behind [`auto_update_enabled`], separated so it can be tested.
///
/// Setting an environment variable in a test is `unsafe` under edition 2024 and
/// the workspace denies `unsafe_code`, so the alternative to this split is not
/// testing the rule at all.
fn enabled_from(value: Option<&str>) -> bool {
    value.map(|v| v.trim() != "0").unwrap_or(true)
}

/// Look for a newer release and stage it if it is safe to apply.
///
/// The daemon's whole job here. Returns what it decided, so the caller can log
/// one line rather than this crate deciding how a daemon talks.
pub fn check_and_stage(install_dir: &Path, target: &str) -> Result<Plan> {
    let work = tempfile::tempdir().context("making a scratch directory for the download")?;
    let manifest = fetch_manifest(work.path())?;
    let decision = plan(
        env!("CARGO_PKG_VERSION"),
        keel_core::shipped_schema_version(),
        &manifest,
        target,
    )?;

    if let Plan::Apply { version, artifact } = &decision {
        let archive = download(work.path(), artifact)?;
        verify(&archive, artifact, &manifest)?;
        let unpacked = unpack(&archive, work.path(), target)?;
        stage(&unpacked, install_dir, version)?;
    }
    Ok(decision)
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
pub fn target() -> Result<&'static str> {
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

    /// `releases/latest` resolves to the latest *non*-prerelease, so anybody on
    /// an rc is ahead of it. Applying that is a downgrade wearing a successful
    /// update's clothes.
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
    fn staging_then_applying_swaps_and_keeps_the_previous() {
        let fresh = tempfile::tempdir().unwrap();
        let live = tempfile::tempdir().unwrap();
        for name in BINARIES {
            std::fs::write(fresh.path().join(name), b"new").unwrap();
            std::fs::write(live.path().join(name), b"old").unwrap();
        }

        stage(fresh.path(), live.path(), "0.1.2").unwrap();

        // Staging alone changes nothing that is running.
        for name in BINARIES {
            assert_eq!(std::fs::read(live.path().join(name)).unwrap(), b"old");
        }

        assert_eq!(apply_staged(live.path()).unwrap().as_deref(), Some("0.1.2"));
        for name in BINARIES {
            assert_eq!(std::fs::read(live.path().join(name)).unwrap(), b"new");
            assert_eq!(
                std::fs::read(live.path().join(format!("{name}.previous"))).unwrap(),
                b"old"
            );
        }
    }

    /// The daemon calls this on every start, so the ordinary answer is "nothing
    /// staged" and it has to be cheap and silent.
    #[test]
    fn applying_with_nothing_staged_is_a_no_op() {
        let live = tempfile::tempdir().unwrap();
        assert_eq!(apply_staged(live.path()).unwrap(), None);
    }

    #[test]
    fn applying_twice_does_not_apply_the_second_time() {
        let fresh = tempfile::tempdir().unwrap();
        let live = tempfile::tempdir().unwrap();
        for name in BINARIES {
            std::fs::write(fresh.path().join(name), b"new").unwrap();
            std::fs::write(live.path().join(name), b"old").unwrap();
        }

        stage(fresh.path(), live.path(), "0.1.2").unwrap();
        apply_staged(live.path()).unwrap();
        // Second start: the marker is gone, so `new` must not become `previous`.
        assert_eq!(apply_staged(live.path()).unwrap(), None);
        for name in BINARIES {
            assert_eq!(
                std::fs::read(live.path().join(format!("{name}.previous"))).unwrap(),
                b"old"
            );
        }
    }

    /// A staging run killed between the two binaries. Applying half of it would
    /// leave `keel` and `keel-daemon` at different versions, which is the exact
    /// drift this whole task exists to prevent.
    #[test]
    fn a_marker_without_its_binaries_is_discarded_not_half_applied() {
        let live = tempfile::tempdir().unwrap();
        for name in BINARIES {
            std::fs::write(live.path().join(name), b"old").unwrap();
        }
        std::fs::write(live.path().join("keel.staged"), b"new").unwrap();
        std::fs::write(live.path().join(STAGED_VERSION), "0.1.2").unwrap();

        let err = apply_staged(live.path()).unwrap_err();
        assert!(
            err.to_string().contains("keel-daemon.staged is missing"),
            "unhelpful error: {err}"
        );
        for name in BINARIES {
            assert_eq!(std::fs::read(live.path().join(name)).unwrap(), b"old");
        }
        // Cleared, so the next start is not stuck on the same failure for good.
        assert_eq!(apply_staged(live.path()).unwrap(), None);
    }

    #[test]
    fn a_partial_archive_stages_nothing_applicable() {
        let fresh = tempfile::tempdir().unwrap();
        let live = tempfile::tempdir().unwrap();
        std::fs::write(fresh.path().join("keel"), b"new").unwrap();

        assert!(stage(fresh.path(), live.path(), "0.1.2").is_err());
        assert_eq!(apply_staged(live.path()).unwrap(), None);
    }

    #[test]
    fn only_zero_turns_the_check_off() {
        assert!(!enabled_from(Some("0")));
        assert!(!enabled_from(Some(" 0 ")));
        assert!(enabled_from(None));
        assert!(enabled_from(Some("1")));
        // A typo should not silently disable an update path.
        assert!(enabled_from(Some("false")));
        assert!(enabled_from(Some("")));
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
