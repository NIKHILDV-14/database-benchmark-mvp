use crate::traits::DatabaseAdapter;
use benchmark_config::DatabaseConfig;
use common::error::{BenchmarkError, Result};
use common::types::DatabaseCapabilities;
use dataset::model::Document;
use postgres::{Client, NoTls};
use tracing::{debug, info};

pub struct PostgresAdapter {
    config: DatabaseConfig,
    client: Option<Client>,
}

impl PostgresAdapter {
    pub fn new(config: DatabaseConfig) -> Self {
        Self {
            config,
            client: None,
        }
    }

    fn get_client(&mut self) -> Result<&mut Client> {
        self.client.as_mut().ok_or_else(|| {
            BenchmarkError::DatabaseConnection("Not connected to PostgreSQL".to_string())
        })
    }

    fn build_connection_string(&self) -> String {
        let host = self.config.host.as_deref().unwrap_or("localhost");
        let port = self.config.port.unwrap_or(5432);
        let user = self.config.username.as_deref().unwrap_or("postgres");
        let pass = self.config.password.as_deref().unwrap_or("postgres");
        let db = self.config.database.as_deref().unwrap_or("benchmark_db");
        format!("postgresql://{}:{}@{}:{}/{}", user, pass, host, port, db)
    }
}

impl DatabaseAdapter for PostgresAdapter {
    fn initialize(&mut self) -> Result<()> {
        let client = self.get_client()?;
        client
            .execute("DROP TABLE IF EXISTS documents", &[])
            .map_err(|e| {
                BenchmarkError::DatabaseConnection(format!("Failed to drop table: {}", e))
            })?;
        client
            .execute(
                "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                title TEXT,
                text TEXT NOT NULL,
                metadata JSONB
            )",
                &[],
            )
            .map_err(|e| {
                BenchmarkError::DatabaseConnection(format!("Failed to create table: {}", e))
            })?;
        info!("Initialized PostgreSQL database table `documents`");
        Ok(())
    }

    fn connect(&mut self) -> Result<()> {
        let conn_str = self.build_connection_string();
        let client = Client::connect(&conn_str, NoTls).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Postgres connection failed: {}", e))
        })?;
        info!("Connected to PostgreSQL");
        self.client = Some(client);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        info!("Disconnected from PostgreSQL");
        self.client = None;
        Ok(())
    }

    fn health(&mut self) -> Result<()> {
        let client = self.get_client()?;
        client.execute("SELECT 1", &[]).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Health check failed: {}", e))
        })?;
        Ok(())
    }

    fn capabilities(&self) -> DatabaseCapabilities {
        DatabaseCapabilities {
            structured_queries: true,
            document_queries: true,
            vector_search: false, // Standard PG no pgvector
            metadata_filter: true,
            hybrid_search: false,
            batch_insert: true,
            transactions: true,
        }
    }

    fn load_dataset(&mut self, dataset: &[Document]) -> Result<()> {
        let client = self.get_client()?;
        debug!("Loading {} documents into PostgreSQL", dataset.len());
        let mut transaction = client.transaction().map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Failed to start transaction: {}", e))
        })?;

        let stmt = transaction
            .prepare(
                "INSERT INTO documents (id, title, text, metadata) VALUES ($1, $2, $3, $4::jsonb)",
            )
            .map_err(|e| {
                BenchmarkError::DatabaseConnection(format!("Failed to prepare statement: {}", e))
            })?;

        for doc in dataset {
            let metadata_str =
                serde_json::to_string(&doc.metadata).unwrap_or_else(|_| "{}".to_string());
            let title: &str = doc.title.as_deref().unwrap_or("");
            transaction
                .execute(&stmt, &[&doc.id, &title, &doc.text, &metadata_str])
                .map_err(|e| {
                    BenchmarkError::DatabaseConnection(format!(
                        "Failed to insert document {}: {}",
                        doc.id, e
                    ))
                })?;
        }

        transaction.commit().map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Failed to commit transaction: {}", e))
        })?;
        info!("Successfully loaded {} documents", dataset.len());
        Ok(())
    }

    fn build_indexes(&mut self) -> Result<()> {
        let client = self.get_client()?;
        client.execute("CREATE INDEX IF NOT EXISTS idx_documents_metadata ON documents USING GIN (metadata)", &[]).map_err(|e| {
            BenchmarkError::DatabaseConnection(format!("Failed to create index: {}", e))
        })?;
        info!("Created index `idx_documents_metadata`");
        Ok(())
    }

    fn execute(&mut self, _query: &str) -> Result<usize> {
        let client = self.get_client()?;
        let rows = client
            .query("SELECT COUNT(*) FROM documents", &[])
            .map_err(|e| BenchmarkError::DatabaseConnection(format!("Query failed: {}", e)))?;
        let count: i64 = rows[0].get(0);
        Ok(count as usize)
    }

    fn cleanup(&mut self) -> Result<()> {
        let client = self.get_client()?;
        client
            .execute("TRUNCATE TABLE documents", &[])
            .map_err(|e| {
                BenchmarkError::DatabaseConnection(format!("Failed to truncate table: {}", e))
            })?;
        info!("Cleaned up PostgreSQL documents table");
        Ok(())
    }
}
