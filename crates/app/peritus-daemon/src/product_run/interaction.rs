//! Conversation semantics, durable input acknowledgements, and public execution activity.

use super::{
    ProductRunService, ProductRunServiceError, SharedConversation, persistence::persist_record,
    snapshot::live_snapshot,
};
use peritus_agent::{DeveloperActivity, DeveloperInput, DeveloperInteraction, DeveloperLoopError};
use peritus_app_protocol::{
    MAX_PRODUCT_ACTIVITIES, MAX_PRODUCT_ACTIVITY_BYTES, ProductActivity, ProductActivityKind,
    ProductInteractionMode, ProductInteractionRequest, ProductInteractionSnapshot,
    ProductRoleModels, ProductRunContinuation, ProductRunConversationQuery,
};
use peritus_product_runner::ConversationView;
use peritus_types::RunId;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct InteractionOptions {
    pub(super) persistence_failed: Arc<std::sync::atomic::AtomicBool>,
    pub(super) mode: ProductInteractionMode,
    pub(super) models: ProductRoleModels,
    pub(super) incorporated: u64,
    pub(super) activities: Vec<ProductActivity>,
    pub(super) next_sequence: u64,
    pub(super) pending_utf8: Vec<u8>,
    pub(super) streaming_text: bool,
}

impl InteractionOptions {
    pub(super) fn new(mode: ProductInteractionMode, models: ProductRoleModels) -> Self {
        Self {
            persistence_failed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            mode,
            models,
            incorporated: 0,
            activities: Vec::new(),
            next_sequence: 1,
            pending_utf8: Vec::new(),
            streaming_text: false,
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
                            record.conversation.revision() > options.incorporated
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
            record.conversation.revision(),
            options.incorporated,
            options.activities.clone(),
            record.settlement,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)
    }

    pub(super) fn live_conversation(
        &self,
        run_id: RunId,
        conversation: Arc<SharedConversation>,
    ) -> Arc<dyn ConversationView> {
        Arc::new(LiveConversation { service: self.clone(), run_id, conversation })
    }
}

struct LiveConversation {
    service: ProductRunService,
    run_id: RunId,
    conversation: Arc<SharedConversation>,
}
impl ConversationView for LiveConversation {
    fn incorporated_revision(&self) -> u64 {
        self.service
            .inner
            .records
            .read()
            .ok()
            .and_then(|records| {
                records
                    .get(&self.run_id)
                    .and_then(|record| record.interaction.as_ref())
                    .map(|options| options.incorporated)
            })
            .unwrap_or(0)
    }
    fn revision(&self) -> u64 {
        self.conversation.revision()
    }
    fn render(&self) -> String {
        self.conversation.render()
    }
    fn interaction(&self) -> Option<&dyn DeveloperInteraction> {
        Some(self)
    }
}
impl DeveloperInteraction for LiveConversation {
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        // Admission holds the write lock until persistence succeeds. A model cannot observe an
        // input revision halfway through its durable receive transaction.
        let records = self.service.inner.records.read().map_err(|_| port_error())?;
        let record = records.get(&self.run_id).ok_or_else(port_error)?;
        if record.interaction.as_ref().is_some_and(|options| {
            options.persistence_failed.load(std::sync::atomic::Ordering::Acquire)
        }) {
            return Err(port_error());
        }
        Ok(DeveloperInput {
            revision: record.conversation.revision(),
            conversation: record.conversation.render(),
        })
    }
    fn applied(&self, revision: u64) -> Result<(), DeveloperLoopError> {
        self.update(|options| {
            options.incorporated = options.incorporated.max(revision);
            options.append(
                ProductActivityKind::Status,
                &format!("Input {revision} incorporated into model request"),
                "",
            )
        })
    }
    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        self.update(|options| match activity {
            DeveloperActivity::Text(bytes) => options.text(bytes),
            DeveloperActivity::ModelStarted => {
                options.append(ProductActivityKind::Status, "Model responding", "")
            }
            DeveloperActivity::ToolStarted { name, arguments } => {
                options.append(ProductActivityKind::Tool, &format!("Running {name}"),
                    &format!("{} argument bytes. Raw arguments remain in the trace, not this public activity projection.", arguments.len()))
            }
            DeveloperActivity::ToolFinished { name, output, is_error } => options.append(
                ProductActivityKind::Tool,
                &format!("{} {name}", if is_error { "Failed" } else { "Finished" }),
                &format!("{} output bytes. Raw tool output remains in the trace, not this public activity projection.", output.len()),
            ),
            DeveloperActivity::ToolSkipped { name } => options.append(
                ProductActivityKind::Status,
                &format!("Skipped {name}: newer user input received"),
                "",
            ),
        })
    }
}
impl LiveConversation {
    fn update(
        &self,
        change: impl FnOnce(&mut InteractionOptions) -> Result<(), ProductRunServiceError>,
    ) -> Result<(), DeveloperLoopError> {
        let mut records = self.service.inner.records.write().map_err(|_| port_error())?;
        let record = records.get_mut(&self.run_id).ok_or_else(port_error)?;
        let options = record.interaction.as_mut().ok_or_else(port_error)?;
        change(options).map_err(|_| port_error())?;
        if options.mode != ProductInteractionMode::Build
            && record.snapshot.phase() == peritus_app_protocol::ProductRunPhase::Queued
        {
            record.snapshot = super::replace_snapshot(
                &record.snapshot,
                peritus_app_protocol::ProductRunPhase::Writing,
                "Responding to the conversation",
                record.snapshot.summary(),
            )
            .map_err(|_| port_error())?;
        }
        persist_record(&self.service.inner.directory, record).map_err(|_| port_error())
    }
}
fn port_error() -> DeveloperLoopError {
    DeveloperLoopError::Trace("durable conversation activity unavailable".to_owned())
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
