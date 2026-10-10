use thiserror::Error;

/// Structured errors that can occur during the execution of the benchmark pipeline.
#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Configuration file not found: {0}")]
    ConfigNotFound(String),

    #[error("Configuration parse error: {0}")]
    ConfigParse(String),

    #[error("Configuration validation error: {0}")]
    InvalidConfig(String),

    #[error("Dataset error: {0}")]
    Dataset(String),

    #[error("Environment recording error: {0}")]
    Environment(String),

    #[error("Benchmark execution error: {0}")]
    Benchmark(String),

    #[error("Evaluation error: {0}")]
    Evaluation(String),

    #[error("Recommendation error: {0}")]
    Recommendation(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
