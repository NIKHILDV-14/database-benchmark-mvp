use common::error::Result;
use common::types::DatabaseCapabilities;
use dataset::model::Document;

pub trait DatabaseAdapter {
    fn initialize(&mut self) -> Result<()>;
    fn connect(&mut self) -> Result<()>;
    fn disconnect(&mut self) -> Result<()>;
    fn health(&mut self) -> Result<()>;
    fn capabilities(&self) -> DatabaseCapabilities;
    fn load_dataset(&mut self, dataset: &[Document]) -> Result<()>;
    fn build_indexes(&mut self) -> Result<()>;
    fn execute(&mut self, query: &str) -> Result<()>;
    fn cleanup(&mut self) -> Result<()>;
}
