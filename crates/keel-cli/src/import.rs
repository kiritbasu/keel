//! `keel import` — put a markdown file into Keel as a versioned document.
//!
//! The one-way door from "the specs live in the repo" to "the specs live in
//! Keel". After Keel is the source of truth this is a migration tool, run once
//! per file, not part of the loop — `keel generate` runs the other way and is
//! what runs from then on.
//!
//! Importing records where the file came from, as the artifact's
//! `mirror_path`. That is what closes the round trip: the document remembers
//! which repository file it *is*, so generation puts it back in the same place
//! rather than inventing a new one and leaving the original to rot beside it.
//!
//! Importing the same file twice appends a revision rather than making a second
//! artifact, and re-importing an unchanged file does nothing at all — so it is
//! safe to re-run during the migration, when a file may still be edited by hand
//! once or twice before the switch.

use anyhow::{Context, Result};
use keel_core::{
    Actor, Decision, Document, DocumentStore, DuckStore, Entity, EntityId, EntityQuery,
    EntityStore, EntityType, Provenance, Question, Spec, SpecKind, Surface,
};
use std::path::{Path, PathBuf};

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
    /// The repository path this artifact now claims, if one could be worked
    /// out.
    pub mirror_path: Option<String>,
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
    let mirror_path = repo_relative(store, project_id, path)?;
    let on_disk =
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    // A file that has been generated carries a banner naming the artifact it
    // came from. That is generation's bookkeeping, not the document's content,
    // and storing it would make every import-then-generate cycle stack another
    // banner on the last one.
    let raw = strip_generated_banner(&on_disk);

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
                    s.mirror_path = mirror_path.clone();
                    s.into()
                }
                EntityType::Decision => {
                    let mut d = Decision::new(project_id.clone(), &title);
                    d.mirror_path = mirror_path.clone();
                    d.into()
                }
                EntityType::Question => {
                    let mut q = Question::new(project_id.clone(), &title);
                    q.mirror_path = mirror_path.clone();
                    q.into()
                }
                other => anyhow::bail!(
                    "cannot import a file as a {other}. Prose-bearing types are spec, \
                     decision and question"
                ),
            };
            let created = store.create(entity, &prov)?;
            (created.entity.id().clone(), created.created)
        }
    };

    // An artifact imported before this behaviour existed has no recorded
    // path, and would otherwise be generated into `.keel/` at a slugged name
    // while the file it came from sat beside it going stale.
    adopt_path(store, &entity_id, mirror_path.as_deref(), &prov)?;

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
        mirror_path,
    })
}

/// The path of `file` relative to the project's checkout.
///
/// `None` when the project has no recorded checkout or the file sits outside
/// it — in which case the artifact adopts no file and generation sends it to
/// the `.keel/` mirror instead. Guessing would be worse: a wrong path means
/// generation writes over something it does not own.
fn repo_relative(store: &DuckStore, project_id: &EntityId, file: &Path) -> Result<Option<String>> {
    let Some(Entity::Project(project)) = store.get(project_id)? else {
        return Ok(None);
    };
    let Some(root) = project.root_path.as_deref() else {
        return Ok(None);
    };

    let root = std::fs::canonicalize(root).unwrap_or_else(|_| PathBuf::from(root));
    let absolute = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    Ok(absolute
        .strip_prefix(&root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/")))
}

/// Record the repository file this artifact is, if it does not already say so.
fn adopt_path(
    store: &mut DuckStore,
    entity_id: &EntityId,
    path: Option<&str>,
    prov: &Provenance,
) -> Result<()> {
    let Some(path) = path else { return Ok(()) };
    let Some(entity) = store.get(entity_id)? else {
        return Ok(());
    };
    if entity.mirror_path() == Some(path) {
        return Ok(());
    }

    let mut changes = serde_json::Map::new();
    changes.insert("mirror_path".to_owned(), serde_json::json!(path));
    store.update(entity_id, entity.audit().version, &changes, prov)?;
    Ok(())
}

/// Drop a leading `<!-- keel:generated … -->` banner, if there is one.
fn strip_generated_banner(content: &str) -> String {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("<!-- keel:generated") {
        return content.to_owned();
    }
    match trimmed.find("-->") {
        Some(end) => trimmed[end + 3..].trim_start_matches('\n').to_owned(),
        // An unterminated banner is a damaged file, not a generated one.
        // Storing it whole is the conservative choice: nothing is lost, and
        // the next generate rewrites it anyway.
        None => content.to_owned(),
    }
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
    fn a_generated_banner_is_not_stored_as_content() {
        let generated =
            "<!-- keel:generated spec spc_1 v3\n     do not edit -->\n\n# Title\n\nBody\n";
        assert_eq!(strip_generated_banner(generated), "# Title\n\nBody\n");

        // Idempotent: re-importing a file Keel generated must not slowly eat
        // the top of the document, nor stack banner on banner.
        assert_eq!(
            strip_generated_banner(&strip_generated_banner(generated)),
            "# Title\n\nBody\n"
        );

        // A hand-written file is untouched, comments and all.
        let plain = "<!-- a normal comment -->\n\n# Title\n";
        assert_eq!(strip_generated_banner(plain), plain);
        assert_eq!(strip_generated_banner("# Title\n"), "# Title\n");

        // A truncated banner is damage, not generation: keep everything.
        let broken = "<!-- keel:generated spec spc_1 v3\n# Title\n";
        assert_eq!(strip_generated_banner(broken), broken);
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
