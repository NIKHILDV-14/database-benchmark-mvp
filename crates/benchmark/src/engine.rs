use crate::result::BenchmarkResult;
use crate::runner::BenchmarkRunner;
use crate::workload::Workload;
use adapters::factory::AdapterFactory;
use benchmark_config::{BenchmarkConfig, DatabaseConfig};
use common::types::DatabaseType;
use tracing::info;

/// Coordinates running the same workload against every enabled database.
///
/// # Architecture
///
/// ```text
///                     ┌──────────────────────┐
///                     │   BenchmarkEngine    │
///                     └────────┬─────────────┘
///                              │  for each enabled DB
///                    ┌─────────▼──────────────┐
///                    │    BenchmarkRunner      │
///                    └─────────┬───────────────┘
///                              │  DatabaseAdapter trait
///              ┌───────────────┼──────────────────────┐
///       Postgres     MongoDB        Qdrant       LanceDB
/// ```
///
/// The engine never imports `QdrantAdapter`, `LanceDbAdapter`, etc. directly —
/// it always works through `Box<dyn DatabaseAdapter>` obtained from the factory.
pub struct BenchmarkEngine {
    config: BenchmarkConfig,
}

impl BenchmarkEngine {
    pub fn new(config: BenchmarkConfig) -> Self {
        Self { config }
    }

    /// Run `workload` against every **enabled** database in the configuration.
    ///
    /// Returns one `BenchmarkResult` per enabled database, in the order:
    /// PostgreSQL, MongoDB, Qdrant, LanceDB.
    pub fn run(&self, workload: &Workload) -> Vec<BenchmarkResult> {
        let dbs = self.enabled_databases();
        info!(
            workload = %workload.name,
            db_count = dbs.len(),
            "starting benchmark run"
        );

        dbs.into_iter()
            .map(|(db_type, db_name, db_config)| {
                info!(db = %db_name, "running workload");
                let mut adapter = AdapterFactory::get_adapter(db_type, db_config);
                BenchmarkRunner::run(&db_name, adapter.as_mut(), workload, &self.config.embedding)
            })
            .collect()
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    fn enabled_databases(&self) -> Vec<(DatabaseType, String, DatabaseConfig)> {
        let dbs = &self.config.databases;
        let mut out = Vec::new();

        if dbs.postgres.enabled {
            out.push((
                DatabaseType::Postgres,
                "postgres".to_string(),
                dbs.postgres.clone(),
            ));
        }
        if dbs.mongodb.enabled {
            out.push((
                DatabaseType::MongoDb,
                "mongodb".to_string(),
                dbs.mongodb.clone(),
            ));
        }
        if dbs.qdrant.enabled {
            out.push((
                DatabaseType::Qdrant,
                "qdrant".to_string(),
                dbs.qdrant.clone(),
            ));
        }
        if dbs.lancedb.enabled {
            out.push((
                DatabaseType::LanceDb,
                "lancedb".to_string(),
                dbs.lancedb.clone(),
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workload::{QuerySpec, Workload};
    use benchmark_config::{
        BenchmarkConfig, DatabaseConfig, DatabasesConfig, DatasetConfig, EmbeddingConfig,
        ExperimentConfig,
    };
    use serde_json::json;

    fn make_doc(id: &str) -> dataset::model::Document {
        dataset::model::Document {
            id: id.to_string(),
            title: None,
            text: "test document".to_string(),
            metadata: json!({}),
        }
    }

    fn disabled_db() -> DatabaseConfig {
        DatabaseConfig {
            enabled: false,
            host: None,
            port: None,
            username: None,
            password: None,
            database: None,
            pool_size: None,
            timeout_ms: None,
        }
    }

    fn lancedb_config(name: &str) -> DatabaseConfig {
        DatabaseConfig {
            enabled: true,
            host: Some(format!("/tmp/engine_{}", name)),
            port: None,
            username: None,
            password: None,
            database: Some(name.to_string()),
            pool_size: None,
            timeout_ms: None,
        }
    }

    fn make_config(lancedb: DatabaseConfig) -> BenchmarkConfig {
        BenchmarkConfig {
            experiment: ExperimentConfig {
                name: "test".to_string(),
                seed: 42,
            },
            dataset: DatasetConfig {
                path: "".to_string(),
                max_size_gb: 1.0,
                empty_text_policy: "reject".to_string(),
                trim_whitespace: true,
            },
            databases: DatabasesConfig {
                postgres: disabled_db(),
                mongodb: disabled_db(),
                qdrant: disabled_db(),
                lancedb,
            },
            embedding: EmbeddingConfig {
                provider: "dummy".to_string(),
                batch_size: 8,
                chunk_size: 10,
                chunk_overlap: 2,
                cache_enabled: false,
                dimension: 64,
            },
        }
    }

    #[test]
    fn test_engine_runs_only_enabled_databases() {
        let config = make_config(lancedb_config("engine_enabled"));
        let engine = BenchmarkEngine::new(config);

        let workload = Workload::new("engine-test")
            .with_documents(vec![make_doc("doc1")])
            .with_queries(vec![QuerySpec::new("noop", "SELECT 1")]);

        let results = engine.run(&workload);

        // Only lancedb is enabled
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].database, "lancedb");
    }

    #[test]
    fn test_engine_returns_empty_when_no_databases_enabled() {
        let config = make_config(disabled_db());
        let engine = BenchmarkEngine::new(config);

        let workload = Workload::new("empty-run");
        let results = engine.run(&workload);

        assert!(results.is_empty());
    }

    #[test]
    fn test_engine_result_contains_insert_and_queries() {
        let config = make_config(lancedb_config("engine_metrics"));
        let engine = BenchmarkEngine::new(config);

        let workload = Workload::new("metrics-test")
            .with_documents(vec![make_doc("a"), make_doc("b")])
            .with_queries(vec![
                QuerySpec::new("q1", "SELECT 1"),
                QuerySpec::new("q2", "SELECT 2"),
            ]);

        let results = engine.run(&workload);
        assert_eq!(results.len(), 1);

        let r = &results[0];
        assert!(r.insert.status.is_success());
        assert_eq!(r.queries.len(), 2);
        assert!(r.delete.status.is_success());
    }

    #[test]
    fn test_engine_all_succeeded_on_lancedb_workload() {
        let config = make_config(lancedb_config("engine_success"));
        let engine = BenchmarkEngine::new(config);

        let workload = Workload::new("success-test")
            .with_documents(vec![make_doc("x")])
            .with_queries(vec![]);

        let results = engine.run(&workload);
        assert!(results[0].all_succeeded());
    }
}
