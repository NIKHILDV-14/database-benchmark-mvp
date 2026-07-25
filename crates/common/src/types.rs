use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The four database types this project benchmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    Postgres,
    MongoDb,
    Qdrant,
    LanceDb,
}

/// Feature flags describing what a specific adapter supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DatabaseCapabilities {
    pub structured_queries: bool,
    pub document_queries: bool,
    pub vector_search: bool,
    pub metadata_filter: bool,
    pub hybrid_search: bool,
    pub batch_insert: bool,
    pub transactions: bool,
}

// ──────────────────────────────────────────────────────────────────────────────
// Vector types — single source of truth for the whole workspace
// ──────────────────────────────────────────────────────────────────────────────

/// How similarity is measured between two vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SimilarityMetric {
    #[default]
    Cosine,
    DotProduct,
    Euclidean,
}

/// Which indexing algorithm to use when building a vector index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum IndexType {
    #[default]
    HNSW,
    IVF,
    PQ,
    Flat,
}

/// A record ready to be inserted into a vector store.
///
/// The `embedding` field carries the pre-computed vector.  The adapter is
/// responsible for translating this into whatever the target DB expects
/// (Qdrant `PointStruct`, LanceDB Arrow `RecordBatch`, etc.).
///
/// `metadata` mirrors the `Document::metadata` from the dataset crate so that
/// no information is lost during the SQL → Vector translation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    /// Stable document identifier (same as `Document::id`).
    pub id: String,
    /// Pre-computed dense embedding.
    pub embedding: Vec<f32>,
    /// Arbitrary metadata preserved from the source document.
    pub metadata: HashMap<String, serde_json::Value>,
    /// e.g. `"text"`, `"image"` — useful for future multi-modal support.
    pub modality: String,
}

/// A query issued against a vector index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorQuery {
    /// Dense query vector.
    pub query_embedding: Vec<f32>,
    /// Maximum number of nearest neighbours to return.
    pub top_k: usize,
    /// Optional metadata pre-filter expressed as a JSON object.
    /// Adapters translate this to native filter syntax before querying.
    pub metadata_filter: Option<serde_json::Value>,
    /// Similarity function to use (must match the index).
    pub similarity_metric: SimilarityMetric,
}

impl Default for VectorQuery {
    fn default() -> Self {
        Self {
            query_embedding: vec![],
            top_k: 10,
            metadata_filter: None,
            similarity_metric: SimilarityMetric::Cosine,
        }
    }
}

/// One result row returned from a vector search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    /// The matched document identifier.
    pub id: String,
    /// Similarity score (higher is better for Cosine / DotProduct).
    pub score: f32,
    /// Metadata payload returned alongside the match.
    pub metadata: HashMap<String, serde_json::Value>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Embedding types
// ──────────────────────────────────────────────────────────────────────────────

/// Supported embedding providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProviderType {
    Dummy,
    // Future providers (BGE, OpenAI, etc) would be added here
}

/// Metadata about the embedding model used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelInfo {
    pub provider: EmbeddingProviderType,
    pub name: String,
    pub dimension: usize,
}

/// A raw dense vector produced by an embedding provider.
#[derive(Debug, Clone)]
pub struct EmbeddingResult {
    pub vector: Vec<f32>,
}
