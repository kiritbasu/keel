//! Scoring a gate session from its transcript.
//!
//! # Why the transcript and not the event log
//!
//! The event log answers "what reached the store". It cannot answer "what did
//! the session do", and the difference is the whole measurement. A session that
//! drafted the right artifact and then asked permission leaves *no event*, and
//! is indistinguishable in the log from one that never noticed Specline existed.
//! Those two failures need opposite fixes.
//!
//! Worse, the log was actively misleading. Scoring counted distinct
//! `session_id` values; sessions minted date-based ids, two pairs collided, and
//! a run where five sessions wrote was reported as three — the number a week of
//! strategy was then built on. A transcript is one file per session by
//! construction. It cannot collide with anything.
//!
//! So: the transcript is the unit of observation, and the event log is used
//! only to confirm that an attempted write actually landed.
//!
//! # The rubric
//!
//! A binary "did it write" collapses three different failures into one number.
//! These levels separate them, because each has a different remedy:
//!
//! - **L0** — nothing in the session was worth recording. *Excluded from the
//!   denominator*, or the score punishes sessions for the prompt being dull.
//! - **L1** — no sign of noticing Specline. The orientation mechanism failed.
//! - **L2** — read Specline, wrote nothing, never raised it. Noticed, judged not.
//! - **L3** — drafted or offered the record, did not write it. The permission
//!   failure. **This is the one that looks like success in a transcript.**
//! - **L4** — wrote, but wrote noise.
//! - **L5** — wrote something worth keeping.
//!
//! Recall is `L5 / (L1..L5)`. Ceiling is `(L3+L4+L5) / (L1..L5)` — what recall
//! *would* be if every session that got as far as intending to write actually
//! did. The gap between them is exactly the size of the permission problem.

use serde::{Deserialize, Serialize};

/// How far a session got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    /// Nothing recordable happened. Excluded from the denominator.
    L0NothingToRecord,
    /// No sign of noticing Specline at all.
    L1NoSignOfNoticing,
    /// Read Specline, wrote nothing, never raised it.
    L2NoticedNoDraft,
    /// Drafted or offered a record and did not write it.
    L3OfferedNotWritten,
    /// Wrote, but the content is noise.
    L4WroteJunk,
    /// Wrote something worth keeping.
    L5WroteWell,
}

impl Level {
    /// Whether this level counts toward the denominator.
    ///
    /// L0 does not: a session with nothing worth recording that records nothing
    /// behaved correctly, and counting it as a miss makes the metric punish
    /// prompt selection rather than behaviour.
    pub fn counts(self) -> bool {
        self != Level::L0NothingToRecord
    }

    /// Whether the session got as far as intending to write.
    pub fn intended(self) -> bool {
        matches!(
            self,
            Level::L3OfferedNotWritten | Level::L4WroteJunk | Level::L5WroteWell
        )
    }

    /// Short label for a report.
    pub fn label(self) -> &'static str {
        match self {
            Level::L0NothingToRecord => "L0 nothing to record",
            Level::L1NoSignOfNoticing => "L1 did not notice",
            Level::L2NoticedNoDraft => "L2 noticed, no draft",
            Level::L3OfferedNotWritten => "L3 offered, did not write",
            Level::L4WroteJunk => "L4 wrote junk",
            Level::L5WroteWell => "L5 wrote",
        }
    }
}

/// What one session did, read off its transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRead {
    /// Claude Code's session UUID. One per transcript, so it cannot collide.
    pub session_id: String,
    /// Specline tools the session actually invoked.
    pub keel_tools: Vec<String>,
    /// Write attempts, whether or not they succeeded.
    pub write_attempts: usize,
    /// Write attempts the store rejected — validation, not permission.
    pub write_errors: Vec<String>,
    /// Write attempts denied at the permission layer. A confound, not a result.
    pub permission_denials: usize,
    /// Phrases offering to record rather than recording.
    pub offers: Vec<String>,
    /// Where the level came from.
    pub level: Level,
}

/// Phrases that mark an offer to record instead of a record.
///
/// Derived from the eleven found across ten real transcripts, not invented.
/// Deliberately narrow: a false positive here inflates L3 and makes the
/// permission problem look bigger than it is.
const OFFER_MARKERS: [&str; 8] = [
    "want me to log",
    "want me to record",
    "want me to add",
    "shall i log",
    "shall i record",
    "i'll hold off",
    "let me know and i'll",
    "say the word and i'll",
];

