//! Every surface describes itself into `contracts/`, and CI fails when it drifts.
//!
//! The problem this exists for: a change to a table, a tool schema or a command
//! is caught today by a human reading a diff. That works while the only store in
//! the world is the one on the author's laptop. It stops working the moment
//! somebody else has one, because then a schema is a promise and a migration is
//! their data.
//!
//! So each surface emits a description of itself, the descriptions are checked
//! in, and this test fails when what the code produces differs from what is on
//! disk. `git diff <last-tag>..HEAD -- contracts/` is then the release diff, and
//! the classifier reads it rather than a human.
//!
//! # Why there are no per-release copies
//!
//! An earlier draft of the plan stored a directory per release. At a release
//! every few days that is thirty directories in six months, storing something
//! version control already stores. One always-current directory, and git keeps
//! the history.
//!
//! # Why this is a test rather than a subcommand
//!
//! `UPDATE_CONTRACTS=1 cargo test` regenerates; anything else asserts. That is
//! the shape `insta` already gives this repository seventeen snapshots of, so it
//! is one idea rather than two, and it keeps the emitter out of the shipped CLI
//! surface — which is itself one of the things being described here, and would
//! otherwise have to describe the tool that describes it.
//!
//! # What had to be normalised, and how that was decided
//!
//! Measured rather than assumed (KEEL-193). Nine of ten surfaces already emit
//! byte-identically across a hundred runs. The tenth, `keel generate`, differs on
//! *every* run for exactly one reason: each generated file opens with a banner
//! carrying the generation time, and `manifest.json` carries `generated_at`. Two
//! runs in the same second match; two a second apart differ in all 67 files.
//!
//! Stripping the banner is not a normalisation invented here. `keel generate
//! --check` already does it — a banner four days stale still counts among its
//! "83 current" — so this inherits a rule that exists rather than adding one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::{EntityQuery, EntityStore, EntityType, Store, fixture, generate};
use std::path::{Path, PathBuf};

/// Where the checked-in descriptions live.
fn contracts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("contracts")
}

/// Whether this run rewrites the descriptions or checks them.
fn updating() -> bool {
    std::env::var("UPDATE_CONTRACTS").is_ok_and(|v| v == "1")
}

/// Compare one emitted description against the checked-in copy, or rewrite it.
///
/// Collects failures rather than panicking on the first, so a run that changed
/// four surfaces says so once instead of four times.
fn settle(relative: &str, produced: &str, failures: &mut Vec<String>) {
    // The one path this emitter must never write.
    //
    // `UPDATE_CONTRACTS=1` is what somebody types to make a failing test go
    // away, and the text descriptions are safe to rewrite because git shows
    // what changed. A vintage store is not: it can only be written by the
    // release that wrote it, so once overwritten there is no way to make
    // another. The guard lives here, at the only place that opens a file for
    // writing, rather than as a rule somebody has to remember.
    assert!(
        !relative.starts_with("stores"),
        "the contracts emitter must never write a vintage store: {relative}"
    );

    let path = contracts_dir().join(relative);
    if updating() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, produced).unwrap();
        return;
    }

    match std::fs::read_to_string(&path) {
        Ok(stored) if stored == produced => {}
        Ok(_) => failures.push(format!(
            "{relative} differs from what the code produces now"
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            failures.push(format!("{relative} has never been recorded"))
        }
        Err(e) => failures.push(format!("{relative} could not be read: {e}")),
    }
}

