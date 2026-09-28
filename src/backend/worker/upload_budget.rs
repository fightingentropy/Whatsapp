//! Bound attachment preprocessing and retained upload buffers across batches.
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const MIB: u64 = 1024 * 1024;
const SOURCE_MIB: u32 = 128;

#[derive(Clone)]
pub(super) struct UploadBudget {
    jobs: Arc<Semaphore>,
    bytes: Arc<Semaphore>,
}

impl Default for UploadBudget {
    fn default() -> Self {
        Self {
            jobs: Arc::new(Semaphore::new(if cfg!(target_os = "ios") { 1 } else { 2 })),
            bytes: Arc::new(Semaphore::new(SOURCE_MIB as usize)),
        }
    }
}

impl UploadBudget {
    pub(super) async fn reserve(&self, size: u64) -> (OwnedSemaphorePermit, OwnedSemaphorePermit) {
        let jobs = self
            .jobs
            .clone()
            .acquire_owned()
            .await
            .expect("upload budget stays open");
        // Oversized desktop files run alone. The native iOS bridge caps each
        // source at 100 MiB, so the iPhone always stays within this source budget.
        let weight = size.div_ceil(MIB).clamp(1, u64::from(SOURCE_MIB)) as u32;
        let bytes = self
            .bytes
            .clone()
            .acquire_many_owned(weight)
            .await
            .expect("upload budget stays open");
        (jobs, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reserves_before_reading_and_releases_on_cancellation() {
        let budget = UploadBudget::default();
        let first = budget.reserve(100 * MIB).await;
        let waiting = budget.reserve(40 * MIB);
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut waiting)
                .await
                .is_err()
        );
        drop(first);
        let second = waiting.await;
        let third = if cfg!(target_os = "ios") {
            None
        } else {
            Some(budget.reserve(MIB).await)
        };
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), budget.reserve(MIB))
                .await
                .is_err()
        );
        drop((second, third));
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(100),
                budget.reserve(128 * MIB)
            )
            .await
            .is_ok()
        );
    }
}
