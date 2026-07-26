use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationReport {
    pub workload_name: String,
    pub databases: HashMap<String, DatabaseEvaluation>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseEvaluation {
    pub database_name: String,
    pub validation_status: ValidationStatus,
    pub metrics: HashMap<String, MetricScore>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricScore {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ValidationStatus {
    Valid,
    Invalid(String),
    Skipped,
}

impl Default for EvaluationReport {
    fn default() -> Self {
        Self {
            workload_name: "default".to_string(),
            databases: HashMap::new(),
            timestamp: "".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluation_report_default() {
        let report = EvaluationReport::default();
        assert_eq!(report.workload_name, "default");
        assert!(report.databases.is_empty());
        assert_eq!(report.timestamp, "");
    }

    #[test]
    fn test_validation_status_variants() {
        let valid = ValidationStatus::Valid;
        let invalid = ValidationStatus::Invalid("failed".to_string());
        let skipped = ValidationStatus::Skipped;

        assert_eq!(valid, ValidationStatus::Valid);
        assert_eq!(invalid, ValidationStatus::Invalid("failed".to_string()));
        assert_eq!(skipped, ValidationStatus::Skipped);
    }

    #[test]
    fn test_models_serialization() {
        let mut report = EvaluationReport::default();
        let eval = DatabaseEvaluation {
            database_name: "test_db".to_string(),
            validation_status: ValidationStatus::Valid,
            metrics: HashMap::new(),
        };
        report.databases.insert("test_db".to_string(), eval);

        let serialized = serde_json::to_string(&report).unwrap();
        let deserialized: EvaluationReport = serde_json::from_str(&serialized).unwrap();

        assert_eq!(report, deserialized);
    }
}
