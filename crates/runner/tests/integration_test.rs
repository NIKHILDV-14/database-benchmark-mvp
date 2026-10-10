use benchmark_config::{
    BenchmarkConfig, DatabaseConfig, DatabasesConfig, DatasetConfig, EmbeddingConfig,
    ExperimentConfig,
};
use recommendation::config::{MetricWeights, RecommendationConfig};
use runner::{BenchmarkPipeline, PipelineError, PipelineOptions};
use std::fs;
use std::path::PathBuf;

fn tmp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("runner_test_{}", name))
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
        host: Some(format!("/tmp/runner_integ_{}", name)),
        port: None,
        username: None,
        password: None,
        database: Some(name.to_string()),
        pool_size: None,
        timeout_ms: None,
    }
}

fn make_test_config(dataset_path: &str, lancedb: DatabaseConfig) -> BenchmarkConfig {
    BenchmarkConfig {
        experiment: ExperimentConfig {
            name: "integration-test".to_string(),
            seed: 42,
        },
        dataset: DatasetConfig {
            path: dataset_path.to_string(),
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
            chunk_size: 50,
            chunk_overlap: 5,
            cache_enabled: false,
            dimension: 64,
        },
    }
}

// ── 1. End-to-End with LanceDB & JSONL ─────────────────────────────────────────

#[test]
fn test_end_to_end_pipeline_with_lancedb() {
    let jsonl_path = tmp_file("valid.jsonl");
    let content = concat!(
        r#"{"id": "doc1", "title": "Doc 1", "text": "Databases and distributed systems.", "metadata": {"category": "db"}}"#,
        "\n",
        r#"{"id": "doc2", "title": "Doc 2", "text": "Vector embeddings and neural search.", "metadata": {"category": "search"}}"#,
        "\n"
    );
    fs::write(&jsonl_path, content).unwrap();

    let config = make_test_config(jsonl_path.to_str().unwrap(), lancedb_config("lancedb_ok"));

    let result = BenchmarkPipeline::run(&config).expect("Pipeline should succeed with LanceDB");

    // Environment
    assert!(result.environment.cpu_cores > 0);
    assert!(!result.environment.os.is_empty());

    // Dataset
    assert_eq!(result.dataset.document_count, 2);
    assert_eq!(result.dataset.format, "jsonl");
    assert!(!result.dataset.dataset_hash.is_empty());

    // Benchmark Execution
    assert_eq!(result.benchmark_results.len(), 1);
    let lancedb_res = &result.benchmark_results[0];
    assert_eq!(lancedb_res.database, "lancedb");
    assert!(lancedb_res.insert.status.is_success());
    assert!(lancedb_res.delete.status.is_success());

    // Evaluation
    assert!(result.evaluation_report.databases.contains_key("lancedb"));
    let eval = &result.evaluation_report.databases["lancedb"];
    assert_eq!(
        eval.validation_status,
        evaluation::models::ValidationStatus::Valid
    );

    // Recommendation
    assert!(result.has_recommendation());
    assert_eq!(result.best_database(), Some("lancedb"));
    assert_eq!(result.recommendation_report.ranked_candidates.len(), 1);
    assert!(result.recommendation_report.ranked_candidates[0].score > 0.0);

    // Formatted summary
    let summary = result.format_summary();
    assert!(summary.contains("lancedb"));
    assert!(summary.contains("BEST DATABASE RECOMMENDED"));

    let _ = fs::remove_file(jsonl_path);
}

// ── 2. End-to-End with CSV Dataset ────────────────────────────────────────────

#[test]
fn test_end_to_end_pipeline_with_csv() {
    let csv_path = tmp_file("valid.csv");
    let content = "id,title,text,category\n\
                   c1,First,Relational databases and transactions,relational\n\
                   c2,Second,NoSQL and document storage,nosql\n";
    fs::write(&csv_path, content).unwrap();

    let config = make_test_config(csv_path.to_str().unwrap(), lancedb_config("csv_ok"));

    let result = BenchmarkPipeline::run(&config).expect("Pipeline should succeed with CSV");

    assert_eq!(result.dataset.document_count, 2);
    assert_eq!(result.dataset.format, "csv");
    assert_eq!(result.best_database(), Some("lancedb"));

    let _ = fs::remove_file(csv_path);
}

