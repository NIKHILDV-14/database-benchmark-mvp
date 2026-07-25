use crate::builder::VectorRecordBuilder;
use crate::cache::CacheManager;
use crate::chunk::Chunk;
use crate::collector::OrderedCollector;
use crate::manager::ModelManager;
use crate::metrics::EmbeddingMetrics;
use crate::retry::{with_retry, RetryPolicy};
use crate::scheduler::BatchScheduler;
use crate::validator::EmbeddingValidator;
use anyhow::{Context, Result};
use common::types::VectorRecord;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::time::Instant;

pub struct EmbeddingOrchestrator {
    manager: ModelManager,
    cache: CacheManager,
    metrics: Arc<EmbeddingMetrics>,
    retry_policy: RetryPolicy,
}

impl EmbeddingOrchestrator {
    pub fn new(
        manager: ModelManager,
        cache: CacheManager,
        metrics: Arc<EmbeddingMetrics>,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self {
            manager,
            cache,
            metrics,
            retry_policy,
        }
    }

    pub async fn process(
        &self,
        chunks: Vec<Chunk>,
        provider_id: &str,
        batch_size: usize,
        use_cache: bool,
    ) -> Result<Vec<VectorRecord>> {
        let provider = self.manager.get_provider(provider_id)?;
        let provider_version = provider.model_info().name;
        let dimension = provider.dimension();

        let scheduler = BatchScheduler::new(batch_size);
        let validator = EmbeddingValidator::new(dimension);
        let mut collector = OrderedCollector::new();

        self.metrics
            .chunks_processed
            .fetch_add(chunks.len(), Ordering::Relaxed);

        let batches = scheduler.schedule(&chunks);

        for batch in batches {
            let start = Instant::now();
            let mut texts_to_embed = Vec::new();
            let mut chunks_to_embed = Vec::new();
            let mut cached_results = Vec::new();

            for chunk in batch {
                let cache_key = CacheManager::compute_key(
                    &chunk.text,
                    provider_id,
                    &provider_version,
                    dimension,
                );

                if use_cache {
                    if let Some(cached) = self.cache.lookup(&cache_key) {
                        cached_results.push((chunk.clone(), cached));
                        continue;
                    }
                }

                texts_to_embed.push(chunk.text.as_str());
                chunks_to_embed.push((chunk.clone(), cache_key));
            }

            if !texts_to_embed.is_empty() {
                self.metrics.provider_calls.fetch_add(1, Ordering::Relaxed);

                let provider_clone = provider.clone();
                let result = with_retry(&self.retry_policy, || {
                    let p = provider_clone.clone();
                    let t = texts_to_embed.clone();
                    async move { p.embed_batch(&t).await }
                })
                .await;

                let results = match result {
                    Ok(r) => r,
                    Err(e) => {
                        self.metrics.failed_requests.fetch_add(1, Ordering::Relaxed);
                        return Err(e).context("Provider embedding failed");
                    }
                };

                validator
                    .validate_batch(&results)
                    .context("Validation failed")?;

                for ((chunk, cache_key), res) in chunks_to_embed.into_iter().zip(results) {
                    if use_cache {
                        self.cache.store(&cache_key, res.clone());
                    }
                    cached_results.push((chunk, res));
                }
            }

            let mut batch_chunks = Vec::new();
            let mut batch_embeddings = Vec::new();

            for (chunk, res) in cached_results {
                batch_chunks.push(chunk);
                batch_embeddings.push(res);
            }

            collector.add_batch(&batch_chunks, batch_embeddings);

            self.metrics.record_batch(start.elapsed());
        }

        let ordered_results = collector.take_ordered(&chunks);

        let records = ordered_results
            .into_iter()
            .map(|(chunk, emb)| VectorRecordBuilder::build(chunk, emb))
            .collect();

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache_backend::MemoryCacheBackend;
    use crate::manager::create_default_manager;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_orchestrator_process() {
        let manager = create_default_manager(128);
        let cache = CacheManager::new(Arc::new(MemoryCacheBackend::new()));
        let metrics = Arc::new(EmbeddingMetrics::new());
        let retry = RetryPolicy::default();

        let orchestrator = EmbeddingOrchestrator::new(manager, cache, metrics.clone(), retry);

        let chunk1 = Chunk {
            id: "doc1_0".to_string(),
            document_id: "doc1".to_string(),
            text: "hello world".to_string(),
            metadata: HashMap::new(),
            chunk_index: 0,
        };
        let chunk2 = Chunk {
            id: "doc2_0".to_string(),
            document_id: "doc2".to_string(),
            text: "test".to_string(),
            metadata: HashMap::new(),
            chunk_index: 0,
        };

        let records = orchestrator
            .process(vec![chunk1, chunk2], "dummy", 2, true)
            .await
            .unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, "doc1_0");
        assert_eq!(records[1].id, "doc2_0");
        assert_eq!(records[0].embedding.len(), 128);
        assert_eq!(metrics.chunks_processed.load(Ordering::SeqCst), 2);
        assert_eq!(metrics.provider_calls.load(Ordering::SeqCst), 1);
    }
}
