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
                () = tokio::time::sleep_until(self.next_notice) => {
                    if let Some(interaction) = self.interaction {
                        let elapsed_seconds = self.started.elapsed().as_secs();
                        if let Err(error) = interaction.observe(DeveloperActivity::ModelWaiting {
                            elapsed_seconds,
                        }) {
                            let _ = self.cancellation.cancel();
                            return Err(error);
                        }
                    }
                    self.next_notice = Instant::now() + NOTICE_INTERVAL;
                }
            }
        }
    }
}
