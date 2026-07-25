use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Document {
    pub id: String,
    pub title: Option<String>,
    pub text: String,
    pub metadata: serde_json::Value,
}
