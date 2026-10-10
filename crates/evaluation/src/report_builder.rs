use crate::accuracy::AccuracyEvaluator;
use crate::aggregator::{self, MetricsAggregator};
use crate::config::EvaluationConfig;
use crate::metrics::MetricCalculator;
use crate::models::{DatabaseEvaluation, EvaluationReport, MetricScore, ValidationStatus};
use crate::traits::{MetricEvaluator, ReportBuilder};
use crate::validator::{BenchmarkValidator, ValidationContext};
use benchmark::result::BenchmarkResult;
use std::collections::HashMap;

// ── EvaluationReportBuilder ──────────────────────────────────────────────────

/// Builds a complete [`EvaluationReport`] from a slice of [`BenchmarkResult`]s.
///
/// For each result the builder:
/// 1. Runs [`BenchmarkValidator`] to produce a [`ValidationStatus`].
/// 2. Runs [`MetricCalculator`] to produce named [`MetricScore`]s.
/// 3. Aggregates every numeric metric across all results (cross-database).
/// 4. Aggregates metrics across multiple runs of the **same** database.
/// 5. Emits one [`DatabaseEvaluation`] per unique database name.
///
/// Aggregated cross-database statistics are stored under the reserved key
/// `"__aggregate__<metric_name>"` so that consumers can retrieve them without
/// any schema changes.
///
/// Multi-run per-database aggregation is stored under
/// `"__runs__<database_name>"`.
///
/// Ground-truth accuracy metrics are left empty at this stage because ground
/// truth IDs are not carried inside [`BenchmarkResult`]; they must be supplied
/// externally via [`inject_accuracy`].
pub struct EvaluationReportBuilder {
    workload_name: String,
}

impl EvaluationReportBuilder {
    pub fn new(workload_name: impl Into<String>) -> Self {
        Self {
            workload_name: workload_name.into(),
        }
    }
}

