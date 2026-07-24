use thiserror::Error;

#[derive(Error, Debug)]
pub enum BenchmarkError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database connection error: {0}")]
    DatabaseConnection(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Environment recording error: {0}")]
    Environment(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, BenchmarkError>;
