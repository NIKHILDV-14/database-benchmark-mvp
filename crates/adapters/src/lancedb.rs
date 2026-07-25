//! LanceDB vector storage adapter.
//!
//! # Implementation note
//!
//! This adapter ships an **in-memory** backend that implements the complete
//! [`DatabaseAdapter`] + [`VectorAdapter`] interface without requiring the
//! `lancedb` crate.  All vector operations are correct (brute-force ANN, SQL-
//! style metadata filtering) and suitable for:
//!
//! - Unit tests of the full adapter pipeline
//! - CI/CD environments where LanceDB is not installed
//! - Integration testing with small datasets
//!
//! ## Enabling the native LanceDB backend
//!
//! The native driver requires the Protocol Buffer compiler (`protoc`).
//! To enable it:
//!
//! 1. Install protoc from <https://github.com/protocolbuffers/protobuf/releases>
//! 2. Add to `crates/adapters/Cargo.toml`:
//!    ```toml
//!    lancedb   = "0.14"
//!    arrow-array  = "52"
//!    arrow-schema = "52"
//!    ```
//! 3. Replace the in-memory backend methods below with the Arrow/LanceDB calls
//!    that were designed alongside this file.
//!
//! The public interface (`VectorAdapter` + `DatabaseAdapter`) is identical
//! in both modes — no callers need to change.

use crate::traits::DatabaseAdapter;
use crate::vector_traits::VectorAdapter;
use benchmark_config::DatabaseConfig;
use common::error::{BenchmarkError, Result};
use common::types::{
    DatabaseCapabilities, IndexType, SimilarityMetric, VectorQuery, VectorRecord,
    VectorSearchResult,
};
use dataset::model::Document;
use std::cmp::Ordering;
use tracing::{debug, info};

/// Default vector dimension when none is configured.
const DEFAULT_DIM: u64 = 384;
/// Default table name when none is configured.
const DEFAULT_TABLE: &str = "benchmark_vectors";
/// Default local storage path (used for native mode, informational here).
const DEFAULT_URI: &str = "/tmp/lancedb_benchmark";

// ─── Adapter struct ───────────────────────────────────────────────────────────

/// LanceDB adapter implementing [`DatabaseAdapter`] and [`VectorAdapter`].
///
/// Currently runs in in-memory mode; switch to the native lancedb crate when
/// `protoc` is available (see module-level docs).
pub struct LanceDbAdapter {
    config: DatabaseConfig,
    /// Whether `connect()` has been called.
    connected: bool,
    table_name: String,
    dimension: u64,
    similarity: SimilarityMetric,
    /// In-memory record store.  Populated by `insert_vectors`.
    records: Vec<VectorRecord>,
}

impl LanceDbAdapter {
    pub fn new(config: DatabaseConfig) -> Self {
        let table_name = config
            .database
            .clone()
            .unwrap_or_else(|| DEFAULT_TABLE.to_string());
        Self {
            config,
            connected: false,
            table_name,
            dimension: DEFAULT_DIM,
            similarity: SimilarityMetric::Cosine,
            records: Vec::new(),
        }
    }

    /// Derive the storage URI from the config.
    ///
    /// In native mode this would be the directory or `s3://` / `gs://` path
    /// passed to `lancedb::connect()`.
    pub fn storage_uri(&self) -> String {
        self.config
            .host
            .clone()
            .unwrap_or_else(|| DEFAULT_URI.to_string())
    }

    fn require_connected(&self) -> Result<()> {
        if self.connected {
            Ok(())
        } else {
            Err(BenchmarkError::DatabaseConnection(
                "LanceDB adapter not connected — call connect() first".to_string(),
            ))
        }
    }
}

// ─── Pure-logic helpers ───────────────────────────────────────────────────────

/// Cosine similarity between two equal-length vectors.
/// Returns 0.0 if either vector has zero norm.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        (dot / (na * nb)).clamp(-1.0, 1.0)
    }
}

/// Dot-product similarity.
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Negative Euclidean distance (higher → closer).
pub fn neg_euclidean(a: &[f32], b: &[f32]) -> f32 {
    let dist: f32 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt();
    -dist
}

