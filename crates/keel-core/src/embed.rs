//! Text embeddings.
//!
//! Local, via `fastembed` and `bge-small-en-v1.5` (D-7). Two honest caveats
//! against G8's "runs entirely locally", both stated in SPEC §5 and both still
//! true: the model is downloaded from the Hub on first run, and it executes
//! through ONNX Runtime, a C++ dependency. After that first run it is fully
//! offline.
//!
//! Embedding sits behind a trait for one practical reason and one design one.
//! Practical: a test suite that downloads 130 MB of model weights before it
//! can assert anything is a test suite people stop running. Design: `keel-core`
//! must not decide *whether* to embed — the caller passes an embedder in, or
//! does not, exactly as it passes in the store path.

use crate::{Error, Result};

/// Something that can turn text into vectors.
pub trait Embedder: Send + Sync {
    /// Embed a batch. The output has one vector per input, in order.
    ///
    /// Batched rather than one-at-a-time because re-embedding after a model
    /// change walks the whole corpus, and per-item inference would dominate.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// The model identifier stored on each document, so a later version bump
    /// can find stale rows instead of rewriting everything.
    fn model_name(&self) -> &str;

    /// The vector width. Must match the `documents.embedding` column.
    fn dimensions(&self) -> usize;

    /// Embed one text.
    fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let mut out = self.embed(std::slice::from_ref(&text.to_owned()))?;
        out.pop().ok_or_else(|| Error::Embedding {
            context: "embed a single text".to_owned(),
            reason: "the embedder returned no vectors".to_owned(),
        })
    }
}

/// The local `fastembed` embedder.
///
/// Construction downloads the model if it is not already cached, which is why
/// it is fallible and why `cache_dir` is a parameter — the store keeps its
/// models under `~/.keel/models`, not in a home-directory cache the user never
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
                crate::EMBEDDING_MODEL
            ),
            reason: e.to_string(),
        })?;

        Ok(FastEmbedder {
            inner: std::sync::Mutex::new(inner),
            model_name: crate::EMBEDDING_MODEL.to_owned(),
            dimensions: crate::EMBEDDING_DIM,
        })
    }
}

impl Embedder for FastEmbedder {
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

/// A deterministic embedder for tests, built from a hash of the text.
///
/// Not a stub that returns zeros: vectors derived from content mean that
/// similar text produces similar vectors often enough for a nearest-neighbour
/// assertion to be meaningful, while identical text always produces identical
/// vectors. That is enough to test the *plumbing* — that vectors are written,
/// stored at the right width, and reach the search path — without downloading
/// a model. It says nothing about retrieval quality, which is R-3's problem and
/// needs the real model and a real corpus.
#[derive(Debug, Clone)]
pub struct HashEmbedder {
    dimensions: usize,
}

impl Default for HashEmbedder {
    fn default() -> Self {
        HashEmbedder {
            dimensions: crate::EMBEDDING_DIM,
        }
    }
}

impl HashEmbedder {
    /// An embedder producing vectors of the store's declared width.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Embedder for HashEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        use sha2::{Digest, Sha256};
        Ok(texts
            .iter()
            .map(|text| {
                // One bucket per lowercased word, so texts sharing vocabulary
                // land near each other.
                let mut v = vec![0.0f32; self.dimensions];
                for word in text.split_whitespace() {
                    let digest = Sha256::digest(word.to_lowercase().as_bytes());
                    let bucket =
                        (usize::from(digest[0]) << 8 | usize::from(digest[1])) % self.dimensions;
                    v[bucket] += 1.0;
                }
                // L2-normalise, so cosine distance behaves.
                let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for x in &mut v {
                        *x /= norm;
                    }
                }
                v
            })
            .collect())
    }

    fn model_name(&self) -> &str {
        "test-hash-embedder"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    #[test]
    fn the_test_embedder_is_deterministic_and_correctly_sized() {
        let e = HashEmbedder::new();
        let a = e.embed_one("onboarding is slow").unwrap();
        let b = e.embed_one("onboarding is slow").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), crate::EMBEDDING_DIM);
    }

    #[test]
    fn similar_text_embeds_closer_than_unrelated_text() {
        let e = HashEmbedder::new();
        let base = e
            .embed_one("onboarding flow is confusing for new users")
            .unwrap();
        let similar = e
            .embed_one("new users find the onboarding flow confusing")
            .unwrap();
        let different = e.embed_one("rewrite the billing ledger in Rust").unwrap();

        assert!(
            cosine(&base, &similar) > cosine(&base, &different),
            "the test embedder must at least rank shared vocabulary higher, \
             or search tests built on it prove nothing"
        );
    }

    #[test]
    fn batches_preserve_order() {
        let e = HashEmbedder::new();
        let texts = vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()];
        let batch = e.embed(&texts).unwrap();
        assert_eq!(batch.len(), 3);
        for (i, text) in texts.iter().enumerate() {
            assert_eq!(
                batch[i],
                e.embed_one(text).unwrap(),
                "batch item {i} is out of order"
            );
        }
    }

    #[test]
    fn an_empty_batch_is_not_an_error() {
        assert!(HashEmbedder::new().embed(&[]).unwrap().is_empty());
    }
}
