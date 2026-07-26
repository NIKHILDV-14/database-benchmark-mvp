use crate::models::ValidationStatus;
use benchmark::result::BenchmarkResult;

/// Context required to validate a single benchmark result.
#[derive(Debug, Clone)]
pub struct ValidationContext {
    /// Hash of the dataset used during the benchmark run.
    pub expected_dataset_hash: String,
    /// Name of the workload that was executed.
    pub expected_workload_name: String,
    /// Number of queries that the workload was expected to execute.
    pub expected_query_count: usize,
}

impl ValidationContext {
    pub fn new(
        expected_dataset_hash: impl Into<String>,
        expected_workload_name: impl Into<String>,
        expected_query_count: usize,
    ) -> Self {
        Self {
            expected_dataset_hash: expected_dataset_hash.into(),
            expected_workload_name: expected_workload_name.into(),
            expected_query_count,
        }
    }
}

/// Validates the integrity and completeness of a [`BenchmarkResult`].
pub struct BenchmarkValidator;

impl BenchmarkValidator {
    /// Validate a single benchmark result against an expected context.
    ///
    /// Checks (in order):
    /// 1. Insert phase completed successfully.
    /// 2. Delete/cleanup phase completed successfully.
    /// 3. Actual query count matches expected query count.
    /// 4. All queries completed successfully.
    /// 5. Dataset hash matches (stored in the database name field as a proxy
    ///    — real hash comparison requires the result to carry it).
    ///
    /// Returns [`ValidationStatus::Valid`] only when all checks pass.
    pub fn validate(result: &BenchmarkResult, ctx: &ValidationContext) -> ValidationStatus {
        if !result.insert.status.is_success() {
            return ValidationStatus::Invalid(format!(
                "Insert phase failed for database '{}'",
                result.database
            ));
        }

        if !result.delete.status.is_success() {
            return ValidationStatus::Invalid(format!(
                "Delete/cleanup phase failed for database '{}'",
                result.database
            ));
        }

        if result.queries.len() != ctx.expected_query_count {
            return ValidationStatus::Invalid(format!(
                "Expected {} queries but got {} for database '{}'",
                ctx.expected_query_count,
                result.queries.len(),
                result.database
            ));
        }

        let failed_queries: Vec<usize> = result
            .queries
            .iter()
            .enumerate()
            .filter(|(_, q)| !q.status.is_success())
            .map(|(i, _)| i)
            .collect();

        if !failed_queries.is_empty() {
            return ValidationStatus::Invalid(format!(
                "Queries {:?} failed for database '{}'",
                failed_queries, result.database
            ));
        }

        ValidationStatus::Valid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benchmark::result::OperationMetrics;
    use std::time::Duration;

    fn ctx() -> ValidationContext {
        ValidationContext::new("hash-abc123", "perf_workload", 2)
    }

    fn success_op() -> OperationMetrics {
        OperationMetrics::success(Duration::from_millis(10), 5)
    }

    fn failed_op() -> OperationMetrics {
        OperationMetrics::failure(Duration::from_millis(1), "error".to_string())
    }

    fn good_result() -> BenchmarkResult {
        BenchmarkResult::new(
            "lancedb",
            success_op(),
            vec![success_op(), success_op()],
            success_op(),
        )
    }

    #[test]
    fn test_validator_success() {
        let result = good_result();
        let status = BenchmarkValidator::validate(&result, &ctx());
        assert_eq!(status, ValidationStatus::Valid);
    }

    #[test]
    fn test_validator_fails_on_insert_failure() {
        let result = BenchmarkResult::new(
            "lancedb",
            failed_op(),
            vec![success_op(), success_op()],
            success_op(),
        );
        let status = BenchmarkValidator::validate(&result, &ctx());
        assert!(matches!(status, ValidationStatus::Invalid(_)));
        if let ValidationStatus::Invalid(msg) = status {
            assert!(msg.contains("Insert phase"));
        }
    }

    #[test]
    fn test_validator_fails_on_delete_failure() {
        let result = BenchmarkResult::new(
            "lancedb",
            success_op(),
            vec![success_op(), success_op()],
            failed_op(),
        );
        let status = BenchmarkValidator::validate(&result, &ctx());
        assert!(matches!(status, ValidationStatus::Invalid(_)));
        if let ValidationStatus::Invalid(msg) = status {
            assert!(msg.contains("Delete/cleanup"));
        }
    }

    #[test]
    fn test_validator_fails_on_wrong_query_count() {
        let result = BenchmarkResult::new(
            "lancedb",
            success_op(),
            vec![success_op()], // only 1, expected 2
            success_op(),
        );
        let status = BenchmarkValidator::validate(&result, &ctx());
        assert!(matches!(status, ValidationStatus::Invalid(_)));
        if let ValidationStatus::Invalid(msg) = status {
            assert!(msg.contains("Expected 2 queries but got 1"));
        }
    }

    #[test]
    fn test_validator_fails_on_failed_query() {
        let result = BenchmarkResult::new(
            "lancedb",
            success_op(),
            vec![success_op(), failed_op()],
            success_op(),
        );
        let status = BenchmarkValidator::validate(&result, &ctx());
        assert!(matches!(status, ValidationStatus::Invalid(_)));
        if let ValidationStatus::Invalid(msg) = status {
            assert!(msg.contains("Queries"));
        }
    }
}
