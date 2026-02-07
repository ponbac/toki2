//! Mock embedder implementation for testing.

use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::domain::search::traits::{Embedder, Result};

/// Mock embedder that returns configurable vectors.
///
/// # Examples
///
/// ```
/// use toki_api::domain::search::embedder::MockEmbedder;
///
/// // Return a fixed vector
/// let embedder = MockEmbedder::returning(vec![0.1; 1536]);
///
/// // Return different vectors for each call
/// let embedder = MockEmbedder::with_sequence(vec![
///     vec![0.1; 1536],
///     vec![0.2; 1536],
/// ]);
/// ```
#[derive(Clone)]
pub struct MockEmbedder {
    responses: Arc<Vec<Vec<f32>>>,
    call_count: Arc<AtomicUsize>,
    dimensions: usize,
}

impl MockEmbedder {
    /// Create a mock that always returns the same vector.
    pub fn returning(vector: Vec<f32>) -> Self {
        let dims = vector.len();
        Self {
            responses: Arc::new(vec![vector]),
            call_count: Arc::new(AtomicUsize::new(0)),
            dimensions: dims,
        }
    }

    /// Create a mock that returns vectors in sequence.
    ///
    /// Wraps around if more calls are made than vectors provided.
    pub fn with_sequence(vectors: Vec<Vec<f32>>) -> Self {
        let dims = vectors.first().map(|v| v.len()).unwrap_or(1536);
        Self {
            responses: Arc::new(vectors),
            call_count: Arc::new(AtomicUsize::new(0)),
            dimensions: dims,
        }
    }

    /// Create a mock with default 1536-dimensional zero vectors.
    pub fn default_dims() -> Self {
        Self::returning(vec![0.0; 1536])
    }

    /// Get the number of times `embed` or `embed_batch` was called.
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    /// Reset the call counter.
    pub fn reset(&self) {
        self.call_count.store(0, Ordering::SeqCst);
    }
}

impl Default for MockEmbedder {
    fn default() -> Self {
        Self::default_dims()
    }
}

#[async_trait]
impl Embedder for MockEmbedder {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
        let response_idx = idx % self.responses.len();
        Ok(self.responses[response_idx].clone())
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for _ in texts {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let response_idx = idx % self.responses.len();
            results.push(self.responses[response_idx].clone());
        }
        Ok(results)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_fixed_vector() {
        let embedder = MockEmbedder::returning(vec![1.0, 2.0, 3.0]);

        let result = embedder.embed("test").await.unwrap();
        assert_eq!(result, vec![1.0, 2.0, 3.0]);

        let result = embedder.embed("another").await.unwrap();
        assert_eq!(result, vec![1.0, 2.0, 3.0]);
    }

    #[tokio::test]
    async fn mock_returns_sequence() {
        let embedder = MockEmbedder::with_sequence(vec![
            vec![1.0],
            vec![2.0],
            vec![3.0],
        ]);

        assert_eq!(embedder.embed("a").await.unwrap(), vec![1.0]);
        assert_eq!(embedder.embed("b").await.unwrap(), vec![2.0]);
        assert_eq!(embedder.embed("c").await.unwrap(), vec![3.0]);
        // Wraps around
        assert_eq!(embedder.embed("d").await.unwrap(), vec![1.0]);
    }

    #[tokio::test]
    async fn mock_tracks_call_count() {
        let embedder = MockEmbedder::default();

        assert_eq!(embedder.call_count(), 0);
        embedder.embed("a").await.unwrap();
        assert_eq!(embedder.call_count(), 1);
        embedder.embed("b").await.unwrap();
        assert_eq!(embedder.call_count(), 2);

        embedder.reset();
        assert_eq!(embedder.call_count(), 0);
    }

    #[tokio::test]
    async fn mock_batch_increments_count_per_item() {
        let embedder = MockEmbedder::default();

        embedder.embed_batch(&["a", "b", "c"]).await.unwrap();
        assert_eq!(embedder.call_count(), 3);
    }
}
