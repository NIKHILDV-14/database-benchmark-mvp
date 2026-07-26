use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Expected results for a single query used as ground truth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroundTruth {
    /// Identifier for the query this ground truth belongs to.
    pub query_label: String,
    /// Expected document/vector IDs that should be returned.
    pub expected_ids: Vec<String>,
}

impl GroundTruth {
    pub fn new(query_label: impl Into<String>, expected_ids: Vec<String>) -> Self {
        Self {
            query_label: query_label.into(),
            expected_ids,
        }
    }

    /// Build a `HashSet` of the expected IDs for efficient intersection.
    pub fn expected_set(&self) -> HashSet<&str> {
        self.expected_ids.iter().map(|s| s.as_str()).collect()
    }
}

/// Accuracy scores produced by comparing returned results against ground truth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccuracyScore {
    pub query_label: String,
    /// TP / (TP + FP) — how many returned results were relevant.
    pub precision: f64,
    /// TP / (TP + FN) — how many relevant results were returned.
    pub recall: f64,
    /// Harmonic mean of precision and recall.
    pub f1: f64,
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ground_truth_new() {
        let gt = GroundTruth::new("q1", vec!["a".to_string(), "b".to_string()]);
        assert_eq!(gt.query_label, "q1");
        assert_eq!(gt.expected_ids.len(), 2);
    }

    #[test]
    fn test_expected_set() {
        let gt = GroundTruth::new("q1", vec!["x".to_string(), "y".to_string()]);
        let set = gt.expected_set();
        assert!(set.contains("x"));
        assert!(set.contains("y"));
        assert!(!set.contains("z"));
    }

    #[test]
    fn test_accuracy_score_fields() {
        let score = AccuracyScore {
            query_label: "q1".to_string(),
            precision: 1.0,
            recall: 1.0,
            f1: 1.0,
            true_positives: 5,
            false_positives: 0,
            false_negatives: 0,
        };
        assert_eq!(score.precision, 1.0);
        assert_eq!(score.recall, 1.0);
        assert_eq!(score.f1, 1.0);
    }
}
