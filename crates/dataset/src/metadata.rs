use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DatasetMetadata {
    pub path: String,
    pub format: String,
    pub document_count: usize,
    pub original_size_bytes: u64,
    pub dataset_hash: String,
    pub empty_text_policy: String,
    pub trim_whitespace: bool,
}
