pub mod factory;
pub mod lancedb;
pub mod mongodb;
pub mod postgres;
pub mod qdrant;
pub mod traits;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::DatabaseAdapter;
    use benchmark_config::{
        BenchmarkConfig, DatabaseConfig, DatabasesConfig, DatasetConfig, ExperimentConfig,
    };
    use common::types::DatabaseType;

    #[test]
    fn test_database_type_variants() {
        assert_eq!(DatabaseType::Postgres, DatabaseType::Postgres);
        assert_eq!(DatabaseType::MongoDb, DatabaseType::MongoDb);
        assert_eq!(DatabaseType::Qdrant, DatabaseType::Qdrant);
        assert_eq!(DatabaseType::LanceDb, DatabaseType::LanceDb);
        assert_ne!(DatabaseType::Postgres, DatabaseType::LanceDb);
    }

    #[test]
    fn test_adapter_factory() {
        let _pg: Box<dyn DatabaseAdapter> =
            factory::AdapterFactory::get_adapter(DatabaseType::Postgres);
        let _mgo: Box<dyn DatabaseAdapter> =
            factory::AdapterFactory::get_adapter(DatabaseType::MongoDb);
        let _qdr: Box<dyn DatabaseAdapter> =
            factory::AdapterFactory::get_adapter(DatabaseType::Qdrant);
        let _lnc: Box<dyn DatabaseAdapter> =
            factory::AdapterFactory::get_adapter(DatabaseType::LanceDb);

        // They are all opaque trait objects, but we can verify construction didn't panic.
        assert!(true);
    }

    #[test]
    fn test_config_parsing_includes_lancedb() {
        let config = BenchmarkConfig {
            experiment: ExperimentConfig {
                name: "test".to_string(),
                seed: 42,
            },
            dataset: DatasetConfig {
                path: "".to_string(),
                max_size_gb: 1.0,
                empty_text_policy: "".to_string(),
                trim_whitespace: true,
            },
            databases: DatabasesConfig {
                postgres: DatabaseConfig { enabled: true },
                mongodb: DatabaseConfig { enabled: false },
                qdrant: DatabaseConfig { enabled: false },
                lancedb: DatabaseConfig { enabled: true },
            },
        };

        assert!(config.databases.postgres.enabled);
        assert!(!config.databases.mongodb.enabled);
        assert!(!config.databases.qdrant.enabled);
        assert!(config.databases.lancedb.enabled);
    }

    #[test]
    fn test_stub_adapters_return_errors() {
        let mut adapter = factory::AdapterFactory::get_adapter(DatabaseType::Postgres);

        let result = adapter.connect();
        assert!(result.is_err());
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                msg.contains("Not Yet Implemented"),
                "Expected 'Not Yet Implemented', got: {}",
                msg
            );
        }
    }
}
