use crate::cache_backend::CacheBackend;
use common::types::EmbeddingResult;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub struct CacheManager {
    backend: Arc<dyn CacheBackend>,
}

impl CacheManager {
    pub fn new(backend: Arc<dyn CacheBackend>) -> Self {
        Self { backend }
    }

    /// Computes a deterministic cache key for a chunk.
    pub fn compute_key(
        chunk_text: &str,
        provider: &str,
        provider_version: &str,
        dimension: usize,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(chunk_text.as_bytes());
        hasher.update(b"|");
        hasher.update(provider.as_bytes());
        hasher.update(b"|");
        hasher.update(provider_version.as_bytes());
        hasher.update(b"|");
        hasher.update(dimension.to_string().as_bytes());
        let result = hasher.finalize();

        // Convert to hex string without external crate
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn lookup(&self, key: &str) -> Option<EmbeddingResult> {
        self.backend.lookup(key)
    }

    pub fn store(&self, key: &str, embedding: EmbeddingResult) {
        self.backend.insert(key, embedding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache_backend::MemoryCacheBackend;

    #[test]
    fn test_compute_key_determinism() {
        let key1 = CacheManager::compute_key("hello world", "dummy", "v1", 128);
        let key2 = CacheManager::compute_key("hello world", "dummy", "v1", 128);
        let key3 = CacheManager::compute_key("hello world", "dummy", "v2", 128);

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_cache_manager_lookup_store() {
        let backend = Arc::new(MemoryCacheBackend::new());
        let manager = CacheManager::new(backend.clone());

        let key = CacheManager::compute_key("test", "prov", "v1", 64);

        assert!(manager.lookup(&key).is_none());

        manager.store(
            &key,
            EmbeddingResult {
                vector: vec![1.0, 2.0],
            },
        );

        let res = manager.lookup(&key).unwrap();
        assert_eq!(res.vector, vec![1.0, 2.0]);

        let stats = backend.statistics();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }
}
