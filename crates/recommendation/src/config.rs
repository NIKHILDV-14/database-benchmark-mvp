use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors arising from invalid recommendation configurations.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ConfigError {
    #[error("Negative weight for metric '{metric}': {weight}")]
    NegativeWeight { metric: String, weight: f64 },

    #[error("Non-finite weight (NaN or infinite) for metric '{metric}'")]
    NonFiniteWeight { metric: String },

    #[error("Total metric weight must be positive, but sum of weights is {sum}")]
    ZeroTotalWeight { sum: f64 },

    #[error("Invalid min_success_rate: {0} (must be between 0.0 and 100.0)")]
    InvalidMinSuccessRate(f64),
}

/// Weights assigned to different metric dimensions when scoring database candidates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricWeights {
    pub latency: f64,
    pub throughput: f64,
    pub accuracy: f64,
    pub resource_usage: f64,
}

pub type ScoringWeights = MetricWeights;

impl MetricWeights {
    /// Creates a new `MetricWeights` instance with specified weights.
    pub fn new(latency: f64, throughput: f64, accuracy: f64, resource_usage: f64) -> Self {
        Self {
            latency,
            throughput,
            accuracy,
            resource_usage,
        }
    }

    /// Equal weights across all four dimensions (0.25 each).
    pub fn equal() -> Self {
        Self {
            latency: 0.25,
            throughput: 0.25,
            accuracy: 0.25,
            resource_usage: 0.25,
        }
    }

    /// Latency-focused weighting profile.
    pub fn latency_focused() -> Self {
        Self {
            latency: 0.70,
            throughput: 0.20,
            accuracy: 0.10,
            resource_usage: 0.0,
        }
    }

    /// Throughput-focused weighting profile.
    pub fn throughput_focused() -> Self {
        Self {
            latency: 0.20,
            throughput: 0.70,
            accuracy: 0.10,
            resource_usage: 0.0,
        }
    }

    /// Accuracy-focused weighting profile.
    pub fn accuracy_focused() -> Self {
        Self {
            latency: 0.15,
            throughput: 0.15,
            accuracy: 0.70,
            resource_usage: 0.0,
        }
    }

    /// Sets the latency weight.
    pub fn with_latency(mut self, weight: f64) -> Self {
        self.latency = weight;
        self
    }

    /// Sets the throughput weight.
    pub fn with_throughput(mut self, weight: f64) -> Self {
        self.throughput = weight;
        self
    }

    /// Sets the accuracy weight.
    pub fn with_accuracy(mut self, weight: f64) -> Self {
        self.accuracy = weight;
        self
    }

    /// Sets the resource usage weight.
    pub fn with_resource_usage(mut self, weight: f64) -> Self {
        self.resource_usage = weight;
        self
    }

    /// Validates the metric weights.
    /// Returns `Err` if any weight is negative, non-finite (NaN or Inf),
    /// or if the sum of all weights is non-positive.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let check_weight = |name: &str, val: f64| -> Result<(), ConfigError> {
            if val.is_nan() || val.is_infinite() {
                return Err(ConfigError::NonFiniteWeight {
                    metric: name.to_string(),
                });
            }
            if val < 0.0 {
                return Err(ConfigError::NegativeWeight {
                    metric: name.to_string(),
                    weight: val,
                });
            }
            Ok(())
        };

        check_weight("latency", self.latency)?;
        check_weight("throughput", self.throughput)?;
        check_weight("accuracy", self.accuracy)?;
        check_weight("resource_usage", self.resource_usage)?;

        let sum = self.latency + self.throughput + self.accuracy + self.resource_usage;
        if sum <= 0.0 {
            return Err(ConfigError::ZeroTotalWeight { sum });
        }

        Ok(())
    }

    /// Safely sanitizes invalid weights by clamping negatives and NaNs to 0.0,
    /// and falling back to default weights if the sum is non-positive.
    pub fn sanitized(&self) -> Self {
        let sanitize = |v: f64| {
            if v.is_nan() || v.is_infinite() || v < 0.0 {
                0.0
            } else {
                v
            }
        };

        let mut s = Self {
            latency: sanitize(self.latency),
            throughput: sanitize(self.throughput),
            accuracy: sanitize(self.accuracy),
            resource_usage: sanitize(self.resource_usage),
        };

        let sum = s.latency + s.throughput + s.accuracy + s.resource_usage;
        if sum <= 0.0 {
            s = Self::default();
        }
        s
    }
}

