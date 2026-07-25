use crate::traits::DatabaseAdapter;
use common::error::Result;
use common::types::{IndexType, VectorQuery, VectorRecord, VectorSearchResult};

/// Extension trait for adapters that support vector storage operations.
///
/// # Architecture
///
/// `VectorAdapter` is a **supertrait** of `DatabaseAdapter`.  Every vector
/// database adapter (`QdrantAdapter`, `LanceDbAdapter`, …) therefore
/// implements *both* traits and can be handed to the engine as a plain
/// `Box<dyn DatabaseAdapter>`.
///
/// The base `DatabaseAdapter` methods on vector adapters delegate internally:
/// - `load_dataset`   → translates `Document` → `VectorRecord`, calls `insert_vectors`
/// - `build_indexes`  → calls `build_vector_index(IndexType::HNSW, dim)`
/// - `execute`        → reserved; returns `Ok(())` for now
///
/// # Extensibility
///
/// To add a new vector backend (Milvus, Weaviate, pgvector, etc.):
/// 1. Create `crates/adapters/src/<name>.rs`
/// 2. Implement `DatabaseAdapter` (connect, disconnect, health, capabilities, cleanup, …)
/// 3. Implement `VectorAdapter` (insert_vectors, search_vectors, …)
/// 4. Add `DatabaseType::<Name>` to `common::types`
/// 5. Add arm to `AdapterFactory::get_adapter`
///
/// **Zero changes** required to: `DatabaseAdapter` trait, `VectorAdapter` trait,
/// or the benchmark engine.
pub trait VectorAdapter: DatabaseAdapter {
    /// Insert a batch of pre-computed vector records.
    fn insert_vectors(&mut self, records: &[VectorRecord]) -> Result<()>;

    /// Create or recreate the vector index on the stored collection.
    ///
    /// `dimension` must match the length of the embeddings inserted via
    /// `insert_vectors`.
    fn build_vector_index(&mut self, index_type: IndexType, dimension: u64) -> Result<()>;

    /// Perform an approximate nearest-neighbour search.
    fn search_vectors(&mut self, query: &VectorQuery) -> Result<Vec<VectorSearchResult>>;

    /// Return records matching a metadata predicate (no vector ranking).
    fn metadata_filter(&mut self, filter: &serde_json::Value) -> Result<Vec<VectorSearchResult>>;

    /// Combined vector similarity + metadata filter search.
    fn hybrid_search(
        &mut self,
        query: &VectorQuery,
        filter: &serde_json::Value,
    ) -> Result<Vec<VectorSearchResult>>;

    /// Delete records by their stable document identifier.
    fn delete_vectors(&mut self, ids: &[String]) -> Result<()>;

    /// Return the total number of vectors stored in the active collection.
    fn vector_count(&mut self) -> Result<u64>;
}
