//! Shared daemon state: the store, the lock, and the change broadcast.

use anyhow::{Context, Result};
use specline_core::{EventId, Store};
use std::path::{Path, PathBuf};
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
    /// Which store this daemon is holding, canonicalised.
    ///
    /// Reported by `/api/health` so another process can ask "is the daemon that
    /// answered me holding *my* store?" rather than only "is a daemon running?".
    /// Every write command probes for a daemon and refuses if one answers, and
    /// until this existed the probe could not tell the difference: `specline fixture
    /// --home /tmp/scratch` was refused by a daemon serving `~/.specline`, which has
    /// no interest in `/tmp/scratch` and skips none of the write path for it
    /// (KEEL-194).
    ///
    /// Canonicalised once at startup rather than per request, because the
    /// comparison is only sound if both sides resolve symlinks the same way and
    /// `/tmp` is a symlink to `/private/tmp` on macOS — which is exactly the
    /// kind of difference that would make two names for one store look like two
    /// stores.
    home: Arc<PathBuf>,
    /// The secret a mutating request has to carry, minted for this daemon's
    /// lifetime (KEEL-238).
    ///
    /// Held here rather than re-read per request: the file is the way it
    /// *reaches* other processes, not the authority. Reading it on every call
    /// would mean a request could be judged against a token some other process
    /// had just written, which is the one thing the check must not depend on.
    token: Arc<String>,
}

impl AppState {
    /// Open the store and build the shared state.
    ///
    /// `home` is the directory — `~/.specline` — and the store is one file inside
    /// it. The distinction is worth the sentence because the store used to *be*
    /// the directory, and a caller that passes one where the other is wanted
    /// silently opens an empty store rather than failing.
    pub fn open(home: &Path, embeddings: bool) -> Result<Self> {
        let path = specline_core::store_path(home);
        // The daemon is the process that owns the store, so it is the process
        // that migrates it. Every other caller uses `Store::open` and is told
        // to run `specline migrate` — see the doc comment there for why applying a
        // schema change from whatever command opened the store next is the
        // failure worth this asymmetry.
        // Exclusive, and held for as long as the daemon runs (B-60). A second
        // daemon against the same store is refused here rather than discovered
        // later — which is how 2026-08-13 went: one started with `--home`
        // forgotten, migrated the store under the daemon already serving it,
        // and nothing objected.
        let store = Store::open_and_migrate_exclusive(&path)
            .with_context(|| format!("open the store at {}", path.display()))?;

        // Ask SQLite whether the file is intact, once, at startup.
        //
        // `quick_check` rather than `integrity_check`: it skips index
        // verification, which is most of the cost and rarely the thing that is
        // wrong. The full check is what `specline fsck` runs.
        //
        // A loud log rather than a refusal to start. A damaged page is
        // sometimes in a table nothing reads, and a daemon that will not boot
        // leaves the user with no way to run a backup or export what survives —
        // which turns a recoverable problem into an unrecoverable one.
        match specline_core::fsck::page_integrity(&store, "quick_check") {
            Ok(None) => {}
            Ok(Some(problems)) => tracing::error!(
                store = %path.display(),
                %problems,
                "THE STORE FILE IS DAMAGED. Reads may return wrong answers rather than errors. \
                 Restore from a backup (`specline restore`) and check whether ~/.specline is inside a \
                 Dropbox, iCloud or network folder — copying the .sqlite, -wal and -shm files \
                 at different moments is the usual cause."
            ),
            // The check itself failing is not the same as the store failing it,
            // and saying so is the difference between "your data is damaged"
            // and "I could not tell".
            Err(e) => tracing::warn!(error = %e, "could not run the startup integrity check"),
        }

        // Two conditions that make search quietly worse, said out loud at the
        // one moment somebody is watching the logs.
        //
        // Neither of these fails a query. Search keeps returning keyword hits,
        // which is why a store went months with 227 unembedded documents and
        // nothing anywhere said so: results kept arriving and were merely
        // worse. That is the exact silence this project is built to refuse.
        if !store.vector_search_available() {
            tracing::warn!(
                "sqlite-vec did not register, so `vec_distance_cosine` is unavailable and \
                 search is keyword-only. This is a build problem, not a data problem — \
                 results will keep arriving and will simply be worse."
            );
        }
        match store.documents_missing_embeddings(None) {
            Ok((current, missing)) if missing > 0 => tracing::warn!(
                current,
                missing,
                "{missing} of {current} current document(s) have no vector, so the semantic \
                 half of search cannot see them. Run `specline reembed --missing`."
            ),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "could not count the missing embeddings"),
        }

        let (changes, _) = tokio::sync::broadcast::channel(256);
        let state = AppState {
            store: Arc::new(Mutex::new(store)),
            changes,
            rate_limit: Arc::new(crate::ratelimit::RateLimit::default()),
            projects: Arc::new(std::sync::atomic::AtomicI64::new(-1)),
            // Falling back to the path as given is right rather than merely
            // convenient: a home that cannot be canonicalised is one a
            // comparison should fail on, and reporting the uncanonicalised
            // path makes that failure visible instead of reporting nothing.
            home: Arc::new(home.canonicalize().unwrap_or_else(|_| home.to_path_buf())),
            // Minted at startup, into a file only this user can read. A daemon
            // that cannot write it is a daemon whose mutating endpoints would
            // be unguarded, so this is a refusal to start rather than a warning
            // — the failure mode being avoided is precisely a security check
            // that is absent while everything looks healthy.
            token: Arc::new(specline_core::token::mint(home).with_context(|| {
                format!(
                    "mint the API token in {}. Without it no caller could be told apart from a \
                     web page, so the daemon will not serve unguarded",
                    home.display()
                )
            })?),
        };

