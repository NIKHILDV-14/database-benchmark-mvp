use crate::traits::DatabaseAdapter;
use benchmark_config::DatabaseConfig;
use bson::doc;
use common::error::{BenchmarkError, Result};
use common::types::DatabaseCapabilities;
use dataset::model::Document;
use mongodb::sync::{Client, Collection, Database};
use tracing::{debug, info};

pub struct MongoDbAdapter {
    config: DatabaseConfig,
    client: Option<Client>,
    db: Option<Database>,
    collection: Option<Collection<bson::Document>>,
}

impl MongoDbAdapter {
    pub fn new(config: DatabaseConfig) -> Self {
        Self {
            config,
            client: None,
            db: None,
            collection: None,
        }
    }

    fn get_collection(&mut self) -> Result<&mut Collection<bson::Document>> {
        self.collection.as_mut().ok_or_else(|| {
            BenchmarkError::DatabaseConnection("Not connected to MongoDB".to_string())
        })
    }

    fn build_connection_string(&self) -> String {
        let host = self.config.host.as_deref().unwrap_or("localhost");
        let port = self.config.port.unwrap_or(27017);
        let user = self.config.username.as_deref().unwrap_or("benchmark_user");
        let pass = self.config.password.as_deref().unwrap_or("benchmark_pass");
        let db = self.config.database.as_deref().unwrap_or("benchmark_db");
        format!(
            "mongodb://{}:{}@{}:{}/{}?authSource=admin",
            user, pass, host, port, db
        )
    }
}

impl DatabaseAdapter for MongoDbAdapter {
    fn initialize(&mut self) -> Result<()> {
        let coll = self.get_collection()?;
        coll.drop(None).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Failed to drop collection: {}", e))
        })?;
        // MongoDB creates collections automatically on first insert.
        info!("Initialized MongoDB database collection `documents`");
        Ok(())
    }

    fn connect(&mut self) -> Result<()> {
        let conn_str = self.build_connection_string();
        let client = Client::with_uri_str(&conn_str).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("MongoDB connection failed: {}", e))
        })?;
        let db_name = self.config.database.as_deref().unwrap_or("benchmark_db");
        let db = client.database(db_name);
        let collection = db.collection("documents");

        self.client = Some(client);
        self.db = Some(db);
        self.collection = Some(collection);
        info!("Connected to MongoDB");
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        info!("Disconnected from MongoDB");
        self.collection = None;
        self.db = None;
        self.client = None;
        Ok(())
    }

    fn health(&mut self) -> Result<()> {
        let db = self.db.as_ref().ok_or_else(|| {
            BenchmarkError::DatabaseConnection("Not connected to MongoDB".to_string())
        })?;
        db.run_command(doc! { "ping": 1 }, None).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Health check failed: {}", e))
        })?;
        Ok(())
    }

    fn capabilities(&self) -> DatabaseCapabilities {
        DatabaseCapabilities {
            structured_queries: false,
            document_queries: true,
            vector_search: false, // Atlas has vector search, but standard local mongo does not by default
            metadata_filter: true,
            hybrid_search: false,
            batch_insert: true,
            transactions: false, // Single doc atomic, but multi-doc txn require replica set
        }
    }

    fn load_dataset(&mut self, dataset: &[Document]) -> Result<()> {
        let coll = self.get_collection()?;
        debug!("Loading {} documents into MongoDB", dataset.len());

        if dataset.is_empty() {
            return Ok(());
        }

        let mut docs = Vec::with_capacity(dataset.len());
        for doc in dataset {
            let metadata_bson = bson::to_bson(&doc.metadata).unwrap_or(bson::Bson::Null);
            let bdoc = doc! {
                "_id": &doc.id,
                "title": doc.title.as_deref().unwrap_or(""),
                "text": &doc.text,
                "metadata": metadata_bson
            };
            docs.push(bdoc);
        }

        coll.insert_many(docs, None).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Failed to insert documents: {}", e))
        })?;
        info!("Successfully loaded {} documents", dataset.len());

        Ok(())
    }

    fn build_indexes(&mut self) -> Result<()> {
        let coll = self.get_collection()?;
        let index_model = mongodb::IndexModel::builder()
            .keys(doc! { "metadata": 1 })
            .build();
        coll.create_index(index_model, None).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Failed to create index: {}", e))
        })?;
        info!("Created MongoDB index on `metadata`");
        Ok(())
    }

    fn execute(&mut self, _query: &str) -> Result<usize> {
        let coll = self.get_collection()?;
        let count = coll
            .count_documents(None, None)
            .map_err(|e| BenchmarkError::DatabaseConnection(format!("Query failed: {}", e)))?;
        Ok(count as usize)
    }

    fn cleanup(&mut self) -> Result<()> {
        let coll = self.get_collection()?;
        coll.delete_many(doc! {}, None).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Failed to clean up collection: {}", e))
        })?;
        info!("Cleaned up MongoDB collection");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mongodb_default_connection_string() {
        let config = DatabaseConfig {
            enabled: true,
            host: None,
            port: None,
            username: None,
            password: None,
            database: None,
            pool_size: None,
            timeout_ms: None,
        };
        let adapter = MongoDbAdapter::new(config);
        assert_eq!(
            adapter.build_connection_string(),
            "mongodb://benchmark_user:benchmark_pass@localhost:27017/benchmark_db?authSource=admin"
        );
    }

    #[test]
    fn test_mongodb_explicit_connection_string() {
        let config = DatabaseConfig {
            enabled: true,
            host: Some("10.0.0.1".to_string()),
            port: Some(27018),
            username: Some("custom_user".to_string()),
            password: Some("custom_pass".to_string()),
            database: Some("custom_db".to_string()),
            pool_size: None,
            timeout_ms: None,
        };
        let adapter = MongoDbAdapter::new(config);
        assert_eq!(
            adapter.build_connection_string(),
            "mongodb://custom_user:custom_pass@10.0.0.1:27018/custom_db?authSource=admin"
        );
    }
}
