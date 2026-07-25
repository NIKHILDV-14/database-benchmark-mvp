use crate::model::Document;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// SHA-256 based hasher for producing a deterministic dataset fingerprint.
///
/// # Hashing Strategy
///
/// The hash commits to:
/// - Preprocessing configuration (empty-text policy + trim flag)
/// - For each document in order: `id`, `title`, `text`, and `metadata`
///
/// Metadata keys are alphabetically sorted before hashing to ensure that
/// `{"a":1,"b":2}` and `{"b":2,"a":1}` produce identical hashes.
///
/// Document ordering is **insertion order** (the order they appear in the file).
/// If the input file is shuffled, the hash will differ — this is intentional and
/// makes the hash sensitive to reordering when order matters.
pub struct DatasetHasher {
    hasher: Sha256,
}

impl Default for DatasetHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl DatasetHasher {
    pub fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    /// Commit preprocessing configuration into the hash.
    /// Must be called once before any documents are hashed.
    pub fn update_with_config(&mut self, empty_policy: &str, trim: bool) {
        self.hasher.update(b"config:");
        self.hasher.update(empty_policy.as_bytes());
        self.hasher
            .update(if trim { b"|trim=1|" } else { b"|trim=0|" });
    }

    /// Commit a single preprocessed document into the hash.
    pub fn update_document(&mut self, doc: &Document) {
        self.hasher.update(b"id:");
        self.hasher.update(doc.id.as_bytes());
        self.hasher.update(b"|title:");
        if let Some(t) = &doc.title {
            self.hasher.update(t.as_bytes());
        }
        self.hasher.update(b"|text:");
        self.hasher.update(doc.text.as_bytes());
        self.hasher.update(b"|meta:");
        // Stable, sorted serialization so metadata key order does not affect the hash
        self.hasher
            .update(Self::serialize_sorted_metadata(&doc.metadata).as_bytes());
        self.hasher.update(b"||");
    }

    /// Finalise and return the hex-encoded SHA-256 digest.
    pub fn finalize(self) -> String {
        hex::encode(self.hasher.finalize())
    }

    /// Recursively serialise a JSON value with object keys sorted lexicographically.
    fn serialize_sorted_metadata(val: &Value) -> String {
        match val {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = String::from('{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&format!(
                        "\"{}\":{}",
                        k,
                        Self::serialize_sorted_metadata(&map[*k])
                    ));
                }
                out.push('}');
                out
            }
            Value::Array(arr) => {
                let mut out = String::from('[');
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&Self::serialize_sorted_metadata(v));
                }
                out.push(']');
                out
            }
            _ => val.to_string(),
        }
    }
}