// ── 3. Run from sample configuration file ─────────────────────────────────────

#[test]
fn test_run_from_lancedb_only_config_file() {
    let config_path = "configs/lancedb_only.toml";
    if std::path::Path::new(config_path).exists() {
        let result = BenchmarkPipeline::run_from_file(config_path)
            .expect("Running from lancedb_only.toml should succeed");

        assert_eq!(result.best_database(), Some("lancedb"));
        assert!(result.has_recommendation());
    }
}

// ── 4. Error Handling: Missing Config File ────────────────────────────────────

#[test]
fn test_pipeline_missing_config_file() {
    let res = BenchmarkPipeline::run_from_file("nonexistent_config_file_12345.toml");
    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), PipelineError::ConfigNotFound(_)));
}

// ── 5. Error Handling: Nonexistent Dataset File ───────────────────────────────

#[test]
fn test_pipeline_nonexistent_dataset() {
    let config = make_test_config("nonexistent_dataset.jsonl", lancedb_config("no_file"));
    let res = BenchmarkPipeline::run(&config);
    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), PipelineError::Dataset(_)));
}

// ── 6. Error Handling: Duplicate IDs in Dataset ───────────────────────────────

#[test]
fn test_pipeline_duplicate_ids_rejected() {
    let jsonl_path = tmp_file("dup.jsonl");
    let content = concat!(
        r#"{"id": "same_id", "title": "A", "text": "first", "metadata": {}}"#,
        "\n",
        r#"{"id": "same_id", "title": "B", "text": "second", "metadata": {}}"#,
        "\n"
    );
    fs::write(&jsonl_path, content).unwrap();

    let config = make_test_config(jsonl_path.to_str().unwrap(), lancedb_config("dup_test"));
    let res = BenchmarkPipeline::run(&config);

    assert!(res.is_err());
    assert!(
        matches!(res.unwrap_err(), PipelineError::Dataset(msg) if msg.contains("Duplicate ID"))
    );

    let _ = fs::remove_file(jsonl_path);
}

// ── 7. Unavailable Database Handled Gracefully ────────────────────────────────

#[test]
fn test_pipeline_handles_unavailable_database_gracefully() {
    let jsonl_path = tmp_file("unavail.jsonl");
    fs::write(
        &jsonl_path,
        r#"{"id": "doc1", "title": "A", "text": "test content", "metadata": {}}"#,
    )
    .unwrap();

    let mut config = make_test_config(jsonl_path.to_str().unwrap(), lancedb_config("live_db"));

    // Enable PostgreSQL pointing to an unreachable port
    config.databases.postgres = DatabaseConfig {
        enabled: true,
        host: Some("127.0.0.1".to_string()),
        port: Some(59999), // unreachable port
        username: Some("fake".to_string()),
        password: Some("fake".to_string()),
        database: Some("fake".to_string()),
        pool_size: None,
        timeout_ms: Some(100),
    };

    let result = BenchmarkPipeline::run(&config)
        .expect("Pipeline should not panic when a database is unreachable");

    // Both databases were attempted
    assert_eq!(result.benchmark_results.len(), 2);

    // Postgres failed to connect and was excluded
    let pg_eval = &result.evaluation_report.databases["postgres"];
    assert!(matches!(
        pg_eval.validation_status,
        evaluation::models::ValidationStatus::Invalid(_)
    ));

    // LanceDB succeeded and won the recommendation
    assert_eq!(result.best_database(), Some("lancedb"));
    assert_eq!(result.recommendation_report.excluded_candidates.len(), 1);
    assert_eq!(
        result.recommendation_report.excluded_candidates[0].database,
        "postgres"
    );

    let _ = fs::remove_file(jsonl_path);
}

// ── 8. All Databases Unavailable Handled Cleanly ──────────────────────────────

