//! Shared daemon state: the store, the lock, and the change broadcast.

use anyhow::{Context, Result};
use keel_core::{EventId, Store};
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
    store: Arc<Mutex<Store>>,
    /// Broadcast of changes, for the SSE stream.
    pub changes: tokio::sync::broadcast::Sender<Change>,
    /// The token bucket on `/mcp`. Shared, because the thing it protects — the
    /// store's single write lock — is shared.
    pub rate_limit: Arc<crate::ratelimit::RateLimit>,
    /// The last project count anybody read, so `/api/health` can answer
    /// without taking the store lock. `-1` means nobody has read one yet.
    projects: Arc<std::sync::atomic::AtomicI64>,
}

impl AppState {
    /// Open the store and build the shared state.
    ///
    /// `home` is the directory — `~/.keel` — and the store is one file inside
    /// it. The distinction is worth the sentence because the store used to *be*
    /// the directory, and a caller that passes one where the other is wanted
    /// silently opens an empty store rather than failing.
    pub fn open(home: &Path, embeddings: bool) -> Result<Self> {
        let path = keel_core::store_path(home);
        let mut store =
            Store::open(&path).with_context(|| format!("open the store at {}", path.display()))?;

        if embeddings {
            // Constructed here rather than inside `keel-core`, which must not
            // decide whether to touch the network or where model files live.
            // The directory is derived from `home` rather than asked of the
            // store, because the store is now a file and has no directory of
            // its own to hang a models cache off.
            let models = home.join("models");
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
            projects: Arc::new(std::sync::atomic::AtomicI64::new(-1)),
        })
    }

    /// Build state around an already-open store. Used by tests.
    pub fn from_store(store: Store) -> Self {
        let (changes, _) = tokio::sync::broadcast::channel(256);
        AppState {
            store: Arc::new(Mutex::new(store)),
            changes,
            rate_limit: Arc::new(crate::ratelimit::RateLimit::default()),
            projects: Arc::new(std::sync::atomic::AtomicI64::new(-1)),
        }
    }

    /// Take the write handle *if it is free*, and never wait for it.
    ///
    /// Exists for `/api/health`, which is the probe the CLI uses to decide
    /// whether a daemon is alive. Health taking the store lock made that probe
    /// block exactly when the daemon was busy — so the one question worth
    /// asking during a slow write was the one question that could not be
    /// answered during a slow write.
    pub fn try_store(&self) -> Option<MutexGuard<'_, Store>> {
        match self.store.try_lock() {
            Ok(guard) => Some(guard),
            Err(std::sync::TryLockError::WouldBlock) => None,
            Err(std::sync::TryLockError::Poisoned(p)) => Some(p.into_inner()),
        }
    }

    /// Remember how many projects the store held, so health can answer while
    /// the store is busy.
    pub fn remember_project_count(&self, n: usize) {
        self.projects
            .store(n as i64, std::sync::atomic::Ordering::Relaxed);
    }

    /// The last project count anybody observed, or `None` if nobody has.
    pub fn last_project_count(&self) -> Option<usize> {
        match self.projects.load(std::sync::atomic::Ordering::Relaxed) {
            n if n < 0 => None,
            n => Some(n as usize),
        }
    }

    /// Take the write handle, waiting for it if something else holds it.
    ///
    /// Recovers from a poisoned lock rather than propagating the panic. A
    /// poisoned mutex means some earlier request panicked mid-handler; the
    /// store itself is a database with its own transactional guarantees, so
    /// refusing every subsequent request would turn one bad request into an
    /// outage.
    pub fn store(&self) -> MutexGuard<'_, Store> {
        match self.store.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!(
                    "the store lock was poisoned by an earlier panic; continuing, since \
                     SQLite's own transactions are what actually protect the data"
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