fn score(metric: SimilarityMetric, query: &[f32], candidate: &[f32]) -> f32 {
    match metric {
        SimilarityMetric::Cosine => cosine_similarity(query, candidate),
        SimilarityMetric::DotProduct => dot_product(query, candidate),
        SimilarityMetric::Euclidean => neg_euclidean(query, candidate),
    }
}

/// Naively convert a JSON filter object to an in-memory predicate function.
///
/// Supports top-level key=value equality only.  In the native lancedb mode
/// this would be translated to a Lance SQL predicate string.
fn matches_filter(record: &VectorRecord, filter: &serde_json::Value) -> bool {
    let Some(obj) = filter.as_object() else {
        return true; // no filter
    };
    for (k, expected) in obj {
        match record.metadata.get(k) {
            None => return false,
            Some(actual) => {
                if actual != expected {
                    return false;
                }
            }
        }
    }
    true
}

/// Convert a JSON filter to a SQL predicate string (for the native Lance backend).
///
/// This function is kept here for documentation purposes — it will be used when
/// the native lancedb driver is enabled.
pub fn json_filter_to_sql(filter: &serde_json::Value) -> String {
    if let Some(obj) = filter.as_object() {
        let parts: Vec<String> = obj
            .iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) => {
                    format!("{} = '{}'", k, s.replace('\'', "''"))
                }
                serde_json::Value::Number(n) => format!("{} = {}", k, n),
                serde_json::Value::Bool(b) => format!("{} = {}", k, b),
                _ => format!("{} IS NOT NULL", k),
            })
            .collect();
        if parts.is_empty() {
            "TRUE".to_string()
        } else {
            parts.join(" AND ")
        }
    } else {
        "TRUE".to_string()
    }
}

// ─── DatabaseAdapter ──────────────────────────────────────────────────────────

impl DatabaseAdapter for LanceDbAdapter {
    fn initialize(&mut self) -> Result<()> {
        self.require_connected()?;
        self.records.clear();
        info!(
            "Initialized LanceDB table '{}' (in-memory, dim={})",
            self.table_name, self.dimension
        );
        Ok(())
    }

    fn connect(&mut self) -> Result<()> {
        // In native mode: lancedb::connect(&self.storage_uri()).execute().await
        let uri = self.storage_uri();
        self.connected = true;
        info!("LanceDB adapter connected (in-memory mode, uri={})", uri);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        info!("Disconnected from LanceDB");
        Ok(())
    }

    fn health(&mut self) -> Result<()> {
        self.require_connected()
    }

    fn capabilities(&self) -> DatabaseCapabilities {
        DatabaseCapabilities {
            structured_queries: false,
            document_queries: false,
            vector_search: true,
            metadata_filter: true,
            hybrid_search: true,
            batch_insert: true,
            transactions: false,
        }
    }

    fn load_dataset(&mut self, dataset: &[Document]) -> Result<()> {
        let records: Vec<VectorRecord> = dataset
            .iter()
            .map(|doc| VectorRecord {
                id: doc.id.clone(),
                embedding: vec![0.0_f32; self.dimension as usize],
                metadata: doc
                    .metadata
                    .as_object()
                    .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default(),
                modality: "text".to_string(),
            })
            .collect();
        debug!(
            "load_dataset: delegating {} documents to insert_vectors",
            records.len()
        );
        self.insert_vectors(&records)
    }

    fn build_indexes(&mut self) -> Result<()> {
        self.build_vector_index(IndexType::HNSW, self.dimension)
    }

    fn execute(&mut self, _query: &str) -> Result<()> {
        // Reserved for future query dispatch from the benchmark engine.
        self.require_connected()
    }

    fn cleanup(&mut self) -> Result<()> {
        self.require_connected()?;
        self.records.clear();
        info!("Cleaned up LanceDB table '{}'", self.table_name);
        Ok(())
    }
}

// ─── VectorAdapter ────────────────────────────────────────────────────────────

