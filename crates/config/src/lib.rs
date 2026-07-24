use serde::Deserialize;

pub mod validator;

#[derive(Debug, Deserialize, Clone)]
pub struct BenchmarkConfig {
    pub experiment: ExperimentConfig,
    pub dataset: DatasetConfig,
    pub databases: DatabasesConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExperimentConfig {
    pub name: String,
    pub seed: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatasetConfig {
    pub path: String,
    pub max_size_gb: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabasesConfig {
    pub postgres: DatabaseConfig,
    pub mongodb: DatabaseConfig,
    pub qdrant: DatabaseConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub enabled: bool,
}

impl BenchmarkConfig {
    pub fn load(path: &str) -> common::error::Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::File::with_name(path))
            .build()
            .map_err(|e| common::error::BenchmarkError::Config(e.to_string()))?;

        settings
            .try_deserialize()
            .map_err(|e| common::error::BenchmarkError::Config(e.to_string()))
    }
}
