use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The outcome of a single timed operation against one database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OperationStatus {
    /// The operation completed successfully.
    Success,
    /// The operation failed; the message contains the error description.
    Failure(String),
}

impl OperationStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
}

/// Raw metrics captured for a single benchmark operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationMetrics {
    /// Wall-clock duration of the operation.
    pub duration: Duration,
    /// Whether the operation succeeded.
    pub status: OperationStatus,
    /// Number of rows / documents / vectors returned (0 for writes).
    pub result_count: usize,
}

impl OperationMetrics {
    pub fn success(duration: Duration, result_count: usize) -> Self {
        Self {
            duration,
            status: OperationStatus::Success,
            result_count,
        }
    }

    pub fn failure(duration: Duration, message: String) -> Self {
        Self {
            duration,
            status: OperationStatus::Failure(message),
            result_count: 0,
        }
    }
}

/// All raw metrics collected for one database during a full benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// Human-readable name of the database (e.g. `"lancedb"`, `"postgres"`).
    pub database: String,
    /// Metrics for the dataset insert / load phase.
    pub insert: OperationMetrics,
    /// Metrics per query that was executed.
    pub queries: Vec<OperationMetrics>,
    /// Metrics for the cleanup / delete phase.
    pub delete: OperationMetrics,
}

impl BenchmarkResult {
    pub fn new(
        database: impl Into<String>,
        insert: OperationMetrics,
        queries: Vec<OperationMetrics>,
        delete: OperationMetrics,
    ) -> Self {
        Self {
            database: database.into(),
            insert,
            queries,
            delete,
        }
    }

    /// Returns `true` only when every phase completed without error.
    pub fn all_succeeded(&self) -> bool {
        self.insert.status.is_success()
            && self.queries.iter().all(|q| q.status.is_success())
            && self.delete.status.is_success()
    }

    /// Average query duration across all successful queries.
    pub fn avg_query_duration(&self) -> Option<Duration> {
        let successful: Vec<_> = self
            .queries
            .iter()
            .filter(|q| q.status.is_success())
            .collect();
        if successful.is_empty() {
            return None;
        }
        let total: Duration = successful.iter().map(|q| q.duration).sum();
        Some(total / successful.len() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_status_is_success() {
        assert!(OperationStatus::Success.is_success());
        assert!(!OperationStatus::Failure("err".to_string()).is_success());
    }

    #[test]
    fn test_benchmark_result_all_succeeded() {
        let ok = OperationMetrics::success(Duration::from_millis(10), 5);
        let result = BenchmarkResult::new("lancedb", ok.clone(), vec![ok.clone(), ok.clone()], ok);
        assert!(result.all_succeeded());
    }

    #[test]
    fn test_benchmark_result_fails_when_insert_fails() {
        let ok = OperationMetrics::success(Duration::from_millis(10), 0);
        let fail = OperationMetrics::failure(Duration::from_millis(1), "db down".to_string());
        let result = BenchmarkResult::new("postgres", fail, vec![ok.clone()], ok);
        assert!(!result.all_succeeded());
    }

    #[test]
    fn test_avg_query_duration() {
        let q1 = OperationMetrics::success(Duration::from_millis(100), 1);
        let q2 = OperationMetrics::success(Duration::from_millis(200), 1);
        let ok = OperationMetrics::success(Duration::from_millis(1), 0);
        let result = BenchmarkResult::new("qdrant", ok.clone(), vec![q1, q2], ok);
        assert_eq!(
            result.avg_query_duration(),
            Some(Duration::from_millis(150))
        );
    }

    #[test]
    fn test_avg_query_duration_none_when_all_failed() {
        let fail = OperationMetrics::failure(Duration::from_millis(5), "err".into());
        let ok = OperationMetrics::success(Duration::from_millis(1), 0);
        let result = BenchmarkResult::new("mongo", ok.clone(), vec![fail], ok);
        assert_eq!(result.avg_query_duration(), None);
    }
}