/// The verbs that write, without the tool prefix.
///
/// Matched as a suffix against a transcript's tool name, which arrives as
/// `mcp__<server>__<server>_<verb>`. Splitting the verb from the prefix is what
/// lets both spellings be recognised — see [`is_our_tool`].
const WRITE_VERBS: [&str; 4] = ["_create", "_update", "_write_doc", "_link"];

/// Both spellings of this project's tool prefix, and both are permanent.
///
/// The rubric scores *transcripts that already exist*. Every one recorded
/// before the rename to Specline names its tools `mcp__keel__keel_…`, so a
/// scorer that recognised only the current prefix would read every historical
/// session as having never noticed the store at all — L1, the worst level,
/// awarded silently and for the wrong reason. The measurement in `GATE.md` is
/// frozen and has to stay reproducible, which means the old prefix is not
/// deprecated here; it is part of the record.
const TOOL_PREFIXES: [&str; 2] = ["keel", "specline"];

/// Whether a transcript's tool name belongs to this project, under either name.
fn is_our_tool(name: &str) -> bool {
    TOOL_PREFIXES.iter().any(|p| name.contains(p))
}

/// Classify a session from what it invoked and what it said.
///
/// `landed` is whether the event log confirms a write reached the store, which
/// is the only way to tell L4/L5 from a write that was attempted and rejected.
pub fn classify(
    keel_tools: &[String],
    write_attempts: usize,
    landed: bool,
    offers: &[String],
    recordable: bool,
) -> Level {
    if !recordable {
        return Level::L0NothingToRecord;
    }
    if write_attempts > 0 && landed {
        // Junk-vs-worthwhile is a human judgement (Step 10) and is not
        // inferable from a transcript. Assume good faith and let a hand pass
        // demote it — the alternative silently understates recall.
        return Level::L5WroteWell;
    }
    if write_attempts > 0 || !offers.is_empty() {
        // Attempted and failed is the same behavioural class as offered and
        // stopped: the intent formed and no record exists.
        return Level::L3OfferedNotWritten;
    }
    if keel_tools.is_empty() {
        return Level::L1NoSignOfNoticing;
    }
    Level::L2NoticedNoDraft
}

/// Find offer phrases in a session's final prose.
pub fn find_offers(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    OFFER_MARKERS
        .iter()
        .filter(|m| lower.contains(*m))
        .map(|m| (*m).to_owned())
        .collect()
}

/// Whether a tool name writes, under either spelling of the prefix.
pub fn is_write_tool(name: &str) -> bool {
    is_our_tool(name) && WRITE_VERBS.iter().any(|v| name.ends_with(v))
}

/// The aggregate of a run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunScore {
    /// Sessions the launcher started. Supplied, never inferred.
    pub launched: usize,
    /// Transcripts actually found.
    pub observed: usize,
    /// Per level.
    pub counts: Vec<(String, usize)>,
    /// `L5 / (L1..L5)`.
    pub recall: f64,
    /// `(L3+L4+L5) / (L1..L5)`.
    pub ceiling: f64,
    /// Total offer phrases across the run.
    pub offers: usize,
    /// Write attempts the permission layer refused.
    pub permission_denials: usize,
    /// Whether every launched session was accounted for.
    pub complete: bool,
}

