use crate::chunk::Chunk;

pub struct BatchScheduler {
    pub batch_size: usize,
}

impl BatchScheduler {
    pub fn new(batch_size: usize) -> Self {
        Self {
            batch_size: batch_size.max(1),
        }
    }

    /// Splits chunks into batches of up to `batch_size`, preserving order.
    pub fn schedule<'a>(&self, chunks: &'a [Chunk]) -> Vec<&'a [Chunk]> {
        if chunks.is_empty() {
            return vec![];
        }
        chunks.chunks(self.batch_size).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_chunk(id: usize) -> Chunk {
        Chunk {
            id: format!("doc_{}", id),
            document_id: "doc".to_string(),
            text: "test".to_string(),
            metadata: HashMap::new(),
            chunk_index: id,
        }
    }

    #[test]
    fn test_schedule_batches() {
        let scheduler = BatchScheduler::new(3);
        let chunks: Vec<Chunk> = (0..8).map(make_chunk).collect();

        let batches = scheduler.schedule(&chunks);

        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 3);
        assert_eq!(batches[1].len(), 3);
        assert_eq!(batches[2].len(), 2);
    }

    #[test]
    fn test_schedule_empty() {
        let scheduler = BatchScheduler::new(3);
        let batches = scheduler.schedule(&[]);
        assert!(batches.is_empty());
    }
}
