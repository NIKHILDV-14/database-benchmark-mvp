use anyhow::Result;
use async_trait::async_trait;
use common::types::{EmbeddingModelInfo, EmbeddingResult};

/// Abstract provider for text embeddings.
/// This trait ensures the benchmarking engine is decoupled from specific embedding services
/// (like OpenAI, HuggingFace, local ONNX, etc.).
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string into a dense vector.
    async fn embed_text(&self, text: &str) -> Result<EmbeddingResult>;

    /// Embed a batch of text strings into a list of dense vectors.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<EmbeddingResult>>;

    /// Return the dimensionality of the embeddings produced by this provider.
    fn dimension(&self) -> usize;

    /// Return a human-readable name for the provider.
    fn name(&self) -> &str;

    /// Return metadata about the underlying model being used.
    fn model_info(&self) -> EmbeddingModelInfo;
}
