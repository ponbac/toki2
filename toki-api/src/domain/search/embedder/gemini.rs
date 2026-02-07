//! Gemini embedder implementation using the genai crate.

use async_trait::async_trait;
use genai::embed::EmbedOptions;

use crate::domain::search::traits::{Embedder, Result, SearchError};

/// Gemini embedding model configuration.
pub const GEMINI_MODEL: &str = "gemini-embedding-001";
pub const GEMINI_DIMENSIONS: usize = 1536;

/// Embedder implementation using Google's Gemini API via the `genai` crate.
///
/// The genai client automatically reads `GEMINI_API_KEY` from the environment.
///
/// # Example
///
/// ```ignore
/// let embedder = GeminiEmbedder::new()?;
/// let embedding = embedder.embed("authentication system").await?;
/// assert_eq!(embedding.len(), 1536);
/// ```
#[derive(Clone)]
pub struct GeminiEmbedder {
    client: genai::Client,
    model: String,
    options: EmbedOptions,
}

impl GeminiEmbedder {
    /// Create a new Gemini embedder with the default model.
    ///
    /// Returns an error if the genai client cannot be created.
    pub fn new() -> Result<Self> {
        Self::with_model(GEMINI_MODEL)
    }

    /// Create a new Gemini embedder with a specific model.
    pub fn with_model(model: impl Into<String>) -> Result<Self> {
        let client = genai::Client::default();
        let options = EmbedOptions::new().with_embedding_type("RETRIEVAL_QUERY");

        Ok(Self {
            client,
            model: model.into(),
            options,
        })
    }

    /// Try to create from environment variable.
    ///
    /// Returns `None` if `GEMINI_API_KEY` is not set, or `Some(Err)` if
    /// the client can't be created for another reason.
    pub fn try_from_env() -> Option<Result<Self>> {
        if std::env::var("GEMINI_API_KEY").is_err() {
            return None;
        }
        Some(Self::new())
    }
}

#[async_trait]
impl Embedder for GeminiEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        if text.is_empty() {
            return Ok(vec![0.0; GEMINI_DIMENSIONS]);
        }

        let response = self
            .client
            .embed(&self.model, text, Some(&self.options))
            .await
            .map_err(|e| SearchError::EmbeddingError(e.to_string()))?;

        let embedding = response
            .first_embedding()
            .ok_or_else(|| SearchError::EmbeddingError("No embedding in response".into()))?;

        Ok(embedding.vector().to_vec())
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // Filter empty strings and track their indices
        let mut results = vec![vec![0.0f32; GEMINI_DIMENSIONS]; texts.len()];
        let non_empty: Vec<(usize, String)> = texts
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_empty())
            .map(|(i, t)| (i, t.to_string()))
            .collect();

        if non_empty.is_empty() {
            return Ok(results);
        }

        let batch_texts: Vec<String> = non_empty.iter().map(|(_, t)| t.clone()).collect();

        let response = self
            .client
            .embed_batch(&self.model, batch_texts, Some(&self.options))
            .await
            .map_err(|e| SearchError::EmbeddingError(e.to_string()))?;

        for (batch_idx, (original_idx, _)) in non_empty.iter().enumerate() {
            if let Some(embedding) = response.embeddings.get(batch_idx) {
                results[*original_idx] = embedding.vector().to_vec();
            }
        }

        Ok(results)
    }

    fn dimensions(&self) -> usize {
        GEMINI_DIMENSIONS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedder_dimensions() {
        if std::env::var("GEMINI_API_KEY").is_err() {
            // Can't test without API key
            return;
        }
        let embedder = GeminiEmbedder::new().unwrap();
        assert_eq!(embedder.dimensions(), 1536);
    }

    #[tokio::test]
    async fn embed_empty_returns_zeros() {
        if std::env::var("GEMINI_API_KEY").is_err() {
            return;
        }
        let embedder = GeminiEmbedder::new().unwrap();
        let result = embedder.embed("").await.unwrap();
        assert_eq!(result.len(), GEMINI_DIMENSIONS);
        assert!(result.iter().all(|&x| x == 0.0));
    }
}
