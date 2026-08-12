//! Text embeddings: the trait, and a deterministic one for tests.
//!
//! The real model lives in `keel-embed`, not here, and the reason is the
//! contract at the top of `keel-core`: this crate never opens a network socket.
//! `FastEmbedder` downloads 130 MB from the Hub on first use, so it sat inside
//! a crate whose whole claim was that it did not do that — and every binary
//! linking `keel-core` dragged in the ONNX Runtime C++ dependency to get it,
//! including the CLI and the test suite, neither of which ever embeds anything.
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
