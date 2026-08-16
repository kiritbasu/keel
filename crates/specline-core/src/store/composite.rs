//! The one write that spans an entity, its prose and its bytes.
//!
//! Creating a design with a caption and a screenshot used to be four store
//! calls orchestrated in `specline-mcp` over untyped JSON: create the row, write
//! the first revision, store the blob, then update the row a second time just
//! to record which blob it was. A crash anywhere in that sequence left an
//! entity with no body, or a blob no entity points at — and `fsck` had no blob
//! check, so an orphaned blob was invisible and therefore unreclaimable
//! forever.
//!
//! It is one method and one transaction now. That is a correctness fix, but it
//! is also a simplification: the second `update` round-trip is gone entirely,
//! because a blob id known before the row is inserted can just be part of the
//! row.

use super::entity::{Prepared, insert_created};
use super::{Blob, Store};
use crate::{Document, Entity, EntityId, Error, Provenance, Result};
use chrono::Utc;

/// The outcome of a composite create.
///
/// More than [`super::Created`] because a caller that asked for a body and an
/// image needs to know what became of them — `keel_create` reports the revision
/// back so an agent can cite the version it just wrote.
#[derive(Debug, Clone)]
pub struct CreatedComposite {
    /// The entity, new or pre-existing.
    pub entity: Entity,
    /// Whether this call is what brought it into being.
    pub created: bool,
    /// The first revision, if a body was supplied and written.
    pub document: Option<Document>,
    /// The stored image, if one was supplied and written.
    pub blob_id: Option<crate::BlobId>,
}

impl Store {
    /// Create an entity, its first revision and its image, in one transaction.
    ///
    /// `body` is written as revision 1 for the five types that carry prose and
    /// ignored for the eight that do not — the caller has usually already
    /// folded it into a column by then, and refusing here would make
    /// `keel_create(type: "task", body: …)` an error for no gain.
    ///
    /// `image` is `(bytes, media_type)`. The blob id is minted before the row
    /// is inserted, so the row carries its `blob_id` from the start and there
    /// is no second write to lose. The blob names the entity and the entity
    /// names the blob, both inside the transaction, so neither can exist
    /// without the other.
    ///
    /// An existing entity — same idempotency key, or a title that means the
    /// same thing — is returned untouched with `created: false`, and neither
    /// the body nor the image is written. Attaching a body to a row that
    /// already has history is [`Store::write_revision`]'s job, and doing it
    /// here would silently overwrite prose on a retry.
    pub fn create_with_document(
        &mut self,
        entity: Entity,
        body: Option<String>,
        image: Option<(Vec<u8>, String)>,
        provenance: &Provenance,
    ) -> Result<CreatedComposite> {
        // Before anything is prepared or written, because the answer does not
        // depend on the store and a refusal that has already inserted a row is
        // the bug next door (KEEL-146).
        //
        // Only the types whose document *is* their content. A task with no body
        // still says something; a question with no body is a title recording
        // that somebody wondered about something, with the wondering gone.
        if entity.entity_type().needs_prose() && body.as_deref().unwrap_or("").trim().is_empty() {
            return Err(Error::invalid(
                entity.entity_type(),
                "body",
                format!(
                    "a {} is its prose — there is no summary column to fall back on, so a row \
                     with only a title records that this exists and nothing about what it says",
                    entity.entity_type()
                ),
                "the reasoning: what was asked or chosen, what the options were, and why this \
                 one. Two sentences beats a heading, and it is the part that evaporates if it \
                 is not written down now",
            ));
        }

        let now = Utc::now();
        let mut entity = match self.prepare_create(entity, provenance, now)? {
            Prepared::Existing(existing) => {
                return Ok(CreatedComposite {
                    entity: existing,
                    created: false,
                    document: None,
                    blob_id: None,
                });
            }
            Prepared::Fresh(entity) => entity,
        };

        let entity_type = entity.entity_type();
        let entity_id = entity.id().clone();
        let project_id = entity.project_id().cloned();

        // Built before the row is inserted, so `blob_id` is a column on the
        // insert rather than a second UPDATE afterwards. The old shape needed
        // the entity to exist before the blob could name its owner and the
        // blob to exist before the entity could name it, which is why it was
        // two writes; minting the id up front dissolves the cycle.
        let blob = image.map(|(bytes, media_type)| {
            Blob::new(bytes, media_type, now).owned_by(
                entity_id.clone(),
                project_id.clone().unwrap_or_else(|| entity_id.clone()),
            )
        });
        if let Some(blob) = &blob {
            entity.set_blob_id(&blob.blob_id)?;
        }

        let document = body.filter(|_| entity_type.has_document()).map(|text| {
            Document::first(
                entity_type,
                entity_id.clone(),
                project_id.clone(),
                entity.label().to_owned(),
                text,
                provenance.actor,
                now,
            )
        });
        let document = match document {
            Some(Ok(doc)) => {
                Some(doc.attributed(provenance.session_id.clone(), provenance.surface))
            }
            Some(Err(e)) => return Err(e),
            None => None,
        };

        let embedder = self.embedder.clone();
        let tx = self
            .conn
            .transaction()
            .map_err(Error::storage(format!("create the {entity_type}")))?;

        insert_created(&tx, &entity, provenance, now)?;

        let written = match document {
            Some(doc) => Some(super::docs::write_revision_in(
                &tx,
                embedder.as_deref(),
                doc,
            )?),
            None => None,
        };

        let blob_id = match &blob {
            Some(b) => {
                super::docs::insert_blob_in(&tx, b)?;
                Some(b.blob_id.clone())
            }
            None => None,
        };

        tx.commit().map_err(Error::storage(format!(
            "commit the {entity_type} `{}`",
            entity.label()
        )))?;

        // The header's `current_doc_version` was advanced inside the
        // transaction, on the row in the database rather than on the value in
        // hand. Re-reading is what makes the returned entity agree with what a
        // caller would see if they asked for it a moment later — and the
        // callers all did that re-read themselves before this existed.
        let entity = self.get_entity_after_commit(&entity_id)?.unwrap_or(entity);

        Ok(CreatedComposite {
            entity,
            created: true,
            document: written,
            blob_id,
        })
    }

    /// Read a row back after its transaction committed.
    ///
    /// Named for the one thing it is for, so a future reader does not mistake
    /// it for a general lookup and reach for it where `get` belongs.
    fn get_entity_after_commit(&self, id: &EntityId) -> Result<Option<Entity>> {
        use crate::EntityStore;
        self.get(id)
    }
}
