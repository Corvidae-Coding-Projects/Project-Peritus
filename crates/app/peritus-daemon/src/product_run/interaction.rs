//! Conversation semantics, durable input acknowledgements, and public execution activity.

use super::{
    GoverningStateUnavailable, ProductRunService, ProductRunServiceError,
    snapshot::live_snapshot,
};
use crate::product_control::{CapturedConversation, CapturedFileReaders};
use peritus_app_protocol::{
    MAX_PRODUCT_ACTIVITIES, MAX_PRODUCT_ACTIVITY_BYTES, MAX_PRODUCT_ACTIVITY_PAGE_SEGMENTS,
    MAX_PRODUCT_ACTIVITY_SEGMENT_BYTES, MAX_PRODUCT_RETAINED_ERRORS, ProductActivity,
    ProductActivityKind, ProductActivityPageCursor, ProductActivityPageQuery,
    ProductActivitySegment, ProductActivityWindow, ProductInteractionMode, ProductInteractionPage,
    ProductInteractionQuery, ProductInteractionSnapshot, ProductRoleModels,
};
use peritus_product_runner::ConversationView;
use peritus_types::{RunId, Sha256Digest};
use sha2::{Digest as _, Sha256};
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
    /// Legacy in-memory diagnostic latch retained for fixture compatibility. Live authority
    /// admission is governed by typed control reconciliation and never consults this bit.
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
        let activity = ProductActivity::new(
            self.next_sequence,
            kind,
            text.to_owned(),
            detail.to_owned(),
        )
            .map_err(|error| {
                ProductRunServiceError::invalid_data(
                    "construct public conversation activity",
                    error,
                )
            })?;
        self.next_sequence =
            self.next_sequence.checked_add(1).ok_or(ProductRunServiceError::Unavailable)?;
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
        let streaming = if kind == ProductActivityKind::Assistant {
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
        let text = String::from_utf8(pending.drain(..valid).collect()).map_err(|error| {
            ProductRunServiceError::invalid_provider_output(
                "assemble streamed assistant text",
                error,
            )
        })?;
        if text.is_empty() {
            return Ok(());
        }
        let last = self.activities.last();
        let merge = streaming
            && last.is_some_and(|last| last.kind() == kind && last.detail() == detail);
        if merge {
            let last = self.activities.pop().ok_or(ProductRunServiceError::Unavailable)?;
            self.activities.push(
                ProductActivity::new(
                    last.sequence(),
                    kind,
                    format!("{}{text}", last.text()),
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
            self.append(kind, &text, detail)?;
        }
        self.streaming_text = kind == ProductActivityKind::Assistant;
        Ok(())
    }
}

impl ProductRunService {
    pub(super) fn pending_interactive_input(
        &self,
        run_id: RunId,
        attempt: &Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<bool, ProductRunServiceError> {
        let record = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .get(&run_id)
            .cloned()
            .ok_or(ProductRunServiceError::NotFound)?;
        if record.snapshot.phase() != peritus_app_protocol::ProductRunPhase::WaitingForUser
            || record.cancelled.load(std::sync::atomic::Ordering::Acquire)
        {
            return Ok(false);
        }
        let recovery = self.retained_control_reconciliation(run_id, attempt)?;
        let start = record.interaction.workbench.clone();
        let authoritative_revision = record.interaction.incorporated;
        let mut reported = false;
        loop {
            match self.with_control_reconciliation(&recovery, |store| {
                store.capture_execution(&start)
            }) {
                Ok(capture) => {
                    if reported {
                        crate::diagnostic::report(&format!(
                            "peritusd: pending governing input for {run_id:?} is readable again under its retained owner",
                        ));
                    }
                    return Ok(!capture.inputs().pending().is_empty());
                }
                Err(error)
                    if matches!(
                        &error,
                        crate::product_control::ControlStoreError::Journal(_)
                            | crate::product_control::ControlStoreError::Io(_)
                            | crate::product_control::ControlStoreError::Corrupt(_)
                    ) =>
                {
                    let unavailable = GoverningStateUnavailable::new(
                        run_id,
                        start.clone(),
                        authoritative_revision,
                        "inspect pending governing input",
                        error,
                        Arc::clone(&recovery),
                    );
                    if !reported {
                        crate::diagnostic::report(&format!(
                            "peritusd: {unavailable}; pending input remains unknown while the exact owner retries",
                        ));
                        reported = true;
                    }
                    if attempt.load(std::sync::atomic::Ordering::Acquire) {
                        return Err(ProductRunServiceError::GoverningStateUnavailable(
                            unavailable,
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(error) => return Err(error.into()),
            }
        }
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
        self.project_interaction(&record)
    }

    pub(crate) fn query_activity_page(
        &self,
        query: ProductActivityPageQuery,
    ) -> Result<ProductInteractionPage, ProductRunServiceError> {
        self.synchronize_public_inputs(query.run_id())?;
        let record = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .get(&query.run_id())
            .cloned()
            .ok_or(ProductRunServiceError::NotFound)?;
        let public = public_activities(self, &record);
        let interaction = self.project_interaction_from(&record, &public)?;
        let window = interaction
            .activity_window()
            .ok_or(ProductRunServiceError::InvalidState)?;
        if query.cursor().is_some_and(|cursor| cursor.history() != window.history()) {
            return Err(ProductRunServiceError::InvalidState);
        }
        let (segments, more, cursor_seen) = activity_page_segments(&public, query.cursor())?;
        if query.cursor().is_some() && !cursor_seen {
            return Err(ProductRunServiceError::InvalidState);
        }
        let next = if more {
            let last = segments.last().ok_or(ProductRunServiceError::InvalidState)?;
            Some(
                ProductActivityPageCursor::new(
                    query.run_id(),
                    window.history(),
                    last.sequence(),
                    last.segment(),
                )
                .map_err(|_| ProductRunServiceError::InvalidState)?,
            )
        } else {
            None
        };
        ProductInteractionPage::new(query, interaction, segments, next)
            .map_err(|_| ProductRunServiceError::InvalidState)
    }

    fn project_interaction(
        &self,
        record: &super::RunRecord,
    ) -> Result<ProductInteractionSnapshot, ProductRunServiceError> {
        let public = public_activities(self, record);
        self.project_interaction_from(record, &public)
    }

    fn project_interaction_from(
        &self,
        record: &super::RunRecord,
        public: &[ProductActivity],
    ) -> Result<ProductInteractionSnapshot, ProductRunServiceError> {
        let options = &record.interaction;
        let sequence_base = public
            .first()
            .map_or(0, |activity| activity.sequence().saturating_sub(1));
        if public.iter().enumerate().any(|(index, activity)| {
            u64::try_from(index)
                .ok()
                .and_then(|index| sequence_base.checked_add(index + 1))
                != Some(activity.sequence())
        }) {
            return Err(ProductRunServiceError::InvalidState);
        }
        let persistence_failure = options.persistence_failure();
        let mut snapshot = live_snapshot(&self.inner.directory, record)?;
        let input_revision = if let Some(detail) = persistence_failure.as_ref() {
            snapshot = super::snapshot::replace_snapshot(
                &snapshot,
                peritus_app_protocol::ProductRunPhase::RecoveryRequired,
                "Stopped because run history could not be saved",
                &detail,
            )?;
            self.record_input_revision(record).unwrap_or(options.incorporated)
        } else {
            self.record_input_revision(record)?
        };
        let start = public.len().saturating_sub(MAX_PRODUCT_ACTIVITIES);
        let activities = public[start..]
            .iter()
            .map(activity_preview)
            .collect::<Result<Vec<_>, _>>()?;
        let older_errors = public[..start]
            .iter()
            .filter(|activity| activity.kind() == ProductActivityKind::Error)
            .collect::<Vec<_>>();
        let error_start = older_errors.len().saturating_sub(MAX_PRODUCT_RETAINED_ERRORS);
        let retained_errors = older_errors[error_start..]
            .iter()
            .map(|activity| activity_preview(activity))
            .collect::<Result<Vec<_>, _>>()?;
        let terminal_error = persistence_failure
            .map(|detail| {
                let sequence = public
                    .last()
                    .map_or(1, |activity| activity.sequence().saturating_add(1));
                ProductActivity::new(
                    sequence,
                    ProductActivityKind::Error,
                    "Peritus stopped this run because its conversation history was not durable"
                        .to_owned(),
                    detail,
                )
                .map_err(|_| ProductRunServiceError::InvalidMessage)
                .and_then(|activity| activity_preview(&activity))
            })
            .transpose()?;
        let total = sequence_base
            .checked_add(
                u64::try_from(public.len()).map_err(|_| ProductRunServiceError::Unavailable)?,
            )
            .ok_or(ProductRunServiceError::Unavailable)?;
        let omitted = sequence_base
            .checked_add(u64::try_from(start).map_err(|_| ProductRunServiceError::Unavailable)?)
            .ok_or(ProductRunServiceError::Unavailable)?;
        let omitted_errors =
            u64::try_from(error_start).map_err(|_| ProductRunServiceError::Unavailable)?;
        let window = ProductActivityWindow::new(
            activity_history_digest(record.request.run_id(), public),
            total,
            omitted,
            retained_errors,
            omitted_errors,
            terminal_error,
        )
        .map_err(|_| ProductRunServiceError::InvalidState)?;
        let mode = options.mode;
        let models = options.models.clone();
        let incorporated = options.incorporated;
        let settlement = super::snapshot::delivery_settlement(record);
        ProductInteractionSnapshot::new(
            snapshot,
            mode,
            models,
            input_revision,
            incorporated,
            activities,
            settlement,
        )
        .and_then(|snapshot| snapshot.with_activity_window(window))
        .map_err(|_| ProductRunServiceError::InvalidMessage)
    }

    pub(super) fn live_conversation(
        &self,
        run_id: RunId,
    ) -> Result<Arc<dyn ConversationView>, ProductRunServiceError> {
        Ok(LiveConversation::open(self.clone(), run_id)?)
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

fn public_activities(
    service: &ProductRunService,
    record: &super::RunRecord,
) -> Vec<ProductActivity> {
    let mut activities = if record.checkpoint.is_some() {
        record
            .interaction
            .activities
            .iter()
            .map(presentation::pipeline_activity)
            .collect()
    } else {
        record.interaction.activities.clone()
    };
    if let Some(waiting) = live::waiting::pending(record) {
        live::waiting::project(service, &waiting, &mut activities);
    }
    activities
}

fn activity_preview(
    activity: &ProductActivity,
) -> Result<ProductActivity, ProductRunServiceError> {
    ProductActivity::preview(
        activity.sequence(),
        activity.kind(),
        utf8_prefix(activity.text(), MAX_PRODUCT_ACTIVITY_BYTES).to_owned(),
        u64::try_from(activity.text().len()).map_err(|_| ProductRunServiceError::Unavailable)?,
        utf8_prefix(activity.detail(), MAX_PRODUCT_ACTIVITY_BYTES).to_owned(),
        u64::try_from(activity.detail().len()).map_err(|_| ProductRunServiceError::Unavailable)?,
    )
    .map_err(|error| {
        ProductRunServiceError::invalid_data("project bounded conversation activity", error)
    })
}

fn utf8_prefix(text: &str, maximum: usize) -> &str {
    let mut end = text.len().min(maximum);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn activity_history_digest(run_id: RunId, activities: &[ProductActivity]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-product-activity-history/v1\0");
    hasher.update(run_id.as_bytes());
    hasher.update(u64::try_from(activities.len()).unwrap_or(u64::MAX).to_le_bytes());
    for activity in activities {
        hasher.update(activity.sequence().to_le_bytes());
        hasher.update(activity.kind().tag().to_le_bytes());
        hasher.update(u64::try_from(activity.text().len()).unwrap_or(u64::MAX).to_le_bytes());
        hasher.update(activity.text().as_bytes());
        hasher.update(u64::try_from(activity.detail().len()).unwrap_or(u64::MAX).to_le_bytes());
        hasher.update(activity.detail().as_bytes());
    }
    Sha256Digest::new(hasher.finalize().into())
}

fn activity_page_segments(
    activities: &[ProductActivity],
    cursor: Option<ProductActivityPageCursor>,
) -> Result<(Vec<ProductActivitySegment>, bool, bool), ProductRunServiceError> {
    let after = cursor.map(|cursor| (cursor.after_sequence(), cursor.after_segment()));
    let mut cursor_seen = after.is_none();
    let mut segments = Vec::with_capacity(MAX_PRODUCT_ACTIVITY_PAGE_SEGMENTS + 1);
    'activities: for activity in activities {
        let mut text_offset = 0;
        let mut detail_offset = 0;
        let mut segment = 0_u64;
        while text_offset < activity.text().len() || detail_offset < activity.detail().len() {
            let text = utf8_prefix(
                &activity.text()[text_offset..],
                MAX_PRODUCT_ACTIVITY_SEGMENT_BYTES,
            );
            let detail = utf8_prefix(
                &activity.detail()[detail_offset..],
                MAX_PRODUCT_ACTIVITY_SEGMENT_BYTES,
            );
            let key = (activity.sequence(), segment);
            if after == Some(key) {
                cursor_seen = true;
            } else if cursor_seen {
                segments.push(
                    ProductActivitySegment::new(
                        activity.sequence(),
                        segment,
                        activity.kind(),
                        u64::try_from(text_offset)
                            .map_err(|_| ProductRunServiceError::Unavailable)?,
                        text.to_owned(),
                        activity.text_bytes(),
                        u64::try_from(detail_offset)
                            .map_err(|_| ProductRunServiceError::Unavailable)?,
                        detail.to_owned(),
                        activity.detail_bytes(),
                    )
                    .map_err(|error| {
                        ProductRunServiceError::invalid_data(
                            "segment complete conversation activity",
                            error,
                        )
                    })?,
                );
                if segments.len() > MAX_PRODUCT_ACTIVITY_PAGE_SEGMENTS {
                    break 'activities;
                }
            }
            text_offset += text.len();
            detail_offset += detail.len();
            segment = segment
                .checked_add(1)
                .ok_or(ProductRunServiceError::Unavailable)?;
        }
    }
    let more = segments.len() > MAX_PRODUCT_ACTIVITY_PAGE_SEGMENTS;
    segments.truncate(MAX_PRODUCT_ACTIVITY_PAGE_SEGMENTS);
    Ok((segments, more, cursor_seen))
}

#[cfg(test)]
mod tests;
