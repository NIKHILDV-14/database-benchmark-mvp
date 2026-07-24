use crate::EnvironmentMetadata;
use chrono::Utc;
use common::error::Result;
use sysinfo::System;
use uuid::Uuid;

pub struct EnvironmentRecorder;

impl EnvironmentRecorder {
    pub fn record() -> Result<EnvironmentMetadata> {
        let mut sys = System::new_all();
        sys.refresh_all();

        let metadata = EnvironmentMetadata {
            experiment_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            os: System::long_os_version().unwrap_or_else(|| "Unknown OS".to_string()),
            cpu_cores: sys.cpus().len(),
            total_ram_mb: sys.total_memory() / (1024 * 1024),
        };

        Ok(metadata)
    }
}
