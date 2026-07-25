use crate::lancedb::LanceDbAdapter;
use crate::mongodb::MongoDbAdapter;
use crate::postgres::PostgresAdapter;
use crate::qdrant::QdrantAdapter;
use crate::traits::DatabaseAdapter;
use benchmark_config::DatabaseConfig;
use common::types::DatabaseType;

pub struct AdapterFactory;

impl AdapterFactory {
    pub fn get_adapter(db_type: DatabaseType, config: DatabaseConfig) -> Box<dyn DatabaseAdapter> {
        match db_type {
            DatabaseType::Postgres => Box::new(PostgresAdapter::new(config)),
            DatabaseType::MongoDb => Box::new(MongoDbAdapter::new(config)),
            DatabaseType::Qdrant => Box::new(QdrantAdapter::new(config)),
            DatabaseType::LanceDb => Box::new(LanceDbAdapter::new(config)),
        }
    }
}
