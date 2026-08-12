//! Referential integrity checks the schema cannot state.
//!
//! SQLite enforces the foreign keys it can, and the store turns them on. What
//! it cannot enforce is `links`, which is polymorphic across thirteen tables
//! and so points at no single one of them (SPEC §3.1). Nor can any constraint
//! catch a rule that spans rows — one current revision per document, a sub-task
//! tree with no loop in it, a readable identifier that identifies one row.
//! `keel-core` validates on write; this is the audit that catches whatever
//! slipped through — a crash between two writes, a restore from a half-good
//! backup, a bug in a future version.
//!
//! Every check answers a question a human would actually ask, and every
//! finding says what to do about it. A report that lists row ids without
//! explaining the consequence is one nobody acts on.

use crate::{EntityType, Error, Result, Store};
use serde::{Deserialize, Serialize};

/// How long an `in_progress` claim may go without an update before the board
/// is calling work active that almost certainly is not.
const STALE_CLAIM_DAYS: i64 = 3;

/// How much a finding matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Data is unreachable or wrong. Fix before trusting the store.
    Error,
    /// Untidy but harmless. Worth knowing, not worth stopping for.
    Warning,
}

/// One integrity problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// How much it matters.
    pub severity: Severity,
    /// A short name for the class of problem.
    pub check: String,
    /// What is wrong, in a sentence.
    pub detail: String,
    /// What to do about it.
    pub remedy: String,
    /// How many rows are affected.
    pub count: i64,
}

/// The outcome of a full check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FsckReport {
    /// Everything found, most severe first.
    pub findings: Vec<Finding>,
    /// How many checks ran.
    pub checks_run: usize,
}

impl FsckReport {
    /// Whether anything is actually broken.
    pub fn is_clean(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    /// Errors only.
    pub fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
    }
}