/// The store's shape, through `PRAGMA` rather than a `sqlite_master` dump.
///
/// SQLite stores the original `CREATE TABLE` text verbatim, so reformatting a
/// statement or editing a comment inside one would read as a schema change and
/// a genuine column rename might not stand out beside it. `table_info`,
/// `index_list` and `foreign_key_list` return the shape itself, already
/// structured, and sorting makes the output independent of the order SQLite
/// happens to return rows in.
fn schema_description(path: &Path) -> String {
    let conn = rusqlite::Connection::open(path).unwrap();

    let mut tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables.sort();

    let mut out = serde_json::Map::new();
    for table in tables {
        let columns = rows_of(&conn, &format!("PRAGMA table_info(\"{table}\")"));
        let indexes = rows_of(&conn, &format!("PRAGMA index_list(\"{table}\")"));
        let foreign_keys = rows_of(&conn, &format!("PRAGMA foreign_key_list(\"{table}\")"));
        out.insert(
            table,
            serde_json::json!({
                "columns": columns,
                "indexes": indexes,
                "foreign_keys": foreign_keys,
            }),
        );
    }

    serde_json::to_string_pretty(&serde_json::Value::Object(out)).unwrap() + "\n"
}

/// Run a `PRAGMA` and return its rows as JSON arrays, sorted.
///
/// Sorted because these pragmas do not promise an order, and an emitter whose
/// output depends on one would flap — which is the failure that makes a gate
/// get switched off.
fn rows_of(conn: &rusqlite::Connection, sql: &str) -> Vec<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(sql).unwrap();
    let count = stmt.column_count();
    let mut rows: Vec<Vec<serde_json::Value>> = stmt
        .query_map([], |row| {
            Ok((0..count)
                .map(|i| match row.get_ref(i) {
                    Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                    Ok(rusqlite::types::ValueRef::Integer(n)) => serde_json::json!(n),
                    Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::json!(f),
                    Ok(rusqlite::types::ValueRef::Text(t)) => {
                        serde_json::json!(String::from_utf8_lossy(t))
                    }
                    Ok(rusqlite::types::ValueRef::Blob(_)) | Err(_) => serde_json::Value::Null,
                })
                .collect())
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows.sort_by_key(|r| serde_json::to_string(r).unwrap_or_default());
    rows
}

/// The MCP tool surface, in full.
///
/// The descriptions are included deliberately. They break no caller, so a
/// change to one is additive — and they are also the only documentation a model
/// gets, which makes a silent rewrite of the thing that decides tool selection
/// worth a human reading. Recording them is what makes that possible.
fn tools_description() -> String {
    let tools: Vec<serde_json::Value> = keel_mcp::all_tools()
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "protocol": keel_mcp::PROTOCOL_VERSION,
        "count": tools.len(),
        "tools": tools,
    }))
    .unwrap()
        + "\n"
}

/// Every subcommand's help, which is the CLI's contract with anyone scripting it.
///
/// Taken by running the real binary rather than by rendering the parser in
/// process. `keel` is a binary crate with no library half, so its `Cli` type
/// is not importable — but the better reason is that this way the description
/// comes from the thing that actually ships, including anything a build script
/// or a feature flag changed on the way.
fn cli_description() -> String {
    let exe = env!("CARGO_BIN_EXE_keel");
    let mut out = help_for(exe, &[]);

    // The subcommand list, read out of the top-level help rather than kept as a
    // second copy here. A list maintained by hand would drift, and the drift
    // would be invisible: a subcommand nobody added to it would simply go
    // undescribed, which is the failure this whole file exists to stop.
    let mut names: Vec<String> = out
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  ") || l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .filter(|n| *n != "help")
        .map(std::borrow::ToOwned::to_owned)
        .collect();
    names.sort();
    names.dedup();
    assert!(
        !names.is_empty(),
        "no subcommands were found in the top-level help; the parse below is wrong"
    );

    for name in names {
        out.push_str("\n\n===== ");
        out.push_str(&name);
        out.push_str(" =====\n\n");
        out.push_str(&help_for(exe, &[&name]));
    }
    out
}

