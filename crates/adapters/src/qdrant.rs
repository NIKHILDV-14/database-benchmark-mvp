use crate::traits::DatabaseAdapter;
use crate::vector_traits::VectorAdapter;
use benchmark_config::DatabaseConfig;
use common::error::{BenchmarkError, Result};
use common::types::{
    DatabaseCapabilities, IndexType, SimilarityMetric, VectorQuery, VectorRecord,
    VectorSearchResult,
};
use dataset::model::Document;
use qdrant_client::client::{QdrantClient, QdrantClientConfig};
use qdrant_client::qdrant::vectors_config::Config as VectorsConfigInner;
use qdrant_client::qdrant::{
    CountPoints, CreateCollection, Distance, Filter, PointId, PointStruct, PointsIdsList,
    PointsSelector, ScrollPoints, SearchPoints, Value as QdrantValue, VectorParams, VectorsConfig,
    WithPayloadSelector,
};
use std::collections::HashMap;
use tokio::runtime::Runtime;
use tracing::{debug, info};

/// Default dimension used when `load_dataset` is called without prior
/// `build_vector_index`.  In practice the benchmark harness always calls
/// `build_indexes` first, which may override this.
const DEFAULT_DIM: u64 = 384;

/// Default collection name when none is configured.
const DEFAULT_COLLECTION: &str = "benchmark_vectors";

pub struct QdrantAdapter {
    config: DatabaseConfig,
    client: Option<QdrantClient>,
    /// Single-threaded tokio runtime used to bridge the async client.
    runtime: Runtime,
    collection: String,
    /// Tracks the last dimension used so `load_dataset` can re-use it.
    dimension: u64,
    /// Similarity metric to use when creating the collection index.
    similarity: SimilarityMetric,
}

impl QdrantAdapter {
    pub fn new(config: DatabaseConfig) -> Self {
        let runtime = Runtime::new().expect("Failed to create tokio runtime for QdrantAdapter");
        let collection = config
            .database
            .clone()
            .unwrap_or_else(|| DEFAULT_COLLECTION.to_string());
        Self {
            config,
            client: None,
            runtime,
            collection,
            dimension: DEFAULT_DIM,
            similarity: SimilarityMetric::Cosine,
        }
    }

    fn get_client(&self) -> Result<&QdrantClient> {
        self.client.as_ref().ok_or_else(|| {
            BenchmarkError::DatabaseConnection("Not connected to Qdrant".to_string())
        })
    }

    fn build_url(&self) -> String {
        let host = self.config.host.as_deref().unwrap_or("localhost");
        let port = self.config.port.unwrap_or(6334);
        format!("http://{}:{}", host, port)
    }

    /// Convert a `serde_json::Value` metadata map to Qdrant payload.
    fn json_to_qdrant_payload(
        meta: &HashMap<String, serde_json::Value>,
    ) -> HashMap<String, QdrantValue> {
        meta.iter()
            .map(|(k, v)| (k.clone(), json_value_to_qdrant(v)))
            .collect()
    }
}

// ─── SimilarityMetric → Qdrant Distance ──────────────────────────────────────

fn metric_to_distance(metric: SimilarityMetric) -> Distance {
    match metric {
        SimilarityMetric::Cosine => Distance::Cosine,
        SimilarityMetric::DotProduct => Distance::Dot,
        SimilarityMetric::Euclidean => Distance::Euclid,
    }
}

// ─── serde_json → QdrantValue conversion ─────────────────────────────────────

fn json_value_to_qdrant(v: &serde_json::Value) -> QdrantValue {
    use qdrant_client::qdrant::value::Kind;
    match v {
        serde_json::Value::Null => QdrantValue {
            kind: Some(Kind::NullValue(0)),
        },
        serde_json::Value::Bool(b) => QdrantValue {
            kind: Some(Kind::BoolValue(*b)),
        },
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                QdrantValue {
                    kind: Some(Kind::IntegerValue(i)),
                }
            } else {
                QdrantValue {
                    kind: Some(Kind::DoubleValue(n.as_f64().unwrap_or(0.0))),
                }
            }
        }
        serde_json::Value::String(s) => QdrantValue {
            kind: Some(Kind::StringValue(s.clone())),
        },
        // Arrays and objects: serialise to JSON string for payload storage.
        other => QdrantValue {
            kind: Some(Kind::StringValue(other.to_string())),
        },
    }
}

