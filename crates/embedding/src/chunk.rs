use dataset::model::Document;
use std::collections::HashMap;

/// A text chunk generated from a Document.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    /// Deterministic chunk ID, usually `{document_id}_{chunk_index}`
    pub id: String,
    /// The document this chunk belongs to
    pub document_id: String,
    /// The text content of the chunk
    pub text: String,
    /// Metadata preserved from the original document
    pub metadata: HashMap<String, serde_json::Value>,
    /// The index of this chunk within the document
    pub chunk_index: usize,
}

/// Chunks a text into smaller pieces with an optional overlap.
/// This implementation is deterministic and splits by whitespace to avoid cutting words in half if possible.
/// A production implementation might use a proper tokenizer (like tiktoken).
pub fn chunk_text(doc: &Document, chunk_size: usize, chunk_overlap: usize) -> Vec<Chunk> {
    if chunk_size == 0 {
        return vec![];
    }

    let metadata: HashMap<String, serde_json::Value> = doc
        .metadata
        .as_object()
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    let text = doc.text.trim();
    if text.is_empty() {
        return vec![];
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut chunk_index = 0;

    while start < words.len() {
        let end = std::cmp::min(start + chunk_size, words.len());
        let chunk_words = &words[start..end];
        let chunk_text = chunk_words.join(" ");

        chunks.push(Chunk {
            id: format!("{}_{}", doc.id, chunk_index),
            document_id: doc.id.clone(),
            text: chunk_text,
            metadata: metadata.clone(),
            chunk_index,
        });

        chunk_index += 1;

        if end == words.len() {
            break;
        }

        let advance = chunk_size.saturating_sub(chunk_overlap);
        start += if advance > 0 { advance } else { 1 };
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_chunk_text_basic() {
        let doc = Document {
            id: "doc1".to_string(),
            title: None,
            text: "word1 word2 word3 word4 word5".to_string(),
            metadata: json!({"key": "value"}),
        };

        let chunks = chunk_text(&doc, 3, 1);

        assert_eq!(chunks.len(), 2);

        assert_eq!(chunks[0].id, "doc1_0");
        assert_eq!(chunks[0].text, "word1 word2 word3");
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(
            chunks[0].metadata.get("key").unwrap().as_str().unwrap(),
            "value"
        );

        assert_eq!(chunks[1].id, "doc1_1");
        assert_eq!(chunks[1].text, "word3 word4 word5");
        assert_eq!(chunks[1].chunk_index, 1);
    }

    #[test]
    fn test_chunk_text_empty() {
        let doc = Document {
            id: "doc2".to_string(),
            title: None,
            text: "   ".to_string(),
            metadata: json!({}),
        };

        let chunks = chunk_text(&doc, 3, 1);
        assert_eq!(chunks.len(), 0);
    }
}
