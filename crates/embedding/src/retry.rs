use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;

pub struct RetryPolicy {
    pub max_retries: usize,
    pub base_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 50,
        }
    }
}

/// Executes an async action with an exponential backoff retry strategy.
pub async fn with_retry<F, Fut, T>(policy: &RetryPolicy, mut action: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempts = 0;
    loop {
        match action().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempts += 1;
                if attempts > policy.max_retries {
                    return Err(anyhow::anyhow!("Max retries exceeded: {}", e));
                }
                let delay = policy.base_delay_ms * (2_u64.pow(attempts as u32 - 1));
                sleep(Duration::from_millis(delay)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_success_first_try() {
        let policy = RetryPolicy::default();
        let calls = Arc::new(AtomicUsize::new(0));

        let calls_clone = calls.clone();
        let res = with_retry(&policy, || {
            let c = calls_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, anyhow::Error>(42)
            }
        })
        .await
        .unwrap();

        assert_eq!(res, 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let policy = RetryPolicy {
            max_retries: 3,
            base_delay_ms: 1,
        };
        let calls = Arc::new(AtomicUsize::new(0));

        let calls_clone = calls.clone();
        let res = with_retry(&policy, || {
            let c = calls_clone.clone();
            async move {
                let attempts = c.fetch_add(1, Ordering::SeqCst);
                if attempts < 2 {
                    Err(anyhow::anyhow!("Temporary failure"))
                } else {
                    Ok::<i32, anyhow::Error>(42)
                }
            }
        })
        .await
        .unwrap();

        assert_eq!(res, 42);
        assert_eq!(calls.load(Ordering::SeqCst), 3); // Failed twice, succeeded on 3rd try
    }

    #[tokio::test]
    async fn test_retry_max_retries_exceeded() {
        let policy = RetryPolicy {
            max_retries: 2,
            base_delay_ms: 1,
        };
        let calls = Arc::new(AtomicUsize::new(0));

        let calls_clone = calls.clone();
        let res = with_retry(&policy, || {
            let c = calls_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<i32, anyhow::Error>(anyhow::anyhow!("Permanent failure"))
            }
        })
        .await;

        assert!(res.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 3); // 1 initial + 2 retries
    }
}