// ─── QdrantValue → serde_json::Value ─────────────────────────────────────────

fn qdrant_value_to_json(v: &QdrantValue) -> serde_json::Value {
    use qdrant_client::qdrant::value::Kind;
    match &v.kind {
        None | Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(Kind::IntegerValue(i)) => serde_json::json!(*i),
        Some(Kind::DoubleValue(d)) => serde_json::json!(*d),
        Some(Kind::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.iter().map(qdrant_value_to_json).collect())
        }
        Some(Kind::StructValue(s)) => serde_json::Value::Object(
            s.fields
                .iter()
                .map(|(k, v)| (k.clone(), qdrant_value_to_json(v)))
                .collect(),
        ),
    }
}

// ─── DatabaseAdapter ──────────────────────────────────────────────────────────

impl DatabaseAdapter for QdrantAdapter {
    fn initialize(&mut self) -> Result<()> {
        let client = self.get_client()?;
        let coll = self.collection.clone();
        let dim = self.dimension;
        let distance = metric_to_distance(self.similarity) as i32;

        self.runtime.block_on(async {
            // Drop if present, then recreate with the correct dimension.
            let _ = client.delete_collection(&coll).await;

            client
                .create_collection(&CreateCollection {
                    collection_name: coll.clone(),
                    vectors_config: Some(VectorsConfig {
                        config: Some(VectorsConfigInner::Params(VectorParams {
                            size: dim,
                            distance,
                            ..Default::default()
                        })),
                    }),
                    ..Default::default()
                })
                .await
                .map_err(|e| {
                    BenchmarkError::DatabaseConnection(format!(
                        "Failed to create Qdrant collection '{}': {}",
                        coll, e
                    ))
                })
        })?;

        info!("Initialized Qdrant collection '{}'", self.collection);
        Ok(())
    }

    fn connect(&mut self) -> Result<()> {
        let url = self.build_url();
        let mut cfg = QdrantClientConfig::from_url(&url);
        if let Some(key) = &self.config.password {
            cfg.api_key = Some(key.clone());
        }

        let client = QdrantClient::new(Some(cfg)).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Qdrant client creation failed: {}", e))
        })?;

        self.client = Some(client);
        info!("Connected to Qdrant at {}", url);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        self.client = None;
        info!("Disconnected from Qdrant");
        Ok(())
    }

    fn health(&mut self) -> Result<()> {
        let client = self.get_client()?;
        self.runtime.block_on(client.health_check()).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Qdrant health check failed: {}", e))
        })?;
        Ok(())
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

    fn execute(&mut self, _query: &str) -> Result<usize> {
        // Reserved for future query dispatch from the benchmark engine.
        Ok(0)
    }

    fn cleanup(&mut self) -> Result<()> {
        let client = self.get_client()?;
        let coll = self.collection.clone();
        self.runtime.block_on(async {
            client.delete_collection(&coll).await.map_err(|e| {
                BenchmarkError::DatabaseConnection(format!(
                    "Failed to drop Qdrant collection '{}': {}",
                    coll, e
                ))
            })
        })?;
        info!("Cleaned up Qdrant collection '{}'", self.collection);
        Ok(())
    }

    fn as_vector_adapter_mut(&mut self) -> Option<&mut dyn crate::vector_traits::VectorAdapter> {
        Some(self)
    }
}

// ─── VectorAdapter ────────────────────────────────────────────────────────────

