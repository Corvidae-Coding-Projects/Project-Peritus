//! Conversation semantics, durable input acknowledgements, and public execution activity.

use super::{ProductRunService, ProductRunServiceError, snapshot::live_snapshot};
use peritus_agent::DeveloperInput;
use peritus_app_protocol::{
    MAX_PRODUCT_ACTIVITIES, MAX_PRODUCT_ACTIVITY_BYTES, ProductActivity, ProductActivityKind,
    ProductInteractionMode, ProductInteractionQuery, ProductInteractionSnapshot, ProductRoleModels,
};
use peritus_product_runner::ConversationView;
use peritus_types::RunId;
use std::sync::Arc;

mod live;
use live::LiveConversation;
mod inputs;
mod models;
mod narration;
mod presentation;
const SUMMARY_DETAIL: &str = "Provider thinking summary";
mod tool_activity;

#[derive(Clone)]
pub(super) struct InteractionOptions {
    pub(super) workbench: peritus_product_runner::control::ControlOperation,
    pub(super) persistence_failed: Arc<std::sync::atomic::AtomicBool>,
    persistence_error: Arc<std::sync::RwLock<Option<String>>>,
    pub(super) mode: ProductInteractionMode,
    pub(super) models: ProductRoleModels,
    pub(super) incorporated: u64,
    pub(super) public_input_count: usize,
    pub(super) activities: Vec<ProductActivity>,
    pub(super) next_sequence: u64,
    pub(super) pending_utf8: Vec<u8>,
    pending_summary_utf8: Vec<u8>,
    pub(super) streaming_text: bool,
    pending_tool: Option<tool_activity::PendingTool>,
}

impl InteractionOptions {
    pub(super) fn new(
        workbench: peritus_product_runner::control::ControlOperation,
        mode: ProductInteractionMode,
        models: ProductRoleModels,
    ) -> Self {
        Self {
            workbench,
            persistence_failed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            persistence_error: Arc::new(std::sync::RwLock::new(None)),
            mode,
            models,
            incorporated: 0,
            public_input_count: 0,
            activities: Vec::new(),
            next_sequence: 1,
            pending_utf8: Vec::new(),
            pending_summary_utf8: Vec::new(),
            streaming_text: false,
            pending_tool: None,
        }
    }

    #[cfg(test)]
    pub(super) fn test(mode: ProductInteractionMode, models: ProductRoleModels) -> Self {
        Self::test_for_run(
            mode,
            models,
            RunId::new([5; 16]).expect("run"),
            peritus_types::WorkspaceId::new([4; 16]).expect("workspace"),
        )
    }

    #[cfg(test)]
    pub(super) fn test_for_run(
        mode: ProductInteractionMode,
        models: ProductRoleModels,
        run: RunId,
        workspace: peritus_types::WorkspaceId,
    ) -> Self {
        use peritus_product_runner::control::{ControlIntent, ConversationId, OperationId};
        use peritus_types::ActorId;

        Self::new(
            peritus_product_runner::control::ControlOperation::new(
                OperationId::new([1; 16]).expect("operation"),
                ConversationId::new([2; 16]).expect("conversation"),
                ActorId::new([3; 16]).expect("actor"),
                workspace,
                0,
                ControlIntent::StartExecution { run: run.into_bytes(), settings_digest: [6; 32] },
            ),
            mode,
            models,
        )
    }

    pub(super) fn record_persistence_failure(&self, error: String) {
        if let Ok(mut stored) = self.persistence_error.write() {
            *stored = Some(error);
        }
    }

    pub(super) fn persistence_failure(&self) -> Option<String> {
        self.persistence_error.read().ok().and_then(|stored| stored.clone())
    }

