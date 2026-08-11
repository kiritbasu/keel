//! Shared daemon state: the store, the lock, and the change broadcast.

use anyhow::{Context, Result};
use keel_core::{DuckStore, EventId};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

/// A change worth telling the desktop app about.
///
/// Deliberately thin — an id and a summary, not the entity. A subscriber that
/// wants detail calls the API, which keeps the broadcast cheap and means a slow
/// subscriber can never hold a serialised entity in a queue.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Change {
    /// What kind of write this was: `entity` or `note`.
    ///
    /// Notes are announced separately because they are not events. The daemon
    /// announces an entity change when the latest `events` id advances, and a
    /// note leaves no row there — so before this existed, writing a note
    /// refreshed nothing and an open app showed a stale note stream with no
    /// indication it was stale (TQ-29).
    ///
    /// Carried as a field rather than a second SSE event name so that a client
    /// which ignores it keeps working: an app that wants every change gets one,
    /// and an app that would rather not redraw a board because a note landed on
    /// a task can look.
    pub kind: ChangeKind,
    /// The event that caused it, when there was one. `None` for a note.
    pub event_id: Option<EventId>,
    /// The row the change is about, when it is known.
    pub entity_id: Option<String>,
    /// What happened, in one line.
    pub summary: String,
}

/// What sort of write an SSE change describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    /// A create, update, archive or link — anything that wrote an event.
    Entity,
    /// A note appended to a row's commentary.
    Note,
}

/// Everything a request handler needs.
#[derive(Clone)]
pub struct AppState {
    store: Arc<Mutex<DuckStore>>,
    /// Broadcast of changes, for the SSE stream.
    pub changes: tokio::sync::broadcast::Sender<Change>,
    /// The token bucket on `/mcp`. Shared, because the thing it protects — the
    /// store's single write lock — is shared.
    pub rate_limit: Arc<crate::ratelimit::RateLimit>,
}

impl AppState {
    /// Open the store and build the shared state.
    pub fn open(home: &Path, embeddings: bool) -> Result<Self> {
        let mut store = DuckStore::open(home)
            .with_context(|| format!("open the store at {}", home.display()))?;

        if embeddings {
            // Constructed here rather than inside `keel-core`, which must not
            // decide whether to touch the network or where model files live.
            let models = store.models_dir();
            std::fs::create_dir_all(&models).ok();
            match keel_core::FastEmbedder::new(&models) {
                Ok(e) => {
                    tracing::info!(dir = %models.display(), "embedding model loaded");
                    store = store.with_embedder(Arc::new(e));
                }
                // Degrade rather than refuse to start. Keyword search still
                // works, and a daemon that will not boot because a model
                // download failed is worse than one with weaker search.
                Err(e) => tracing::warn!(
                    error = %e,
                    "could not load the embedding model; semantic search is disabled and \
                     search falls back to keyword only"
                ),
            }
        }

        let (changes, _) = tokio::sync::broadcast::channel(256);
        Ok(AppState {
            store: Arc::new(Mutex::new(store)),
            changes,
            rate_limit: Arc::new(crate::ratelimit::RateLimit::default()),
        })
    }

    /// Build state around an already-open store. Used by tests.
    pub fn from_store(store: DuckStore) -> Self {
        let (changes, _) = tokio::sync::broadcast::channel(256);
        AppState {
            store: Arc::new(Mutex::new(store)),
            changes,
            rate_limit: Arc::new(crate::ratelimit::RateLimit::default()),
        }
    }

    /// Take the write handle.
    ///
    /// Recovers from a poisoned lock rather than propagating the panic. A
    /// poisoned mutex means some earlier request panicked mid-handler; the
    /// store itself is a database with its own transactional guarantees, so
    /// refusing every subsequent request would turn one bad request into an
    /// outage.
    pub fn store(&self) -> MutexGuard<'_, DuckStore> {
        match self.store.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!(
                    "the store lock was poisoned by an earlier panic; continuing, since \
                     DuckDB's own transactions are what actually protect the data"
                );
                poisoned.into_inner()
            }
        }
    }

    /// Announce a change to any SSE subscribers.
    ///
    /// Errors are ignored on purpose: `send` fails only when nobody is
    /// listening, which is the normal case when no desktop app is open.
    pub fn announce(&self, event_id: EventId, summary: impl Into<String>) {
        let _ = self.changes.send(Change {
            kind: ChangeKind::Entity,
            event_id: Some(event_id),
            entity_id: None,
            summary: summary.into(),
        });
    }

    /// Announce a note, which writes no event row of its own.
    pub fn announce_note(&self, entity_id: Option<String>, summary: impl Into<String>) {
        let _ = self.changes.send(Change {
            kind: ChangeKind::Note,
            event_id: None,
            entity_id,
            summary: summary.into(),
        });
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("subscribers", &self.changes.receiver_count())
            .finish()
    }
}
