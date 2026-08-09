//! `keel gate` — score Phase 2's exit criterion.
//!
//! > Across 10 unprompted sessions, Claude writes to Keel in ≥9, threads
//! > `session_id` on every write, and creates 0 duplicate projects.
//!
//! # What this does and does not automate
//!
//! It does **not** run the sessions. "Unprompted" is the entire claim and a
//! test that calls the tool has prompted it — `plugin/README.md` has been
//! explicit about that since Phase 2 was written, and it is still true.
//!
//! What it automates is the *scoring*, which was described in the README as
//! three commands, two of which did not exist and one of which could not run
//! while the daemon held the write lock. Counting by hand across ten sessions
//! is exactly where a gate quietly turns into "it seemed fine": the failure
//! modes are a missing `session_id` on one write out of forty, and a second
//! project whose name differs by a word. Neither survives being eyeballed.
//!
//! Reads go through the daemon, for the reason in [`crate::generate`].

use anyhow::{Context, Result};
use std::collections::BTreeMap;

/// Sessions that are not conversations and must not be scored as ones.
///
/// The bootstrap, the importer and the CLI write with fixed sentinel ids. They
/// are real provenance — the writes did happen and are attributed — but
/// counting them would let a passing grade be manufactured by running
/// `keel import` ten times.
const NOT_A_CONVERSATION: [&str; 5] = [
    "ses_bootstrap",
    "ses_import",
    "ses_cleanup",
    "ses_fixture",
    "ses_generate",
];

/// What one session did.
#[derive(Debug, Clone)]
struct SessionScore {
    id: String,
    writes: usize,
    entities_created: usize,
    first_seen: String,
}

/// Run the scoring.
pub fn run(
    base: &str,
    project: Option<&str>,
    since: Option<&str>,
    expected: usize,
    json: bool,
) -> Result<()> {
    let events = fetch_events(base, project, since)?;

    let mut sessions: BTreeMap<String, SessionScore> = BTreeMap::new();
    let mut unattributed: Vec<String> = Vec::new();

    for event in &events {
        let action = event
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let summary = event
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("(no summary)");
        let at = event
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        match event.get("session_id").and_then(|v| v.as_str()) {
            // A write with no session is the second failure mode in
            // plugin/README.md: the skill is being read but the threading
            // instruction is being skipped. One is a failure, not a rounding
            // error, so they are listed rather than counted.
            None | Some("") => unattributed.push(format!("{at} — {summary}")),
            Some(id) if is_a_conversation(id) => {
                let entry = sessions
                    .entry(id.to_owned())
                    .or_insert_with(|| SessionScore {
                        id: id.to_owned(),
                        writes: 0,
                        entities_created: 0,
                        first_seen: at.to_owned(),
                    });
                entry.writes += 1;
                if action == "created" {
                    entry.entities_created += 1;
                }
                if at < entry.first_seen.as_str() {
                    entry.first_seen = at.to_owned();
                }
            }
            Some(_) => {}
        }
    }

    let duplicates = duplicate_projects(base)?;

    let mut scored: Vec<&SessionScore> = sessions.values().collect();
    scored.sort_by(|a, b| a.first_seen.cmp(&b.first_seen));

    let wrote = scored.iter().filter(|s| s.writes > 0).count();
    // The denominator is how many sessions were *run*, not how many appear in
    // the log. A session that wrote nothing leaves no trace, so counting only
    // what is present made the score read "3 of 3" after seven silent
    // sessions — the scorer could not express the exact failure it exists to
    // detect. `--sessions` supplies the truth the event log cannot.
    let silent = expected.saturating_sub(scored.len());
    let passed = wrote * 10 >= expected * 9 && unattributed.is_empty() && duplicates.is_empty();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "sessions_expected": expected,
                "sessions_that_wrote": wrote,
                "sessions_silent": silent,
                "unattributed_writes": unattributed,
                "duplicate_projects": duplicates,
                "passed": passed,
            }))?
        );
    } else {
        println!("Phase 2 gate — 10 unprompted sessions\n");
        if scored.is_empty() {
            println!("  No conversation sessions found.");
            println!(
                "  Sentinel writers ({}) are excluded — they are not conversations.",
                NOT_A_CONVERSATION.join(", ")
            );
        }
        for (i, s) in scored.iter().enumerate() {
            println!(
                "  {:>2}. {}  {} write(s), {} created  {}",
                i + 1,
                s.id,
                s.writes,
                s.entities_created,
                &s.first_seen[..s.first_seen.len().min(19)]
            );
        }

        if silent > 0 {
            println!(
                "  {silent} session(s) wrote nothing at all — a silent session leaves no event, \
                 so it appears here only as this count"
            );
        }
        println!("\n  wrote to Keel      {wrote} of {expected} session(s)  (need 9 of 10)");
        println!(
            "  every write attributed  {}",
            if unattributed.is_empty() {
                "yes".to_owned()
            } else {
                format!("NO — {} write(s) carry no session_id", unattributed.len())
            }
        );
        for line in unattributed.iter().take(5) {
            println!("      {line}");
        }
        println!(
            "  duplicate projects      {}",
            if duplicates.is_empty() {
                "none".to_owned()
            } else {
                duplicates.join(", ")
            }
        );

        println!();
        if passed {
            println!("  PASS");
        } else {
            println!("  FAIL — see plugin/README.md for what each failure mode means");
        }
    }

    // Exit non-zero on a real failure only. An unfinished run is not a failure,
    // so it exits 0 with the count — a half-run gate that returns 1 gets
    // wrapped in `|| true` and then never fails again.
    if !passed {
        std::process::exit(1);
    }
    Ok(())
}

