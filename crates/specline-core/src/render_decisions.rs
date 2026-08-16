//! The decision-log renderer — `product/DECISIONS.md` and its equivalents.
//!
//! The second half of the same dogfooding move that produced
//! [`crate::render_status`]. The decision log was a hand-maintained numbered
//! table, `B-1` to `B-25`, sitting alongside one generated file per decision
//! under `.specline/decisions/`. Neither contained the other: 39 rows against 25
//! table entries, only 11 of which carried a `B-n` at all, and where they did
//! overlap the table held several hundred words of reasoning against the row's
//! three-sentence summary.
//!
//! Two registers that must agree by hand is not a register. The reasoning now
//! lives in the rows and this renders it, so "which one is right" stops being
//! a question anyone can ask.
//!
//! Why a renderer rather than one adopted document: a decision log is a *view
//! over rows*, like the tracker and unlike a spec. No single artifact is
//! `product/DECISIONS.md` the way one spec artifact is `product/SPEC.md`, so
//! the destination belongs to the project (`decisions_path`) and there is
//! nothing an edit to the file could become a revision of.
//!
//! Output is one-directional, like everything else here: generated, never read
//! back.

use crate::{
    DecisionStatus, Entity, EntityId, EntityQuery, EntityStore, EntityType, Error, Result, Store,
};
use std::fmt::Write as _;

/// The section heading a row uses to explain what replaced it.
///
/// Matched rather than stored as a column: a reversal is prose about *why* the
/// original was wrong, and a column would either truncate that or duplicate it.
const SUPERSEDED: &str = "## Superseded";

/// Render a decision log for one project.
///
/// Ordered by number, which is assignment order, so the sequence reads as the
/// project's history — the same reasoning as `KEEL-1` being the oldest task.
pub fn render(store: &Store, project_id: &EntityId) -> Result<String> {
    let Some(Entity::Project(project)) = store.get(project_id)? else {
        return Err(Error::NotFound {
            entity_type: EntityType::Project,
            id: project_id.to_string(),
        });
    };

    let page = store.list(
        &EntityQuery::in_project(project_id.clone())
            .of_type(EntityType::Decision)
            .limited(5_000),
    )?;

    let mut decisions: Vec<_> = page
        .items
        .iter()
        .filter_map(|e| match e {
            Entity::Decision(d) => Some(d),
            _ => None,
        })
        .collect();
    decisions.sort_by_key(|d| d.number);

    let mut out = String::new();
    writeln!(out, "# {} — Decision log\n", project.name)?;
    writeln!(
        out,
        "<!-- specline:generated decisions {} -->\n> Generated from the decision \
         rows — edits here are not saved.\n",
        project.id
    )?;
    writeln!(
        out,
        "Every decision made while building, with the reasoning and what was \
         rejected. In six months nobody will remember why a library was chosen \
         or an approach abandoned, and one line written now saves an hour of \
         archaeology later.\n"
    )?;
    writeln!(
        out,
        "`B-12` is a real identifier, not a convention: it resolves to a row, \
         `keel_get KEEL-B12` returns it, and `fsck` checks that citations of it \
         point at something. It was prose until 2026-08-10, which is why every \
         `B-n` citation in this repository was unverifiable until then.\n"
    )?;

    if decisions.is_empty() {
        writeln!(out, "*Nothing decided yet.*")?;
        return Ok(out);
    }

    // The index earns its place: the bodies are long enough that scanning them
    // to find one decision is worse than a second lookup.
    writeln!(out, "## Index\n")?;
    writeln!(out, "| | Decision | Status |")?;
    writeln!(out, "|---|---|---|")?;
    for d in &decisions {
        writeln!(
            out,
            "| B-{} | [{}](#b-{}) | `{}` |",
            d.number,
            escape_cell(&d.title),
            d.number,
            d.status
        )?;
    }
    writeln!(out)?;

    // Reversals before the decisions themselves. A reader who needs this
    // section needs it *before* acting on something below it, and a reversal
    // buried under forty entries is one nobody meets in time.
    let reversed: Vec<_> = decisions
        .iter()
        .filter(|d| d.status == DecisionStatus::Superseded)
        .collect();
    writeln!(out, "## Reversals\n")?;
    if reversed.is_empty() {
        writeln!(
            out,
            "*Nothing has been reversed.* A decision that turns out to be wrong \
             is marked `superseded` with a `## Superseded` section saying what \
             replaced it, rather than being edited — knowing something was tried \
             and abandoned is as useful as knowing what was chosen.\n"
        )?;
    } else {
        for d in &reversed {
            writeln!(out, "**B-{} — {}**\n", d.number, d.title)?;
            let body = store
                .revision(&d.id, None)?
                .map(|r| r.body)
                .unwrap_or_default();
            match section(&body, SUPERSEDED) {
                Some(why) => writeln!(out, "{why}\n")?,
                // A row marked superseded with nothing saying why is a data
                // problem, and saying so beats rendering a heading over silence.
                None => writeln!(
                    out,
                    "*Marked superseded with no `## Superseded` section — what \
                     replaced it is not recorded.*\n"
                )?,
            }
        }
    }

    writeln!(out, "---\n")?;
    writeln!(out, "## Decisions\n")?;
    for d in &decisions {
        writeln!(out, "### B-{} — {}\n", d.number, d.title)?;
        let mut meta = format!("`{}`", d.status);
        if let Some(at) = d.decided_at {
            let _ = write!(meta, " · decided {}", at.format("%Y-%m-%d"));
        }
        let _ = write!(meta, " · `{}`", d.id);
        writeln!(out, "{meta}\n")?;

        let body = store
            .revision(&d.id, None)?
            .map(|r| r.body)
            .unwrap_or_default();
        if body.trim().is_empty() {
            writeln!(out, "*No reasoning recorded.*\n")?;
        } else {
            // Demoted so the body's own `## Decision` sits under this entry
            // rather than beside it — otherwise every decision's subheadings
            // read as top-level sections of the log.
            writeln!(out, "{}\n", demote(body.trim()))?;
        }
    }

    Ok(out)
}