    pub(super) fn append(
        &mut self,
        kind: ProductActivityKind,
        text: &str,
        detail: &str,
    ) -> Result<(), ProductRunServiceError> {
        let text = bounded(text);
        let activity = ProductActivity::new(self.next_sequence, kind, text, bounded(detail))
            .map_err(|error| {
                ProductRunServiceError::invalid_data(
                    "construct public conversation activity",
                    error,
                )
            })?;
        self.next_sequence =
            self.next_sequence.checked_add(1).ok_or(ProductRunServiceError::Unavailable)?;
        if self.activities.len() == MAX_PRODUCT_ACTIVITIES {
            self.activities.remove(0);
        }
        self.activities.push(activity);
        self.streaming_text = false;
        Ok(())
    }

    fn text(&mut self, bytes: &[u8]) -> Result<(), ProductRunServiceError> {
        self.stream_text(bytes, ProductActivityKind::Assistant, "")
    }

    fn summary(&mut self, bytes: &[u8]) -> Result<(), ProductRunServiceError> {
        self.stream_text(bytes, ProductActivityKind::Status, SUMMARY_DETAIL)
    }

    fn stream_text(
        &mut self,
        bytes: &[u8],
        kind: ProductActivityKind,
        detail: &str,
    ) -> Result<(), ProductRunServiceError> {
        let mut streaming = if kind == ProductActivityKind::Assistant {
            self.streaming_text
        } else {
            self.activities
                .last()
                .is_some_and(|last| last.kind() == kind && last.detail() == detail)
        };
        let pending = if kind == ProductActivityKind::Assistant {
            &mut self.pending_utf8
        } else {
            &mut self.pending_summary_utf8
        };
        pending.extend_from_slice(bytes);
        let valid = match std::str::from_utf8(pending) {
            Ok(text) => text.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(error) => {
                return Err(ProductRunServiceError::invalid_provider_output(
                    "decode streamed assistant text",
                    error,
                ));
            }
        };
        if !streaming
            && std::str::from_utf8(&pending[..valid])
                .is_ok_and(|text| text.chars().all(char::is_whitespace))
        {
            // Some compatible providers begin a post-tool response with standalone whitespace.
            // Keep a small prefix so it can join the first real text delta, but never let an
            // unbounded whitespace stream consume memory or create an invalid empty activity.
            if valid > 256 {
                pending.drain(..valid);
            }
            return Ok(());
        }
        let text = String::from_utf8(pending.drain(..valid).collect()).map_err(|error| {
            ProductRunServiceError::invalid_provider_output(
                "assemble streamed assistant text",
                error,
            )
        })?;
        if text.is_empty() {
            return Ok(());
        }
        let mut remaining = text.as_str();
        while !remaining.is_empty() {
            let last = self.activities.last();
            let merge = streaming
                && last.is_some_and(|last| {
                    last.kind() == kind
                        && last.detail() == detail
                        && last.text().len() < MAX_PRODUCT_ACTIVITY_BYTES.saturating_sub(4)
                });
            let available = if merge {
                MAX_PRODUCT_ACTIVITY_BYTES - last.map_or(0, |last| last.text().len())
            } else {
                MAX_PRODUCT_ACTIVITY_BYTES
            };
            let mut count = remaining.len().min(available);
            while !remaining.is_char_boundary(count) {
                count -= 1;
            }
            let piece = &remaining[..count];
            if merge {
                let last = self.activities.pop().ok_or(ProductRunServiceError::Unavailable)?;
                self.activities.push(
                    ProductActivity::new(
                        last.sequence(),
                        kind,
                        format!("{}{piece}", last.text()),
                        detail.to_owned(),
                    )
                    .map_err(|error| {
                        ProductRunServiceError::invalid_data(
                            "extend public assistant activity",
                            error,
                        )
                    })?,
                );
            } else {
                self.append(kind, piece, detail)?;
            }
            streaming = true;
            self.streaming_text = kind == ProductActivityKind::Assistant;
            remaining = &remaining[count..];
        }
        Ok(())
    }
}