/// One `--help` invocation, normalised to end with a single newline.
fn help_for(exe: &str, args: &[&str]) -> String {
    let output = std::process::Command::new(exe)
        .args(args)
        .arg("--help")
        // Cleared, because `clap` prints an env-backed argument as
        // `[env: KEEL_HOME=<the value right now>]` — the *current* value, not
        // just the name. Inheriting the environment therefore records the
        // machine into the contract, and the contract is meant to be the CLI's
        // shape.
        //
        // It is not hypothetical and it is not only about CI. Anyone with
        // `KEEL_HOME`, `KEEL_BIND` or `KEEL_DAEMON_URL` exported — which is a
        // reasonable thing for someone running two stores to have — would have
        // failed this test with a diff that looked like a CLI change. It was
        // found when CI set `KEEL_HOME` to a scratch directory to keep the test
        // suite away from the real store, and every leg went red on `cli.txt`.
        //
        // `PATH` is put back because the process still has to be found and
        // linked; nothing else is needed to print help.
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .output()
        .expect("run the built keel binary");
    assert!(
        output.status.success(),
        "`keel {} --help` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    format!("{}\n", text.trim_end())
}

#[test]
fn every_surface_matches_its_recorded_description() {
    let mut failures = Vec::new();

    // One fixture store, built fresh, so nothing here depends on the machine it
    // runs on or on whatever the author's own store happens to contain.
    let dir = tempfile::tempdir().unwrap();
    let store_path = dir.path().join("keel.sqlite");
    let mut store = Store::open(&store_path).unwrap();
    fixture::load(&mut store).unwrap();

    settle(
        "schema.json",
        &schema_description(&store_path),
        &mut failures,
    );
    settle(
        "schema_version",
        &format!("{}\n", keel_core::shipped_schema_version()),
        &mut failures,
    );
    settle("tools.json", &tools_description(), &mut failures);
    settle("cli.txt", &cli_description(), &mut failures);

    // The generated markdown, with the two wall-clock timestamps taken out —
    // see the module comment for the measurement that made this necessary.
    // Every project, not the first one the store happens to return. Taking
    // `.next()` off an unsorted list made this depend on an ordering nothing
    // promises, and it described one small project while claiming to describe
    // the generator. Sorted by id so the output is the same on any machine.
    let mut projects = store
        .list(&EntityQuery::default().of_type(EntityType::Project))
        .unwrap()
        .items;
    assert!(!projects.is_empty(), "the fixture should have projects");
    projects.sort_by_key(|p| p.id().to_string());

    let mut generated = String::new();
    for project in &projects {
        let repo = dir.path().join("repo").join(project.id().to_string());
        std::fs::create_dir_all(&repo).unwrap();
        generate::all(&store, project.id(), &repo, generate::Mode::Write).unwrap();
        generated.push_str(&generated_description(&repo));
    }
    settle("generated.txt", &generated, &mut failures);

    drop(store);

    assert!(
        failures.is_empty(),
        "the contract descriptions are out of date:\n  {}\n\n\
         If the change was intended, re-record them with:\n\
         \x20   UPDATE_CONTRACTS=1 cargo test -p keel --test contracts\n\n\
         Then read the diff before committing it. That diff is the release \
         diff, and a breaking difference in it needs an entry saying so.",
        failures.join("\n  ")
    );
}

/// The generated tree as one sorted listing of path and content hash.
///
/// A hash per file rather than the files themselves: the point is to notice that
/// the layout changed, and 67 files of prose in the contracts directory would
/// bury that signal in its own volume.
///
/// `manifest.json` is excluded and the `keel:generated` banner line is dropped
/// from every file, because both carry a wall-clock time. Without this the
/// listing changes every second and the gate is worse than useless — it fails
/// constantly, gets switched off, and leaves the project believing it is guarded.
fn generated_description(root: &Path) -> String {
    let mut lines = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name().is_some_and(|n| n == "manifest.json") {
                continue;
            }
            let body = std::fs::read_to_string(&path).unwrap_or_default();
            let stripped: String = body
                .lines()
                .filter(|l| !l.contains("keel:generated"))
                .map(redact_ids)
                .map(|l| redact_dates(&l))
                .collect::<Vec<_>>()
                .join("\n");
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            lines.push(format!("{relative}  {}", short_hash(&stripped)));
        }
    }
    lines.sort();
    lines.join("\n") + "\n"
}

