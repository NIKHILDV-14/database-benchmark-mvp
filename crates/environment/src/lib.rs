pub mod recorder;

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize, Clone)]
pub struct EnvironmentMetadata {
    pub experiment_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub os: String,
    pub cpu_cores: usize,
    pub total_ram_mb: u64,
}
