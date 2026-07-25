use crate::lancedb::LanceDbAdapter;
use crate::mongodb::MongoDbAdapter;
use crate::postgres::PostgresAdapter;
use crate::qdrant::QdrantAdapter;
use crate::traits::DatabaseAdapter;
use common::types::DatabaseType;

pub struct AdapterFactory;

impl AdapterFactory {
    pub fn get_adapter(db_type: DatabaseType) -> Box<dyn DatabaseAdapter> {
        match db_type {
            DatabaseType::Postgres => Box::new(PostgresAdapter::new()),
            DatabaseType::MongoDb => Box::new(MongoDbAdapter::new()),
            DatabaseType::Qdrant => Box::new(QdrantAdapter::new()),
            DatabaseType::LanceDb => Box::new(LanceDbAdapter::new()),
        }
    }
}
