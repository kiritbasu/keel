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
    /// The event that caused it.
    pub event_id: EventId,
    /// What happened, in one line.
    pub summary: String,
}

/// Everything a request handler needs.
#[derive(Clone)]
pub struct AppState {
    store: Arc<Mutex<DuckStore>>,
    /// Broadcast of changes, for the SSE stream.
    pub changes: tokio::sync::broadcast::Sender<Change>,
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
        })
    }

    /// Build state around an already-open store. Used by tests.
    pub fn from_store(store: DuckStore) -> Self {
        let (changes, _) = tokio::sync::broadcast::channel(256);
        AppState {
            store: Arc::new(Mutex::new(store)),
            changes,
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
            event_id,
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
