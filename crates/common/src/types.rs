#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    Postgres,
    MongoDb,
    Qdrant,
    LanceDb,
}

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