        if embeddings {
            #[cfg(feature = "embeddings")]
            state.load_embedder_in_background(home.join("models"));

            // Asked for something this binary cannot do. Said once, loudly, at
            // startup: the alternative is a daemon that accepts `--embeddings`,
            // reports a version, and quietly answers every search with keyword
            // hits — which is indistinguishable from one where the model simply
            // has not finished loading.
            #[cfg(not(feature = "embeddings"))]
            tracing::warn!(
                "--embeddings was asked for and this build has no embedding model compiled in, \
                 so search stays keyword-only. `specline doctor` reports this, and the arm64 macOS \
                 release is the build that carries one"
            );
        }

        Ok(state)
    }

    /// Load the embedding model on another thread and attach it when it is
    /// ready.
    ///
    /// It used to load inline, before the socket was bound, so the first run on
    /// a fresh machine left the daemon unreachable for the length of a 130 MB
    /// download — which reads as a broken install, not a slow one. Nothing
    /// needs the model to answer a request: search without it returns keyword
    /// hits, which is what it will do for the first minute either way.
    ///
    /// Constructed here rather than inside `specline-core`, which must not decide
    /// whether to touch the network or where model files live. The directory is
    /// derived from `home` rather than asked of the store, because the store is
    /// a file now and has no directory of its own to hang a cache off.
    #[cfg(feature = "embeddings")]
    fn load_embedder_in_background(&self, models: std::path::PathBuf) {
        let store = self.store.clone();
        std::thread::spawn(move || {
            std::fs::create_dir_all(&models).ok();
            match specline_embed::FastEmbedder::new(&models) {
                Ok(e) => {
                    // Taking the lock here is safe and brief: it is one field
                    // assignment, and by now the daemon is already serving.
                    match store.lock() {
                        Ok(mut guard) => {
                            guard.set_embedder(Arc::new(e));
                            tracing::info!(
                                dir = %models.display(),
                                "embedding model loaded; semantic search is live"
                            );
                        }
                        Err(poisoned) => {
                            poisoned.into_inner().set_embedder(Arc::new(e));
                            tracing::info!("embedding model loaded after a poisoned lock");
                        }
                    }
                }
                // Degrade rather than take the daemon down. Keyword search
                // still works, and a daemon that dies because a model download
                // failed is worse than one with weaker search.
                Err(e) => tracing::warn!(
                    error = %e,
                    "could not load the embedding model; semantic search stays off and search \
                     falls back to keyword only"
                ),
            }
        });
    }

    /// Turn a search tool call's query text into a vector, before the store
    /// lock is taken.
    ///
    /// Embedding is model inference — tens of milliseconds of CPU — and it used
    /// to happen inside `search`, which happens inside the critical section
    /// that every other request is queued behind. Nothing about it needs the
    /// store; it needs the model, and the model is reachable without waiting.
    ///
    /// `None` in three cases, all of which fall back to the old behaviour and
    /// none of which is an error: the call is not a search, there is no query
    /// text to embed, or the store is busy right now — the handle lives on the
    /// store, so getting it means a lock, and this one will not wait for one.
    /// A search that arrives during a slow write embeds inside the lock as it
    /// always did, which is no worse than before and far better than blocking
    /// here to avoid blocking later.
    pub fn embed_query(&self, tool: &str, arguments: &serde_json::Value) -> Option<Vec<f32>> {
        if tool != "keel_search" {
            return None;
        }
        let text = arguments.get("query").and_then(|v| v.as_str())?;
        if text.trim().is_empty() {
            return None;
        }
        let embedder = self.try_store()?.embedder_handle()?;
        match embedder.embed_one(text) {
            Ok(vector) => Some(vector),
            // The store would have hit the same failure and handled it the same
            // way — semantic half empty, keyword half answers. Logged rather
            // than swallowed, because a search that has quietly stopped being
            // hybrid is this codebase's defining failure.
            Err(e) => {
                tracing::warn!(error = %e, "could not embed the search query ahead of the lock");
                None
            }
        }
    }

    /// Build state around an already-open store. Used by tests.
    ///
    /// The home reported here is empty rather than invented. A test store is
    /// usually a `tempfile` handle with no directory anyone would compare
    /// against, and a plausible-looking fake path is worse than an obviously
    /// absent one: the whole point of reporting a home is that another process
    /// can trust the answer.
    pub fn from_store(store: Store) -> Self {
        Self::from_store_with_token(store, "test-token")
    }

    /// The same, with a chosen token, for tests about the guard itself.
    ///
    /// A fixed value rather than a minted one: a test that has to read a file
    /// to learn what it should send is testing the filesystem, and the property
    /// under test is "the header has to match", not "the token is unguessable".
    /// That second property is [`specline_core::token`]'s and is tested there.
    pub fn from_store_with_token(store: Store, token: &str) -> Self {
        let (changes, _) = tokio::sync::broadcast::channel(256);
        AppState {
            store: Arc::new(Mutex::new(store)),
            changes,
            rate_limit: Arc::new(crate::ratelimit::RateLimit::default()),
            projects: Arc::new(std::sync::atomic::AtomicI64::new(-1)),
            home: Arc::new(PathBuf::new()),
            token: Arc::new(token.to_owned()),
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

    /// The store this daemon holds, canonicalised at startup.
    ///
    /// The answer to "whose store is this?", which is the question every write
    /// command's daemon probe should have been asking and was not.
    pub fn home(&self) -> &Path {
        self.home.as_path()
    }

    /// The token this daemon will accept on a mutating request.
    pub fn token(&self) -> &str {
        self.token.as_str()
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
