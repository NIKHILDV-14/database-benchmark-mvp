use crate::result::{BenchmarkResult, OperationMetrics};
use crate::workload::Workload;
use std::time::Instant;
use tracing::{debug, warn};

/// Runs a [`Workload`] against a single database adapter and returns raw metrics.
///
/// The runner does **not** know which database it is targeting — it speaks only
/// through the `DatabaseAdapter` trait.  This keeps the benchmark engine fully
/// decoupled from Qdrant, LanceDB, PostgreSQL, and MongoDB internals.
pub struct BenchmarkRunner;

impl BenchmarkRunner {
    /// Execute the complete benchmark lifecycle against `adapter`:
    ///
    /// 1. `connect` → `initialize`
    /// 2. **Insert**: timed `load_dataset` + `build_indexes`
    /// 3. **Query**: timed `execute` for each `QuerySpec`
    /// 4. **Delete/cleanup**: timed `cleanup`
    /// 5. `disconnect`
    pub fn run(
        db_name: &str,
        adapter: &mut dyn adapters::traits::DatabaseAdapter,
        workload: &Workload,
        embedding_cfg: &benchmark_config::EmbeddingConfig,
    ) -> BenchmarkResult {
        // ── Connect ──────────────────────────────────────────────────────────
        if let Err(e) = adapter.connect() {
            warn!(db = db_name, "connect failed: {}", e);
            let fail = OperationMetrics::failure(Default::default(), e.to_string());
            return BenchmarkResult::new(db_name, fail.clone(), vec![], fail);
        }
        if let Err(e) = adapter.initialize() {
            warn!(db = db_name, "initialize failed: {}", e);
            let _ = adapter.disconnect();
            let fail = OperationMetrics::failure(Default::default(), e.to_string());
            return BenchmarkResult::new(db_name, fail.clone(), vec![], fail);
        }

        // ── Pre-compute Embeddings ───────────────────────────────────────────
        let mut precomputed_records = None;
        if adapter.capabilities().vector_search {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let records = rt
                .block_on(adapters::pipeline::embed_documents(
                    &workload.documents,
                    embedding_cfg,
                ))
                .map_err(common::error::BenchmarkError::Other);
            match records {
                Ok(recs) => precomputed_records = Some(recs),
                Err(e) => {
                    warn!(db = db_name, "embedding generation failed: {}", e);
                    let fail = OperationMetrics::failure(Default::default(), e.to_string());
                    return BenchmarkResult::new(db_name, fail.clone(), vec![], fail);
                }
            }
        }

        // ── Insert ───────────────────────────────────────────────────────────
        let insert = {
            let t = Instant::now();

            let r = if adapter.capabilities().vector_search {
                if let Some(vec_adapter) = adapter.as_vector_adapter_mut() {
                    vec_adapter
                        .insert_vectors(precomputed_records.as_ref().unwrap())
                        .map_err(|e| {
                            common::error::BenchmarkError::Other(anyhow::anyhow!(
                                "insert_vectors failed: {}",
                                e
                            ))
                        })
                } else {
                    adapter.load_dataset(&workload.documents)
                }
            } else {
                adapter.load_dataset(&workload.documents)
            };

            let r = r.and_then(|_| adapter.build_indexes());

            let elapsed = t.elapsed();
            match r {
                Ok(_) => {
                    debug!(db = db_name, elapsed = ?elapsed, "insert ok");
                    OperationMetrics::success(elapsed, workload.documents.len())
                }
                Err(e) => {
                    warn!(db = db_name, "insert failed: {}", e);
                    OperationMetrics::failure(elapsed, e.to_string())
                }
            }
        };

        // ── Queries ──────────────────────────────────────────────────────────
        let queries: Vec<OperationMetrics> = workload
            .queries
            .iter()
            .map(|qs| {
                let t = Instant::now();
                match adapter.execute(&qs.query) {
                    Ok(count) => {
                        let elapsed = t.elapsed();
                        debug!(db = db_name, label = %qs.label, elapsed = ?elapsed, "query ok");
                        OperationMetrics::success(elapsed, count)
                    }
                    Err(e) => {
                        let elapsed = t.elapsed();
                        warn!(db = db_name, label = %qs.label, "query failed: {}", e);
                        OperationMetrics::failure(elapsed, e.to_string())
                    }
                }
            })
            .collect();

        // ── Delete / cleanup ─────────────────────────────────────────────────
        let delete = {
            let t = Instant::now();
            match adapter.cleanup() {
                Ok(_) => {
                    let elapsed = t.elapsed();
                    debug!(db = db_name, elapsed = ?elapsed, "cleanup ok");
                    OperationMetrics::success(elapsed, 0)
                }
                Err(e) => {
                    let elapsed = t.elapsed();
                    warn!(db = db_name, "cleanup failed: {}", e);
                    OperationMetrics::failure(elapsed, e.to_string())
                }
            }
        };

        let _ = adapter.disconnect();

        BenchmarkResult::new(db_name, insert, queries, delete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workload::QuerySpec;
    use adapters::lancedb::LanceDbAdapter;
    use benchmark_config::DatabaseConfig;
    use serde_json::json;

    fn lancedb_config(name: &str) -> DatabaseConfig {
        DatabaseConfig {
            enabled: true,
            host: Some(format!("/tmp/bench_runner_{}", name)),
            port: None,
            username: None,
            password: None,
            database: Some(name.to_string()),
            pool_size: None,
            timeout_ms: None,
        }
    }

    fn sample_workload() -> Workload {
        let docs = vec![
            dataset::model::Document {
                id: "a".to_string(),
                title: None,
                text: "hello world".to_string(),
                metadata: json!({}),
            },
            dataset::model::Document {
                id: "b".to_string(),
                title: None,
                text: "foo bar baz".to_string(),
                metadata: json!({}),
            },
        ];
        Workload::new("runner-test")
            .with_documents(docs)
            .with_queries(vec![QuerySpec::new("noop", "SELECT 1")])
    }

    fn test_embedding_cfg() -> benchmark_config::EmbeddingConfig {
        benchmark_config::EmbeddingConfig {
            provider: "dummy".to_string(),
            batch_size: 8,
            chunk_size: 10,
            chunk_overlap: 2,
            cache_enabled: false,
            dimension: 64,
        }
    }

    #[test]
    fn test_runner_succeeds_with_lancedb() {
        let mut adapter = LanceDbAdapter::new(lancedb_config("runner_ok"));
        let workload = sample_workload();
        let cfg = test_embedding_cfg();
        let result = BenchmarkRunner::run("lancedb", &mut adapter, &workload, &cfg);

        assert_eq!(result.database, "lancedb");
        assert!(result.insert.status.is_success(), "insert must succeed");
        assert_eq!(result.queries.len(), 1);
        assert!(result.delete.status.is_success(), "cleanup must succeed");
        assert!(result.all_succeeded());
    }

    #[test]
    fn test_runner_records_query_count() {
        let mut adapter = LanceDbAdapter::new(lancedb_config("runner_qcount"));
        let workload = sample_workload().with_queries(vec![
            QuerySpec::new("q1", "SELECT 1"),
            QuerySpec::new("q2", "SELECT 2"),
            QuerySpec::new("q3", "SELECT 3"),
        ]);
        let cfg = test_embedding_cfg();
        let result = BenchmarkRunner::run("lancedb", &mut adapter, &workload, &cfg);
        assert_eq!(result.queries.len(), 3);
    }

    #[test]
    fn test_runner_insert_duration_is_nonzero() {
        let mut adapter = LanceDbAdapter::new(lancedb_config("runner_dur"));
        let workload = sample_workload();
        let cfg = test_embedding_cfg();
        let result = BenchmarkRunner::run("lancedb", &mut adapter, &workload, &cfg);
        // Duration must be non-zero (even nanoseconds); just check it was recorded
        assert!(result.insert.duration.as_nanos() > 0);
    }

    #[test]
    fn test_runner_returns_failure_on_disconnect_before_connect() {
        // Pass a config that makes connect return an error for postgres
        // (no real server) — runner must still return a BenchmarkResult.
        let mut adapter = adapters::postgres::PostgresAdapter::new(lancedb_config("unused"));
        let workload = Workload::new("fail-test");
        let cfg = test_embedding_cfg();
        let result = BenchmarkRunner::run("postgres", &mut adapter, &workload, &cfg);
        // Connect will fail; the result must still be returned (not panic).
        assert!(!result.all_succeeded());
    }
}
