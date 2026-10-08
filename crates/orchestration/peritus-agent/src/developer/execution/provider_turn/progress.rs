//! Honest waiting observations around an owned provider future, never simulated model output.

use std::{future::Future, pin::Pin, time::Duration};

use tokio::time::Instant;

use super::{DeveloperInteraction, DeveloperLoopError};

const NOTICE_INTERVAL: Duration = Duration::from_secs(20);
type NoticeFuture<'a> = Pin<Box<dyn Future<Output = Result<(), DeveloperLoopError>> + Send + 'a>>;
struct NoticeDelivery<'a> {
    observer: &'a dyn DeveloperInteraction,
    operation: NoticeFuture<'a>,
}

pub(super) struct ProviderProgress<'a> {
    interaction: Option<&'a dyn DeveloperInteraction>,
    delivery: Option<NoticeDelivery<'a>>,
    started: Instant,
    next_notice: Instant,
}

impl<'a> ProviderProgress<'a> {
    pub(super) fn new(interaction: Option<&'a dyn DeveloperInteraction>) -> Self {
        let started = Instant::now();
        Self { interaction, delivery: None, started, next_notice: started + NOTICE_INTERVAL }
    }

    pub(super) fn text_received(&mut self) {
        self.interaction = None;
    }

    pub(super) fn summary_received(&mut self) {
        self.next_notice = Instant::now() + NOTICE_INTERVAL;
    }

    pub(super) async fn wait<T>(
        &mut self,
        operation: impl Future<Output = Result<T, DeveloperLoopError>>,
    ) -> Result<T, DeveloperLoopError> {
        tokio::pin!(operation);
        loop {
            tokio::select! {
                biased;
                result = &mut operation => return result,
                delivered = async {
                    match self.delivery.as_mut() {
                        Some(delivery) => delivery.operation.as_mut().await,
                        None => std::future::pending().await,
                    }
                }, if self.delivery.is_some() => {
                    let delivery = self.delivery.take();
                    if let Err(error) = delivered
                        && let Some(delivery) = delivery
                    {
                        delivery.observer.waiting_observation_failed(&error);
                    }
                }
                () = tokio::time::sleep_until(self.next_notice) => {
                    if self.delivery.is_none()
                        && let Some(interaction) = self.interaction
                    {
                        let elapsed_seconds = self.started.elapsed().as_secs();
                        self.delivery = Some(NoticeDelivery {
                            observer: interaction,
                            operation: interaction.observe_waiting(elapsed_seconds),
                        });
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
            Ok(DeveloperInput { revision: 1, conversation: String::new(), images: Vec::new() })
        }

        fn prepare_request(
            &self,
            _: u64,
            _: &peritus_model_protocol::ModelRequest,
        ) -> Result<crate::DeveloperRequestAdmission, DeveloperLoopError> {
            Ok(crate::DeveloperRequestAdmission::Accepted)
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
        let mut progress = ProviderProgress::new(Some(&observer));
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
    }

    #[tokio::test]
    async fn failed_observation_does_not_settle_the_owned_provider_future() {
        let observer = Observer { notices: AtomicUsize::new(0), fail: true };
        let mut progress = ProviderProgress::new(Some(&observer));
        progress.next_notice = Instant::now();
        let result = progress.wait(async {
            while observer.notices.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
            tokio::task::yield_now().await;
            Ok("provider completed")
        });
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), result)
                .await
                .expect("provider completion remains live")
                .expect("provider result"),
            "provider completed"
        );
        assert_eq!(observer.notices.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn ready_results_and_streaming_text_do_not_get_wait_notices() {
        let observer = Observer { notices: AtomicUsize::new(0), fail: false };
        let mut progress = ProviderProgress::new(Some(&observer));
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

    #[tokio::test]
    async fn pending_provider_turn_has_no_wall_clock_deadline() {
        let observer = Observer { notices: AtomicUsize::new(0), fail: false };
        let mut progress = ProviderProgress::new(Some(&observer));
        progress.next_notice = Instant::now();
        let result = progress.wait(std::future::pending::<Result<(), DeveloperLoopError>>());
        assert!(tokio::time::timeout(Duration::from_millis(25), result).await.is_err());
        assert!(observer.notices.load(Ordering::SeqCst) > 0);
    }
}