impl ProductRunService {
    pub(super) fn pending_interactive_input(&self, run_id: RunId) -> bool {
        self.inner
            .records
            .read()
            .ok()
            .and_then(|records| {
                records.get(&run_id).map(|record| {
                    record.snapshot.phase() == peritus_app_protocol::ProductRunPhase::WaitingForUser
                        && !record.cancelled.load(std::sync::atomic::Ordering::Acquire)
                        && self.pending_record_input(record).unwrap_or(false)
                        && !record
                            .interaction
                            .persistence_failed
                            .load(std::sync::atomic::Ordering::Acquire)
                })
            })
            .unwrap_or(false)
    }
    pub(crate) fn query_interaction(
        &self,
        query: ProductInteractionQuery,
    ) -> Result<ProductInteractionSnapshot, ProductRunServiceError> {
        self.synchronize_public_inputs(query.run_id())?;
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&query.run_id()).ok_or(ProductRunServiceError::NotFound)?;
        let options = &record.interaction;
        let persistence_failure = options.persistence_failure();
        let mut snapshot = live_snapshot(&self.inner.directory, record)?;
        let mut activities = if record.checkpoint.is_some() {
            options.activities.iter().map(presentation::pipeline_activity).collect()
        } else {
            options.activities.clone()
        };
        let input_revision = if let Some(detail) = persistence_failure {
            snapshot = super::snapshot::replace_snapshot(
                &snapshot,
                peritus_app_protocol::ProductRunPhase::RecoveryRequired,
                "Stopped because run history could not be saved",
                &detail,
            )?;
            if activities.len() == MAX_PRODUCT_ACTIVITIES {
                activities.remove(0);
            }
            activities.push(
                ProductActivity::new(
                    options.next_sequence,
                    ProductActivityKind::Error,
                    "Peritus stopped this run because its conversation history was not durable"
                        .to_owned(),
                    bounded(&detail),
                )
                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
            );
            self.record_input_revision(record).unwrap_or(options.incorporated)
        } else {
            self.record_input_revision(record)?
        };
        ProductInteractionSnapshot::new(
            snapshot,
            options.mode,
            options.models.clone(),
            input_revision,
            options.incorporated,
            activities,
            super::snapshot::delivery_settlement(record),
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)
    }

    pub(super) fn live_conversation(&self, run_id: RunId) -> Arc<dyn ConversationView> {
        Arc::new(LiveConversation { service: self.clone(), run_id })
    }

    pub(super) fn record_input_revision(
        &self,
        record: &super::RunRecord,
    ) -> Result<u64, ProductRunServiceError> {
        self.with_controls(false, |store| store.capture_execution(&record.interaction.workbench))
            .map(|capture| capture.inputs().generation())
            .map_err(Into::into)
    }

    pub(super) fn pending_record_input(
        &self,
        record: &super::RunRecord,
    ) -> Result<bool, ProductRunServiceError> {
        self.with_controls(false, |store| store.capture_execution(&record.interaction.workbench))
            .map(|capture| !capture.inputs().pending().is_empty())
            .map_err(Into::into)
    }

    fn record_input(
        &self,
        record: &super::RunRecord,
    ) -> Result<DeveloperInput, ProductRunServiceError> {
        let captured = self
            .with_controls(false, |store| store.capture_execution(&record.interaction.workbench))?;
        Ok(DeveloperInput {
            revision: captured.inputs().generation(),
            conversation: captured.conversation_with_guidance()?,
            images: captured.images().to_vec(),
        })
    }
}

pub(super) fn terminal_activity(record: &mut super::RunRecord) {
    use peritus_app_protocol::ProductRunPhase;
    let failed = matches!(
        record.snapshot.phase(),
        ProductRunPhase::Failed | ProductRunPhase::RecoveryRequired
    );
    let detail = if record.snapshot.phase() == ProductRunPhase::WaitingForUser {
        ""
    } else {
        record.snapshot.summary()
    };
    let _ = record.interaction.append(
        if failed { ProductActivityKind::Error } else { ProductActivityKind::Status },
        record.snapshot.status(),
        detail,
    );
}
fn bounded(text: &str) -> String {
    if text.len() <= MAX_PRODUCT_ACTIVITY_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_PRODUCT_ACTIVITY_BYTES - 3;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

#[cfg(test)]
mod tests;
