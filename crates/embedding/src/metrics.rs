use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct EmbeddingMetrics {
    pub chunks_processed: AtomicUsize,
    pub cache_hits: AtomicUsize,
    pub cache_misses: AtomicUsize,
    pub batch_count: AtomicUsize,
    pub provider_calls: AtomicUsize,
    pub failed_requests: AtomicUsize,
    pub retry_count: AtomicUsize,

    total_embedding_duration_ms: AtomicU64,
}

impl EmbeddingMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_batch(&self, duration: Duration) {
        self.batch_count.fetch_add(1, Ordering::Relaxed);
        self.total_embedding_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
    }

    pub fn average_batch_duration(&self) -> Duration {
        let count = self.batch_count.load(Ordering::Relaxed);
        if count == 0 {
            return Duration::ZERO;
        }
        let total = self.total_embedding_duration_ms.load(Ordering::Relaxed);
        Duration::from_millis(total / count as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_averages() {
        let metrics = EmbeddingMetrics::new();

        assert_eq!(metrics.average_batch_duration(), Duration::ZERO);

        metrics.record_batch(Duration::from_millis(100));
        metrics.record_batch(Duration::from_millis(200));

        assert_eq!(metrics.average_batch_duration(), Duration::from_millis(150));
        assert_eq!(metrics.batch_count.load(Ordering::SeqCst), 2);
    }
}
