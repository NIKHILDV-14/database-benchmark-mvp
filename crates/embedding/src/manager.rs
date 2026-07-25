use crate::dummy_provider::DummyEmbeddingProvider;
use crate::provider::EmbeddingProvider;
use anyhow::Result;
use std::sync::Arc;

pub struct ModelManager {
    providers: std::collections::HashMap<String, Arc<dyn EmbeddingProvider>>,
}

impl Default for ModelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelManager {
    pub fn new() -> Self {
        Self {
            providers: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, id: &str, provider: Arc<dyn EmbeddingProvider>) {
        self.providers.insert(id.to_lowercase(), provider);
    }

    pub fn get_provider(&self, id: &str) -> Result<Arc<dyn EmbeddingProvider>> {
        self.providers
            .get(&id.to_lowercase())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", id))
    }
}

pub fn create_default_manager(dimension: usize) -> ModelManager {
    let mut manager = ModelManager::new();
    manager.register("dummy", Arc::new(DummyEmbeddingProvider::new(dimension)));
    manager
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_registration_and_lookup() {
        let manager = create_default_manager(128);

        let provider = manager.get_provider("dummy").unwrap();
        assert_eq!(provider.dimension(), 128);

        // Case insensitivity
        assert!(manager.get_provider("DUMMY").is_ok());

        // Missing provider
        assert!(manager.get_provider("openai").is_err());
    }
}
