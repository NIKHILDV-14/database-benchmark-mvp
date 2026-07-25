use crate::chunk::Chunk;
use common::types::{EmbeddingResult, VectorRecord};

pub struct VectorRecordBuilder;

impl VectorRecordBuilder {
    pub fn build(chunk: Chunk, result: EmbeddingResult) -> VectorRecord {
        VectorRecord {
            id: chunk.id,
            embedding: result.vector,
            metadata: chunk.metadata,
            modality: "text".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_build_vector_record() {
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), serde_json::json!("value"));

        let chunk = Chunk {
            id: "doc1_0".to_string(),
            document_id: "doc1".to_string(),
            text: "test".to_string(),
            metadata: metadata.clone(),
            chunk_index: 0,
        };

        let result = EmbeddingResult {
            vector: vec![0.1, 0.2],
        };

        let record = VectorRecordBuilder::build(chunk, result);

        assert_eq!(record.id, "doc1_0");
        assert_eq!(record.embedding, vec![0.1, 0.2]);
        assert_eq!(record.modality, "text");
        assert_eq!(
            record.metadata.get("key").unwrap().as_str().unwrap(),
            "value"
        );
    }
}
