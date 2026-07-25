use anyhow::{bail, Result};
use common::types::EmbeddingResult;

pub struct EmbeddingValidator {
    pub expected_dimension: usize,
}

impl EmbeddingValidator {
    pub fn new(expected_dimension: usize) -> Self {
        Self { expected_dimension }
    }

    pub fn validate(&self, result: &EmbeddingResult) -> Result<()> {
        if result.vector.is_empty() {
            bail!("Embedding is empty");
        }
        if result.vector.len() != self.expected_dimension {
            bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.expected_dimension,
                result.vector.len()
            );
        }
        for (i, &val) in result.vector.iter().enumerate() {
            if val.is_nan() {
                bail!("Embedding contains NaN at index {}", i);
            }
            if val.is_infinite() {
                bail!("Embedding contains Infinity at index {}", i);
            }
        }
        Ok(())
    }

    pub fn validate_batch(&self, results: &[EmbeddingResult]) -> Result<()> {
        for (idx, res) in results.iter().enumerate() {
            self.validate(res)
                .map_err(|e| anyhow::anyhow!("Item {}: {}", idx, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_embedding() {
        let validator = EmbeddingValidator::new(3);
        let valid = EmbeddingResult {
            vector: vec![0.1, 0.2, 0.3],
        };
        assert!(validator.validate(&valid).is_ok());
    }

    #[test]
    fn test_empty_embedding() {
        let validator = EmbeddingValidator::new(3);
        let empty = EmbeddingResult { vector: vec![] };
        assert!(validator.validate(&empty).is_err());
    }

    #[test]
    fn test_wrong_dimension() {
        let validator = EmbeddingValidator::new(3);
        let wrong_dim = EmbeddingResult {
            vector: vec![0.1, 0.2],
        };
        assert!(validator.validate(&wrong_dim).is_err());
    }

    #[test]
    fn test_nan_embedding() {
        let validator = EmbeddingValidator::new(2);
        let nan = EmbeddingResult {
            vector: vec![0.1, f32::NAN],
        };
        assert!(validator.validate(&nan).is_err());
    }

    #[test]
    fn test_inf_embedding() {
        let validator = EmbeddingValidator::new(2);
        let inf = EmbeddingResult {
            vector: vec![f32::INFINITY, 0.2],
        };
        assert!(validator.validate(&inf).is_err());
    }
}
