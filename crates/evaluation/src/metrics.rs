use crate::config::EvaluationConfig;
use crate::models::MetricScore;
use crate::statistics;
use crate::traits::MetricEvaluator;
use benchmark::result::BenchmarkResult;

pub struct MetricCalculator;

impl MetricEvaluator for MetricCalculator {
    fn evaluate(&self, result: &BenchmarkResult, _config: &EvaluationConfig) -> Vec<MetricScore> {
        let mut scores = Vec::new();

        // Operation counts
        let total_ops = 2 + result.queries.len(); // insert + delete + queries
        let mut successful_ops = 0;
        let mut failed_ops = 0;

        let mut total_duration_secs = 0.0;
        let mut query_durations = Vec::new();

        // Process insert
        if result.insert.status.is_success() {
            successful_ops += 1;
            total_duration_secs += result.insert.duration.as_secs_f64();
            scores.push(MetricScore {
                name: "insert_latency".to_string(),
                value: result.insert.duration.as_secs_f64() * 1000.0,
                unit: "ms".to_string(),
            });
        } else {
            failed_ops += 1;
        }

        // Process queries
        for q in &result.queries {
            if q.status.is_success() {
                successful_ops += 1;
                total_duration_secs += q.duration.as_secs_f64();
                query_durations.push(q.duration);
            } else {
                failed_ops += 1;
            }
        }

        // Process delete
        if result.delete.status.is_success() {
            successful_ops += 1;
            total_duration_secs += result.delete.duration.as_secs_f64();
            scores.push(MetricScore {
                name: "delete_latency".to_string(),
                value: result.delete.duration.as_secs_f64() * 1000.0,
                unit: "ms".to_string(),
            });
        } else {
            failed_ops += 1;
        }

        // Query latency stats
        if let Some(avg) = statistics::mean(&query_durations) {
            scores.push(MetricScore {
                name: "average_query_latency".to_string(),
                value: avg.as_secs_f64() * 1000.0,
                unit: "ms".to_string(),
            });
        }

        if let Some(min) = statistics::min(&query_durations) {
            scores.push(MetricScore {
                name: "min_query_latency".to_string(),
                value: min.as_secs_f64() * 1000.0,
                unit: "ms".to_string(),
            });
        }

        if let Some(max) = statistics::max(&query_durations) {
            scores.push(MetricScore {
                name: "max_query_latency".to_string(),
                value: max.as_secs_f64() * 1000.0,
                unit: "ms".to_string(),
            });
        }

        let mut all_durations = query_durations.clone();
        if result.insert.status.is_success() {
            all_durations.push(result.insert.duration);
        }
        if result.delete.status.is_success() {
            all_durations.push(result.delete.duration);
        }

        if let Some(avg) = statistics::mean(&all_durations) {
            scores.push(MetricScore {
                name: "average_latency".to_string(),
                value: avg.as_secs_f64() * 1000.0,
                unit: "ms".to_string(),
            });
        }

        if let Some(min) = statistics::min(&all_durations) {
            scores.push(MetricScore {
                name: "min_latency".to_string(),
                value: min.as_secs_f64() * 1000.0,
                unit: "ms".to_string(),
            });
        }

        if let Some(max) = statistics::max(&all_durations) {
            scores.push(MetricScore {
                name: "max_latency".to_string(),
                value: max.as_secs_f64() * 1000.0,
                unit: "ms".to_string(),
            });
        }

        // Throughput
        let throughput = if total_duration_secs > 0.0 {
            successful_ops as f64 / total_duration_secs
        } else {
            0.0
        };

        scores.push(MetricScore {
            name: "throughput".to_string(),
            value: throughput,
            unit: "ops/sec".to_string(),
        });

        // Success rate
        let success_rate = if total_ops > 0 {
            (successful_ops as f64 / total_ops as f64) * 100.0
        } else {
            0.0
        };

        scores.push(MetricScore {
            name: "success_rate".to_string(),
            value: success_rate,
            unit: "%".to_string(),
        });

        scores.push(MetricScore {
            name: "total_operations".to_string(),
            value: total_ops as f64,
            unit: "count".to_string(),
        });

        scores.push(MetricScore {
            name: "successful_operations".to_string(),
            value: successful_ops as f64,
            unit: "count".to_string(),
        });

        scores.push(MetricScore {
            name: "failed_operations".to_string(),
            value: failed_ops as f64,
            unit: "count".to_string(),
        });

        scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benchmark::result::{OperationMetrics, OperationStatus};
    use std::time::Duration;

    fn get_value(scores: &[MetricScore], name: &str) -> Option<f64> {
        scores.iter().find(|s| s.name == name).map(|s| s.value)
    }

    #[test]
    fn test_metric_calculator_all_success() {
        let result = BenchmarkResult {
            database: "test".to_string(),
            insert: OperationMetrics::success(Duration::from_millis(100), 10),
            queries: vec![
                OperationMetrics::success(Duration::from_millis(10), 5),
                OperationMetrics::success(Duration::from_millis(20), 5),
            ],
            delete: OperationMetrics::success(Duration::from_millis(50), 10),
        };

        let calculator = MetricCalculator;
        let scores = calculator.evaluate(&result, &EvaluationConfig::default());

        assert_eq!(get_value(&scores, "total_operations"), Some(4.0));
        assert_eq!(get_value(&scores, "successful_operations"), Some(4.0));
        assert_eq!(get_value(&scores, "failed_operations"), Some(0.0));
        assert_eq!(get_value(&scores, "success_rate"), Some(100.0));

        assert_eq!(get_value(&scores, "insert_latency"), Some(100.0));
        assert_eq!(get_value(&scores, "delete_latency"), Some(50.0));
        assert_eq!(get_value(&scores, "average_query_latency"), Some(15.0));

        // Throughput: 4 ops / 0.18s = 22.222...
        let throughput = get_value(&scores, "throughput").unwrap();
        assert!((throughput - 22.222).abs() < 0.01);
    }

    #[test]
    fn test_metric_calculator_some_failures() {
        let result = BenchmarkResult {
            database: "test".to_string(),
            insert: OperationMetrics::success(Duration::from_millis(100), 10),
            queries: vec![
                OperationMetrics::success(Duration::from_millis(10), 5),
                OperationMetrics::failure(Duration::from_millis(5), "err".to_string()),
            ],
            delete: OperationMetrics::failure(Duration::from_millis(5), "err".to_string()),
        };

        let calculator = MetricCalculator;
        let scores = calculator.evaluate(&result, &EvaluationConfig::default());

        assert_eq!(get_value(&scores, "total_operations"), Some(4.0));
        assert_eq!(get_value(&scores, "successful_operations"), Some(2.0));
        assert_eq!(get_value(&scores, "failed_operations"), Some(2.0));
        assert_eq!(get_value(&scores, "success_rate"), Some(50.0));

        // only successful operations are factored into latency and throughput durations
        assert_eq!(get_value(&scores, "insert_latency"), Some(100.0));
        assert_eq!(get_value(&scores, "delete_latency"), None);
        assert_eq!(get_value(&scores, "average_query_latency"), Some(10.0));
    }
}