/// Pull one `## Heading` section out of a body, without its heading.
fn section<'a>(body: &'a str, heading: &str) -> Option<&'a str> {
    let start = body.find(heading)? + heading.len();
    let rest = &body[start..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    let text = rest[..end].trim();
    (!text.is_empty()).then_some(text)
}

/// Push every markdown heading in `body` down two levels.
///
/// Fenced code blocks are skipped: `#` at the start of a line inside a fence is
/// a comment in most languages, and rewriting it would corrupt the sample.
fn demote(body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 32);
    let mut fenced = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        }
        if !fenced && trimmed.starts_with('#') {
            out.push_str("##");
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Make a title safe inside a markdown table cell.
fn escape_cell(title: &str) -> String {
    title.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_are_demoted_but_code_fences_are_left_alone() {
        let body =
            "## Decision\n\nUse it.\n\n```sh\n# not a heading\n```\n\n## Reasoning\n\nBecause.";
        let out = demote(body);
        assert!(out.contains("#### Decision"), "{out}");
        assert!(out.contains("#### Reasoning"), "{out}");
        assert!(
            out.contains("\n# not a heading\n"),
            "a comment inside a fence must survive: {out}"
        );
    }

    #[test]
    fn a_section_stops_at_the_next_heading() {
        let body = "## Superseded\n\nReplaced by B-42.\n\n## Notes\n\nUnrelated.";
        assert_eq!(section(body, SUPERSEDED), Some("Replaced by B-42."));
        assert_eq!(section(body, "## Missing"), None);
    }

    #[test]
    fn an_empty_section_reads_as_absent() {
        // A heading with nothing under it must not render as a reversal that
        // explains itself.
        assert_eq!(section("## Superseded\n\n\n", SUPERSEDED), None);
    }

    #[test]
    fn a_pipe_in_a_title_cannot_break_the_index_table() {
        assert_eq!(
            escape_cell("Surface is chat | code | cli"),
            "Surface is chat \\| code \\| cli"
        );
    }
}
