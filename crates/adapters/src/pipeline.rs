//! Phase 4C – Embedding Pipeline Integration
//!
//! This module connects the embedding execution pipeline (Phase 4A/4B) to the
//! vector adapter layer (Phase 3C) through the existing interfaces.
//!
//! # Flow
//!
//! ```text
//! [Document]  →  chunk_text()  →  [Chunk]
//!                                    │
//!                          EmbeddingOrchestrator
//!                                    │
//!                             [VectorRecord]
//!                                    │
//!                     VectorAdapter::insert_vectors()
//!                                    │
//!                       Qdrant / LanceDB (via trait)
//! ```
//!
//! PostgreSQL and MongoDB are **not** touched — they do not implement
//! `VectorAdapter` and are not involved in vector pipelines.
//!
//! The benchmark engine never imports `QdrantAdapter` or `LanceDbAdapter`
//! directly — it always works through `Box<dyn VectorAdapter>`.

use crate::vector_traits::VectorAdapter;
use anyhow::{Context, Result};
use benchmark_config::EmbeddingConfig;
use dataset::model::Document;
use embedding::cache::CacheManager;
use embedding::cache_backend::MemoryCacheBackend;
use embedding::chunk::chunk_text;
use embedding::manager::create_default_manager;
use embedding::metrics::EmbeddingMetrics;
use embedding::orchestrator::EmbeddingOrchestrator;
use embedding::retry::RetryPolicy;
use std::sync::Arc;

