use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationConfig {
    pub enable_validation: bool,
    pub strict_mode: bool,
    pub default_thresholds: DefaultThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DefaultThresholds {
    pub min_recall: f64,
    pub max_latency_ms: u64,
}

impl Default for EvaluationConfig {
    fn default() -> Self {
        Self {
            enable_validation: true,
            strict_mode: false,
            default_thresholds: DefaultThresholds::default(),
        }
    }
}

impl Default for DefaultThresholds {
    fn default() -> Self {
        Self {
            min_recall: 0.95,
            max_latency_ms: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluation_config_default() {
        let config = EvaluationConfig::default();
        assert!(config.enable_validation);
        assert!(!config.strict_mode);
        assert_eq!(config.default_thresholds.min_recall, 0.95);
        assert_eq!(config.default_thresholds.max_latency_ms, 100);
    }

    #[test]
    fn test_evaluation_config_serialization() {
        let config = EvaluationConfig::default();
        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: EvaluationConfig = serde_json::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }
}
