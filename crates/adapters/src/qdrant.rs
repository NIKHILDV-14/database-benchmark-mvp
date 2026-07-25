use crate::traits::DatabaseAdapter;
use benchmark_config::DatabaseConfig;
use common::error::{BenchmarkError, Result};
use common::types::DatabaseCapabilities;
use dataset::model::Document;

pub struct QdrantAdapter {
    #[allow(dead_code)]
    config: DatabaseConfig,
}

impl QdrantAdapter {
    pub fn new(config: DatabaseConfig) -> Self {
        Self { config }
    }
}

impl DatabaseAdapter for QdrantAdapter {
    fn initialize(&mut self) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }

    fn connect(&mut self) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }

    fn disconnect(&mut self) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }

    fn health(&mut self) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }

    fn capabilities(&self) -> DatabaseCapabilities {
        DatabaseCapabilities {
            vector_search: true,
            metadata_filter: true,
            hybrid_search: true,
            batch_insert: true,
            ..Default::default()
        }
    }

    fn load_dataset(&mut self, _dataset: &[Document]) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }

    fn build_indexes(&mut self) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }

    fn execute(&mut self, _query: &str) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }

    fn cleanup(&mut self) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }
}
