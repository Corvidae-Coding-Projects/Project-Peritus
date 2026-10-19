//! Honest waiting observations around an owned provider future, never simulated model output.

use std::{future::Future, time::Duration};

use crate::DeveloperProviderProgress;
use peritus_model_protocol::{ModelEvent, ProviderExtension};
use peritus_provider_core::CancellationToken;
use tokio::time::Instant;

use super::{DeveloperActivity, DeveloperInteraction, DeveloperLoopError};

const NOTICE_INTERVAL: Duration = Duration::from_secs(20);

pub(super) struct ProviderProgress<'a> {
    interaction: Option<&'a dyn DeveloperInteraction>,
    cancellation: &'a CancellationToken,
    started: Instant,
    next_notice: Instant,
    progress: DeveloperProviderProgress,
}

impl<'a> ProviderProgress<'a> {
    pub(super) fn new(
        interaction: Option<&'a dyn DeveloperInteraction>,
        cancellation: &'a CancellationToken,
    ) -> Self {
        let started = Instant::now();
        Self {
            interaction,
            cancellation,
            started,
            next_notice: started + NOTICE_INTERVAL,
            progress: DeveloperProviderProgress::default(),
        }
    }

    pub(super) fn text_received(&mut self) {
        self.interaction = None;
    }

    pub(super) fn summary_received(&mut self) {
        self.next_notice = Instant::now() + NOTICE_INTERVAL;
    }

    pub(super) fn observe(&mut self, event: &ModelEvent) -> Result<(), DeveloperLoopError> {
        let ModelEvent::ProviderEvent(extension) = event else { return Ok(()) };
        let Some(progress) = runtime_progress(extension) else { return Ok(()) };
        self.progress = progress;
        if let Some(interaction) = self.interaction {
            interaction.observe(DeveloperActivity::ModelWaiting {
                elapsed_seconds: self.started.elapsed().as_secs(),
                progress,
            })?;
        }
        self.next_notice = Instant::now() + NOTICE_INTERVAL;
        Ok(())
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
                            progress: self.progress,
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

fn runtime_progress(extension: &ProviderExtension) -> Option<DeveloperProviderProgress> {
    if extension.name().as_str() != "peritus.runtime_progress" {
        return None;
    }
    let value: serde_json::Value =
        serde_json::from_slice(extension.value().canonical_bytes()).ok()?;
    match value["phase"].as_str()? {
        "accepted" => Some(DeveloperProviderProgress::Accepted),
        "reasoning" => Some(DeveloperProviderProgress::Reasoning),
        "generating" => Some(DeveloperProviderProgress::Generating {
            output_bytes: value["output_bytes"].as_u64()?,
        }),
        _ => None,
    }
}
