use crate::BenchmarkConfig;
use common::error::{BenchmarkError, Result};

pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate(config: &BenchmarkConfig) -> Result<()> {
        if config.dataset.max_size_gb <= 0.0 {
            return Err(BenchmarkError::Validation(
                "max_size_gb must be > 0".to_string(),
            ));
        }

        if config.dataset.max_size_gb > 2.0 {
            return Err(BenchmarkError::Validation(
                "MVP only supports up to 2 GB datasets".to_string(),
            ));
        }

        if !config.databases.postgres.enabled
            && !config.databases.mongodb.enabled
            && !config.databases.qdrant.enabled
            && !config.databases.lancedb.enabled
        {
            return Err(BenchmarkError::Validation(
                "At least one database must be enabled".to_string(),
            ));
        }

        Ok(())
    }
}