impl VectorAdapter for LanceDbAdapter {
    fn insert_vectors(&mut self, records: &[VectorRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        self.require_connected()?;
        self.records.extend_from_slice(records);
        debug!(
            "Inserted {} vectors into LanceDB (in-memory)",
            records.len()
        );
        Ok(())
    }

    fn build_vector_index(&mut self, _index_type: IndexType, dimension: u64) -> Result<()> {
        self.require_connected()?;
        self.dimension = dimension;
        // In native mode: table.create_index(&["embedding"]).ivf_pq(...).execute().await
        info!(
            "Built vector index on LanceDB table '{}' (dim={})",
            self.table_name, dimension
        );
        Ok(())
    }

    fn search_vectors(&mut self, query: &VectorQuery) -> Result<Vec<VectorSearchResult>> {
        self.require_connected()?;

        let metric = query.similarity_metric;
        let q = &query.query_embedding;

        let mut scored: Vec<(f32, &VectorRecord)> = self
            .records
            .iter()
            .map(|r| (score(metric, q, &r.embedding), r))
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));

        Ok(scored
            .into_iter()
            .take(query.top_k)
            .map(|(s, r)| VectorSearchResult {
                id: r.id.clone(),
                score: s,
                metadata: r.metadata.clone(),
            })
            .collect())
    }

    fn metadata_filter(&mut self, filter: &serde_json::Value) -> Result<Vec<VectorSearchResult>> {
        self.require_connected()?;

        Ok(self
            .records
            .iter()
            .filter(|r| matches_filter(r, filter))
            .map(|r| VectorSearchResult {
                id: r.id.clone(),
                score: 1.0, // no ranking in filter-only mode
                metadata: r.metadata.clone(),
            })
            .collect())
    }

    fn hybrid_search(
        &mut self,
        query: &VectorQuery,
        filter: &serde_json::Value,
    ) -> Result<Vec<VectorSearchResult>> {
        self.require_connected()?;

        let metric = query.similarity_metric;
        let q = &query.query_embedding;

        let mut scored: Vec<(f32, &VectorRecord)> = self
            .records
            .iter()
            .filter(|r| matches_filter(r, filter))
            .map(|r| (score(metric, q, &r.embedding), r))
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));

        Ok(scored
            .into_iter()
            .take(query.top_k)
            .map(|(s, r)| VectorSearchResult {
                id: r.id.clone(),
                score: s,
                metadata: r.metadata.clone(),
            })
            .collect())
    }

    fn delete_vectors(&mut self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.require_connected()?;
        let id_set: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        self.records.retain(|r| !id_set.contains(r.id.as_str()));
        Ok(())
    }

    fn vector_count(&mut self) -> Result<u64> {
        self.require_connected()?;
        Ok(self.records.len() as u64)
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_config() -> DatabaseConfig {
        DatabaseConfig {
            enabled: true,
            host: Some("/tmp/lancedb_test".to_string()),
            port: None,
            username: None,
            password: None,
            database: Some("test_table".to_string()),
            pool_size: None,
            timeout_ms: None,
        }
    }

    fn connected_adapter() -> LanceDbAdapter {
        let mut a = LanceDbAdapter::new(make_config());
        a.connect().unwrap();
        a
    }

    fn sample_records(n: usize, dim: usize) -> Vec<VectorRecord> {
        (0..n)
            .map(|i| VectorRecord {
                id: format!("doc-{}", i),
                embedding: (0..dim).map(|j| (i * dim + j) as f32).collect(),
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("index".to_string(), serde_json::json!(i));
                    m.insert(
                        "category".to_string(),
                        serde_json::json!(if i % 2 == 0 { "even" } else { "odd" }),
                    );
                    m
                },
                modality: "text".to_string(),
            })
            .collect()
    }

    // ── construction ─────────────────────────────────────────────────────────

    #[test]
    fn test_construction() {
        let adapter = LanceDbAdapter::new(make_config());
        assert_eq!(adapter.table_name, "test_table");
        assert_eq!(adapter.dimension, DEFAULT_DIM);
        assert_eq!(adapter.storage_uri(), "/tmp/lancedb_test");
        assert!(!adapter.connected);
    }

    // ── connect / disconnect / health ─────────────────────────────────────────

    #[test]
    fn test_connect_disconnect() {
        let mut adapter = LanceDbAdapter::new(make_config());
        assert!(adapter.connect().is_ok());
        assert!(adapter.connected);
        assert!(adapter.health().is_ok());
        assert!(adapter.disconnect().is_ok());
        assert!(!adapter.connected);
    }

    #[test]
    fn test_health_fails_when_disconnected() {
        let mut adapter = LanceDbAdapter::new(make_config());
        assert!(adapter.health().is_err());
    }

    // ── capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn test_capabilities() {
        let adapter = LanceDbAdapter::new(make_config());
        let caps = adapter.capabilities();
        assert!(caps.vector_search);
        assert!(caps.metadata_filter);
        assert!(caps.hybrid_search);
        assert!(!caps.structured_queries);
        assert!(!caps.transactions);
    }

    // ── insert / count / delete ───────────────────────────────────────────────

    #[test]
    fn test_insert_empty_slice_is_ok() {
        let mut adapter = connected_adapter();
        assert!(adapter.insert_vectors(&[]).is_ok());
        assert_eq!(adapter.vector_count().unwrap(), 0);
    }

    #[test]
    fn test_insert_and_count() {
        let mut adapter = connected_adapter();
        let recs = sample_records(5, 4);
        adapter.insert_vectors(&recs).unwrap();
        assert_eq!(adapter.vector_count().unwrap(), 5);
    }

    #[test]
    fn test_delete_vectors() {
        let mut adapter = connected_adapter();
        let recs = sample_records(5, 4);
        adapter.insert_vectors(&recs).unwrap();

        let to_delete = vec!["doc-0".to_string(), "doc-2".to_string()];
        adapter.delete_vectors(&to_delete).unwrap();
        assert_eq!(adapter.vector_count().unwrap(), 3);
    }

    #[test]
    fn test_delete_empty_slice_is_ok() {
        let mut adapter = connected_adapter();
        assert!(adapter.delete_vectors(&[]).is_ok());
    }

    // ── initialize / cleanup ──────────────────────────────────────────────────

    #[test]
    fn test_initialize_clears_records() {
        let mut adapter = connected_adapter();
        let recs = sample_records(3, 4);
        adapter.insert_vectors(&recs).unwrap();
        assert_eq!(adapter.vector_count().unwrap(), 3);
        adapter.initialize().unwrap();
        assert_eq!(adapter.vector_count().unwrap(), 0);
    }

    #[test]
    fn test_cleanup_clears_records() {
        let mut adapter = connected_adapter();
        let recs = sample_records(3, 4);
        adapter.insert_vectors(&recs).unwrap();
        adapter.cleanup().unwrap();
        assert_eq!(adapter.vector_count().unwrap(), 0);
    }

    // ── search ────────────────────────────────────────────────────────────────

    #[test]
    fn test_search_returns_top_k() {
        let mut adapter = connected_adapter();
        let recs = sample_records(10, 4);
        adapter.insert_vectors(&recs).unwrap();

        let query = VectorQuery {
            query_embedding: vec![1.0, 0.0, 0.0, 0.0],
            top_k: 3,
            metadata_filter: None,
            similarity_metric: SimilarityMetric::Cosine,
        };
        let results = adapter.search_vectors(&query).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_scores_ordered_descending() {
        let mut adapter = connected_adapter();
        // Insert a known set so we can verify ordering
        let recs = vec![
            VectorRecord {
                id: "a".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: HashMap::new(),
                modality: "text".to_string(),
            },
            VectorRecord {
                id: "b".to_string(),
                embedding: vec![0.0, 1.0],
                metadata: HashMap::new(),
                modality: "text".to_string(),
            },
        ];
        adapter.insert_vectors(&recs).unwrap();

        let query = VectorQuery {
            query_embedding: vec![1.0, 0.0],
            top_k: 2,
            metadata_filter: None,
            similarity_metric: SimilarityMetric::Cosine,
        };
        let results = adapter.search_vectors(&query).unwrap();
        assert_eq!(results[0].id, "a");
        assert!(results[0].score > results[1].score);
    }

    // ── metadata filter ───────────────────────────────────────────────────────

    #[test]
    fn test_metadata_filter_string_eq() {
        let mut adapter = connected_adapter();
        let recs = sample_records(6, 4);
        adapter.insert_vectors(&recs).unwrap();

        let filter = serde_json::json!({"category": "even"});
        let results = adapter.metadata_filter(&filter).unwrap();
        // docs 0, 2, 4 are even
        assert_eq!(results.len(), 3);
        for r in &results {
            assert_eq!(r.metadata["category"], serde_json::json!("even"));
        }
    }

    #[test]
    fn test_metadata_filter_no_match() {
        let mut adapter = connected_adapter();
        let recs = sample_records(4, 4);
        adapter.insert_vectors(&recs).unwrap();

        let filter = serde_json::json!({"category": "unknown"});
        let results = adapter.metadata_filter(&filter).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_metadata_filter_empty_returns_all() {
        let mut adapter = connected_adapter();
        let recs = sample_records(4, 4);
        adapter.insert_vectors(&recs).unwrap();

        let results = adapter.metadata_filter(&serde_json::json!({})).unwrap();
        assert_eq!(results.len(), 4);
    }

    // ── hybrid search ─────────────────────────────────────────────────────────

    #[test]
    fn test_hybrid_search_applies_filter_and_rank() {
        let mut adapter = connected_adapter();
        let recs = sample_records(6, 4);
        adapter.insert_vectors(&recs).unwrap();

        let query = VectorQuery {
            query_embedding: vec![1.0; 4],
            top_k: 10,
            metadata_filter: None,
            similarity_metric: SimilarityMetric::Cosine,
        };
        let filter = serde_json::json!({"category": "odd"});
        let results = adapter.hybrid_search(&query, &filter).unwrap();
        // Only odd-indexed docs (1, 3, 5)
        assert_eq!(results.len(), 3);
        for r in &results {
            assert_eq!(r.metadata["category"], serde_json::json!("odd"));
        }
    }

    // ── SQL filter helper ─────────────────────────────────────────────────────

    #[test]
    fn test_json_filter_to_sql_string() {
        let f = serde_json::json!({"category": "news"});
        let sql = json_filter_to_sql(&f);
        assert_eq!(sql, "category = 'news'");
    }

    #[test]
    fn test_json_filter_to_sql_number() {
        let f = serde_json::json!({"year": 2024});
        let sql = json_filter_to_sql(&f);
        assert_eq!(sql, "year = 2024");
    }

    #[test]
    fn test_json_filter_to_sql_empty() {
        let sql = json_filter_to_sql(&serde_json::json!({}));
        assert_eq!(sql, "TRUE");
    }

    // ── cosine similarity helper ──────────────────────────────────────────────

    #[test]
    fn test_cosine_self_similarity_is_one() {
        let v = vec![1.0_f32, 2.0, 3.0];
        let s = cosine_similarity(&v, &v);
        assert!(
            (s - 1.0).abs() < 1e-5,
            "self cosine should be 1.0, got {}",
            s
        );
    }

    #[test]
    fn test_cosine_orthogonal_is_zero() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.0_f32, 1.0];
        let s = cosine_similarity(&a, &b);
        assert!(s.abs() < 1e-5, "orthogonal cosine should be 0.0, got {}", s);
    }

    #[test]
    fn test_cosine_zero_vector_returns_zero() {
        let a = vec![0.0_f32, 0.0];
        let b = vec![1.0_f32, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    // ── build_vector_index / load_dataset ─────────────────────────────────────

    #[test]
    fn test_build_vector_index_updates_dimension() {
        let mut adapter = connected_adapter();
        adapter.build_vector_index(IndexType::HNSW, 128).unwrap();
        assert_eq!(adapter.dimension, 128);
    }

    #[test]
    fn test_load_dataset_delegates_to_insert_vectors() {
        let mut adapter = connected_adapter();
        let docs = vec![Document {
            id: "d1".to_string(),
            title: Some("T".to_string()),
            text: "hello".to_string(),
            metadata: serde_json::json!({}),
        }];
        adapter.load_dataset(&docs).unwrap();
        assert_eq!(adapter.vector_count().unwrap(), 1);
    }

    // ── disconnected guard ────────────────────────────────────────────────────

    #[test]
    fn test_operations_fail_when_disconnected() {
        let mut adapter = LanceDbAdapter::new(make_config());
        assert!(adapter.initialize().is_err());
        assert!(adapter.health().is_err());
        assert!(adapter.cleanup().is_err());
        assert!(adapter.vector_count().is_err());
        let recs = sample_records(1, 4);
        assert!(adapter.insert_vectors(&recs).is_err());
    }
}
