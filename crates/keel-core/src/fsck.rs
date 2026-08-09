//! Cross-engine referential integrity checks.
//!
//! DuckDB supports `FOREIGN KEY`, but Keel cannot use it for the two places it
//! would matter most: `links` is polymorphic across thirteen tables, and
//! `documents` lives in Lance where DuckDB's constraint machinery cannot see
//! it (SPEC §3.1). `keel-core` validates on write; this is the audit that
//! catches whatever slipped through — a crash between two writes, a restore
//! from a half-good backup, a bug in a future version.
//!
//! Every check answers a question a human would actually ask, and every
//! finding says what to do about it. A report that lists row ids without
//! explaining the consequence is one nobody acts on.

use crate::{DuckStore, EntityType, Error, Result};
use serde::{Deserialize, Serialize};

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

/// Run every integrity check.
pub fn check(store: &DuckStore) -> Result<FsckReport> {
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
                    "archive the offending links: UPDATE links SET archived_at = now() \
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
                 AND NOT EXISTS (SELECT 1 FROM lancedb.documents d \
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
        "SELECT count(*) FROM lancedb.documents d \
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
        "SELECT count(*) FROM (SELECT entity_id FROM lancedb.documents \
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
