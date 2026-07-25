use crate::traits::DatabaseAdapter;
use common::error::{BenchmarkError, Result};
use dataset::model::Document;

pub struct MongoDbAdapter;

impl MongoDbAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl DatabaseAdapter for MongoDbAdapter {
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

    fn initialize_database(&mut self) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }

    fn load_dataset(&mut self, _dataset: &[Document]) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }

    fn create_indexes(&mut self) -> Result<()> {
        Err(BenchmarkError::Other(anyhow::anyhow!(
            "Not Yet Implemented"
        )))
    }

    fn execute_query(&mut self, _query: &str) -> Result<()> {
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
