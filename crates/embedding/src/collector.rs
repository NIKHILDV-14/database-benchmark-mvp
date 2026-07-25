use crate::chunk::Chunk;
use common::types::EmbeddingResult;
use std::collections::HashMap;

/// Collects batches of embeddings and restores their original ordering based on chunk IDs.
pub struct OrderedCollector {
    results: HashMap<String, EmbeddingResult>,
}

impl Default for OrderedCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderedCollector {
    pub fn new() -> Self {
        Self {
            results: HashMap::new(),
        }
    }

    pub fn add_batch(&mut self, chunks: &[Chunk], embeddings: Vec<EmbeddingResult>) {
        for (chunk, emb) in chunks.iter().zip(embeddings) {
            self.results.insert(chunk.id.clone(), emb);
        }
    }

    pub fn take_ordered(&mut self, ordered_chunks: &[Chunk]) -> Vec<(Chunk, EmbeddingResult)> {
        let mut final_results = Vec::with_capacity(ordered_chunks.len());
        for chunk in ordered_chunks {
            if let Some(emb) = self.results.remove(&chunk.id) {
                final_results.push((chunk.clone(), emb));
            }
        }
        final_results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordered_collection() {
        let mut collector = OrderedCollector::new();

        let chunk1 = Chunk {
            id: "1".to_string(),
            document_id: "doc".to_string(),
            text: "test".to_string(),
            metadata: HashMap::new(),
            chunk_index: 0,
        };
        let mut chunk2 = chunk1.clone();
        chunk2.id = "2".to_string();

        let chunks_ordered = vec![chunk1.clone(), chunk2.clone()];

        // Add in reverse order
        collector.add_batch(
            &[chunk2.clone(), chunk1.clone()],
            vec![
                EmbeddingResult { vector: vec![2.0] },
                EmbeddingResult { vector: vec![1.0] },
            ],
        );

        let ordered = collector.take_ordered(&chunks_ordered);

        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[0].0.id, "1");
        assert_eq!(ordered[0].1.vector, vec![1.0]);
        assert_eq!(ordered[1].0.id, "2");
        assert_eq!(ordered[1].1.vector, vec![2.0]);
    }
}
