#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    Postgres,
    MongoDb,
    Qdrant,
    LanceDb,
}