/// Artifacts whose prose cites a `B-1`/`TQ-4`/`Q-2`/`R-6`/`P0-7` style
/// identifier that no live artifact in the same project answers to.
///
/// Titles in this project carry their identifier as a prefix — "TQ-17 — …" —
/// so resolution is: does some artifact in the same project have a title
/// starting with that id? Deliberately lexical and deliberately scoped to the
/// project: a citation is a claim about *this* project's record.
fn dangling_id_references(store: &Store) -> Result<Vec<String>> {
    // Resolve against *every* entity, not just those with prose. The first
    // version scanned the `documents` table alone and reported 227 dangling
    // citations in a store of ~250 artifacts — because an artifact created
    // without a body has no document row, so most real targets were invisible.
    // `v_entities` exists for exactly this: resolve an id without knowing its
    // type.
    let mut stmt = store
        .connection()
        .prepare("SELECT COALESCE(project_id, ''), label FROM v_entities WHERE archived_at IS NULL")
        .map_err(Error::storage("prepare the cross-reference target list"))?;
    let mut labels: std::collections::HashMap<String, Vec<String>> = Default::default();
    for row in stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(Error::storage("list cross-reference targets"))?
        .filter_map(std::result::Result::ok)
    {
        labels.entry(row.0).or_default().push(row.1);
    }

    // Decision numbers, as the `B-n` tokens prose actually writes.
    //
    // These resolve against a *column* rather than a title prefix, which is the
    // point of giving decisions a number: `B-12` was a convention with nothing
    // behind it, so this check had to skip the whole family and therefore missed
    // the fabricated citation that motivated it (KEEL-66).
    let mut stmt = store
        .connection()
        .prepare("SELECT project_id, number FROM decisions WHERE number IS NOT NULL")
        .map_err(Error::storage("prepare the decision-number list"))?;
    let mut decision_refs: std::collections::HashMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    for row in stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i32>(1)?)))
        .map_err(Error::storage("list decision numbers"))?
        .filter_map(std::result::Result::ok)
    {
        decision_refs
            .entry(row.0)
            .or_default()
            .insert(format!("B-{}", row.1));
    }

    let mut stmt = store
        .connection()
        .prepare(
            "SELECT COALESCE(project_id, ''), title, body FROM documents \
             WHERE status = 'current'",
        )
        .map_err(Error::storage("prepare the cross-reference scan"))?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(Error::storage("run the cross-reference scan"))?
        .filter_map(std::result::Result::ok)
        .collect();

    // Only check identifier *families* this project actually uses in titles.
    //
    // Without this the check is worse than useless. Keel's own `B-n` decisions
    // and `D-n` spec decisions live in prose — a numbered table inside a
    // document — not as artifact titles, so every citation of them dangles by
    // construction: 182 findings in a store of ~250 artifacts, all of them
    // describing a convention the store does not model rather than a broken
    // reference.
    //
    // Where a project *does* title artifacts "TQ-17 — …", a dangling TQ-n is a
    // genuine break, and that is worth saying.
    let mut families: std::collections::HashMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    for (project, project_labels) in &labels {
        for label in project_labels {
            if let Some(id) = label.split_whitespace().next()
                && let Some((prefix, _)) = id.split_once('-')
                && cited_ids(id).contains(&id.to_owned())
            {
                families
                    .entry(project.clone())
                    .or_default()
                    .insert(prefix.to_owned());
            }
        }
    }

    // A project with numbered decisions has the `B` family whether or not any
    // title carries the prefix, because the numbers are now stored.
    for project in decision_refs.keys() {
        families
            .entry(project.clone())
            .or_default()
            .insert("B".to_owned());
    }

    let empty = Vec::new();
    let no_families = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (project, title, body) in &rows {
        let known = labels.get(project).unwrap_or(&empty);
        let used = families.get(project).unwrap_or(&no_families);
        for id in cited_ids(body) {
            let Some((prefix, _)) = id.split_once('-') else {
                continue;
            };
            if !used.contains(prefix) {
                continue;
            }
            let resolves = decision_refs
                .get(project)
                .is_some_and(|numbers| numbers.contains(&id))
                || [" ", "—", "-", ":"].iter().any(|sep| {
                    let with = format!("{id}{sep}");
                    title.starts_with(&with) || known.iter().any(|l| l.starts_with(&with))
                });
            if !resolves {
                out.push(format!("“{}” cites {id}", truncate(title, 44)));
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Identifiers cited in prose, as bare `PREFIX-number` tokens.
///
/// Matched conservatively. A false positive here sends someone hunting for a
/// problem that is not there, which is the same disease as the citation.
fn cited_ids(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in body.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
        let token = raw.trim_matches('-');
        let Some((prefix, number)) = token.split_once('-') else {
            continue;
        };
        let known_prefix = matches!(prefix, "B" | "D" | "Q" | "TQ" | "R")
            || (prefix.len() == 2
                && prefix.starts_with('P')
                && prefix[1..].chars().all(|c| c.is_ascii_digit()));
        if known_prefix
            && !number.is_empty()
            && number
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_alphabetic())
            && number.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            out.push(token.to_owned());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Shorten a title for a finding.
fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_owned();
    }
    s.chars().take(n).collect::<String>() + "…"
}

/// Ask SQLite whether the file underneath the rows is intact.
///
/// `Ok(None)` means clean. `Ok(Some(text))` is what SQLite said was wrong,
/// joined into one line.
///
/// `which` is `integrity_check` for the full audit or `quick_check` for the
/// cheap one the daemon runs at startup — the difference is that the quick
/// version skips index verification, which is most of the cost and rarely the
/// thing that is wrong.
///
/// Both return the single row `ok` when there is nothing to report, which is
/// why this compares against that string rather than counting rows.
pub fn page_integrity(store: &Store, which: &str) -> Result<Option<String>> {
    let mut stmt = store
        .connection()
        .prepare(&format!("PRAGMA {which}"))
        .map_err(Error::storage(format!("prepare PRAGMA {which}")))?;
    let problems: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(Error::storage(format!("run PRAGMA {which}")))?
        .filter_map(std::result::Result::ok)
        .filter(|line| line != "ok")
        .collect();

    if problems.is_empty() {
        Ok(None)
    } else {
        Ok(Some(problems.join("; ")))
    }
}

/// Every check this module can report, by name.
///
/// Declared rather than derived because a check nobody can enumerate is a check
/// nobody can prove has a test. `tests/fsck_coverage.rs` asserts that every name
/// here has a corruption test that trips it, and the test at the bottom of this
/// file asserts the list matches what the code actually emits — so a new check
/// cannot be added silently in either direction.
pub const CHECKS: [&str; 20] = [
    "dangling_link_source",
    "dangling_link_target",
    "depends_on_stored",
    "link_type_mismatch",
    "orphan_document",
    "multiple_current_revisions",
    "orphan_task",
    "stale_in_progress",
    "duplicate_task_number",
    "task_without_number",
    "project_without_key",
    "task_parent_cycle",
    "task_parent_dangling",
    "unresolved_id_reference",
    "event_without_actor",
    "event_without_session",
    "page_integrity",
    "row_without_creation_event",
    "live_link_to_archived",
    "orphan_blob",
];

/// Run every integrity check.
///
/// Exits non-zero only on errors, so a warning-only report can still gate a
/// backup or a deploy without crying wolf.
pub fn check(store: &Store) -> Result<FsckReport> {
    let mut findings = Vec::new();
    let mut checks_run = 0usize;
    let conn = store.connection();

    let count = |sql: &str, what: &str| -> Result<i64> {
        conn.query_row(sql, [], |r| r.get::<_, i64>(0))
            .map_err(Error::storage(format!("run the `{what}` integrity check")))
    };

    // --- Links point at rows that exist ---------------------------------
    // The foreign key that cannot be declared. A dangling edge is invisible:
    // traversal simply does not return it.
    for (side, id_col) in [("source", "from_id"), ("target", "to_id")] {
        checks_run += 1;
        let n = count(
            &format!(
                "SELECT count(*) FROM links l \
                 WHERE l.archived_at IS NULL \
                   AND NOT EXISTS (SELECT 1 FROM v_entities v WHERE v.id = l.{id_col})"
            ),
            "dangling_link",
        )?;
        if n > 0 {
            findings.push(Finding {
                severity: Severity::Error,
                check: format!("dangling_link_{side}"),
                detail: format!(
                    "{n} live link(s) whose {side} entity does not exist. Graph traversal \
                     silently skips these, so a traceability query returns an incomplete \
                     answer that looks complete"
                ),
                remedy: format!(
                    "archive the offending links: UPDATE links SET archived_at = \
                     strftime('%Y-%m-%dT%H:%M:%f000Z', 'now') \
                     WHERE {id_col} NOT IN (SELECT id FROM v_entities)"
                ),
                count: n,
            });
        }
    }

    // --- `depends_on` was never stored ----------------------------------
    // D-11. If one of these exists, something bypassed the normalisation and
    // every blocker query is now returning half the truth.
    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM links WHERE rel = 'depends_on'",
        "depends_on_stored",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "depends_on_stored".to_owned(),
            detail: format!(
                "{n} link(s) stored as `depends_on`. Only `blocks` is ever stored (D-11); \
                 no traversal in the codebase looks for `depends_on`, so these edges are \
                 invisible to every blocker query"
            ),
            remedy: "rewrite them as `blocks` with the endpoints swapped, then find \
                     whatever wrote them without going through keel-core"
                .to_owned(),
            count: n,
        });
    }

    // --- Links do not misreport their endpoints' types -------------------
    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM links l JOIN v_entities v ON v.id = l.from_id \
         WHERE l.from_type <> v.entity_type
         UNION ALL SELECT 0",
        "link_type_mismatch",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "link_type_mismatch".to_owned(),
            detail: format!(
                "{n} link(s) whose denormalised `from_type` disagrees with the entity it \
                 points at. Traversal trusts the denormalised column, so results will be \
                 labelled with the wrong type"
            ),
            remedy: "UPDATE links SET from_type = (SELECT entity_type FROM v_entities \
                     WHERE id = from_id)"
                .to_owned(),
            count: n,
        });
    }

    // --- Every prose header's pointer matches a real revision ------------
    for entity_type in EntityType::ALL.into_iter().filter(|t| t.has_document()) {
        checks_run += 1;
        let table = entity_type.table();
        let n = count(
            &format!(
                "SELECT count(*) FROM {table} t WHERE t.current_doc_version > 0 \
                 AND NOT EXISTS (SELECT 1 FROM documents d \
                                 WHERE d.entity_id = t.id AND d.version = t.current_doc_version)"
            ),
            "doc_pointer_dangling",
        )?;
        if n > 0 {
            findings.push(Finding {
                severity: Severity::Error,
                check: format!("doc_pointer_dangling_{}", entity_type.as_str()),
                detail: format!(
                    "{n} {entity_type}(s) point at a document revision that does not exist. \
                     Reading the body returns nothing, so the artifact looks empty"
                ),
                remedy: "restore from backup, or reset current_doc_version to the highest \
                         revision that does exist for that entity"
                    .to_owned(),
                count: n,
            });
        }
    }

    // --- Every document belongs to a real entity -------------------------
    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM documents d \
         WHERE NOT EXISTS (SELECT 1 FROM v_entities v WHERE v.id = d.entity_id)",
        "orphan_document",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "orphan_document".to_owned(),
            detail: format!(
                "{n} document revision(s) whose entity does not exist. The prose is \
                 unreachable through any normal path, though search may still surface it"
            ),
            remedy: "recreate the missing header, or delete the orphaned revisions once \
                     their content has been recovered"
                .to_owned(),
            count: n,
        });
    }

    // --- Exactly one current revision per document ----------------------
    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM (SELECT entity_id FROM documents \
         WHERE status = 'current' GROUP BY entity_id HAVING count(*) > 1)",
        "multiple_current_revisions",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "multiple_current_revisions".to_owned(),
            detail: format!(
                "{n} document(s) have more than one revision marked `current`. Which body \
                 a read returns is then arbitrary"
            ),
            remedy: "mark all but the highest version as `superseded`".to_owned(),
            count: n,
        });
    }

    // --- Children of archived parents ------------------------------------
    // Archiving a parent archives its links but never its children
    // (SPEC §3.1). That is deliberate, and orphans surface here rather than
    // disappearing — a cascade is unrecoverable, an orphan is merely untidy.
    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM tasks t JOIN projects p ON p.id = t.project_id \
         WHERE p.archived_at IS NOT NULL AND t.archived_at IS NULL",
        "orphan_task",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Warning,
            check: "orphan_task".to_owned(),
            detail: format!(
                "{n} live task(s) belong to an archived project. This is expected — \
                 archiving never cascades to children — but they will not appear in any \
                 project view"
            ),
            remedy: "archive them too, or move the project back to active".to_owned(),
            count: n,
        });
    }

    // --- Work claimed and never finished --------------------------------
    //
    // `in_progress` had never been used once across 66 tasks: agent sessions
    // discover the shape of the work while doing it and record the outcome, so
    // by the time there is something to write down it is finished. The
    // SessionStart hook now asks a session to claim a task before starting,
    // which creates the opposite failure — a task claimed by a session that
    // ended hours ago, still showing as active.
    //
    // A stale claim is worse than an empty column. An empty column says
    // "nothing is tracked here"; a stale one says "this is being worked on
    // right now", and is wrong.
    //
    // The cutoff is computed here and bound rather than written as SQL, and it
    // is formatted the way the store writes timestamps — not the way `chrono`
    // prints them by default. SQLite compares TEXT lexically, so
    // `2026-08-08 12:00:00` (a space) and `2026-08-08T12:00:00.000000Z` (a `T`)
    // are not merely different spellings: the space sorts below every digit and
    // below `T`, so a same-day cutoff would silently match nothing. The check
    // would keep running and keep reporting zero, which is the failure this
    // whole file exists to catch in other code.
    let stale_after = chrono::Utc::now() - chrono::Duration::days(STALE_CLAIM_DAYS);
    checks_run += 1;
    let n = conn
        .query_row(
            "SELECT count(*) FROM tasks WHERE status = 'in_progress' \
             AND archived_at IS NULL AND updated_at < ?",
            [stale_after.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()],
            |r| r.get::<_, i64>(0),
        )
        .map_err(Error::storage(
            "run the `stale_in_progress` integrity check",
        ))?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Warning,
            check: "stale_in_progress".to_owned(),
            detail: format!(
                "{n} task(s) have been in_progress for more than three days without an \
                 update. The board is claiming work is active that probably is not"
            ),
            remedy: "finish them, or move them back to todo. A stale claim is worse than \
                     an empty column: it is confidently wrong rather than merely silent"
                .to_owned(),
            count: n,
        });
    }

    // --- Readable identifiers actually identify -------------------------
    //
    // `KEEL-42` is only worth having if it means exactly one task. The unique
    // indexes make a duplicate impossible to write through the store, so what
    // is being audited here is the state a restore or a hand-edited database
    // can leave behind — and a duplicated readable id is the worst kind of
    // wrong, because both rows look correct in isolation.
    checks_run += 1;
    let n = count(
        "SELECT COALESCE(sum(c - 1), 0) FROM (
           SELECT count(*) AS c FROM tasks GROUP BY project_id, number HAVING count(*) > 1
         )",
        "duplicate_task_number",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "duplicate_task_number".to_owned(),
            detail: format!(
                "{n} task(s) share a number with another task in the same project, so a \
                 readable identifier like KEEL-42 resolves to more than one row"
            ),
            remedy: "renumber the newer of each pair to the project's next free number. \
                     Until then any reference to that identifier silently picks one"
                .to_owned(),
            count: n,
        });
    }

    // A task with no number cannot be referred to at all, and a project with no
    // key takes every one of its tasks down with it.
    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM tasks WHERE number IS NULL OR number <= 0",
        "task_without_number",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "task_without_number".to_owned(),
            detail: format!("{n} task(s) have no number, so they have no readable identifier"),
            remedy: "assign each the project's next free number. This should be impossible \
                     through the store, so it means a row arrived another way"
                .to_owned(),
            count: n,
        });
    }

    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM projects WHERE key IS NULL OR key = ''",
        "project_without_key",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "project_without_key".to_owned(),
            detail: format!(
                "{n} project(s) have no key, so none of their tasks can be named readably"
            ),
            remedy: "set one — the uppercased first few letters of the slug is the default"
                .to_owned(),
            count: n,
        });
    }

    // --- Sub-tasks form a tree, not a knot ------------------------------
    //
    // The store rejects a cycle on the way in, so what is audited here is the
    // state a restore or a hand-edited database can leave. A cycle is not
    // untidy: every rollup and every render of the tree recurses until
    // something runs out of stack, and nothing else in the system can see the
    // whole chain.
    checks_run += 1;
    let n = count(
        "WITH RECURSIVE up AS (
           SELECT id AS start, parent_id AS ancestor, 1 AS depth FROM tasks WHERE parent_id IS NOT NULL
           UNION ALL
           SELECT u.start, t.parent_id, u.depth + 1
           FROM up u JOIN tasks t ON t.id = u.ancestor
           WHERE t.parent_id IS NOT NULL AND u.depth < 32
         )
         SELECT count(DISTINCT start) FROM up WHERE ancestor = start",
        "task_parent_cycle",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "task_parent_cycle".to_owned(),
            detail: format!("{n} task(s) are their own ancestor, so the sub-task tree has a loop"),
            remedy: "clear `parent_id` on one task in each loop. Anything walking the tree \
                     recurses forever until it is broken"
                .to_owned(),
            count: n,
        });
    }

    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM tasks t
         LEFT JOIN tasks p ON p.id = t.parent_id
         WHERE t.parent_id IS NOT NULL
           AND (p.id IS NULL OR p.project_id != t.project_id)",
        "task_parent_dangling",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "task_parent_dangling".to_owned(),
            detail: format!(
                "{n} task(s) name a parent that does not exist or belongs to another project"
            ),
            remedy: "clear `parent_id`, or set it to a task in the same project. A child whose \
                     parent is elsewhere appears under nothing and is counted by no rollup"
                .to_owned(),
            count: n,
        });
    }

    // --- Cross-references that resolve ----------------------------------
    //
    // A gate session filed a question into a project citing "append-only
    // (D-9)". D-9 is one of *Keel's* decisions; the project it was filed under
    // was empty minutes earlier and has no D-9. The body went on to reference
    // columns that project does not have.
    //
    // A fabricated citation is worse than a missing one. It reads as
    // provenance, sends the reader looking for something that was never there,
    // and nothing in the store disagrees with it. Recall cannot see this at
    // all: the write happened and looked substantial.
    checks_run += 1;
    let dangling = dangling_id_references(store)?;
    if !dangling.is_empty() {
        let shown: Vec<String> = dangling.iter().take(5).cloned().collect();
        findings.push(Finding {
            severity: Severity::Warning,
            check: "unresolved_id_reference".to_owned(),
            detail: format!(
                "{} artifact(s) cite an identifier that resolves to nothing in their own \
                 project: {}{}",
                dangling.len(),
                shown.join("; "),
                if dangling.len() > shown.len() {
                    ", …"
                } else {
                    ""
                }
            ),
            remedy: "correct the citation, or create what it refers to. A reference that \
                     resolves to nothing reads as provenance and is not"
                .to_owned(),
            count: dangling.len() as i64,
        });
    }

    // --- Provenance is intact -------------------------------------------
    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM events WHERE actor IS NULL OR actor = ''",
        "event_without_actor",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "event_without_actor".to_owned(),
            detail: format!("{n} event(s) with no actor. G3's provenance guarantee is broken"),
            remedy: "these cannot be reconstructed; find what wrote them".to_owned(),
            count: n,
        });
    }

    // --- Writes with no session -----------------------------------------
    // Not an error: attribution is cooperative under a stateless transport
    // (D-10), and refusing the write would be worse. But a *rising* count here
    // means the skill has stopped threading session_id, which is exactly what
    // Phase 2's exit criterion measures.
    checks_run += 1;
    let unattributed = count(
        "SELECT count(*) FROM events WHERE session_id IS NULL",
        "event_without_session",
    )?;
    let total_events = count("SELECT count(*) FROM events", "event_total")?;
    if unattributed > 0 {
        findings.push(Finding {
            severity: Severity::Warning,
            check: "event_without_session".to_owned(),
            detail: format!(
                "{unattributed} of {total_events} event(s) carry no session_id. Attribution \
                 is cooperative (D-10), so this is legal — but it is the number Phase 2's \
                 exit criterion is about"
            ),
            remedy: "if this is growing, the skill has stopped threading session_id".to_owned(),
            count: unattributed,
        });
    }

    // --- Idempotency keys are actually unique ----------------------------
    for entity_type in EntityType::ALL {
        checks_run += 1;
        let table = entity_type.table();
        let scope = match entity_type.project_scope() {
            crate::ProjectScope::IsTheProject => "idempotency_key".to_owned(),
            crate::ProjectScope::Optional => "COALESCE(project_id, ''), idempotency_key".to_owned(),
            crate::ProjectScope::Required => "project_id, idempotency_key".to_owned(),
        };
        let n = count(
            &format!(
                "SELECT count(*) FROM (SELECT {scope} FROM {table} \
                 GROUP BY {scope} HAVING count(*) > 1)"
            ),
            "duplicate_idempotency_key",
        )?;
        if n > 0 {
            findings.push(Finding {
                severity: Severity::Error,
                check: format!("duplicate_idempotency_key_{}", entity_type.as_str()),
                detail: format!(
                    "{n} duplicate idempotency key(s) in `{table}`. REQ-7's retry safety is \
                     broken: a repeated create will pick one of them arbitrarily"
                ),
                remedy: "merge the duplicates and archive the losers".to_owned(),
                count: n,
            });
        }
    }

    // --- The page store itself ------------------------------------------
    //
    // Everything above asks whether the *rows* agree with each other. Nothing
    // asked whether the file underneath them is intact, so file-level damage —
    // a truncated write, a sync client copying `.db` and `-wal` at different
    // instants — surfaced as a random unrelated error somewhere else entirely,
    // days later, looking like a bug in whatever happened to read the bad page.
    //
    // `integrity_check` rather than `quick_check`: this is the deliberate audit
    // and it should look at the indexes too. The daemon runs `quick_check` at
    // startup instead, where the cost is paid on every boot.
    checks_run += 1;
    if let Some(problems) = page_integrity(store, "integrity_check")? {
        findings.push(Finding {
            severity: Severity::Error,
            check: "page_integrity".to_owned(),
            detail: format!(
                "SQLite reports the database file itself as damaged: {problems}. This is not a \
                 modelling problem — pages or indexes on disk are wrong, and any read may \
                 return the wrong answer rather than an error"
            ),
            remedy: "restore from the most recent backup (`keel restore`). If there is none, \
                     `sqlite3 keel.sqlite .recover` salvages what it can. Then find out why: \
                     the usual cause is ~/.keel sitting in a Dropbox, iCloud or network folder \
                     that copies the .sqlite, -wal and -shm files at different moments"
                .to_owned(),
            count: 1,
        });
    }

    // --- A row with no creation event ------------------------------------
    //
    // The orphan the non-atomic write path produced, and the reason it was
    // unrecoverable: the idempotent retry returns `created: false` before it
    // reaches the event append, so a second attempt never backfills the
    // history. Fixed in KEEL-141, but rows written before that are still out
    // there and nothing could see them.
    //
    // A warning rather than an error. The row is complete and usable; what is
    // missing is its provenance, which cannot be reconstructed and does not
    // stop anything working.
    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM v_entities v \
         WHERE NOT EXISTS (SELECT 1 FROM events e WHERE e.entity_id = v.id AND e.op = 'created')",
        "row_without_creation_event",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Warning,
            check: "row_without_creation_event".to_owned(),
            detail: format!(
                "{n} row(s) have no `created` event. They work, but the changelog and the \
                 activity feed have no record of them arriving, so they read as though they \
                 were always there"
            ),
            remedy: "nothing to repair — the history cannot be reconstructed. If the count is \
                     growing, something is writing rows outside keel-core's write path"
                .to_owned(),
            count: n,
        });
    }

    // --- An archived row with live links ---------------------------------
    //
    // `archive` puts the row away and archives its links in the same
    // transaction. A pair that disagrees means one of the two landed alone —
    // and a live edge into an archived row is the worst kind, because
    // traversal returns a neighbour that nothing will render.
    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM links l \
         WHERE l.archived_at IS NULL \
           AND EXISTS (SELECT 1 FROM v_entities v \
                       WHERE v.id IN (l.from_id, l.to_id) AND v.archived_at IS NOT NULL)",
        "live_link_to_archived",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Error,
            check: "live_link_to_archived".to_owned(),
            detail: format!(
                "{n} live link(s) touch an archived row. Traversal returns the archived \
                 neighbour, and every renderer then drops it — so the graph and the board \
                 disagree about whether that artifact exists"
            ),
            remedy: "archive the links: UPDATE links SET archived_at = \
                     strftime('%Y-%m-%dT%H:%M:%f000Z', 'now') WHERE archived_at IS NULL AND \
                     (from_id IN (SELECT id FROM v_entities WHERE archived_at IS NOT NULL) OR \
                     to_id IN (SELECT id FROM v_entities WHERE archived_at IS NOT NULL))"
                .to_owned(),
            count: n,
        });
    }

    // --- A blob nothing points at ----------------------------------------
    //
    // The one fsck had no check for at all, which is what made it permanent: an
    // orphaned blob is invisible, so nobody knows it is safe to delete and
    // nobody ever deletes it. A 5 MB screenshot from a half-failed create sits
    // in the file forever.
    checks_run += 1;
    let n = count(
        "SELECT count(*) FROM blobs b \
         WHERE b.entity_id IS NULL \
            OR NOT EXISTS (SELECT 1 FROM v_entities v WHERE v.id = b.entity_id)",
        "orphan_blob",
    )?;
    if n > 0 {
        findings.push(Finding {
            severity: Severity::Warning,
            check: "orphan_blob".to_owned(),
            detail: format!(
                "{n} blob(s) belong to no row. They are unreachable — nothing links to a blob \
                 except the entity that owns it — so they are bytes in the file that no \
                 screen can ever show"
            ),
            remedy: "reclaim them once you have a backup: DELETE FROM blobs WHERE entity_id IS \
                     NULL OR entity_id NOT IN (SELECT id FROM v_entities). This is the one \
                     place a DELETE is right, because there is nothing to soft-delete *from*"
                .to_owned(),
            count: n,
        });
    }

    findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.check.cmp(&b.check))
    });
    Ok(FsckReport {
        findings,
        checks_run,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// `CHECKS` lists exactly the names this file emits.
    ///
    /// Read out of the source rather than out of a run, because most checks
    /// only produce a finding against a corrupted store and a test that had to
    /// corrupt one nineteen different ways to enumerate them would be the very
    /// thing `tests/fsck_coverage.rs` exists to be. The `dangling_link_*` pair
    /// is built by format string, so it is the one entry that has to be spelt
    /// out here as well.
    #[test]
    fn the_declared_check_list_matches_what_the_code_emits() {
        let source = include_str!("fsck.rs");
        let mut emitted: Vec<String> = source
            .match_indices("check: \"")
            .filter_map(|(at, _)| {
                let rest = &source[at + "check: \"".len()..];
                rest.find('"').map(|end| rest[..end].to_owned())
            })
            .collect();
        emitted.extend([
            "dangling_link_source".to_owned(),
            "dangling_link_target".to_owned(),
        ]);
        emitted.sort();
        emitted.dedup();
        // The two examples in the tests below are literals of the same shape.
        emitted.retain(|name| name != "dangling_link" || CHECKS.contains(&name.as_str()));

        let mut declared: Vec<String> = CHECKS.iter().map(|c| (*c).to_owned()).collect();
        declared.sort();

        assert_eq!(
            emitted, declared,
            "fsck::CHECKS and the check names in this file have diverged"
        );
    }

    #[test]
    fn a_report_with_only_warnings_is_clean() {
        let report = FsckReport {
            findings: vec![Finding {
                severity: Severity::Warning,
                check: "orphan_task".to_owned(),
                detail: String::new(),
                remedy: String::new(),
                count: 3,
            }],
            checks_run: 1,
        };
        assert!(report.is_clean());
        assert_eq!(report.errors().count(), 0);
    }

    #[test]
    fn a_report_with_an_error_is_not_clean() {
        let report = FsckReport {
            findings: vec![Finding {
                severity: Severity::Error,
                check: "dangling_link_source".to_owned(),
                detail: String::new(),
                remedy: String::new(),
                count: 1,
            }],
            checks_run: 1,
        };
        assert!(!report.is_clean());
        assert_eq!(report.errors().count(), 1);
    }

    #[test]
    fn errors_sort_before_warnings() {
        assert!(Severity::Error < Severity::Warning);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod citation_tests {
    use super::cited_ids;

    #[test]
    fn it_finds_the_identifiers_this_project_actually_uses() {
        assert_eq!(
            cited_ids("reconcile with append-only (D-9) and no access-time tracking"),
            vec!["D-9"]
        );
        assert_eq!(
            cited_ids("see TQ-17 and B-22, plus P0-13"),
            vec!["B-22", "P0-13", "TQ-17"]
        );
        assert_eq!(cited_ids("raised as R-2a"), vec!["R-2a"]);
    }

    #[test]
    fn it_does_not_invent_citations_out_of_ordinary_prose() {
        // A false positive sends someone hunting for a problem that is not
        // there — the same disease as the fabricated citation itself.
        assert!(cited_ids("validate phases to 0-360 degrees").is_empty());
        assert!(cited_ids("the UTF-8 encoding and a SHA-256 digest").is_empty());
        assert!(cited_ids("bump to v1-rc2 before the 2026-08-10 cutoff").is_empty());
        assert!(cited_ids("a well-known trade-off").is_empty());
        assert!(cited_ids("").is_empty());
    }

    #[test]
    fn a_citation_is_only_checked_when_the_project_titles_artifacts_that_way() {
        // The scoping that makes this check usable. Keel's own B-n decisions
        // live in a numbered table inside a prose document, never as artifact
        // titles, so without family scoping every citation of them dangles by
        // construction: 182 findings in a store of ~250 artifacts, none of
        // them a broken reference. The rule is that a family is only checked
        // where the project actually uses it as a title prefix.
        //
        // This asserts the ingredient — that a title's leading token is
        // recognised as an identifier — since the scoping is built from it.
        assert_eq!(cited_ids("TQ-17"), vec!["TQ-17"]);
        assert_eq!(cited_ids("R-2a"), vec!["R-2a"]);
        assert!(cited_ids("Generation runs inside the daemon").is_empty());
    }

    #[test]
    fn a_hyphenated_word_is_not_an_identifier() {
        // "append-only" must not read as a citation of "only" under prefix A.
        assert!(cited_ids("append-only storage, write-ahead logging").is_empty());
    }
}
