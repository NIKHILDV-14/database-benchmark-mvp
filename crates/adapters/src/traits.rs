use common::error::Result;
use dataset::model::Document;

pub trait DatabaseAdapter {
    fn connect(&mut self) -> Result<()>;
    fn disconnect(&mut self) -> Result<()>;
    fn initialize_database(&mut self) -> Result<()>;
    fn load_dataset(&mut self, dataset: &[Document]) -> Result<()>;
    fn create_indexes(&mut self) -> Result<()>;
    fn execute_query(&mut self, query: &str) -> Result<()>;
    fn cleanup(&mut self) -> Result<()>;
}