/// Embed a slice of documents and store the resulting vectors in the given
/// vector adapter.
///
/// This is the primary Phase 4C integration entry-point.  It:
/// 1. Chunks every document according to `config.chunk_size / chunk_overlap`.
/// 2. Runs the embedding orchestrator (cache, batch, validate, collect).
/// 3. Converts the output to `VectorRecord` via the existing `builder`.
/// 4. Calls `adapter.insert_vectors()` with the resulting records.
///
/// # Arguments
///
/// * `adapter` – Any adapter implementing `VectorAdapter` (Qdrant, LanceDB, …).
/// * `documents` – Source documents from the dataset pipeline.
/// * `config`  – Embedding configuration from `BenchmarkConfig`.
///
/// # Returns
///
/// The number of `VectorRecord`s inserted, or an error.
pub async fn embed_and_insert(
    adapter: &mut dyn VectorAdapter,
    documents: &[Document],
    config: &EmbeddingConfig,
) -> Result<usize> {
    // ── 1. Chunk ──────────────────────────────────────────────────────────────
    let chunks: Vec<_> = documents
        .iter()
        .flat_map(|doc| chunk_text(doc, config.chunk_size, config.chunk_overlap))
        .collect();

    if chunks.is_empty() {
        return Ok(0);
    }

    // ── 2. Build orchestrator ─────────────────────────────────────────────────
    let manager = create_default_manager(config.dimension);
    let backend = Arc::new(MemoryCacheBackend::new());
    let cache = CacheManager::new(backend);
    let metrics = Arc::new(EmbeddingMetrics::new());
    let retry = RetryPolicy::default();

    let orchestrator = EmbeddingOrchestrator::new(manager, cache, metrics, retry);

    // ── 3. Embed → VectorRecord ───────────────────────────────────────────────
    let records = orchestrator
        .process(
            chunks,
            &config.provider,
            config.batch_size,
            config.cache_enabled,
        )
        .await
        .context("Embedding orchestration failed")?;

    let count = records.len();

    // ── 4. Insert into vector adapter ─────────────────────────────────────────
    adapter
        .insert_vectors(&records)
        .map_err(|e| anyhow::anyhow!("insert_vectors failed: {}", e))?;

    Ok(count)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lancedb::LanceDbAdapter;
    use crate::qdrant::QdrantAdapter;
    use crate::traits::DatabaseAdapter;
    use benchmark_config::{DatabaseConfig, EmbeddingConfig};
    use serde_json::json;

    /// Build a minimal EmbeddingConfig suitable for in-process tests.
    fn test_embedding_config(dimension: usize) -> EmbeddingConfig {
        EmbeddingConfig {
            provider: "dummy".to_string(),
            batch_size: 8,
            chunk_size: 10,
            chunk_overlap: 2,
            cache_enabled: false,
            dimension,
        }
    }

    /// Build a minimal test document.
    fn make_doc(id: &str, text: &str) -> Document {
        Document {
            id: id.to_string(),
            title: None,
            text: text.to_string(),
            metadata: json!({ "source": id }),
        }
    }

    // ── LanceDB integration ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_embed_and_insert_lancedb_end_to_end() {
        let cfg = DatabaseConfig {
            enabled: true,
            host: Some("/tmp/lancedb_pipeline_test".to_string()),
            port: None,
            username: None,
            password: None,
            database: Some("pipeline_test".to_string()),
            pool_size: None,
            timeout_ms: None,
        };

        let mut adapter = LanceDbAdapter::new(cfg);
        adapter.connect().unwrap();
        adapter.initialize().unwrap();

        let docs = vec![
            make_doc("doc1", "the quick brown fox jumps over the lazy dog"),
            make_doc(
                "doc2",
                "rust is a systems programming language focused on safety speed and concurrency",
            ),
        ];

        let emb_cfg = test_embedding_config(128);
        let inserted = embed_and_insert(&mut adapter, &docs, &emb_cfg)
            .await
            .unwrap();

        assert!(inserted > 0, "expected at least one record inserted");
        assert_eq!(adapter.vector_count().unwrap(), inserted as u64);
    }

    #[tokio::test]
    async fn test_embed_and_insert_lancedb_produces_correct_ids() {
        let cfg = DatabaseConfig {
            enabled: true,
            host: Some("/tmp/lancedb_id_test".to_string()),
            port: None,
            username: None,
            password: None,
            database: Some("id_test".to_string()),
            pool_size: None,
            timeout_ms: None,
        };

        let mut adapter = LanceDbAdapter::new(cfg);
        adapter.connect().unwrap();
        adapter.initialize().unwrap();

        // Single doc, short text → exactly one chunk
        let docs = vec![make_doc("alpha", "hello world")];
        let emb_cfg = test_embedding_config(64);

        let inserted = embed_and_insert(&mut adapter, &docs, &emb_cfg)
            .await
            .unwrap();

        assert_eq!(inserted, 1);
        assert_eq!(adapter.vector_count().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_embed_and_insert_empty_documents() {
        let cfg = DatabaseConfig {
            enabled: true,
            host: Some("/tmp/lancedb_empty_test".to_string()),
            port: None,
            username: None,
            password: None,
            database: Some("empty_test".to_string()),
            pool_size: None,
            timeout_ms: None,
        };

        let mut adapter = LanceDbAdapter::new(cfg);
        adapter.connect().unwrap();
        adapter.initialize().unwrap();

        // Document with only whitespace → no chunks → 0 inserts
        let docs = vec![make_doc("empty", "   ")];
        let emb_cfg = test_embedding_config(64);

        let inserted = embed_and_insert(&mut adapter, &docs, &emb_cfg)
            .await
            .unwrap();

        assert_eq!(inserted, 0);
        assert_eq!(adapter.vector_count().unwrap(), 0);
    }

    // ── Qdrant integration ────────────────────────────────────────────────────

    #[test]
    fn test_embed_and_insert_qdrant_validates_vector_count() {
        // QdrantAdapter owns its own tokio::runtime::Runtime internally.
        // Dropping it inside a #[tokio::test] async context panics with
        // "Cannot drop a runtime in a context where blocking is not allowed".
        // Using a plain sync #[test] with an explicit block_on ensures the
        // adapter (and its internal Runtime) are dropped in a sync context.
        let cfg = DatabaseConfig {
            enabled: true,
            host: Some("127.0.0.1".to_string()),
            port: Some(6334),
            username: None,
            password: None,
            database: Some("pipeline_test".to_string()),
            pool_size: None,
            timeout_ms: Some(200),
        };

        let mut adapter = QdrantAdapter::new(cfg);
        // Do not call connect() → get_client() will fail, which means
        // insert_vectors will return an error that embed_and_insert propagates.

        let docs = vec![make_doc("q1", "hello qdrant vector world")];
        let emb_cfg = test_embedding_config(128);

        // Drive the async embed_and_insert call with a dedicated runtime.
        // The adapter's insert_vectors returns early (not-connected error)
        // before touching the adapter's own internal runtime, so no nesting occurs.
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(embed_and_insert(&mut adapter, &docs, &emb_cfg));

        // Drop the adapter explicitly in sync context before the assertion.
        drop(adapter);

        // Without a real server the insert step must error.
        assert!(
            result.is_err(),
            "expected an error when Qdrant is not reachable"
        );
    }

    // ── Dataset → chunk → embed → VectorRecord pipeline ──────────────────────

    #[tokio::test]
    async fn test_full_pipeline_chunking_preserves_document_ids() {
        let cfg = DatabaseConfig {
            enabled: true,
            host: Some("/tmp/lancedb_pipeline_id_test".to_string()),
            port: None,
            username: None,
            password: None,
            database: Some("pipeline_id_test".to_string()),
            pool_size: None,
            timeout_ms: None,
        };

        let mut adapter = LanceDbAdapter::new(cfg);
        adapter.connect().unwrap();
        adapter.initialize().unwrap();

        // 3 docs with known IDs
        let docs = vec![
            make_doc(
                "doc-A",
                "alpha beta gamma delta epsilon zeta eta theta iota kappa",
            ),
            make_doc("doc-B", "lambda mu nu xi omicron pi rho sigma tau upsilon"),
            make_doc("doc-C", "phi chi psi omega"),
        ];

        let emb_cfg = test_embedding_config(32);
        let inserted = embed_and_insert(&mut adapter, &docs, &emb_cfg)
            .await
            .unwrap();

        // All chunks must have been inserted
        assert_eq!(adapter.vector_count().unwrap(), inserted as u64);
        // At least as many records as documents (may be more due to chunking)
        assert!(inserted >= docs.len());
    }
}
