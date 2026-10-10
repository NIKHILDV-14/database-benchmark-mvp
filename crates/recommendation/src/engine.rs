use crate::config::{ConfigError, RecommendationConfig};
use crate::models::RecommendationReport;
use crate::scoring::ScoreCalculator;
use evaluation::models::EvaluationReport;

/// The Recommendation Engine evaluates database performance from Phase 6
/// reports and recommends the most suitable database for a given workload.
#[derive(Debug, Clone)]
pub struct RecommendationEngine {
    config: RecommendationConfig,
}

impl RecommendationEngine {
    /// Creates a new `RecommendationEngine` with the given configuration.
    /// Safely sanitizes any invalid weights to prevent runtime panics.
    pub fn new(config: RecommendationConfig) -> Self {
        Self {
            config: config.sanitized(),
        }
    }

    /// Creates a new `RecommendationEngine` with strict validation.
    /// Returns `Err(ConfigError)` if the configuration is invalid.
    pub fn try_new(config: RecommendationConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Returns a reference to the active recommendation configuration.
    pub fn config(&self) -> &RecommendationConfig {
        &self.config
    }

    /// Evaluates database candidates in the `EvaluationReport` and produces
    /// a ranked `RecommendationReport`.
    pub fn recommend(&self, report: &EvaluationReport) -> RecommendationReport {
        ScoreCalculator::compute_recommendations(report, &self.config)
    }

    /// Alias for `recommend`.
    pub fn evaluate(&self, report: &EvaluationReport) -> RecommendationReport {
        self.recommend(report)
    }
}

impl Default for RecommendationEngine {
    fn default() -> Self {
        Self::new(RecommendationConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MetricWeights;
    use crate::models::CandidateStatus;
    use evaluation::models::{DatabaseEvaluation, MetricScore, ValidationStatus};
    use std::collections::HashMap;

    // ── Helper fixtures ───────────────────────────────────────────────────────

    fn make_metric(name: &str, value: f64, unit: &str) -> MetricScore {
        MetricScore {
            name: name.to_string(),
            value,
            unit: unit.to_string(),
        }
    }

    fn make_valid_eval(db: &str, latency_ms: f64, throughput_ops: f64) -> DatabaseEvaluation {
        let mut metrics = HashMap::new();
        metrics.insert(
            "average_query_latency".to_string(),
            make_metric("average_query_latency", latency_ms, "ms"),
        );
        metrics.insert(
            "throughput".to_string(),
            make_metric("throughput", throughput_ops, "ops/sec"),
        );
        metrics.insert(
            "success_rate".to_string(),
            make_metric("success_rate", 100.0, "%"),
        );

        DatabaseEvaluation {
            database_name: db.to_string(),
            validation_status: ValidationStatus::Valid,
            metrics,
        }
    }

    fn make_report(workload: &str, evals: Vec<DatabaseEvaluation>) -> EvaluationReport {
        let mut databases = HashMap::new();
        for e in evals {
            databases.insert(e.database_name.clone(), e);
        }

        EvaluationReport {
            workload_name: workload.to_string(),
            databases,
            timestamp: "2026-10-10T12:00:00Z".to_string(),
        }
    }

    // ── Required Test Cases ───────────────────────────────────────────────────

    #[test]
    fn test_ranking_using_different_metric_values() {
        // qdrant has low latency and high throughput -> clearly superior
        // postgres has high latency and low throughput -> inferior
        let db1 = make_valid_eval("qdrant", 10.0, 1000.0);
        let db2 = make_valid_eval("postgres", 100.0, 100.0);

        let report = make_report("search_workload", vec![db1, db2]);
        let engine = RecommendationEngine::default();
        let rec = engine.recommend(&report);

        assert_eq!(rec.ranked_candidates.len(), 2);
        assert_eq!(rec.best_candidate.as_deref(), Some("qdrant"));
        assert_eq!(rec.ranked_candidates[0].database, "qdrant");
        assert_eq!(rec.ranked_candidates[0].rank, 1);
        assert!(rec.ranked_candidates[0].score > rec.ranked_candidates[1].score);
        assert_eq!(rec.ranked_candidates[1].database, "postgres");
        assert_eq!(rec.ranked_candidates[1].rank, 2);
    }

    #[test]
    fn test_configurable_weights() {
        // db_fast_low_tp has ultra low latency (5ms), but modest throughput (200 ops/s)
        // db_slow_high_tp has higher latency (50ms), but huge throughput (2000 ops/s)
        let db_a = make_valid_eval("db_fast_low_tp", 5.0, 200.0);
        let db_b = make_valid_eval("db_slow_high_tp", 50.0, 2000.0);

        let report = make_report("weight_test", vec![db_a, db_b]);

        // When latency is heavily weighted: db_fast_low_tp should win
        let latency_weights = MetricWeights::default()
            .with_latency(0.9)
            .with_throughput(0.1)
            .with_accuracy(0.0)
            .with_resource_usage(0.0);
        let latency_engine = RecommendationEngine::new(RecommendationConfig::new(latency_weights));
        let rec_lat = latency_engine.recommend(&report);

        assert_eq!(rec_lat.best_candidate.as_deref(), Some("db_fast_low_tp"));
        assert_eq!(rec_lat.ranked_candidates[0].database, "db_fast_low_tp");

        // When throughput is heavily weighted: db_slow_high_tp should win
        let tp_weights = MetricWeights::default()
            .with_latency(0.1)
            .with_throughput(0.9)
            .with_accuracy(0.0)
            .with_resource_usage(0.0);
        let tp_engine = RecommendationEngine::new(RecommendationConfig::new(tp_weights));
        let rec_tp = tp_engine.recommend(&report);

        assert_eq!(rec_tp.best_candidate.as_deref(), Some("db_slow_high_tp"));
        assert_eq!(rec_tp.ranked_candidates[0].database, "db_slow_high_tp");
    }

    #[test]
    fn test_invalid_evaluations_excluded() {
        let valid_db = make_valid_eval("lancedb", 15.0, 800.0);

        let mut invalid_metrics = HashMap::new();
        invalid_metrics.insert(
            "throughput".to_string(),
            make_metric("throughput", 5000.0, "ops/s"),
        );
        let invalid_db = DatabaseEvaluation {
            database_name: "broken_db".to_string(),
            validation_status: ValidationStatus::Invalid(
                "Insert failed due to connection refused".to_string(),
            ),
            metrics: invalid_metrics,
        };

        let report = make_report("validation_exclusion", vec![valid_db, invalid_db]);
        let engine = RecommendationEngine::default();
        let rec = engine.recommend(&report);

        // Only lancedb should be ranked
        assert_eq!(rec.ranked_candidates.len(), 1);
        assert_eq!(rec.ranked_candidates[0].database, "lancedb");
        assert_eq!(rec.best_candidate.as_deref(), Some("lancedb"));

        // broken_db must be in excluded_candidates with proper reason
        assert_eq!(rec.excluded_candidates.len(), 1);
        assert_eq!(rec.excluded_candidates[0].database, "broken_db");
        assert!(rec.excluded_candidates[0].is_excluded());
        assert!(matches!(
            rec.excluded_candidates[0].status,
            CandidateStatus::Excluded(ref reason) if reason.contains("connection refused")
        ));
        assert!(rec.excluded_candidates[0]
            .explanation
            .contains("validation failed"));
    }

    #[test]
    fn test_missing_metrics_handled_without_inventing_values() {
        // db_full has latency, throughput, and accuracy
        let mut full_metrics = HashMap::new();
        full_metrics.insert(
            "average_query_latency".to_string(),
            make_metric("average_query_latency", 10.0, "ms"),
        );
        full_metrics.insert(
            "throughput".to_string(),
            make_metric("throughput", 1000.0, "ops/s"),
        );
        full_metrics.insert(
            "accuracy".to_string(),
            make_metric("accuracy", 0.95, "ratio"),
        );
        let db_full = DatabaseEvaluation {
            database_name: "db_full".to_string(),
            validation_status: ValidationStatus::Valid,
            metrics: full_metrics,
        };

        // db_partial is missing accuracy and resource usage
        let mut partial_metrics = HashMap::new();
        partial_metrics.insert(
            "average_query_latency".to_string(),
            make_metric("average_query_latency", 20.0, "ms"),
        );
        partial_metrics.insert(
            "throughput".to_string(),
            make_metric("throughput", 500.0, "ops/s"),
        );
        let db_partial = DatabaseEvaluation {
            database_name: "db_partial".to_string(),
            validation_status: ValidationStatus::Valid,
            metrics: partial_metrics,
        };

        let report = make_report("missing_metrics_test", vec![db_full, db_partial]);
        let engine = RecommendationEngine::default();
        let rec = engine.recommend(&report);

        assert_eq!(rec.ranked_candidates.len(), 2);

        let full_res = rec
            .ranked_candidates
            .iter()
            .find(|r| r.database == "db_full")
            .unwrap();
        let partial_res = rec
            .ranked_candidates
            .iter()
            .find(|r| r.database == "db_partial")
            .unwrap();

        // db_full has accuracy in contributing metrics
        assert!(full_res.contributing_metrics.contains_key("accuracy"));
        assert!(full_res.contributing_metrics.contains_key("latency"));
        assert!(full_res.contributing_metrics.contains_key("throughput"));

        // db_partial does NOT have accuracy invented into contributing metrics
        assert!(!partial_res.contributing_metrics.contains_key("accuracy"));
        assert!(partial_res.contributing_metrics.contains_key("latency"));
        assert!(partial_res.contributing_metrics.contains_key("throughput"));

        // Both candidates have finite valid scores
        assert!(full_res.score >= 0.0 && full_res.score <= 1.0);
        assert!(partial_res.score >= 0.0 && partial_res.score <= 1.0);
    }

    #[test]
    fn test_equal_scores_and_deterministic_tie_breaking() {
        // Two databases with identical metrics
        let db_z = make_valid_eval("z_database", 25.0, 500.0);
        let db_a = make_valid_eval("a_database", 25.0, 500.0);

        let report = make_report("tie_breaking", vec![db_z, db_a]);
        let engine = RecommendationEngine::default();

        // Run multiple times to verify determinism
        for _ in 0..5 {
            let rec = engine.recommend(&report);
            assert_eq!(rec.ranked_candidates.len(), 2);
            assert_eq!(
                rec.ranked_candidates[0].score,
                rec.ranked_candidates[1].score
            );

            // Alphabetical tie-breaking: a_database must come first
            assert_eq!(rec.ranked_candidates[0].database, "a_database");
            assert_eq!(rec.ranked_candidates[0].rank, 1);
            assert_eq!(rec.ranked_candidates[1].database, "z_database");
            assert_eq!(rec.ranked_candidates[1].rank, 2);
            assert_eq!(rec.best_candidate.as_deref(), Some("a_database"));
        }
    }

    #[test]
    fn test_empty_input() {
        let empty_report = EvaluationReport {
            workload_name: "empty_workload".to_string(),
            databases: HashMap::new(),
            timestamp: "".to_string(),
        };

        let engine = RecommendationEngine::default();
        let rec = engine.recommend(&empty_report);

        assert_eq!(rec.workload_name, "empty_workload");
        assert!(rec.ranked_candidates.is_empty());
        assert!(rec.excluded_candidates.is_empty());
        assert_eq!(rec.best_candidate, None);
        assert!(rec.is_empty());
        assert!(rec.summary.contains("No database candidates evaluated"));
    }

    #[test]
    fn test_invalid_configuration() {
        // Negative weight
        let neg_weights = MetricWeights::default().with_latency(-1.0);
        let neg_cfg = RecommendationConfig::new(neg_weights);
        assert!(neg_cfg.validate().is_err());
        assert!(RecommendationEngine::try_new(neg_cfg.clone()).is_err());

        // Safe constructor should sanitize rather than panic
        let safe_engine = RecommendationEngine::new(neg_cfg);
        assert!(safe_engine.config().validate().is_ok());

        // Zero total weight
        let zero_weights = MetricWeights::new(0.0, 0.0, 0.0, 0.0);
        let zero_cfg = RecommendationConfig::new(zero_weights);
        assert!(zero_cfg.validate().is_err());

        // Invalid min success rate
        let invalid_sr = RecommendationConfig::default().with_min_success_rate(-5.0);
        assert!(invalid_sr.validate().is_err());
    }

    #[test]
    fn test_no_eligible_candidates() {
        let mut m1 = HashMap::new();
        m1.insert("latency".to_string(), make_metric("latency", 10.0, "ms"));
        let db1 = DatabaseEvaluation {
            database_name: "db1".to_string(),
            validation_status: ValidationStatus::Invalid("Schema error".to_string()),
            metrics: m1,
        };

        let mut m2 = HashMap::new();
        m2.insert("latency".to_string(), make_metric("latency", 20.0, "ms"));
        let db2 = DatabaseEvaluation {
            database_name: "db2".to_string(),
            validation_status: ValidationStatus::Invalid("Timeout exceeded".to_string()),
            metrics: m2,
        };

        let report = make_report("all_invalid", vec![db1, db2]);
        let engine = RecommendationEngine::default();
        let rec = engine.recommend(&report);

        assert!(rec.ranked_candidates.is_empty());
        assert_eq!(rec.best_candidate, None);
        assert_eq!(rec.excluded_candidates.len(), 2);
        assert!(!rec.has_eligible_candidates());
        assert!(rec
            .summary
            .contains("All 2 evaluated candidate(s) were excluded"));
    }

    #[test]
    fn test_explanation_consistency_with_calculated_scores() {
        let db1 = make_valid_eval("lancedb", 10.0, 1000.0);
        let db2 = make_valid_eval("qdrant", 20.0, 500.0);

        let report = make_report("explanation_test", vec![db1, db2]);
        let engine = RecommendationEngine::default();
        let rec = engine.recommend(&report);

        for candidate in &rec.ranked_candidates {
            // Explanation must mention rank and score
            assert!(candidate
                .explanation
                .contains(&format!("Ranked #{}", candidate.rank)));
            let score_str = format!("{:.2}", candidate.score);
            assert!(candidate.explanation.contains(&score_str));

            // Explanation must mention each contributing metric
            for (metric_name, &val) in &candidate.contributing_metrics {
                assert!(candidate.explanation.contains(metric_name));
                let val_str = format!("{:.2}", val);
                assert!(candidate.explanation.contains(&val_str));
            }
        }
    }

    #[test]
    fn test_resource_usage_metric_lower_is_better() {
        let mut m_low_res = HashMap::new();
        m_low_res.insert(
            "average_latency".to_string(),
            make_metric("average_latency", 10.0, "ms"),
        );
        m_low_res.insert(
            "throughput".to_string(),
            make_metric("throughput", 1000.0, "ops/s"),
        );
        m_low_res.insert(
            "memory_mb".to_string(),
            make_metric("memory_mb", 100.0, "MB"),
        ); // 100 MB

        let mut m_high_res = HashMap::new();
        m_high_res.insert(
            "average_latency".to_string(),
            make_metric("average_latency", 10.0, "ms"),
        );
        m_high_res.insert(
            "throughput".to_string(),
            make_metric("throughput", 1000.0, "ops/s"),
        );
        m_high_res.insert(
            "memory_mb".to_string(),
            make_metric("memory_mb", 1000.0, "MB"),
        ); // 1000 MB

        let db_low = DatabaseEvaluation {
            database_name: "db_lean".to_string(),
            validation_status: ValidationStatus::Valid,
            metrics: m_low_res,
        };
        let db_high = DatabaseEvaluation {
            database_name: "db_heavy".to_string(),
            validation_status: ValidationStatus::Valid,
            metrics: m_high_res,
        };

        let report = make_report("resource_test", vec![db_low, db_high]);

        // When resource usage has weight, db_lean should win
        let weights = MetricWeights::default().with_resource_usage(0.5);
        let engine = RecommendationEngine::new(RecommendationConfig::new(weights));
        let rec = engine.recommend(&report);

        assert_eq!(rec.best_candidate.as_deref(), Some("db_lean"));
        let lean_result = rec
            .ranked_candidates
            .iter()
            .find(|r| r.database == "db_lean")
            .unwrap();
        let heavy_result = rec
            .ranked_candidates
            .iter()
            .find(|r| r.database == "db_heavy")
            .unwrap();

        assert_eq!(
            lean_result.contributing_metrics.get("resource_usage"),
            Some(&1.0)
        );
        assert_eq!(
            heavy_result.contributing_metrics.get("resource_usage"),
            Some(&0.0)
        );
        assert!(lean_result.score > heavy_result.score);
    }

    #[test]
    fn test_internal_keys_filtered_out() {
        let valid_db = make_valid_eval("lancedb", 10.0, 1000.0);

        let mut agg_metrics = HashMap::new();
        agg_metrics.insert("mean".to_string(), make_metric("mean", 10.0, "ms"));
        let agg_entry = DatabaseEvaluation {
            database_name: "__aggregate__insert_latency".to_string(),
            validation_status: ValidationStatus::Skipped,
            metrics: agg_metrics,
        };

        let runs_entry = DatabaseEvaluation {
            database_name: "__runs__lancedb".to_string(),
            validation_status: ValidationStatus::Skipped,
            metrics: HashMap::new(),
        };

        let report = make_report("filter_internal", vec![valid_db, agg_entry, runs_entry]);
        let engine = RecommendationEngine::default();
        let rec = engine.recommend(&report);

        // Only lancedb should be evaluated or ranked
        assert_eq!(rec.ranked_candidates.len(), 1);
        assert_eq!(rec.ranked_candidates[0].database, "lancedb");
        assert!(rec.excluded_candidates.is_empty());
    }

    #[test]
    fn test_multi_query_accuracy_averaging() {
        let mut m1 = HashMap::new();
        m1.insert(
            "average_latency".to_string(),
            make_metric("average_latency", 10.0, "ms"),
        );
        m1.insert(
            "throughput".to_string(),
            make_metric("throughput", 100.0, "ops/s"),
        );
        m1.insert(
            "accuracy/q1/f1".to_string(),
            make_metric("accuracy/q1/f1", 1.0, "ratio"),
        );
        m1.insert(
            "accuracy/q2/f1".to_string(),
            make_metric("accuracy/q2/f1", 0.8, "ratio"),
        ); // avg = 0.9

        let mut m2 = HashMap::new();
        m2.insert(
            "average_latency".to_string(),
            make_metric("average_latency", 10.0, "ms"),
        );
        m2.insert(
            "throughput".to_string(),
            make_metric("throughput", 100.0, "ops/s"),
        );
        m2.insert(
            "accuracy/q1/f1".to_string(),
            make_metric("accuracy/q1/f1", 0.6, "ratio"),
        );
        m2.insert(
            "accuracy/q2/f1".to_string(),
            make_metric("accuracy/q2/f1", 0.4, "ratio"),
        ); // avg = 0.5

        let db1 = DatabaseEvaluation {
            database_name: "high_acc_db".to_string(),
            validation_status: ValidationStatus::Valid,
            metrics: m1,
        };
        let db2 = DatabaseEvaluation {
            database_name: "low_acc_db".to_string(),
            validation_status: ValidationStatus::Valid,
            metrics: m2,
        };

        let report = make_report("accuracy_avg_test", vec![db1, db2]);
        let weights = MetricWeights::accuracy_focused();
        let engine = RecommendationEngine::new(RecommendationConfig::new(weights));
        let rec = engine.recommend(&report);

        assert_eq!(rec.best_candidate.as_deref(), Some("high_acc_db"));
        assert!(rec.ranked_candidates[0].score > rec.ranked_candidates[1].score);
    }
}
