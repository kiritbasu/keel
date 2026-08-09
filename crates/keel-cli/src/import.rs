//! `keel import` — put a markdown file into Keel as a versioned document.
//!
//! The bridge from "the specs live in the repo" to "the specs live in Keel".
//!
//! Importing the same file twice appends a revision rather than making a second
//! artifact, and re-importing an unchanged file does nothing at all — so this
//! is safe to re-run, and safe to put in a script. That is the property that
//! makes it a bridge rather than a one-way door: the repo copy can stay
//! authoritative for as long as you like, with Keel kept in step, until you
//! decide to delete it.

use anyhow::{Context, Result};
use keel_core::{
    Actor, Decision, Document, DocumentStore, DuckStore, Entity, EntityId, EntityQuery,
    EntityStore, EntityType, Provenance, Question, Spec, SpecKind, Surface,
};
use std::path::Path;

/// What an import did to one file.
pub struct Imported {
    /// The artifact it landed in.
    pub entity_id: EntityId,
    /// Its title.
    pub title: String,
    /// The revision now current.
    pub version: i32,
    /// Whether the artifact itself was created by this import.
    pub created: bool,
    /// Whether this import produced a new revision, or the content was
    /// already identical.
    pub revised: bool,
    /// Bytes of body stored.
    pub bytes: usize,
}

/// Import one markdown file.
pub fn file(
    store: &mut DuckStore,
    path: &Path,
    project_id: &EntityId,
    entity_type: EntityType,
    kind: Option<SpecKind>,
    title_override: Option<String>,
) -> Result<Imported> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

    let title = title_override
        .or_else(|| heading_of(&raw))
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".to_owned())
        });

    let prov = Provenance {
        actor: Actor::Human,
        session_id: Some("ses_import".to_owned()),
        surface: Some(Surface::Cli),
    };

    // Find an existing artifact with this title before creating one. The
    // create is idempotent anyway, but resolving first means a re-import lands
    // on the same artifact even if its title was edited in Keel afterwards —
    // which is the common case once the store is the source of truth.
    let existing = find_by_title(store, project_id, entity_type, &title)?;

    let (entity_id, created) = match existing {
        Some(id) => (id, false),
        None => {
            let entity: Entity = match entity_type {
                EntityType::Spec => {
                    let mut s = Spec::new(project_id.clone(), &title);
                    s.kind = kind.unwrap_or_else(|| infer_kind(path, &title));
                    s.into()
                }
                EntityType::Decision => Decision::new(project_id.clone(), &title).into(),
                EntityType::Question => Question::new(project_id.clone(), &title).into(),
                other => anyhow::bail!(
                    "cannot import a file as a {other}. Prose-bearing types are spec, \
                     decision and question"
                ),
            };
            let created = store.create(entity, &prov)?;
            (created.entity.id().clone(), created.created)
        }
    };

    let before = store.revision(&entity_id, None)?.map(|d| d.version);
    let doc = Document::first(
        entity_type,
        entity_id.clone(),
        Some(project_id.clone()),
        &title,
        &raw,
        prov.actor,
        chrono::Utc::now(),
    )?
    .attributed(prov.session_id.clone(), prov.surface);
    let written = store.write_revision(doc)?;

    Ok(Imported {
        entity_id,
        title,
        version: written.version,
        created,
        // `write_revision` is content-addressed: an unchanged body returns the
        // existing revision rather than appending a duplicate.
        revised: before != Some(written.version),
        bytes: raw.len(),
    })
}

/// The first level-one heading, which is what these files call themselves.
fn heading_of(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        line.strip_prefix("# ")
            .map(|h| h.trim().trim_end_matches(" —").trim().to_owned())
            .filter(|h| !h.is_empty())
    })
}

/// Guess the kind from the filename and title.
///
/// A guess, and overridable with `--kind`. Getting it wrong costs a wrong
/// badge in the UI, not data.
fn infer_kind(path: &Path, title: &str) -> SpecKind {
    let hay = format!(
        "{} {}",
        path.file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default(),
        title.to_lowercase()
    );
    if hay.contains("prd") || hay.contains("requirement") || hay.contains("product") {
        SpecKind::Prd
    } else if hay.contains("rfc") {
        SpecKind::Rfc
    } else if hay.contains("spec") {
        SpecKind::Spec
    } else if hay.contains("design") {
        SpecKind::DesignDoc
    } else {
        SpecKind::Note
    }
}

