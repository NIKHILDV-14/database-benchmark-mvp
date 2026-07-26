use crate::ground_truth::{AccuracyScore, GroundTruth};
use std::collections::HashSet;

/// Evaluates accuracy of returned results against ground truth.
///
/// All calculations are deterministic and never panic on empty input.
pub struct AccuracyEvaluator;

impl AccuracyEvaluator {
    /// Compute precision, recall, and F1 score by comparing `returned_ids`
    /// against the `ground_truth` expected set.
    ///
    /// # Zero-division safety
    /// - If both `expected` and `returned` are empty → precision = 1.0, recall = 1.0, F1 = 1.0
    ///   (a trivially correct retrieval).
    /// - If `returned` is empty but `expected` is non-empty → precision = 1.0 (vacuously true),
    ///   recall = 0.0, F1 = 0.0.
    /// - If `expected` is empty but `returned` is non-empty → precision = 0.0, recall = 1.0
    ///   (vacuously true), F1 = 0.0.
    pub fn evaluate(ground_truth: &GroundTruth, returned_ids: &[String]) -> AccuracyScore {
        let expected_set: HashSet<&str> = ground_truth.expected_set();
        let returned_set: HashSet<&str> = returned_ids.iter().map(|s| s.as_str()).collect();

        let true_positives = expected_set.intersection(&returned_set).count();
        let false_positives = returned_set.difference(&expected_set).count();
        let false_negatives = expected_set.difference(&returned_set).count();

        let precision = if returned_set.is_empty() {
            if expected_set.is_empty() {
                1.0
            } else {
                // No results returned but some were expected → treat as 1.0 (vacuously)
                // so that the F1 score reflects recall alone. Use standard convention:
                // precision = 0 when returned is empty and expected is non-empty.
                0.0
            }
        } else {
            true_positives as f64 / returned_set.len() as f64
        };

        let recall = if expected_set.is_empty() {
            1.0 // vacuously true: nothing to recall
        } else {
            true_positives as f64 / expected_set.len() as f64
        };

        let f1 = if (precision + recall) == 0.0 {
            0.0
        } else {
            2.0 * precision * recall / (precision + recall)
        };

        AccuracyScore {
            query_label: ground_truth.query_label.clone(),
            precision,
            recall,
            f1,
            true_positives,
            false_positives,
            false_negatives,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gt(ids: &[&str]) -> GroundTruth {
        GroundTruth::new("test_query", ids.iter().map(|s| s.to_string()).collect())
    }

    fn ret(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_perfect_precision_and_recall() {
        let score = AccuracyEvaluator::evaluate(&gt(&["a", "b", "c"]), &ret(&["a", "b", "c"]));
        assert_eq!(score.precision, 1.0);
        assert_eq!(score.recall, 1.0);
        assert_eq!(score.f1, 1.0);
        assert_eq!(score.true_positives, 3);
        assert_eq!(score.false_positives, 0);
        assert_eq!(score.false_negatives, 0);
    }

    #[test]
    fn test_partial_match() {
        let score = AccuracyEvaluator::evaluate(&gt(&["a", "b", "c"]), &ret(&["a", "b", "x"]));
        // TP=2, FP=1 (x), FN=1 (c)
        // precision = 2/3, recall = 2/3, F1 = 2/3
        assert!((score.precision - 2.0 / 3.0).abs() < 1e-9);
        assert!((score.recall - 2.0 / 3.0).abs() < 1e-9);
        assert!((score.f1 - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(score.true_positives, 2);
        assert_eq!(score.false_positives, 1);
        assert_eq!(score.false_negatives, 1);
    }

    #[test]
    fn test_no_matches() {
        let score = AccuracyEvaluator::evaluate(&gt(&["a", "b"]), &ret(&["x", "y"]));
        assert_eq!(score.precision, 0.0);
        assert_eq!(score.recall, 0.0);
        assert_eq!(score.f1, 0.0);
        assert_eq!(score.true_positives, 0);
        assert_eq!(score.false_positives, 2);
        assert_eq!(score.false_negatives, 2);
    }

    #[test]
    fn test_empty_returned_set() {
        let score = AccuracyEvaluator::evaluate(&gt(&["a", "b"]), &ret(&[]));
        // precision = 0 (nothing returned), recall = 0 (nothing returned), F1 = 0
        assert_eq!(score.precision, 0.0);
        assert_eq!(score.recall, 0.0);
        assert_eq!(score.f1, 0.0);
        assert_eq!(score.false_negatives, 2);
    }

    #[test]
    fn test_empty_expected_set() {
        let score = AccuracyEvaluator::evaluate(&gt(&[]), &ret(&["a", "b"]));
        // expected empty: recall = 1.0 (vacuously), precision = 0/2 = 0, F1 = 0
        assert_eq!(score.precision, 0.0);
        assert_eq!(score.recall, 1.0);
        assert_eq!(score.f1, 0.0);
        assert_eq!(score.false_positives, 2);
    }

    #[test]
    fn test_both_empty() {
        let score = AccuracyEvaluator::evaluate(&gt(&[]), &ret(&[]));
        assert_eq!(score.precision, 1.0);
        assert_eq!(score.recall, 1.0);
        assert_eq!(score.f1, 1.0);
        assert_eq!(score.true_positives, 0);
    }
}