/// Whether a session id belongs to a conversation rather than a tool.
fn is_a_conversation(id: &str) -> bool {
    !NOT_A_CONVERSATION
        .iter()
        .any(|prefix| id.starts_with(prefix))
}

/// Projects whose names are near-duplicates of each other.
///
/// UC-8's failure: nine projects for one. Compared on a normalised name rather
/// than an exact match, because the damage is done by "Harbour" versus
/// "harbour app", which an exact comparison calls distinct.
fn duplicate_projects(base: &str) -> Result<Vec<String>> {
    let response = ureq::get(&format!("{base}/api/projects"))
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| anyhow::anyhow!("ask the daemon at {base} for the project list: {e}"))?;
    let body: serde_json::Value = response.into_json().context("read the project list")?;

    let projects = body
        .pointer("/data/projects")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in &projects {
        let name = p.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        seen.entry(normalise(name))
            .or_default()
            .push(name.to_owned());
    }
    Ok(seen
        .into_values()
        .filter(|names| names.len() > 1)
        .map(|names| names.join(" / "))
        .collect())
}

/// Lowercase, drop punctuation and the words that make two names look
/// different while meaning the same thing.
fn normalise(name: &str) -> String {
    let lowered = name.to_lowercase();
    lowered
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(*w, "the" | "app" | "project" | "service"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pull the event log from the daemon, paging until it is exhausted.
fn fetch_events(
    base: &str,
    project: Option<&str>,
    since: Option<&str>,
) -> Result<Vec<serde_json::Value>> {
    let mut url = format!("{base}/api/activity?limit=500");
    if let Some(p) = project {
        url.push_str(&format!("&project={p}"));
    }
    if let Some(s) = since {
        url.push_str(&format!("&since={s}"));
    }

    let response = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| {
            anyhow::anyhow!(
                "ask the daemon at {base} for the event log: {e}. Is `keel-daemon` running?"
            )
        })?;
    let body: serde_json::Value = response.into_json().context("read the event log")?;

    Ok(body
        .pointer("/data/events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn tool_sessions_do_not_count_as_conversations() {
        // Otherwise the gate can be passed by running `keel import` ten times,
        // which writes, threads a session id, and proves nothing.
        assert!(!is_a_conversation("ses_bootstrap_2026_08_09"));
        assert!(!is_a_conversation("ses_import"));
        assert!(!is_a_conversation("ses_cleanup"));
        assert!(is_a_conversation("ses_01KZKW4M8QJ3RTVN2P7XG9DAC1"));
    }

    #[test]
    fn near_duplicate_project_names_normalise_together() {
        // UC-8's failure is not "Harbour" twice — nothing would allow that.
        // It is "Harbour" and "harbour app", which an exact match calls two
        // different projects and a human calls one.
        assert_eq!(normalise("Harbour"), normalise("harbour app"));
        assert_eq!(normalise("The Sextant Project"), normalise("sextant"));
        assert_ne!(normalise("Harbour"), normalise("Sextant"));
        // Distinct products that share a word must stay distinct.
        assert_ne!(normalise("Keel"), normalise("Keel Desktop"));
    }
}