/// Replace every calendar date with a placeholder.
///
/// The same argument as [`redact_ids`], and it was found the same way — by the
/// check failing for a reason that had nothing to do with the code.
///
/// `fixture::load` dates its corpus relative to `Utc::now()`:
/// `d.decided_at = Some(now - Duration::days(30))`, and a dozen others like it.
/// So a decision rendered today says `**Decided:** 2026-07-16` and the same
/// decision rendered tomorrow says `2026-07-17`. Every file carrying a date
/// changes hash once a day, for ever, with nothing in the tree having moved.
///
/// **This made the gate fail every day from the day it landed** (KEEL-197, and
/// it was recorded on 2026-08-14; the first run on 2026-08-15 failed). That is
/// the worse half. The documented remedy is `UPDATE_CONTRACTS=1` followed by
/// "read the diff before committing it — that diff is the release diff", and a
/// diff that is *always* noise teaches whoever meets it to re-record without
/// reading. A real breaking change would then land inside a diff nobody looks
/// at any more, which is precisely the failure this file exists to prevent.
///
/// The dates are data; the contract is the layout — so the value is hidden and
/// the surrounding text is not. A change to how a date is *rendered* still
/// shows, because `**Decided:** ` shows.
///
/// Deliberately narrow: `YYYY-MM-DD` with four-digit years only. It leaves
/// version numbers, durations, and anything else that merely contains digits
/// alone.
fn redact_dates(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        let looks_like_date = i + 10 <= chars.len()
            && chars[i..i + 4].iter().all(char::is_ascii_digit)
            && chars[i + 4] == '-'
            && chars[i + 5..i + 7].iter().all(char::is_ascii_digit)
            && chars[i + 7] == '-'
            && chars[i + 8..i + 10].iter().all(char::is_ascii_digit);
        // Not the tail of something longer — a hash, or an id that survived
        // redaction — and not the head of a longer number either.
        let starts_a_word = i == 0 || !chars[i - 1].is_ascii_alphanumeric();
        // `get`, not indexing: this is evaluated whether or not `looks_like_date`
        // held, so `i + 10` is routinely past the end.
        let ends_cleanly = chars.get(i + 10).is_none_or(|c| !c.is_ascii_digit());

        if looks_like_date && starts_a_word && ends_cleanly {
            out.push_str("<date>");
            i += 10;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Replace every Keel id with a placeholder.
///
/// A ULID carries a timestamp and randomness, and `fixture::load` mints new ones
/// on every run — so the generated markdown, which prints ids, differs on every
/// run even though nothing about the generator changed. The check caught this on
/// its first day, which is the check working: the earlier determinism
/// measurement ran `keel generate` against a store built once, so it never saw
/// it.
///
/// Redacting rather than pinning is the right answer rather than the convenient
/// one. **The ids are data; the contract is the layout.** What this file is
/// asserting is "a generated decision looks like this" — which heading, which
/// fields, in which order — not which decisions the fixture happens to contain.
/// A change to how an id is *rendered* still shows up, because the surrounding
/// text does; only the value is hidden.
///
/// Anchored to Keel's own shape — three lowercase letters, an underscore, then
/// 26 Crockford base32 characters — so ordinary prose is left alone.
fn redact_ids(line: &str) -> String {
    let bytes: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < bytes.len() {
        let looks_like_id = i + 30 <= bytes.len()
            && bytes[i..i + 3].iter().all(char::is_ascii_lowercase)
            && bytes[i + 3] == '_'
            && bytes[i + 4..i + 30]
                .iter()
                .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase());
        // A preceding word character means this is the tail of something
        // longer, not an id — a filename or a hash that happens to line up.
        let starts_a_word = i == 0 || !(bytes[i - 1].is_alphanumeric() || bytes[i - 1] == '_');

        if looks_like_id && starts_a_word {
            out.push_str(&bytes[i..i + 3].iter().collect::<String>());
            out.push_str("_<id>");
            i += 30;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// A short content hash. Short because these are read by people in a diff, and
/// the full 64 characters would wrap without telling anyone more.
fn short_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().take(8).fold(String::new(), |mut acc, b| {
        use std::fmt::Write;
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

/// The emission is stable across runs.
///
/// This is the property everything else rests on, and it was measured before any
/// of it was built (KEEL-193). Asserting it here rather than trusting that
/// measurement is the difference between "it was deterministic in August" and
/// "it is deterministic": a `HashMap` iterated somewhere new, a timestamp added
/// to a payload, an unsorted `PRAGMA` — any of them would make the gate flap,
/// and a flapping gate is switched off within a month, after which the project
/// believes it is guarded and is not.
///
/// Three runs rather than a hundred. A hundred belongs in the standalone harness
/// at `scripts/determinism-check.sh`, which exists and takes a minute; this is
/// the version cheap enough to run on every commit, and non-determinism from
/// iteration order shows up almost immediately.
#[test]
fn the_same_state_produces_the_same_description_every_time() {
    let dir = tempfile::tempdir().unwrap();
    let store_path = dir.path().join("keel.sqlite");
    let mut store = Store::open(&store_path).unwrap();
    fixture::load(&mut store).unwrap();
    drop(store);

    let schema: Vec<String> = (0..3).map(|_| schema_description(&store_path)).collect();
    assert_eq!(schema[0], schema[1], "the schema description is not stable");
    assert_eq!(schema[1], schema[2], "the schema description is not stable");

    let tools: Vec<String> = (0..3).map(|_| tools_description()).collect();
    assert_eq!(tools[0], tools[1], "the tool description is not stable");
    assert_eq!(tools[1], tools[2], "the tool description is not stable");
}

/// The date redaction, on its own.
///
/// It exists because the gate failed for a calendar reason rather than a code
/// one, so what it has to guarantee is that the same corpus rendered on two
/// different days hashes the same. These are the cases that guarantee rests on.
#[test]
fn a_date_is_hidden_but_the_line_around_it_is_not() {
    assert_eq!(
        redact_dates("**Decided:** 2026-07-16"),
        "**Decided:** <date>",
        "the value goes, the label stays — a change to how a date is rendered \
         must still show up in the diff"
    );
    assert_eq!(
        redact_dates("shipped 2026-01-02 and again 2026-12-31"),
        "shipped <date> and again <date>",
        "every date on the line, not just the first"
    );
}

/// The property the whole thing is for: two days, one hash.
#[test]
fn the_same_line_a_day_apart_redacts_identically() {
    assert_eq!(
        redact_dates("**Decided:** 2026-07-16"),
        redact_dates("**Decided:** 2026-07-17"),
        "this is the failure that made the gate cry wolf every single day"
    );
}

/// Narrow on purpose. Anything that merely contains digits is left alone,
/// because a redaction that swallows real content hides the changes the gate
/// exists to show.
#[test]
fn things_that_are_not_dates_are_left_alone() {
    for untouched in [
        "version 0.1.0",
        "90 days",
        "12345-67-89",      // five-digit year
        "2026-07-160",      // a trailing digit, so not a bare date
        "a18802b4a2da5999", // a content hash
    ] {
        assert_eq!(
            redact_dates(untouched),
            untouched,
            "{untouched} is not a date and must survive untouched"
        );
    }

    // And the other direction, so the rule above cannot be satisfied by a
    // redactor that simply never fires.
    assert!(
        redact_dates("protocol 2026-07-28 is current").contains("<date>"),
        "a real date in ordinary prose is still a date"
    );
}