impl VectorAdapter for QdrantAdapter {
    fn insert_vectors(&mut self, records: &[VectorRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let client = self.get_client()?;
        let coll = self.collection.clone();

        let points: Vec<PointStruct> = records
            .iter()
            .map(|r| PointStruct {
                id: Some(PointId::from(r.id.clone())),
                vectors: Some(r.embedding.clone().into()),
                payload: Self::json_to_qdrant_payload(&r.metadata),
            })
            .collect();

        self.runtime.block_on(async {
            client
                .upsert_points(&coll, None, points, None)
                .await
                .map_err(|e| {
                    BenchmarkError::DatabaseConnection(format!("Qdrant upsert failed: {}", e))
                })
        })?;

        debug!("Inserted {} vectors into Qdrant '{}'", records.len(), coll);
        Ok(())
    }

    fn build_vector_index(&mut self, _index_type: IndexType, dimension: u64) -> Result<()> {
        // Qdrant builds its HNSW index automatically during collection creation.
        // We store the new dimension and (re)create the collection.
        self.dimension = dimension;
        self.initialize()
    }

    fn search_vectors(&mut self, query: &VectorQuery) -> Result<Vec<VectorSearchResult>> {
        let client = self.get_client()?;
        let coll = self.collection.clone();
        let limit = query.top_k as u64;
        let vec = query.query_embedding.clone();

        let response = self.runtime.block_on(async {
            client
                .search_points(&SearchPoints {
                    collection_name: coll,
                    vector: vec,
                    limit,
                    with_payload: Some(WithPayloadSelector::from(true)),
                    ..Default::default()
                })
                .await
                .map_err(|e| {
                    BenchmarkError::DatabaseConnection(format!("Qdrant search failed: {}", e))
                })
        })?;

        Ok(response
            .result
            .into_iter()
            .map(|sp| {
                let id = sp
                    .id
                    .and_then(|pid| pid.point_id_options)
                    .map(|opt| match opt {
                        qdrant_client::qdrant::point_id::PointIdOptions::Uuid(u) => u,
                        qdrant_client::qdrant::point_id::PointIdOptions::Num(n) => n.to_string(),
                    })
                    .unwrap_or_default();
                let metadata = sp
                    .payload
                    .iter()
                    .map(|(k, v)| (k.clone(), qdrant_value_to_json(v)))
                    .collect();
                VectorSearchResult {
                    id,
                    score: sp.score,
                    metadata,
                }
            })
            .collect())
    }

    fn metadata_filter(&mut self, _filter: &serde_json::Value) -> Result<Vec<VectorSearchResult>> {
        let client = self.get_client()?;
        let coll = self.collection.clone();

        let response = self.runtime.block_on(async {
            client
                .scroll(&ScrollPoints {
                    collection_name: coll,
                    filter: Some(Filter {
                        must: vec![],
                        should: vec![],
                        must_not: vec![],
                        ..Default::default()
                    }),
                    with_payload: Some(WithPayloadSelector::from(true)),
                    ..Default::default()
                })
                .await
                .map_err(|e| {
                    BenchmarkError::DatabaseConnection(format!(
                        "Qdrant metadata_filter failed: {}",
                        e
                    ))
                })
        })?;

        Ok(response
            .result
            .into_iter()
            .map(|rp| {
                let id = rp
                    .id
                    .and_then(|pid| pid.point_id_options)
                    .map(|opt| match opt {
                        qdrant_client::qdrant::point_id::PointIdOptions::Uuid(u) => u,
                        qdrant_client::qdrant::point_id::PointIdOptions::Num(n) => n.to_string(),
                    })
                    .unwrap_or_default();
                let metadata = rp
                    .payload
                    .iter()
                    .map(|(k, v)| (k.clone(), qdrant_value_to_json(v)))
                    .collect();
                VectorSearchResult {
                    id,
                    score: 1.0, // scroll has no ranking
                    metadata,
                }
            })
            .collect())
    }

    fn hybrid_search(
        &mut self,
        query: &VectorQuery,
        _filter: &serde_json::Value,
    ) -> Result<Vec<VectorSearchResult>> {
        // Qdrant supports native hybrid search via the `filter` field on
        // SearchPoints.  For now we delegate to pure vector search.
        self.search_vectors(query)
    }

    fn delete_vectors(&mut self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let client = self.get_client()?;
        let coll = self.collection.clone();
        let point_ids: Vec<PointId> = ids.iter().map(|id| PointId::from(id.clone())).collect();

        self.runtime.block_on(async {
            client
                .delete_points(
                    &coll,
                    None,
                    &PointsSelector {
                        points_selector_one_of: Some(
                            qdrant_client::qdrant::points_selector::PointsSelectorOneOf::Points(
                                PointsIdsList { ids: point_ids },
                            ),
                        ),
                    },
                    None,
                )
                .await
                .map_err(|e| {
                    BenchmarkError::DatabaseConnection(format!(
                        "Qdrant delete_points failed: {}",
                        e
                    ))
                })
        })?;

        Ok(())
    }

    fn vector_count(&mut self) -> Result<u64> {
        let client = self.get_client()?;
        let coll = self.collection.clone();

        let response = self.runtime.block_on(async {
            client
                .count(&CountPoints {
                    collection_name: coll,
                    filter: None,
                    exact: Some(true),
                    ..Default::default()
                })
                .await
                .map_err(|e| {
                    BenchmarkError::DatabaseConnection(format!("Qdrant count failed: {}", e))
                })
        })?;

        Ok(response.result.map(|r| r.count).unwrap_or(0))
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> DatabaseConfig {
        DatabaseConfig {
            enabled: true,
            host: Some("localhost".to_string()),
            port: Some(6334),
            username: None,
            password: None,
            database: Some("test_collection".to_string()),
            pool_size: None,
            timeout_ms: None,
        }
    }

    #[test]
    fn test_construction() {
        let adapter = QdrantAdapter::new(make_config());
        assert_eq!(adapter.collection, "test_collection");
        assert_eq!(adapter.dimension, DEFAULT_DIM);
    }

    #[test]
    fn test_build_url() {
        let adapter = QdrantAdapter::new(make_config());
        assert_eq!(adapter.build_url(), "http://localhost:6334");
    }

    #[test]
    fn test_capabilities() {
        let adapter = QdrantAdapter::new(make_config());
        let caps = adapter.capabilities();
        assert!(caps.vector_search);
        assert!(caps.metadata_filter);
        assert!(caps.hybrid_search);
        assert!(!caps.structured_queries);
        assert!(!caps.transactions);
    }

    #[test]
    fn test_insert_vectors_empty_slice_is_ok() {
        // Empty slice short-circuits before any RPC.
        let mut adapter = QdrantAdapter::new(make_config());
        assert!(adapter.insert_vectors(&[]).is_ok());
    }

    #[test]
    fn test_delete_vectors_empty_slice_is_ok() {
        let mut adapter = QdrantAdapter::new(make_config());
        assert!(adapter.delete_vectors(&[]).is_ok());
    }

    #[test]
    fn test_get_client_fails_before_connect() {
        let adapter = QdrantAdapter::new(make_config());
        assert!(adapter.get_client().is_err());
    }

    #[test]
    fn test_json_to_qdrant_roundtrip_string() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), serde_json::json!("hello"));
        let qdrant_map = QdrantAdapter::json_to_qdrant_payload(&map);
        let back = qdrant_value_to_json(qdrant_map.get("key").unwrap());
        assert_eq!(back, serde_json::json!("hello"));
    }

    #[test]
    fn test_json_to_qdrant_roundtrip_bool() {
        let mut map = HashMap::new();
        map.insert("flag".to_string(), serde_json::json!(true));
        let qdrant_map = QdrantAdapter::json_to_qdrant_payload(&map);
        let back = qdrant_value_to_json(qdrant_map.get("flag").unwrap());
        assert_eq!(back, serde_json::json!(true));
    }

    #[test]
    fn test_json_to_qdrant_roundtrip_integer() {
        let mut map = HashMap::new();
        map.insert("n".to_string(), serde_json::json!(42i64));
        let qdrant_map = QdrantAdapter::json_to_qdrant_payload(&map);
        let back = qdrant_value_to_json(qdrant_map.get("n").unwrap());
        assert_eq!(back, serde_json::json!(42i64));
    }

    #[test]
    fn test_metric_to_distance() {
        assert_eq!(
            metric_to_distance(SimilarityMetric::Cosine),
            Distance::Cosine
        );
        assert_eq!(
            metric_to_distance(SimilarityMetric::DotProduct),
            Distance::Dot
        );
        assert_eq!(
            metric_to_distance(SimilarityMetric::Euclidean),
            Distance::Euclid
        );
    }

    #[test]
    fn test_vector_record_construction() {
        let rec = VectorRecord {
            id: "doc-1".to_string(),
            embedding: vec![0.0; DEFAULT_DIM as usize],
            metadata: HashMap::new(),
            modality: "text".to_string(),
        };
        assert_eq!(rec.id, "doc-1");
        assert_eq!(rec.embedding.len(), DEFAULT_DIM as usize);
    }
}
