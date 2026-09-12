//! Cancellation, restart recovery, and retry lifecycle for product runs.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use peritus_app_protocol::{ProductConversationRole, ProductRunPhase, ProductRunSnapshot};
use peritus_provider_core::CancellationToken;
use peritus_types::RunId;

use super::{ProductRunService, ProductRunServiceError, RunProgress};
use super::{persistence::persist_record, snapshot::initial_snapshot};
use super::{snapshot::replace_snapshot, snapshot::workspace_has_active_run};

const RESTART_NOTICE: &str =
    "The daemon restarted; I am continuing this goal from its preserved workspace.";

impl ProductRunService {
    pub(crate) async fn shutdown(&self, timeout: Duration) {
        let mut interrupted = Vec::new();
        if let Ok(mut records) = self.inner.records.write() {
            for (run_id, record) in records.iter_mut() {
                if !record.snapshot.phase().terminal() {
                    interrupted.push((*run_id, record.user_cancelled));
                    if let Ok(snapshot) = replace_snapshot(
                        &record.snapshot,
                        record.snapshot.phase(),
                        "Stopping safely after the current effect boundary",
                        record.snapshot.summary(),
                    ) {
                        record.snapshot = snapshot;
                        let _ = persist_record(&self.inner.directory, record);
                    }
                }
                record.cancelled.store(true, Ordering::Release);
                let _ = record.provider_cancellation.cancel();
            }
        }
        let mut tasks = self.inner.tasks.lock().await;
        for task in tasks.drain(..) {
            let _ = tokio::time::timeout(timeout, task).await;
        }
        if let Ok(mut records) = self.inner.records.write() {
            for (run_id, user_cancelled) in interrupted {
                let Some(record) = records.get_mut(&run_id) else { continue };
                if user_cancelled && !record.snapshot.phase().terminal() {
                    if let Ok(snapshot) = replace_snapshot(
                        &record.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled before daemon shutdown completed",
                    ) {
                        record.snapshot = snapshot;
                        let _ = persist_record(&self.inner.directory, record);
                    }
                } else if !user_cancelled
                    && record.snapshot.phase() != ProductRunPhase::Complete
                    && let Ok(snapshot) = replace_snapshot(
                        &record.snapshot,
                        ProductRunPhase::RecoveryRequired,
                        "Daemon restart interrupted this run; continuing automatically",
                        record.snapshot.summary(),
                    )
                {
                    record.snapshot = snapshot;
                    let _ = persist_record(&self.inner.directory, record);
                }
            }
        }
    }

    /// Restarts product goals that were active when the previous daemon process ended.
    pub(crate) async fn resume_interrupted(&self) {
        let run_ids = self
            .inner
            .records
            .read()
            .map(|records| {
                records
                    .iter()
                    .filter_map(|(run_id, record)| {
                        (record.snapshot.phase() == ProductRunPhase::RecoveryRequired)
                            .then_some(*run_id)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for run_id in run_ids {
            if let Ok(mut records) = self.inner.records.write()
                && let Some(record) = records.get_mut(&run_id)
            {
                let already_notified = record
                    .conversation
                    .messages()
                    .ok()
                    .and_then(|messages| messages.last().cloned())
                    .is_some_and(|message| {
                        message.role() == ProductConversationRole::Agent
                            && message.content() == RESTART_NOTICE
                    });
                if !already_notified {
                    let _ = record
                        .conversation
                        .append(ProductConversationRole::Agent, RESTART_NOTICE.to_owned());
                    let _ = persist_record(&self.inner.directory, record);
                }
            }
            let _ = self.retry(run_id).await;
        }
    }

    pub(super) fn cancel(
        &self,
        run_id: RunId,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let mut records =
            self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get_mut(&run_id).ok_or(ProductRunServiceError::NotFound)?;
        if record.snapshot.phase() == ProductRunPhase::WaitingForUser {
            record.snapshot = replace_snapshot(
                &record.snapshot,
                ProductRunPhase::Cancelled,
                "Run cancelled",
                "Cancelled while waiting for your reply",
            )?;
            let _ = record.conversation.append(
                ProductConversationRole::Agent,
                "Cancelled while waiting for your reply".to_owned(),
            );
            persist_record(&self.inner.directory, record)?;
            return Ok(record.snapshot.clone());
        }
        if record.snapshot.phase().terminal() {
            return Err(ProductRunServiceError::InvalidState);
        }
        record.cancelled.store(true, Ordering::Release);
        record.user_cancelled = true;
        let _ = record.provider_cancellation.cancel();
        record.snapshot = replace_snapshot(
            &record.snapshot,
            record.snapshot.phase(),
            "Cancellation requested",
            record.snapshot.summary(),
        )?;
        persist_record(&self.inner.directory, record)?;
        Ok(record.snapshot.clone())
    }

    pub(super) async fn retry(
        &self,
        run_id: RunId,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let (
            request,
            root,
            providers,
            cancelled,
            token,
            conversation,
            finding_state,
            resume,
            snapshot,
        ) = {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            let workspace_id = records
                .get(&run_id)
                .ok_or(ProductRunServiceError::NotFound)?
                .request
                .workspace_id();
            if workspace_has_active_run(&records, workspace_id, Some(run_id)) {
                return Err(ProductRunServiceError::InvalidState);
            }
            let record = records.get_mut(&run_id).expect("checked product run exists");
            let pending_chat = record.snapshot.phase() == ProductRunPhase::WaitingForUser
                && self.pending_record_input(record)?;
            if !record.snapshot.phase().retryable() && !pending_chat {
                return Err(ProductRunServiceError::InvalidState);
            }
            self.validate_workspace_mode(workspace_id, record.interaction.as_ref())?;
            let providers = self.resolve_selected_providers(
                record.request.providers(),
                record.interaction.as_ref(),
            )?;
            let root = self
                .inner
                .workspaces
                .get(&record.request.workspace_id())
                .cloned()
                .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
            let cancelled = Arc::new(AtomicBool::new(false));
            let token = CancellationToken::new();
            record.cancelled = Arc::clone(&cancelled);
            record.user_cancelled = false;
            record.provider_cancellation = token.clone();
            record.snapshot = initial_snapshot(&record.request)?;
            record.progress = RunProgress::default();
            record.settlement = None;
            record.interruption_cause.clear();
            persist_record(&self.inner.directory, record)?;
            (
                record.request.clone(),
                root,
                providers,
                cancelled,
                token,
                Arc::clone(&record.conversation),
                record.finding_state.clone(),
                record.resume.clone(),
                record.snapshot.clone(),
            )
        };
        self.spawn(request, root, providers, cancelled, token, conversation, finding_state, resume)
            .await;
        Ok(snapshot)
    }
}
