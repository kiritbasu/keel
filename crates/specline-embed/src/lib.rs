//! The local embedding model.
//!
//! Split out of `specline-core` because that crate's contract says it never opens a
//! network socket, and this downloads `bge-small-en-v1.5` from the Hub on first
//! use (D-7). Two honest caveats against G8's "runs entirely locally", both
//! stated in SPEC §5 and both still true: that first download needs network
//! access, and inference runs through ONNX Runtime, a C++ dependency. After the
//! first run it is fully offline.
//!
//! The split is not tidiness. Everything linking `specline-core` used to pull in
//! ONNX Runtime whether or not it ever embedded anything — the CLI, the MCP
//! crate, every test binary. Only the daemon and `specline reembed` construct one.
//!
//! [`specline_core::Embedder`] and the test `HashEmbedder` stay in core, because
//! the trait is what the store talks to and the hash one has no dependencies at
//! all.

use specline_core::{Error, Result};

/// The local `fastembed` embedder.
///
/// Construction downloads the model if it is not already cached, which is why
/// it is fallible and why `cache_dir` is a parameter — the store keeps its
/// models under `~/.specline/models`, not in a home-directory cache the user never
/// chose (SPEC §11).
pub struct FastEmbedder {
    // `TextEmbedding::embed` takes `&mut self`, but the trait exposes `&self`
    // so callers can share one embedder. A mutex is the honest way to bridge
    // that: inference is CPU-bound and serialised anyway, and the daemon owns
    // the single write path, so there is nothing to contend with.
    inner: std::sync::Mutex<fastembed::TextEmbedding>,
    model_name: String,
    dimensions: usize,
}

impl std::fmt::Debug for FastEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FastEmbedder")
            .field("model_name", &self.model_name)
            .field("dimensions", &self.dimensions)
            .finish()
    }
}

impl FastEmbedder {
    /// Load the model, downloading it into `cache_dir` if absent.
    ///
    /// The first call on a fresh machine needs network access. That is the
    /// single point at which G8's "no cloud dependency" is not literally true,
    /// and it is worth surfacing in the error rather than letting it read as a
    /// mysterious failure.
    pub fn new(cache_dir: impl Into<std::path::PathBuf>) -> Result<Self> {
        let options = fastembed::InitOptions::new(fastembed::EmbeddingModel::BGESmallENV15)
            .with_cache_dir(cache_dir.into())
            .with_show_download_progress(false);

        let inner = fastembed::TextEmbedding::try_new(options).map_err(|e| Error::Embedding {
            context: format!(
                "load the `{}` embedding model. The first run downloads it, which needs \
                 network access; afterwards it is fully offline",
                specline_core::EMBEDDING_MODEL
            ),
            reason: e.to_string(),
        })?;

        Ok(FastEmbedder {
            inner: std::sync::Mutex::new(inner),
            model_name: specline_core::EMBEDDING_MODEL.to_owned(),
            dimensions: specline_core::EMBEDDING_DIM,
        })
    }
}

impl specline_core::Embedder for FastEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let vectors = guard.embed(texts, None).map_err(|e| Error::Embedding {
            context: format!("embed {} document(s)", texts.len()),
            reason: e.to_string(),
        })?;

        if let Some(bad) = vectors.iter().find(|v| v.len() != self.dimensions) {
            return Err(Error::Embedding {
                context: "embed documents".to_owned(),
                reason: format!(
                    "the model returned a {}-dimensional vector but the documents table \
                     expects {}. The model and the schema have diverged",
                    bad.len(),
                    self.dimensions
                ),
            });
        }
        Ok(vectors)
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}