/// Aggregate a run.
pub fn score(launched: usize, sessions: &[SessionRead]) -> RunScore {
    let counting: Vec<&SessionRead> = sessions.iter().filter(|s| s.level.counts()).collect();
    let denom = counting.len().max(1) as f64;
    let wrote = counting
        .iter()
        .filter(|s| s.level == Level::L5WroteWell)
        .count();
    let intended = counting.iter().filter(|s| s.level.intended()).count();

    let mut counts: Vec<(String, usize)> = Vec::new();
    for level in [
        Level::L0NothingToRecord,
        Level::L1NoSignOfNoticing,
        Level::L2NoticedNoDraft,
        Level::L3OfferedNotWritten,
        Level::L4WroteJunk,
        Level::L5WroteWell,
    ] {
        let n = sessions.iter().filter(|s| s.level == level).count();
        if n > 0 {
            counts.push((level.label().to_owned(), n));
        }
    }

    RunScore {
        launched,
        observed: sessions.len(),
        counts,
        recall: wrote as f64 / denom,
        ceiling: intended as f64 / denom,
        offers: sessions.iter().map(|s| s.offers.len()).sum(),
        permission_denials: sessions.iter().map(|s| s.permission_denials).sum(),
        complete: sessions.len() == launched,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn read(level: Level, offers: usize) -> SessionRead {
        SessionRead {
            session_id: "s".into(),
            keel_tools: vec![],
            write_attempts: 0,
            write_errors: vec![],
            permission_denials: 0,
            offers: vec!["want me to log".into(); offers],
            level,
        }
    }

    #[test]
    fn a_session_that_offered_and_a_session_that_never_noticed_are_not_the_same_failure() {
        // The binary gate scored both as zero. They need opposite fixes: one is
        // an orientation problem, the other is a permission problem.
        let offered = classify(
            &["specline_context".into()],
            0,
            false,
            &["want me to log".into()],
            true,
        );
        let oblivious = classify(&[], 0, false, &[], true);
        assert_eq!(offered, Level::L3OfferedNotWritten);
        assert_eq!(oblivious, Level::L1NoSignOfNoticing);
        assert_ne!(offered, oblivious);
    }

    #[test]
    fn an_attempted_write_that_did_not_land_is_not_a_write() {
        // Two run-4 writes were rejected for `priority: "high"`. Had they not
        // been retried, the intent existed and the record did not — which is
        // behaviourally L3, not L5.
        assert_eq!(
            classify(&["specline_create".into()], 1, false, &[], true),
            Level::L3OfferedNotWritten
        );
        assert_eq!(
            classify(&["specline_create".into()], 1, true, &[], true),
            Level::L5WroteWell
        );
    }

    #[test]
    fn nothing_recordable_is_excluded_from_the_denominator() {
        let sessions = vec![
            read(Level::L0NothingToRecord, 0),
            read(Level::L5WroteWell, 0),
        ];
        let s = score(2, &sessions);
        assert_eq!(s.recall, 1.0, "one of one *recordable* session wrote");
    }

    #[test]
    fn the_ceiling_shows_how_much_of_the_gap_is_permission() {
        // Four sessions: one wrote, two offered, one never noticed. Recall is
        // 1/4, but three of four formed the intent — so three quarters of the
        // gap is reachable by fixing the ask, not the orientation.
        let sessions = vec![
            read(Level::L5WroteWell, 0),
            read(Level::L3OfferedNotWritten, 1),
            read(Level::L3OfferedNotWritten, 2),
            read(Level::L1NoSignOfNoticing, 0),
        ];
        let s = score(4, &sessions);
        assert_eq!(s.recall, 0.25);
        assert_eq!(s.ceiling, 0.75);
        assert_eq!(s.offers, 3);
    }

    #[test]
    fn a_missing_transcript_makes_the_run_incomplete() {
        // Survivorship bias: the sessions that fail hardest are exactly the
        // ones that might leave nothing behind. Silence must be an assertion
        // failure, not an absence nobody notices.
        let s = score(10, &[read(Level::L5WroteWell, 0)]);
        assert!(!s.complete);
        assert_eq!(s.observed, 1);
        assert_eq!(s.launched, 10);
    }

    #[test]
    fn offer_detection_matches_what_real_transcripts_said() {
        assert!(
            !find_offers("Want me to log the open design question so it's not lost?").is_empty()
        );
        assert!(!find_offers("I'll hold off until you say go.").is_empty());
        assert!(!find_offers("Say the word and I'll write that benchmark.").is_empty());
        // Must not fire on ordinary prose, or L3 inflates and the permission
        // problem looks bigger than it is.
        assert!(find_offers("I fixed the bug and added a test.").is_empty());
        assert!(find_offers("The datum offset shifts every height by a constant.").is_empty());
    }
}

// ---------------------------------------------------------------------------
// Reading a transcript
// ---------------------------------------------------------------------------

use anyhow::{Context, Result};
use std::path::Path;

/// Read one Claude Code transcript (JSONL) into a [`SessionRead`].
///
/// The transcript is the archive of record. It survives the store being torn
/// down — which is how run 4 was re-audited after the fact — and it holds the
/// tool calls, their inputs, their results and the final prose. The `tee`'d
/// session logs hold only the last assistant message, which is why they could
/// not answer any of the questions that mattered.
pub fn read_transcript(path: &Path) -> Result<SessionRead> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read transcript {}", path.display()))?;

    let mut session_id = String::new();
    let mut keel_tools: Vec<String> = Vec::new();
    let mut write_attempts = 0usize;
    let mut write_errors: Vec<String> = Vec::new();
    let mut permission_denials = 0usize;
    let mut last_text = String::new();
    let mut pending_writes: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if session_id.is_empty()
            && let Some(id) = value.get("sessionId").and_then(|v| v.as_str())
        {
            session_id = id.to_owned();
        }
        let Some(content) = value.pointer("/message/content") else {
            continue;
        };

        if let Some(s) = content.as_str() {
            last_text = s.to_owned();
        }
        let Some(blocks) = content.as_array() else {
            continue;
        };

        for block in blocks {
            match block.get("type").and_then(|v| v.as_str()) {
                Some("text") => {
                    if let Some(s) = block.get("text").and_then(|v| v.as_str()) {
                        last_text = s.to_owned();
                    }
                }
                Some("tool_use") => {
                    let Some(name) = block.get("name").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if !is_our_tool(name) {
                        continue;
                    }
                    let short = name.rsplit("__").next().unwrap_or(name).to_owned();
                    keel_tools.push(short);
                    if is_write_tool(name) {
                        write_attempts += 1;
                        if let Some(id) = block.get("id").and_then(|v| v.as_str()) {
                            pending_writes.insert(id.to_owned());
                        }
                    }
                }
                Some("tool_result") => {
                    let id = block
                        .get("tool_use_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if !pending_writes.contains(id) {
                        continue;
                    }
                    let body = block
                        .get("content")
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    let is_error = block
                        .get("is_error")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    // A permission refusal and a validation rejection look the
                    // same in a naive count and mean opposite things: one is a
                    // confound in the harness, the other is a finding about the
                    // schema.
                    let lower = body.to_lowercase();
                    if lower.contains("requires approval") || lower.contains("permission") {
                        permission_denials += 1;
                    } else if is_error || body.contains("-32602") {
                        write_errors.push(body.chars().take(160).collect());
                    }
                }
                _ => {}
            }
        }
    }

    let offers = find_offers(&last_text);
    Ok(SessionRead {
        session_id,
        keel_tools,
        write_attempts,
        write_errors,
        permission_denials,
        offers,
        level: Level::L1NoSignOfNoticing, // replaced by the caller, which knows `landed`
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod transcript_tests {
    use super::*;

    fn write_jsonl(dir: &Path, name: &str, lines: &[&str]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, lines.join("\n")).unwrap();
        p
    }

    /// The rename must not change what a transcript scores.
    ///
    /// Every transcript recorded before the rename names its tools
    /// `mcp__keel__keel_…`. If the scorer stopped recognising that prefix it
    /// would read all of them as L1 — never noticed the store — which is a
    /// plausible, silent, and completely wrong answer, and it would invalidate
    /// the frozen measurement in `GATE.md` rather than failing anything.
    #[test]
    fn both_spellings_of_the_prefix_score_identically() {
        let dir = tempfile::tempdir().unwrap();

        for (name, prefix) in [("old.jsonl", "keel"), ("new.jsonl", "specline")] {
            let line = format!(
                r#"{{"sessionId":"s","message":{{"content":[{{"type":"tool_use","name":"mcp__{prefix}__{prefix}_create","id":"w1"}}]}}}}"#
            );
            let path = write_jsonl(dir.path(), name, &[&line]);
            let read = read_transcript(&path).unwrap();

            assert_eq!(read.write_attempts, 1, "{prefix}: the write was not seen");
            assert_eq!(
                classify(
                    &read.keel_tools,
                    read.write_attempts,
                    true,
                    &read.offers,
                    true
                ),
                Level::L5WroteWell,
                "{prefix}: scored differently from the other spelling"
            );
        }
    }

    /// A tool from somebody else's MCP server is still not ours.
    #[test]
    fn a_foreign_tool_is_not_counted_under_either_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_jsonl(
            dir.path(),
            "foreign.jsonl",
            &[
                r#"{"sessionId":"s","message":{"content":[{"type":"tool_use","name":"mcp__github__create_issue","id":"x1"}]}}"#,
            ],
        );
        let read = read_transcript(&path).unwrap();

        assert!(read.keel_tools.is_empty(), "a foreign tool was counted");
        assert_eq!(read.write_attempts, 0);
    }

    #[test]
    fn a_transcript_with_no_keel_calls_reads_as_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_jsonl(
            dir.path(),
            "a.jsonl",
            &[
                r#"{"sessionId":"abc","message":{"content":[{"type":"tool_use","name":"Read","id":"t1"}]}}"#,
                r#"{"sessionId":"abc","message":{"content":[{"type":"text","text":"I fixed the bug."}]}}"#,
            ],
        );
        let r = read_transcript(&p).unwrap();
        assert_eq!(r.session_id, "abc");
        assert!(r.keel_tools.is_empty());
        assert_eq!(r.write_attempts, 0);
        assert!(r.offers.is_empty());
        assert_eq!(
            classify(&r.keel_tools, 0, false, &r.offers, true),
            Level::L1NoSignOfNoticing
        );
    }

    #[test]
    fn an_offer_in_the_closing_message_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_jsonl(
            dir.path(),
            "b.jsonl",
            &[
                r#"{"sessionId":"b","message":{"content":[{"type":"tool_use","name":"mcp__keel__keel_context","id":"t1"}]}}"#,
                r#"{"sessionId":"b","message":{"content":[{"type":"text","text":"Want me to log the open design question? I'll hold off until you say go."}]}}"#,
            ],
        );
        let r = read_transcript(&p).unwrap();
        // The transcript above is a real one, from before the rename, so the
        // name that comes back out of it is the one that went in. Extraction
        // reports what the session actually called, not what it would be
        // called today — a scorer that normalised the name would be quietly
        // rewriting the evidence.
        assert_eq!(r.keel_tools, vec!["keel_context"]);
        assert_eq!(r.offers.len(), 2, "two distinct markers in one sentence");
        assert_eq!(
            classify(&r.keel_tools, 0, false, &r.offers, true),
            Level::L3OfferedNotWritten
        );
    }

    #[test]
    fn a_validation_rejection_is_not_a_permission_denial() {
        // Run 4 had both shapes. Counting them together would have hidden the
        // finding that `priority: "high"` is rejected, or invented a
        // permission confound that did not exist.
        let dir = tempfile::tempdir().unwrap();
        let p = write_jsonl(
            dir.path(),
            "c.jsonl",
            &[
                r#"{"sessionId":"c","message":{"content":[{"type":"tool_use","name":"mcp__keel__keel_create","id":"w1"}]}}"#,
                r#"{"sessionId":"c","message":{"content":[{"type":"tool_result","tool_use_id":"w1","is_error":true,"content":"error -32602 unknown variant `high`"}]}}"#,
            ],
        );
        let r = read_transcript(&p).unwrap();
        assert_eq!(r.write_attempts, 1);
        assert_eq!(r.permission_denials, 0);
        assert_eq!(r.write_errors.len(), 1);
        assert!(r.write_errors[0].contains("-32602"));
    }

    #[test]
    fn a_permission_denial_is_counted_separately_because_it_is_a_confound() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_jsonl(
            dir.path(),
            "d.jsonl",
            &[
                r#"{"sessionId":"d","message":{"content":[{"type":"tool_use","name":"mcp__keel__keel_create","id":"w1"}]}}"#,
                r#"{"sessionId":"d","message":{"content":[{"type":"tool_result","tool_use_id":"w1","is_error":true,"content":"This tool requires approval"}]}}"#,
            ],
        );
        let r = read_transcript(&p).unwrap();
        assert_eq!(r.permission_denials, 1);
        assert!(
            r.write_errors.is_empty(),
            "a denial is not a validation error"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod known_answer_fixtures {
    //! Run these before every real run.
    //!
    //! The bug they exist to catch already happened: the scorer took its
    //! denominator from the event log, so seven sessions that wrote nothing
    //! left no trace and it reported "3 of 3" instead of "3 of 10". A scorer
    //! with no known-answer test is a scorer nobody has checked, and this one
    //! was wrong in the direction that flatters the result.

    use super::*;

    fn canned(dir: &Path, n: usize, wrote: bool, offered: bool) -> SessionRead {
        let mut lines = vec![format!(
            r#"{{"sessionId":"s{n}","message":{{"content":[{{"type":"tool_use","name":"mcp__keel__keel_context","id":"c{n}"}}]}}}}"#
        )];
        if wrote {
            lines.push(format!(
                r#"{{"sessionId":"s{n}","message":{{"content":[{{"type":"tool_use","name":"mcp__keel__keel_create","id":"w{n}"}}]}}}}"#
            ));
        }
        let closing = if offered {
            "Want me to log that as an open question?"
        } else {
            "Done. Fixed the bug and added a test."
        };
        lines.push(format!(
            r#"{{"sessionId":"s{n}","message":{{"content":[{{"type":"text","text":"{closing}"}}]}}}}"#
        ));
        let p = dir.join(format!("s{n}.jsonl"));
        std::fs::write(&p, lines.join("\n")).unwrap();
        read_transcript(&p).unwrap()
    }

    #[test]
    fn ten_sessions_that_wrote_nothing_score_zero_of_ten() {
        let dir = tempfile::tempdir().unwrap();
        let mut reads: Vec<SessionRead> = (0..10)
            .map(|n| canned(dir.path(), n, false, false))
            .collect();
        for r in &mut reads {
            r.level = classify(&r.keel_tools, r.write_attempts, false, &r.offers, true);
        }
        let s = score(10, &reads);
        assert_eq!(s.recall, 0.0, "no writes must score zero recall");
        assert_eq!(s.observed, 10);
        assert!(s.complete, "all ten were observed");
        assert_eq!(s.offers, 0);
    }

    #[test]
    fn ten_sessions_that_all_wrote_score_ten_of_ten() {
        let dir = tempfile::tempdir().unwrap();
        let mut reads: Vec<SessionRead> = (0..10)
            .map(|n| canned(dir.path(), n, true, false))
            .collect();
        for r in &mut reads {
            r.level = classify(&r.keel_tools, r.write_attempts, true, &r.offers, true);
        }
        let s = score(10, &reads);
        assert_eq!(s.recall, 1.0);
        assert_eq!(s.ceiling, 1.0);
        assert!(s.complete);
    }

    #[test]
    fn a_run_that_loses_transcripts_fails_the_completeness_check() {
        // The exact shape of the original bug: fewer observations than
        // launches, and the missing ones are the failures.
        let dir = tempfile::tempdir().unwrap();
        let mut reads: Vec<SessionRead> =
            (0..3).map(|n| canned(dir.path(), n, true, false)).collect();
        for r in &mut reads {
            r.level = classify(&r.keel_tools, r.write_attempts, true, &r.offers, true);
        }
        let s = score(10, &reads);
        assert!(
            !s.complete,
            "three observations for ten launches must not be reportable as a score"
        );
        assert_eq!(
            s.recall, 1.0,
            "and the naive rate reads 100% — which is precisely why `complete` has to gate it"
        );
    }

    #[test]
    fn offers_separate_the_permission_failure_from_the_orientation_failure() {
        let dir = tempfile::tempdir().unwrap();
        let mut reads = Vec::new();
        for n in 0..5 {
            reads.push(canned(dir.path(), n, false, true)); // offered, did not write
        }
        for n in 5..10 {
            let p = dir.path().join(format!("q{n}.jsonl"));
            std::fs::write(
                &p,
                format!(
                    r#"{{"sessionId":"q{n}","message":{{"content":[{{"type":"text","text":"Fixed it."}}]}}}}"#
                ),
            )
            .unwrap();
            reads.push(read_transcript(&p).unwrap()); // never touched Specline
        }
        for r in &mut reads {
            r.level = classify(&r.keel_tools, r.write_attempts, false, &r.offers, true);
        }
        let s = score(10, &reads);
        assert_eq!(s.recall, 0.0, "nobody wrote");
        assert_eq!(
            s.ceiling, 0.5,
            "but half formed the intent — the binary gate reported both halves as the same zero"
        );
        assert_eq!(s.offers, 5);
    }
}
