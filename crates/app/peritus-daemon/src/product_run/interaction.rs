//! Conversation semantics, durable input acknowledgements, and public execution activity.

#[cfg(not(verus_only))]
use super::persistence::persist_record;
use super::{ProductRunService, ProductRunServiceError, snapshot::live_snapshot};
use peritus_agent::DeveloperInput;
use peritus_app_protocol::{
    MAX_PRODUCT_ACTIVITIES, MAX_PRODUCT_ACTIVITY_BYTES, ProductActivity, ProductActivityKind,
    ProductInteractionMode, ProductInteractionRequest, ProductInteractionSnapshot,
    ProductRoleModels, ProductRunContinuation, ProductRunConversationQuery,
};
use peritus_product_runner::ConversationView;
use peritus_types::RunId;
use std::sync::Arc;

mod live;
use live::LiveConversation;
mod models;
mod narration;
mod tool_activity;

#[derive(Clone)]
pub(super) struct InteractionOptions {
    pub(super) workbench: Option<peritus_product_runner::control::ControlOperation>,
    pub(super) persistence_failed: Arc<std::sync::atomic::AtomicBool>,
    pub(super) mode: ProductInteractionMode,
    pub(super) models: ProductRoleModels,
    pub(super) incorporated: u64,
    pub(super) activities: Vec<ProductActivity>,
    pub(super) next_sequence: u64,
    pub(super) pending_utf8: Vec<u8>,
    pub(super) streaming_text: bool,
    pending_tool: Option<tool_activity::PendingTool>,
}

impl InteractionOptions {
    pub(super) fn new(mode: ProductInteractionMode, models: ProductRoleModels) -> Self {
        Self {
            workbench: None,
            persistence_failed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            mode,
            models,
            incorporated: 0,
            activities: Vec::new(),
            next_sequence: 1,
            pending_utf8: Vec::new(),
            streaming_text: false,
            pending_tool: None,
        }
    }

    pub(super) fn append(
        &mut self,
        kind: ProductActivityKind,
        text: &str,
        detail: &str,
    ) -> Result<(), ProductRunServiceError> {
        let text = bounded(text);
        let activity = ProductActivity::new(self.next_sequence, kind, text, bounded(detail))
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
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
        self.pending_utf8.extend_from_slice(bytes);
        let valid = match std::str::from_utf8(&self.pending_utf8) {
            Ok(text) => text.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => return Err(ProductRunServiceError::InvalidMessage),
        };
        let text = String::from_utf8(self.pending_utf8.drain(..valid).collect())
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        if text.is_empty() {
            return Ok(());
        }
        let mut remaining = text.as_str();
        while !remaining.is_empty() {
            let last = self.activities.last();
            let merge = self.streaming_text
                && last.is_some_and(|last| {
                    last.kind() == ProductActivityKind::Assistant
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
                        ProductActivityKind::Assistant,
                        format!("{}{piece}", last.text()),
                        String::new(),
                    )
                    .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                );
            } else {
                self.append(ProductActivityKind::Assistant, piece, "")?;
            }
            self.streaming_text = true;
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
                        && record.interaction.as_ref().is_some_and(|options| {
                            self.pending_record_input(record).unwrap_or(false)
                                && !options
                                    .persistence_failed
                                    .load(std::sync::atomic::Ordering::Acquire)
                        })
                })
            })
            .unwrap_or(false)
    }
    pub(crate) async fn interact(
        &self,
        request: ProductInteractionRequest,
    ) -> Result<ProductInteractionSnapshot, ProductRunServiceError> {
        let run_id = request.request().run_id();
        let options = InteractionOptions::new(request.mode(), request.models().clone());
        self.validate_models(request.request().providers(), &options).await?;
        let exists = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            if let Some(record) = records.get(&run_id) {
                let _ = record.interaction.as_ref().ok_or(ProductRunServiceError::InvalidState)?;
                if record.request.workspace_id() != request.request().workspace_id()
                    || record.request.providers() != request.request().providers()
                {
                    return Err(ProductRunServiceError::InvalidState);
                }
                true
            } else {
                false
            }
        };
        if exists {
            let continuation =
                ProductRunContinuation::new(run_id, request.request().task().to_owned())
                    .map_err(|_| ProductRunServiceError::InvalidMessage)?;
            self.continue_configured(&continuation, Some(options)).await?;
        } else {
            self.start_configured(request.request().clone(), Some(options)).await?;
        }
        self.query_interaction(ProductRunConversationQuery::new(run_id))
    }

    pub(crate) fn query_interaction(
        &self,
        query: ProductRunConversationQuery,
    ) -> Result<ProductInteractionSnapshot, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&query.run_id()).ok_or(ProductRunServiceError::NotFound)?;
        let options = record.interaction.as_ref().ok_or(ProductRunServiceError::InvalidState)?;
        if options.persistence_failed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ProductRunServiceError::Unavailable);
        }
        ProductInteractionSnapshot::new(
            live_snapshot(record)?,
            options.mode,
            options.models.clone(),
            self.record_input_revision(record)?,
            options.incorporated,
            options.activities.clone(),
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
        if let Some(start) =
            record.interaction.as_ref().and_then(|options| options.workbench.as_ref())
        {
            return self
                .with_controls(false, |store| store.capture_execution(start))
                .map(|capture| capture.inputs().generation())
                .map_err(Into::into);
        }
        Ok(record.conversation.revision())
    }

    pub(super) fn pending_record_input(
        &self,
        record: &super::RunRecord,
    ) -> Result<bool, ProductRunServiceError> {
        if let Some(start) =
            record.interaction.as_ref().and_then(|options| options.workbench.as_ref())
        {
            return self
                .with_controls(false, |store| store.capture_execution(start))
                .map(|capture| !capture.inputs().pending().is_empty())
                .map_err(Into::into);
        }
        Ok(record
            .interaction
            .as_ref()
            .is_some_and(|options| record.conversation.revision() > options.incorporated))
    }

    fn record_input(
        &self,
        record: &super::RunRecord,
    ) -> Result<DeveloperInput, ProductRunServiceError> {
        if let Some(start) =
            record.interaction.as_ref().and_then(|options| options.workbench.as_ref())
        {
            let captured = self.with_controls(false, |store| store.capture_execution(start))?;
            return Ok(DeveloperInput {
                revision: captured.inputs().generation(),
                conversation: captured.conversation_with_guidance()?,
                images: captured.images().to_vec(),
            });
        }
        Ok(DeveloperInput {
            revision: record.conversation.revision(),
            conversation: record.conversation.render(),
            images: Vec::new(),
        })
    }
}

pub(super) fn terminal_activity(record: &mut super::RunRecord) {
    use peritus_app_protocol::ProductRunPhase;
    if let Some(options) = record.interaction.as_mut() {
        let failed = matches!(
            record.snapshot.phase(),
            ProductRunPhase::Failed | ProductRunPhase::RecoveryRequired
        );
        let detail = if record.snapshot.phase() == ProductRunPhase::WaitingForUser {
            ""
        } else {
            record.snapshot.summary()
        };
        let _ = options.append(
            if failed { ProductActivityKind::Error } else { ProductActivityKind::Status },
            record.snapshot.status(),
            detail,
        );
    }
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
