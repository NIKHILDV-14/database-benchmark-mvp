use common::types::EmbeddingResult;
use std::collections::HashMap;
use std::sync::RwLock;

/// Interface for caching embeddings to avoid redundant provider calls.
pub trait CacheBackend: Send + Sync {
    fn lookup(&self, key: &str) -> Option<EmbeddingResult>;
    fn insert(&self, key: &str, embedding: EmbeddingResult);
    fn remove(&self, key: &str);
    fn clear(&self);
    fn statistics(&self) -> CacheStats;
}

#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub items: usize,
    pub hits: usize,
    pub misses: usize,
}

/// A simple thread-safe in-memory cache.
pub struct MemoryCacheBackend {
    store: RwLock<HashMap<String, EmbeddingResult>>,
    stats: RwLock<CacheStats>,
}

impl Default for MemoryCacheBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryCacheBackend {
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            stats: RwLock::new(CacheStats::default()),
        }
    }
}

impl CacheBackend for MemoryCacheBackend {
    fn lookup(&self, key: &str) -> Option<EmbeddingResult> {
        let store = self.store.read().unwrap();
        let result = store.get(key).cloned();

        let mut stats = self.stats.write().unwrap();
        if result.is_some() {
            stats.hits += 1;
        } else {
            stats.misses += 1;
        }

        result
    }

    fn insert(&self, key: &str, embedding: EmbeddingResult) {
        let mut store = self.store.write().unwrap();
        store.insert(key.to_string(), embedding);
        let mut stats = self.stats.write().unwrap();
        stats.items = store.len();
    }

    fn remove(&self, key: &str) {
        let mut store = self.store.write().unwrap();
        store.remove(key);
        let mut stats = self.stats.write().unwrap();
        stats.items = store.len();
    }

    fn clear(&self) {
        let mut store = self.store.write().unwrap();
        store.clear();
        let mut stats = self.stats.write().unwrap();
        stats.items = 0;
    }

    fn statistics(&self) -> CacheStats {
        self.stats.read().unwrap().clone()
    }
}
