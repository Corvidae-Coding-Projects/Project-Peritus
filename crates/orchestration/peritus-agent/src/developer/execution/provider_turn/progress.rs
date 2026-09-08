//! Honest waiting observations around an owned provider future, never simulated model output.

use std::{future::Future, time::Duration};

use peritus_provider_core::CancellationToken;
use tokio::time::Instant;

use super::{DeveloperActivity, DeveloperInteraction, DeveloperLoopError};

const NOTICE_INTERVAL: Duration = Duration::from_secs(20);

pub(super) struct ProviderProgress<'a> {
    interaction: Option<&'a dyn DeveloperInteraction>,
    cancellation: &'a CancellationToken,
    started: Instant,
    next_notice: Instant,
}

impl<'a> ProviderProgress<'a> {
    pub(super) fn new(
        interaction: Option<&'a dyn DeveloperInteraction>,
        cancellation: &'a CancellationToken,
    ) -> Self {
        let started = Instant::now();
        Self { interaction, cancellation, started, next_notice: started + NOTICE_INTERVAL }
    }

    pub(super) fn text_received(&mut self) {
        self.interaction = None;
    }

    pub(super) async fn wait<T>(
        &mut self,
        operation: impl Future<Output = Result<T, DeveloperLoopError>>,
    ) -> Result<T, DeveloperLoopError> {
        let Some(interaction) = self.interaction else { return operation.await };
        tokio::pin!(operation);
        loop {
            tokio::select! {
                biased;
                result = &mut operation => return result,
                () = tokio::time::sleep_until(self.next_notice) => {
                    let elapsed_seconds = self.started.elapsed().as_secs();
                    if let Err(error) = interaction.observe(DeveloperActivity::ModelWaiting {
                        elapsed_seconds,
                    }) {
                        let _ = self.cancellation.cancel();
                        return Err(error);
                    }
                    self.next_notice = Instant::now() + NOTICE_INTERVAL;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DeveloperInput;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Observer {
        notices: AtomicUsize,
        fail: bool,
    }

    impl DeveloperInteraction for Observer {
        fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
            Ok(DeveloperInput { revision: 1, conversation: String::new() })
        }

        fn applied(&self, _: u64) -> Result<(), DeveloperLoopError> {
            Ok(())
        }

        fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
            assert!(matches!(activity, DeveloperActivity::ModelWaiting { .. }));
            self.notices.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(DeveloperLoopError::Trace("observation failed".to_owned()))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn notice_arrives_while_the_same_provider_future_is_still_pending() {
        let observer = Observer { notices: AtomicUsize::new(0), fail: false };
        let cancellation = CancellationToken::new();
        let mut progress = ProviderProgress::new(Some(&observer), &cancellation);
        progress.next_notice = Instant::now();
        let result = progress.wait(async {
            while observer.notices.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
            Ok("completed")
        });
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), result)
                .await
                .expect("notice before completion")
                .expect("result"),
            "completed"
        );
        assert_eq!(observer.notices.load(Ordering::SeqCst), 1);
        assert!(!cancellation.is_cancelled());
    }

    #[tokio::test]
    async fn failed_observation_cancels_the_owned_pending_request() {
        let observer = Observer { notices: AtomicUsize::new(0), fail: true };
        let cancellation = CancellationToken::new();
        let mut progress = ProviderProgress::new(Some(&observer), &cancellation);
        progress.next_notice = Instant::now();
        let result = progress.wait(std::future::pending::<Result<(), DeveloperLoopError>>());
        assert!(
            tokio::time::timeout(Duration::from_secs(1), result)
                .await
                .expect("bounded failure")
                .is_err()
        );
        assert!(cancellation.is_cancelled());
    }

    #[tokio::test]
    async fn ready_results_and_streaming_text_do_not_get_wait_notices() {
        let observer = Observer { notices: AtomicUsize::new(0), fail: false };
        let cancellation = CancellationToken::new();
        let mut progress = ProviderProgress::new(Some(&observer), &cancellation);
        progress.next_notice = Instant::now();
        progress.wait(async { Ok(()) }).await.expect("ready result wins");
        progress.text_received();
        progress
            .wait(async {
                tokio::task::yield_now().await;
                Ok(())
            })
            .await
            .expect("streamed fragment");
        assert_eq!(observer.notices.load(Ordering::SeqCst), 0);
    }
}
