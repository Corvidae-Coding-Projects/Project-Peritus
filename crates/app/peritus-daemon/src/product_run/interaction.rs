//! Conversation semantics, durable input acknowledgements, and public execution activity.

use super::{ProductRunService, ProductRunServiceError, snapshot::live_snapshot};
use crate::product_control::{CapturedConversation, CapturedFileReaders};
use peritus_agent::DeveloperInput;
use peritus_app_protocol::{
    MAX_PRODUCT_ACTIVITIES, MAX_PRODUCT_ACTIVITY_BYTES, ProductActivity, ProductActivityKind,
    ProductInteractionMode, ProductInteractionQuery, ProductInteractionSnapshot, ProductRoleModels,
};
use peritus_product_runner::ConversationView;
use peritus_types::RunId;
use std::sync::Arc;

use super::publication::{MutationDisposition, RunMutationKind};

mod live;
use live::LiveConversation;
mod inputs;
mod models;
mod narration;
mod presentation;
mod sources;
const SUMMARY_DETAIL: &str = "Provider thinking summary";
mod tool_activity;

#[derive(Clone)]
pub(super) struct InteractionOptions {
    pub(super) workbench: peritus_product_runner::control::ControlOperation,
    pub(super) persistence_failed: Arc<std::sync::atomic::AtomicBool>,
    persistence_error: Arc<std::sync::RwLock<Option<String>>>,
    file_readers: Arc<std::sync::Mutex<Option<Arc<CapturedFileReaders>>>>,
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
            file_readers: Arc::new(std::sync::Mutex::new(None)),
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
        let record = self
            .inner
            .records
            .read()
            .ok()
            .and_then(|records| records.get(&run_id).cloned());
        record.is_some_and(|record| {
            record.snapshot.phase() == peritus_app_protocol::ProductRunPhase::WaitingForUser
                && !record.cancelled.load(std::sync::atomic::Ordering::Acquire)
                && self.pending_record_input(&record).unwrap_or(false)
                && !record
                    .interaction
                    .persistence_failed
                    .load(std::sync::atomic::Ordering::Acquire)
        })
    }
    pub(crate) fn query_interaction(
        &self,
        query: ProductInteractionQuery,
    ) -> Result<ProductInteractionSnapshot, ProductRunServiceError> {
        self.synchronize_public_inputs(query.run_id())?;
        let record = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .get(&query.run_id())
            .cloned()
            .ok_or(ProductRunServiceError::NotFound)?;
        let options = &record.interaction;
        let persistence_failure = options.persistence_failure();
        let mut snapshot = live_snapshot(&self.inner.directory, &record)?;
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
            self.record_input_revision(&record).unwrap_or(options.incorporated)
        } else {
            self.record_input_revision(&record)?
        };
        let waiting = live::waiting::pending(&record);
        let mode = options.mode;
        let models = options.models.clone();
        let incorporated = options.incorporated;
        let settlement = super::snapshot::delivery_settlement(&record);
        if let Some(waiting) = waiting {
            live::waiting::project(self, &waiting, &mut activities);
        }
        ProductInteractionSnapshot::new(
            snapshot,
            mode,
            models,
            input_revision,
            incorporated,
            activities,
            settlement,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)
    }

    pub(super) fn live_conversation(&self, run_id: RunId) -> Arc<dyn ConversationView> {
        let attempt_cancelled = self
            .inner
            .records
            .read()
            .ok()
            .and_then(|records| records.get(&run_id).map(|record| Arc::clone(&record.cancelled)))
            .unwrap_or_else(|| Arc::new(std::sync::atomic::AtomicBool::new(true)));
        Arc::new(LiveConversation {
            service: self.clone(),
            run_id,
            attempt_cancelled,
            request_sources: std::sync::Mutex::new(None),
        })
    }

    pub(super) fn record_input_revision(
        &self,
        record: &super::RunRecord,
    ) -> Result<u64, ProductRunServiceError> {
        self.record_capture(record)
            .map(|capture| capture.inputs().generation())
    }

    pub(super) fn pending_record_input(
        &self,
        record: &super::RunRecord,
    ) -> Result<bool, ProductRunServiceError> {
        self.with_control_conversation(record.interaction.workbench.conversation(), |store| {
            store.capture_execution(&record.interaction.workbench)
        })
            .map(|capture| !capture.inputs().pending().is_empty())
            .map_err(Into::into)
    }

    fn record_input(
        &self,
        record: &super::RunRecord,
    ) -> Result<DeveloperInput, ProductRunServiceError> {
        let captured = self.record_capture(record)?;
        Ok(DeveloperInput {
            revision: captured.inputs().generation(),
            conversation: captured.conversation_with_guidance()?,
            images: captured.images().to_vec(),
        })
    }

    fn record_capture(
        &self,
        record: &super::RunRecord,
    ) -> Result<crate::product_control::CapturedConversation, ProductRunServiceError> {
        let source = record.continuation_sources.iter().rev().find(|source| !source.settled);
        self.with_control_conversation(record.interaction.workbench.conversation(), |store| {
            source.map_or_else(
                || store.capture_execution(&record.interaction.workbench),
                |source| {
                    if record.interaction.incorporated < source.generation {
                        store.capture_execution_revision(
                            &record.interaction.workbench,
                            source.revision,
                        )
                    } else {
                        store.capture_execution_revision_incorporated(
                            &record.interaction.workbench,
                            source.revision,
                        )
                    }
                },
            )
        })
        .map_err(Into::into)
    }

    fn active_continuation_source(
        record: &super::RunRecord,
    ) -> Option<super::ContinuationSource> {
        record
            .continuation_sources
            .iter()
            .rev()
            .find(|source| !source.settled)
            .copied()
    }

    /// Captures current file authority and reconciles readers owned by the durable run record.
    fn current_file_sources(
        &self,
        run_id: RunId,
    ) -> Result<(CapturedConversation, Arc<CapturedFileReaders>), ProductRunServiceError> {
        let record = self
            .inner
            .records
            .read()
            .map_err(|_| {
                ProductRunServiceError::internal(
                    "read authoritative file sources",
                    "the product-run record lock was poisoned",
                )
            })?
            .get(&run_id)
            .cloned()
            .ok_or(ProductRunServiceError::NotFound)?;
        // Serialize capture with cache replacement so an older concurrent capture cannot replace
        // a newer selected scope after that newer scope has already installed its readers.
        let mut cached = record.interaction.file_readers.lock().map_err(|_| {
            ProductRunServiceError::internal(
                "retain authoritative file sources",
                "the file-source reader lock was poisoned",
            )
        })?;
        let captured = self.record_capture(&record)?;
        let readers = captured.file_readers(cached.as_ref());
        *cached = Some(Arc::clone(&readers));
        Ok((captured, readers))
    }

    /// Lists ordinary run attachments from the exact control capture governing this run.
    fn attachment_context_sources(
        &self,
        run_id: RunId,
        after: Option<u64>,
    ) -> Result<peritus_product_runner::ContextSourcePage, ProductRunServiceError> {
        let (captured, _readers) = self.current_file_sources(run_id)?;
        captured.context_sources(after).map_err(Into::into)
    }

    /// Reads one exact ordinary-run attachment slice from its durable selected version.
    fn read_attachment_context_source(
        &self,
        run_id: RunId,
        source: u64,
        offset: u64,
    ) -> Result<peritus_product_runner::ContextSourceSlice, ProductRunServiceError> {
        let (captured, readers) = self.current_file_sources(run_id)?;
        self.read_captured_file_source(&captured, readers.as_ref(), source, offset)
    }

    /// Pages existing attachment or improvement sources first, then the reserved finding range.
    pub(super) fn combined_context_sources(
        &self,
        run_id: RunId,
        after: Option<u64>,
    ) -> Result<peritus_product_runner::ContextSourcePage, ProductRunServiceError> {
        use peritus_product_runner::{ContextSourcePage, MAX_CONTEXT_SOURCE_PAGE};
        use peritus_review::PRODUCT_FINDING_SOURCE_ORDINAL_BASE;

        let base = if after.is_some_and(|after| {
            after >= PRODUCT_FINDING_SOURCE_ORDINAL_BASE
        }) {
            ContextSourcePage::new(after, Vec::new(), None).map_err(|error| {
                ProductRunServiceError::internal("page product context sources", error)
            })?
        } else if let Some(page) = self.improvement_context_sources(run_id, after)? {
            page
        } else {
            self.attachment_context_sources(run_id, after)?
        };
        if base.sources().iter().any(|source| {
            source.ordinal() >= PRODUCT_FINDING_SOURCE_ORDINAL_BASE
        }) || base.next().is_some_and(|next| {
            next >= PRODUCT_FINDING_SOURCE_ORDINAL_BASE
        }) {
            return Err(ProductRunServiceError::internal(
                "page product context sources",
                "an existing source provider entered the reserved finding ordinal range",
            ));
        }
        if base.next().is_some() {
            return Ok(base);
        }
        let finding_catalog = {
            let records = self
                .inner
                .records
                .read()
                .map_err(|_| ProductRunServiceError::Unavailable)?;
            records
                .get(&run_id)
                .ok_or(ProductRunServiceError::NotFound)?
                .finding_catalog
                .clone()
        };
        let remaining = MAX_CONTEXT_SOURCE_PAGE.saturating_sub(base.sources().len());
        let finding_page = self
            .inner
            .finding_bodies
            .page(&finding_catalog, after, remaining)?;
        let mut sources = base.sources().to_vec();
        sources.extend(finding_page.sources);
        let next = if finding_page.has_more {
            sources.last().map(peritus_product_runner::ContextSource::ordinal)
        } else {
            None
        };
        ContextSourcePage::new(after, sources, next).map_err(|error| {
            ProductRunServiceError::internal("page product context sources", error)
        })
    }

    /// Routes the collision-free upper ordinal range to immutable finding bodies.
    pub(super) fn read_combined_context_source(
        &self,
        run_id: RunId,
        source: u64,
        offset: u64,
    ) -> Result<peritus_product_runner::ContextSourceSlice, ProductRunServiceError> {
        if source >= peritus_review::PRODUCT_FINDING_SOURCE_ORDINAL_BASE {
            let finding_catalog = {
                let records = self
                    .inner
                    .records
                    .read()
                    .map_err(|_| ProductRunServiceError::Unavailable)?;
                records
                    .get(&run_id)
                    .ok_or(ProductRunServiceError::NotFound)?
                    .finding_catalog
                    .clone()
            };
            return self
                .inner
                .finding_bodies
                .read(&finding_catalog, source, offset);
        }
        if let Some(slice) = self.read_improvement_context_source(run_id, source, offset)? {
            return Ok(slice);
        }
        self.read_attachment_context_source(run_id, source, offset)
    }

    /// Publishes the compact ledger head only after the runner has synchronized every body.
    pub(super) fn adopt_finding_state(
        &self,
        run_id: RunId,
        finding_state: &str,
    ) -> Result<(), ProductRunServiceError> {
        let identity = self.capture_run_identity(run_id)?;
        let ledger = peritus_product_runner::ProductRunner::decode_finding_state(finding_state)
            .map_err(|error| {
                ProductRunServiceError::internal(
                    "adopt product finding state",
                    error.to_string(),
                )
            })?;
        if ledger.has_inline_bodies() {
            return Err(ProductRunServiceError::internal(
                "adopt product finding state",
                "a governed finding ledger still contains inline bodies",
            ));
        }
        let finding_catalog = super::persistence::FindingBodyStore::catalog_from_ledger(
            finding_state,
            &ledger,
        )?;
        if identity.finding_head == finding_catalog.head_digest
            && identity.finding_state == finding_state
        {
            return Ok(());
        }
        let input_digest = finding_catalog.head_digest;
        let finding_state = finding_state.to_owned();
        let expected_attempt = Arc::clone(&identity.cancelled);
        let (_, ticket) = self.mutate_run(
            run_id,
            Some(&expected_attempt),
            RunMutationKind::InteractionSource,
            input_digest,
            MutationDisposition::DurabilityRequired,
            move |record| {
                if record.finding_catalog.head_digest != identity.finding_head
                    || record.finding_state != identity.finding_state
                {
                    return Err(ProductRunServiceError::InvalidState);
                }
                record.finding_state = finding_state;
                record.finding_catalog = finding_catalog;
                Ok(())
            },
        )?;
        self.await_run_durable(ticket)
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