impl Default for MetricWeights {
    fn default() -> Self {
        Self {
            latency: 0.40,
            throughput: 0.40,
            accuracy: 0.20,
            resource_usage: 0.0,
        }
    }
}

/// Configuration for the Recommendation Engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecommendationConfig {
    /// Weights for each metric dimension.
    pub weights: MetricWeights,
    /// Minimum success rate required for a candidate to be eligible (0.0 to 100.0).
    pub min_success_rate: f64,
    /// Whether to require `ValidationStatus::Valid` or `Skipped` (rejects `Invalid`).
    pub require_valid_status: bool,
}

impl RecommendationConfig {
    /// Creates a new configuration with the given weights.
    pub fn new(weights: MetricWeights) -> Self {
        Self {
            weights,
            min_success_rate: 0.0,
            require_valid_status: true,
        }
    }

    /// Sets the minimum required success rate.
    pub fn with_min_success_rate(mut self, min_rate: f64) -> Self {
        self.min_success_rate = min_rate;
        self
    }

    /// Configures whether to require valid status.
    pub fn with_require_valid_status(mut self, require: bool) -> Self {
        self.require_valid_status = require;
        self
    }

    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.weights.validate()?;
        if self.min_success_rate.is_nan()
            || self.min_success_rate.is_infinite()
            || self.min_success_rate < 0.0
            || self.min_success_rate > 100.0
        {
            return Err(ConfigError::InvalidMinSuccessRate(self.min_success_rate));
        }
        Ok(())
    }

    /// Safely sanitizes the configuration, ensuring all values are valid.
    pub fn sanitized(&self) -> Self {
        let sanitized_sr = if self.min_success_rate.is_nan() || self.min_success_rate < 0.0 {
            0.0
        } else if self.min_success_rate > 100.0 {
            100.0
        } else {
            self.min_success_rate
        };

        Self {
            weights: self.weights.sanitized(),
            min_success_rate: sanitized_sr,
            require_valid_status: self.require_valid_status,
        }
    }
}

impl Default for RecommendationConfig {
    fn default() -> Self {
        Self {
            weights: MetricWeights::default(),
            min_success_rate: 0.0,
            require_valid_status: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = RecommendationConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_negative_weight_validation() {
        let weights = MetricWeights::default().with_latency(-0.5);
        let config = RecommendationConfig::new(weights);
        assert!(matches!(
            config.validate(),
            Err(ConfigError::NegativeWeight { .. })
        ));
    }

    #[test]
    fn test_nan_weight_validation() {
        let weights = MetricWeights::default().with_throughput(f64::NAN);
        let config = RecommendationConfig::new(weights);
        assert!(matches!(
            config.validate(),
            Err(ConfigError::NonFiniteWeight { .. })
        ));
    }

    #[test]
    fn test_zero_total_weight_validation() {
        let weights = MetricWeights::new(0.0, 0.0, 0.0, 0.0);
        let config = RecommendationConfig::new(weights);
        assert!(matches!(
            config.validate(),
            Err(ConfigError::ZeroTotalWeight { .. })
        ));
    }

    #[test]
    fn test_invalid_min_success_rate() {
        let config = RecommendationConfig::default().with_min_success_rate(-10.0);
        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidMinSuccessRate(_))
        ));

        let config2 = RecommendationConfig::default().with_min_success_rate(150.0);
        assert!(matches!(
            config2.validate(),
            Err(ConfigError::InvalidMinSuccessRate(_))
        ));
    }

    #[test]
    fn test_sanitized_config() {
        let invalid = RecommendationConfig {
            weights: MetricWeights::new(-1.0, f64::NAN, 0.0, 0.0),
            min_success_rate: -5.0,
            require_valid_status: true,
        };
        let sanitized = invalid.sanitized();
        assert!(sanitized.validate().is_ok());
        assert_eq!(sanitized.min_success_rate, 0.0);
    }
}