#[test]
fn test_pipeline_all_databases_unavailable() {
    let jsonl_path = tmp_file("all_down.jsonl");
    fs::write(
        &jsonl_path,
        r#"{"id": "doc1", "title": "A", "text": "test content", "metadata": {}}"#,
    )
    .unwrap();

    let mut config = make_test_config(jsonl_path.to_str().unwrap(), disabled_db());
    config.databases.postgres = DatabaseConfig {
        enabled: true,
        host: Some("127.0.0.1".to_string()),
        port: Some(59999), // unreachable
        username: None,
        password: None,
        database: None,
        pool_size: None,
        timeout_ms: Some(50),
    };

    let result = BenchmarkPipeline::run(&config)
        .expect("Pipeline should complete gracefully even if all databases fail");

    assert!(!result.has_recommendation());
    assert_eq!(result.best_database(), None);
    assert_eq!(result.recommendation_report.ranked_candidates.len(), 0);
    assert_eq!(result.recommendation_report.excluded_candidates.len(), 1);

    let _ = fs::remove_file(jsonl_path);
}

// ── 9. Custom Recommendation Weights ──────────────────────────────────────────

#[test]
fn test_pipeline_with_custom_weights() {
    let jsonl_path = tmp_file("weights.jsonl");
    fs::write(
        &jsonl_path,
        r#"{"id": "d1", "title": "T", "text": "content", "metadata": {}}"#,
    )
    .unwrap();

    let config = make_test_config(
        jsonl_path.to_str().unwrap(),
        lancedb_config("custom_weights"),
    );

    let options = PipelineOptions {
        recommendation_config: RecommendationConfig::new(
            MetricWeights::default()
                .with_latency(0.8)
                .with_throughput(0.2)
                .with_accuracy(0.0)
                .with_resource_usage(0.0),
        ),
        ..Default::default()
    };

    let result = BenchmarkPipeline::run_with_options(&config, &options).unwrap();
    assert_eq!(result.best_database(), Some("lancedb"));
    assert_eq!(
        result.recommendation_report.ranked_candidates[0]
            .contributing_metrics
            .get("latency"),
        Some(&1.0)
    );

    let _ = fs::remove_file(jsonl_path);
}

// ── 10. JSON Serialization of PipelineResult ──────────────────────────────────

#[test]
fn test_pipeline_result_json_serialization() {
    let jsonl_path = tmp_file("ser.jsonl");
    fs::write(
        &jsonl_path,
        r#"{"id": "doc1", "title": "T", "text": "hello", "metadata": {}}"#,
    )
    .unwrap();

    let config = make_test_config(jsonl_path.to_str().unwrap(), lancedb_config("ser_test"));
    let result = BenchmarkPipeline::run(&config).unwrap();

    let json_str = serde_json::to_string_pretty(&result).expect("Serialization should succeed");
    assert!(json_str.contains("environment"));
    assert!(json_str.contains("dataset"));
    assert!(json_str.contains("recommendation_report"));

    let _ = fs::remove_file(jsonl_path);
}

// ── 11. CLI Execution: Clean JSON Stdout Without Log Pollution ─────────────────

#[test]
fn test_cli_json_output_directly_parseable() {
    let config_path = "configs/lancedb_only.toml";
    if std::path::Path::new(config_path).exists() {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_runner"))
            .arg(config_path)
            .arg("--json")
            .output()
            .expect("Failed to execute runner CLI binary");

        assert!(
            output.status.success(),
            "Runner CLI should exit successfully"
        );

        // Standard output MUST be valid JSON directly with zero substring manipulation
        let stdout_str = String::from_utf8(output.stdout).expect("Stdout should be UTF-8");
        let parsed: serde_json::Value = serde_json::from_str(&stdout_str)
            .expect("Stdout should be parsed directly by serde_json without removing log lines");

        assert_eq!(parsed["recommendation_report"]["best_candidate"], "lancedb");
        assert!(parsed["environment"]["cpu_cores"].is_number());
        assert_eq!(parsed["dataset"]["format"], "jsonl");

        // Stderr should contain the tracing log lines
        let stderr_str = String::from_utf8(output.stderr).expect("Stderr should be UTF-8");
        assert!(stderr_str.contains("Running database benchmark pipeline"));
    }
}