impl ReportBuilder for EvaluationReportBuilder {
    /// Build a complete [`EvaluationReport`] from a slice of benchmark results.
    ///
    /// When the same database appears in multiple results (multiple runs),
    /// the *last* result's per-operation metrics win for the named
    /// `DatabaseEvaluation`, while the cross-run aggregate is stored under the
    /// `"__runs__<database>"` key.
    fn build_report(
        &self,
        results: &[BenchmarkResult],
        config: &EvaluationConfig,
    ) -> EvaluationReport {
        let calculator = MetricCalculator;

        // ── Per-database evaluations ──────────────────────────────────────────
        let mut databases: HashMap<String, DatabaseEvaluation> = HashMap::new();

        // Collect metric values keyed by name for cross-database aggregation.
        let mut cross_db_values: HashMap<String, Vec<f64>> = HashMap::new();

        // Collect metric values per database for multi-run aggregation.
        let mut per_db_aggregators: HashMap<String, MetricsAggregator> = HashMap::new();

        for result in results {
            let ctx = ValidationContext::new(
                "", // hash not carried in BenchmarkResult
                &self.workload_name,
                result.queries.len(),
            );
            let validation_status = BenchmarkValidator::validate(result, &ctx);

            let scores: Vec<MetricScore> = calculator.evaluate(result, config);

            // Accumulate values for cross-database aggregation.
            for score in &scores {
                cross_db_values
                    .entry(score.name.clone())
                    .or_default()
                    .push(score.value);
            }

            // Accumulate values for per-database multi-run aggregation.
            let db_agg = per_db_aggregators
                .entry(result.database.clone())
                .or_default();
            for score in &scores {
                db_agg.push(&score.name, score.value);
            }

            let metrics: HashMap<String, MetricScore> =
                scores.into_iter().map(|s| (s.name.clone(), s)).collect();

            // Last run per database wins for the primary evaluation entry.
            databases.insert(
                result.database.clone(),
                DatabaseEvaluation {
                    database_name: result.database.clone(),
                    validation_status,
                    metrics,
                },
            );
        }

        // ── Cross-database aggregated metrics ─────────────────────────────────
        // Stored under the key "__aggregate__<metric_name>" so consumers can
        // look them up without schema changes.
        for (metric_name, values) in &cross_db_values {
            if let Some(stat) = aggregator::aggregate(values) {
                let agg_db = databases
                    .entry(format!("__aggregate__{}", metric_name))
                    .or_insert_with(|| DatabaseEvaluation {
                        database_name: format!("__aggregate__{}", metric_name),
                        validation_status: ValidationStatus::Skipped,
                        metrics: HashMap::new(),
                    });

                insert_stat(agg_db, "mean", stat.mean, "");
                insert_stat(agg_db, "median", stat.median, "");
                insert_stat(agg_db, "min", stat.min, "");
                insert_stat(agg_db, "max", stat.max, "");
                insert_stat(agg_db, "variance", stat.variance, "");
                insert_stat(agg_db, "count", stat.count as f64, "count");
            }
        }

        // ── Per-database multi-run aggregated metrics ─────────────────────────
        // Stored under the key "__runs__<database_name>".
        // Only written when more than one run exists for a database.
        for (db_name, agg) in &per_db_aggregators {
            // Only produce a multi-run summary when there really are multiple
            // runs (i.e. any metric was observed more than once).
            let has_multiple_runs = agg
                .metric_names()
                .any(|m| agg.compute_for(m).map(|s| s.count > 1).unwrap_or(false));

            if has_multiple_runs {
                let run_entry_key = format!("__runs__{}", db_name);
                let run_db =
                    databases
                        .entry(run_entry_key.clone())
                        .or_insert_with(|| DatabaseEvaluation {
                            database_name: run_entry_key,
                            validation_status: ValidationStatus::Skipped,
                            metrics: HashMap::new(),
                        });

                for (metric_name, stat) in agg.compute() {
                    insert_stat(run_db, &format!("{}/mean", metric_name), stat.mean, "");
                    insert_stat(run_db, &format!("{}/median", metric_name), stat.median, "");
                    insert_stat(run_db, &format!("{}/min", metric_name), stat.min, "");
                    insert_stat(run_db, &format!("{}/max", metric_name), stat.max, "");
                    insert_stat(
                        run_db,
                        &format!("{}/variance", metric_name),
                        stat.variance,
                        "",
                    );
                    insert_stat(
                        run_db,
                        &format!("{}/count", metric_name),
                        stat.count as f64,
                        "count",
                    );
                }
            }
        }

        EvaluationReport {
            workload_name: self.workload_name.clone(),
            databases,
            timestamp: String::new(), // timestamp injection left to the caller
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Insert a single named stat into a [`DatabaseEvaluation`]'s metrics map.
fn insert_stat(db: &mut DatabaseEvaluation, name: &str, value: f64, unit: &str) {
    db.metrics.insert(
        name.to_string(),
        MetricScore {
            name: name.to_string(),
            value,
            unit: unit.to_string(),
        },
    );
}

// ── inject_accuracy ───────────────────────────────────────────────────────────

/// Evaluate accuracy for a single database result against provided ground truth
/// and inject the scores into an existing [`DatabaseEvaluation`].
///
/// Accuracy keys follow the pattern `"accuracy/<query_label>/<metric>"` where
/// `<metric>` is one of `precision`, `recall`, or `f1`.
///
/// Both slices are iterated in lock-step; any excess entries in either slice
/// are ignored.
pub fn inject_accuracy(
    evaluation: &mut DatabaseEvaluation,
    ground_truth_entries: &[crate::ground_truth::GroundTruth],
    returned_ids_per_query: &[Vec<String>],
) {
    for (gt, returned) in ground_truth_entries
        .iter()
        .zip(returned_ids_per_query.iter())
    {
        let score = AccuracyEvaluator::evaluate(gt, returned);
        let prefix = format!("accuracy/{}", gt.query_label);
        evaluation.metrics.insert(
            format!("{}/precision", prefix),
            MetricScore {
                name: format!("{}/precision", prefix),
                value: score.precision,
                unit: "ratio".to_string(),
            },
        );
        evaluation.metrics.insert(
            format!("{}/recall", prefix),
            MetricScore {
                name: format!("{}/recall", prefix),
                value: score.recall,
                unit: "ratio".to_string(),
            },
        );
        evaluation.metrics.insert(
            format!("{}/f1", prefix),
            MetricScore {
                name: format!("{}/f1", prefix),
                value: score.f1,
                unit: "ratio".to_string(),
            },
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use benchmark::result::OperationMetrics;
    use std::time::Duration;

    // ── Fixtures ─────────────────────────────────────────────────────────────

    fn success_op(ms: u64) -> OperationMetrics {
        OperationMetrics::success(Duration::from_millis(ms), 5)
    }

    fn failed_op() -> OperationMetrics {
        OperationMetrics::failure(Duration::from_millis(1), "error".to_string())
    }

    fn make_result(db: &str, q_ms: &[u64]) -> BenchmarkResult {
        BenchmarkResult::new(
            db,
            success_op(100),
            q_ms.iter().map(|&ms| success_op(ms)).collect(),
            success_op(50),
        )
    }

    // ── Report generation ─────────────────────────────────────────────────────

    #[test]
    fn test_build_report_empty_results() {
        let builder = EvaluationReportBuilder::new("empty_workload");
        let report = builder.build_report(&[], &EvaluationConfig::default());
        assert_eq!(report.workload_name, "empty_workload");
        assert!(report.databases.is_empty());
    }

    #[test]
    fn test_build_report_single_database() {
        let result = make_result("lancedb", &[10, 20]);
        let builder = EvaluationReportBuilder::new("perf_test");
        let report = builder.build_report(&[result], &EvaluationConfig::default());

        assert_eq!(report.workload_name, "perf_test");
        assert!(report.databases.contains_key("lancedb"));

        let eval = &report.databases["lancedb"];
        assert_eq!(eval.database_name, "lancedb");
        assert_eq!(eval.validation_status, ValidationStatus::Valid);
        assert!(eval.metrics.contains_key("insert_latency"));
        assert!(eval.metrics.contains_key("delete_latency"));
        assert!(eval.metrics.contains_key("success_rate"));
    }

    #[test]
    fn test_build_report_multiple_databases() {
        let results = vec![make_result("lancedb", &[10]), make_result("qdrant", &[20])];
        let builder = EvaluationReportBuilder::new("multi_db_test");
        let report = builder.build_report(&results, &EvaluationConfig::default());

        assert!(report.databases.contains_key("lancedb"));
        assert!(report.databases.contains_key("qdrant"));
    }

    // ── Metrics propagation ───────────────────────────────────────────────────

    #[test]
    fn test_metrics_propagated_to_evaluation() {
        let result = make_result("lancedb", &[10, 20, 30]);
        let builder = EvaluationReportBuilder::new("metric_prop_test");
        let report = builder.build_report(&[result], &EvaluationConfig::default());

        let eval = &report.databases["lancedb"];
        // Check several core metrics are present and have plausible values.
        assert!(eval.metrics.contains_key("insert_latency"));
        assert!(eval.metrics.contains_key("average_query_latency"));
        assert!(eval.metrics.contains_key("throughput"));
        assert!(eval.metrics.contains_key("success_rate"));
        assert_eq!(eval.metrics["success_rate"].value, 100.0);
        assert_eq!(eval.metrics["insert_latency"].value, 100.0); // 100 ms fixture
    }

    // ── Aggregation ───────────────────────────────────────────────────────────

    #[test]
    fn test_aggregation_keys_present() {
        let results = vec![make_result("lancedb", &[10]), make_result("qdrant", &[20])];
        let builder = EvaluationReportBuilder::new("agg_test");
        let report = builder.build_report(&results, &EvaluationConfig::default());

        // Aggregate entries are stored under __aggregate__<metric_name>
        let agg_key = "__aggregate__insert_latency";
        assert!(
            report.databases.contains_key(agg_key),
            "Expected aggregate entry for insert_latency"
        );
        let agg = &report.databases[agg_key];
        assert!(agg.metrics.contains_key("mean"));
        assert!(agg.metrics.contains_key("min"));
        assert!(agg.metrics.contains_key("max"));
        assert!(agg.metrics.contains_key("variance"));
        assert!(agg.metrics.contains_key("median"));
        assert!(agg.metrics.contains_key("count"));
    }

    #[test]
    fn test_aggregation_values_correct() {
        // Two databases with the same insert_latency fixture (100 ms each)
        let results = vec![make_result("lancedb", &[10]), make_result("qdrant", &[20])];
        let builder = EvaluationReportBuilder::new("agg_values_test");
        let report = builder.build_report(&results, &EvaluationConfig::default());

        let agg = &report.databases["__aggregate__insert_latency"];
        // Both inserts are 100 ms → mean = 100, min = 100, max = 100
        assert_eq!(agg.metrics["mean"].value, 100.0);
        assert_eq!(agg.metrics["min"].value, 100.0);
        assert_eq!(agg.metrics["max"].value, 100.0);
        assert_eq!(agg.metrics["count"].value, 2.0);
    }

    // ── Multiple benchmark runs (same database) ───────────────────────────────

    #[test]
    fn test_multiple_runs_same_database_produces_run_aggregate() {
        // Simulate two runs of the same database in the results slice.
        let run1 = BenchmarkResult::new(
            "lancedb",
            success_op(100),
            vec![success_op(10)],
            success_op(50),
        );
        let run2 = BenchmarkResult::new(
            "lancedb",
            success_op(200),
            vec![success_op(20)],
            success_op(60),
        );
        let builder = EvaluationReportBuilder::new("multi_run_test");
        let report = builder.build_report(&[run1, run2], &EvaluationConfig::default());

        // The primary entry should exist (last run wins).
        assert!(report.databases.contains_key("lancedb"));

        // The multi-run aggregate entry should be produced.
        assert!(
            report.databases.contains_key("__runs__lancedb"),
            "Expected a __runs__lancedb entry for multi-run aggregation"
        );

        let runs = &report.databases["__runs__lancedb"];
        // insert_latency was 100 ms in run1 and 200 ms in run2 → mean = 150.
        let mean_key = "insert_latency/mean";
        assert!(
            runs.metrics.contains_key(mean_key),
            "Expected {mean_key} in multi-run aggregate"
        );
        assert_eq!(runs.metrics[mean_key].value, 150.0);

        let min_key = "insert_latency/min";
        assert_eq!(runs.metrics[min_key].value, 100.0);

        let max_key = "insert_latency/max";
        assert_eq!(runs.metrics[max_key].value, 200.0);
    }

    #[test]
    fn test_single_run_does_not_produce_run_aggregate() {
        let result = make_result("lancedb", &[10]);
        let builder = EvaluationReportBuilder::new("single_run_test");
        let report = builder.build_report(&[result], &EvaluationConfig::default());

        // With only one run, no __runs__ entry should be produced.
        assert!(
            !report.databases.contains_key("__runs__lancedb"),
            "Unexpected __runs__lancedb entry for a single run"
        );
    }

    // ── Validation propagation ────────────────────────────────────────────────

    #[test]
    fn test_validation_failure_propagated() {
        let result = BenchmarkResult::new(
            "postgres",
            failed_op(),
            vec![success_op(10)],
            success_op(50),
        );
        let builder = EvaluationReportBuilder::new("fail_test");
        let report = builder.build_report(&[result], &EvaluationConfig::default());

        let eval = &report.databases["postgres"];
        assert!(matches!(
            eval.validation_status,
            ValidationStatus::Invalid(_)
        ));
    }

    #[test]
    fn test_validation_success_propagated() {
        let result = make_result("lancedb", &[10, 20]);
        let builder = EvaluationReportBuilder::new("valid_test");
        let report = builder.build_report(&[result], &EvaluationConfig::default());

        let eval = &report.databases["lancedb"];
        assert_eq!(eval.validation_status, ValidationStatus::Valid);
    }

    #[test]
    fn test_validation_delete_failure_propagated() {
        let result = BenchmarkResult::new(
            "mongo",
            success_op(100),
            vec![success_op(10)],
            failed_op(), // delete failure
        );
        let builder = EvaluationReportBuilder::new("del_fail_test");
        let report = builder.build_report(&[result], &EvaluationConfig::default());

        let eval = &report.databases["mongo"];
        assert!(matches!(
            eval.validation_status,
            ValidationStatus::Invalid(_)
        ));
        if let ValidationStatus::Invalid(msg) = &eval.validation_status {
            assert!(msg.contains("Delete/cleanup"));
        }
    }

    // ── Accuracy propagation ──────────────────────────────────────────────────

    #[test]
    fn test_inject_accuracy_perfect() {
        use crate::ground_truth::GroundTruth;

        let result = make_result("lancedb", &[10]);
        let builder = EvaluationReportBuilder::new("acc_test");
        let mut report = builder.build_report(&[result], &EvaluationConfig::default());

        let eval = report.databases.get_mut("lancedb").unwrap();
        let gt = vec![GroundTruth::new(
            "q1",
            vec!["a".to_string(), "b".to_string()],
        )];
        let returned = vec![vec!["a".to_string(), "b".to_string()]];
        inject_accuracy(eval, &gt, &returned);

        assert!(eval.metrics.contains_key("accuracy/q1/precision"));
        assert_eq!(eval.metrics["accuracy/q1/precision"].value, 1.0);
        assert_eq!(eval.metrics["accuracy/q1/recall"].value, 1.0);
        assert_eq!(eval.metrics["accuracy/q1/f1"].value, 1.0);
    }

    #[test]
    fn test_inject_accuracy_partial_match() {
        use crate::ground_truth::GroundTruth;

        let result = make_result("lancedb", &[10]);
        let builder = EvaluationReportBuilder::new("acc_partial_test");
        let mut report = builder.build_report(&[result], &EvaluationConfig::default());

        let eval = report.databases.get_mut("lancedb").unwrap();
        // Expected: a, b, c  – returned: a, b, x
        // TP=2, FP=1, FN=1 → precision=2/3, recall=2/3, f1=2/3
        let gt = vec![GroundTruth::new(
            "q1",
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        )];
        let returned = vec![vec!["a".to_string(), "b".to_string(), "x".to_string()]];
        inject_accuracy(eval, &gt, &returned);

        let p = eval.metrics["accuracy/q1/precision"].value;
        let r = eval.metrics["accuracy/q1/recall"].value;
        let f = eval.metrics["accuracy/q1/f1"].value;
        assert!((p - 2.0 / 3.0).abs() < 1e-9);
        assert!((r - 2.0 / 3.0).abs() < 1e-9);
        assert!((f - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_inject_accuracy_multiple_queries() {
        use crate::ground_truth::GroundTruth;

        let result = make_result("lancedb", &[10, 10]);
        let builder = EvaluationReportBuilder::new("acc_multi_q_test");
        let mut report = builder.build_report(&[result], &EvaluationConfig::default());

        let eval = report.databases.get_mut("lancedb").unwrap();
        let gt = vec![
            GroundTruth::new("q1", vec!["a".to_string()]),
            GroundTruth::new("q2", vec!["b".to_string()]),
        ];
        let returned = vec![vec!["a".to_string()], vec!["b".to_string()]];
        inject_accuracy(eval, &gt, &returned);

        assert_eq!(eval.metrics["accuracy/q1/precision"].value, 1.0);
        assert_eq!(eval.metrics["accuracy/q2/precision"].value, 1.0);
    }

    #[test]
    fn test_inject_accuracy_no_matches() {
        use crate::ground_truth::GroundTruth;

        let result = make_result("lancedb", &[10]);
        let builder = EvaluationReportBuilder::new("acc_no_match_test");
        let mut report = builder.build_report(&[result], &EvaluationConfig::default());

        let eval = report.databases.get_mut("lancedb").unwrap();
        let gt = vec![GroundTruth::new(
            "q1",
            vec!["a".to_string(), "b".to_string()],
        )];
        let returned = vec![vec!["x".to_string(), "y".to_string()]];
        inject_accuracy(eval, &gt, &returned);

        assert_eq!(eval.metrics["accuracy/q1/precision"].value, 0.0);
        assert_eq!(eval.metrics["accuracy/q1/recall"].value, 0.0);
        assert_eq!(eval.metrics["accuracy/q1/f1"].value, 0.0);
    }

    // ── Workload information ──────────────────────────────────────────────────

    #[test]
    fn test_report_contains_workload_name() {
        let result = make_result("db", &[10]);
        let builder = EvaluationReportBuilder::new("my_workload");
        let report = builder.build_report(&[result], &EvaluationConfig::default());
        assert_eq!(report.workload_name, "my_workload");
    }

    // ── Database information ──────────────────────────────────────────────────

    #[test]
    fn test_report_contains_database_name() {
        let result = make_result("super_db", &[10]);
        let builder = EvaluationReportBuilder::new("wl");
        let report = builder.build_report(&[result], &EvaluationConfig::default());

        let eval = &report.databases["super_db"];
        assert_eq!(eval.database_name, "super_db");
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    #[test]
    fn test_empty_benchmark_list_is_safe() {
        let builder = EvaluationReportBuilder::new("empty");
        let report = builder.build_report(&[], &EvaluationConfig::default());
        assert!(report.databases.is_empty());
        assert_eq!(report.workload_name, "empty");
    }

    #[test]
    fn test_three_databases_aggregation_count() {
        let results = vec![
            make_result("a", &[10]),
            make_result("b", &[20]),
            make_result("c", &[30]),
        ];
        let builder = EvaluationReportBuilder::new("three_db_test");
        let report = builder.build_report(&results, &EvaluationConfig::default());

        let agg = &report.databases["__aggregate__insert_latency"];
        assert_eq!(agg.metrics["count"].value, 3.0);
        // All three inserts are 100 ms in the fixture
        assert_eq!(agg.metrics["mean"].value, 100.0);
        assert_eq!(agg.metrics["variance"].value, 0.0);
    }
}
