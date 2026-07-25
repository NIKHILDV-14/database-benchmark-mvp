use crate::provider::EmbeddingProvider;
use anyhow::Result;
use async_trait::async_trait;
use common::types::{EmbeddingModelInfo, EmbeddingProviderType, EmbeddingResult};

/// A dummy embedding provider used strictly for testing.
/// It produces deterministic vectors by mapping text characters to floats.
pub struct DummyEmbeddingProvider {
    pub dimension: usize,
}

impl DummyEmbeddingProvider {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

#[async_trait]
impl EmbeddingProvider for DummyEmbeddingProvider {
    async fn embed_text(&self, text: &str) -> Result<EmbeddingResult> {
        // Deterministic generation: sum of character codes modulo some prime
        let mut vector = vec![0.0; self.dimension];
        let bytes = text.as_bytes();

        for (i, &b) in bytes.iter().enumerate() {
            let idx = i % self.dimension;
            vector[idx] += (b as f32) / 255.0;
        }

        // Normalize if not all zeros
        let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in vector.iter_mut() {
                *v /= norm;
            }
        }

        Ok(EmbeddingResult { vector })
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<EmbeddingResult>> {
        let mut results = Vec::with_capacity(texts.len());
        for &text in texts {
            results.push(self.embed_text(text).await?);
        }
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn name(&self) -> &str {
        "DummyProvider"
    }

    fn model_info(&self) -> EmbeddingModelInfo {
        EmbeddingModelInfo {
            provider: EmbeddingProviderType::Dummy,
            name: "dummy-v1".to_string(),
            dimension: self.dimension,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dummy_provider_deterministic() {
        let provider = DummyEmbeddingProvider::new(128);

        let res1 = provider.embed_text("hello world").await.unwrap();
        let res2 = provider.embed_text("hello world").await.unwrap();

        assert_eq!(res1.vector, res2.vector);
        assert_eq!(res1.vector.len(), 128);
    }

    #[tokio::test]
    async fn test_dummy_provider_batch() {
        let provider = DummyEmbeddingProvider::new(64);

        let batch = provider.embed_batch(&["test 1", "test 2"]).await.unwrap();

        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].vector.len(), 64);
        assert_eq!(batch[1].vector.len(), 64);
        assert_ne!(batch[0].vector, batch[1].vector); // Different inputs, different output
    }
}