/// Find a live artifact of this type with this title.
fn find_by_title(
    store: &DuckStore,
    project_id: &EntityId,
    entity_type: EntityType,
    title: &str,
) -> Result<Option<EntityId>> {
    let page = store.list(
        &EntityQuery::in_project(project_id.clone())
            .of_type(entity_type)
            .limited(5_000),
    )?;
    Ok(page
        .items
        .iter()
        .find(|e| e.label().eq_ignore_ascii_case(title))
        .map(|e| e.id().clone()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_title_comes_from_the_first_heading() {
        assert_eq!(
            heading_of("# Keel — Technical Specification\n\nBody\n").as_deref(),
            Some("Keel — Technical Specification")
        );
        // A heading further down still counts; frontmatter and blank lines
        // above it are common.
        assert_eq!(
            heading_of("<!-- generated -->\n\n# Real Title\n").as_deref(),
            Some("Real Title")
        );
        assert_eq!(heading_of("no heading here\n"), None);
        assert_eq!(heading_of("#not a heading\n"), None);
    }

    #[test]
    fn the_kind_is_guessed_from_the_name() {
        assert_eq!(
            infer_kind(Path::new("PRD.md"), "Product Requirements"),
            SpecKind::Prd
        );
        assert_eq!(
            infer_kind(Path::new("SPEC.md"), "Technical Specification"),
            SpecKind::Spec
        );
        assert_eq!(
            infer_kind(Path::new("0001-rfc.md"), "Some RFC"),
            SpecKind::Rfc
        );
        assert_eq!(
            infer_kind(Path::new("HANDOFF.md"), "Handoff"),
            SpecKind::Note
        );
    }

    #[test]
    fn importing_the_same_file_twice_does_not_duplicate_or_re_version() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = DuckStore::open(dir.path()).unwrap();
        let project = store
            .create(
                keel_core::Project::new("keel", "Keel").into(),
                &Provenance::anonymous(Actor::Human),
            )
            .unwrap()
            .entity
            .id()
            .clone();

        let path = dir.path().join("SPEC.md");
        std::fs::write(&path, "# Storage\n\nDuckDB and Lance.\n").unwrap();

        let first = file(&mut store, &path, &project, EntityType::Spec, None, None).unwrap();
        assert!(first.created);
        assert!(first.revised);
        assert_eq!(first.version, 1);

        let again = file(&mut store, &path, &project, EntityType::Spec, None, None).unwrap();
        assert!(
            !again.created,
            "a re-import must not create a second artifact"
        );
        assert!(
            !again.revised,
            "unchanged content must not append a revision"
        );
        assert_eq!(again.version, 1);
        assert_eq!(again.entity_id, first.entity_id);

        // A real edit does append.
        std::fs::write(&path, "# Storage\n\nDuckDB and Lance, attached.\n").unwrap();
        let edited = file(&mut store, &path, &project, EntityType::Spec, None, None).unwrap();
        assert!(edited.revised);
        assert_eq!(edited.version, 2);
        assert_eq!(edited.entity_id, first.entity_id);
    }

    #[test]
    fn the_whole_body_is_stored_not_a_summary() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = DuckStore::open(dir.path()).unwrap();
        let project = store
            .create(
                keel_core::Project::new("keel", "Keel").into(),
                &Provenance::anonymous(Actor::Human),
            )
            .unwrap()
            .entity
            .id()
            .clone();

        let body = format!("# Big\n\n{}", "Paragraph of real prose.\n\n".repeat(2_000));
        let path = dir.path().join("BIG.md");
        std::fs::write(&path, &body).unwrap();

        let imported = file(&mut store, &path, &project, EntityType::Spec, None, None).unwrap();
        assert_eq!(imported.bytes, body.len());

        let stored = store.revision(&imported.entity_id, None).unwrap().unwrap();
        assert_eq!(stored.body, body, "the file must be stored whole");
    }
}
