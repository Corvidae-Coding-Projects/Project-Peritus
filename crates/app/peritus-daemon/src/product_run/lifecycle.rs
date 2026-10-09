//! Cancellation, restart recovery, and retry lifecycle for product runs.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use peritus_app_protocol::{ProductRunPhase, ProductRunSnapshot};
use peritus_provider_core::CancellationToken;
use peritus_types::RunId;

use super::persistence::persist_record;
use super::{ProductRunService, ProductRunServiceError};
use super::{snapshot::replace_snapshot, snapshot::workspace_has_active_run};

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
        let tasks = {
            let mut owned = self.inner.tasks.lock().await;
            owned.drain(..).map(|(_, task)| task).collect::<Vec<_>>()
        };
        for task in tasks {
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
                        "Daemon shutdown interrupted this run; explicit retry is required after restart",
                        record.snapshot.summary(),
                    )
                {
                    record.snapshot = snapshot;
                    let _ = persist_record(&self.inner.directory, record);
                }
            }
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
            record.interaction.append(
                peritus_app_protocol::ProductActivityKind::Status,
                "Run cancelled",
                "Cancelled while waiting for your reply",
            )?;
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
        self.retry_admitted(run_id, None).await
    }

    pub(super) async fn retry_admitted(
        &self,
        run_id: RunId,
        goal_resume: Option<&peritus_product_runner::control::ControlOperation>,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let (request, root, providers, cancelled, token, finding_state, resume, snapshot) = {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            let workspace_id = records
                .get(&run_id)
                .ok_or(ProductRunServiceError::NotFound)?
                .request
                .workspace_id();
            let record = records.get(&run_id).expect("checked product run exists");
            let explicit_goal = if let Some(operation) = goal_resume {
                if !self.goal_resume_pending(record, operation)? {
                    return Ok(record.snapshot.clone());
                }
                true
            } else {
                false
            };
            if workspace_has_active_run(&records, workspace_id, Some(run_id)) {
                return Err(ProductRunServiceError::InvalidState);
            }
            super::deliverable::discard::workspace_available(
                &self.inner.directory,
                &records,
                workspace_id,
            )?;
            let record = records.get_mut(&run_id).expect("checked product run exists");
            let pending_chat = (record.snapshot.phase() == ProductRunPhase::WaitingForUser
                || record.snapshot.phase() == ProductRunPhase::Complete)
                && self.pending_record_input(record)?;
            let explicit_idle = explicit_goal
                && matches!(
                    record.snapshot.phase(),
                    ProductRunPhase::WaitingForUser | ProductRunPhase::Complete
                );
            if !(record.snapshot.phase().retryable() || pending_chat || explicit_idle) {
                return Err(ProductRunServiceError::InvalidState);
            }
            self.validate_workspace_mode(workspace_id, record.interaction.mode)?;
            let providers =
                self.resolve_selected_providers(record.request.providers(), &record.interaction)?;
            let root = self
                .inner
                .workspaces
                .get(&record.request.workspace_id())
                .cloned()
                .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
            let cancelled = Arc::new(AtomicBool::new(false));
            let token = CancellationToken::new();
            let previous = record.clone();
            if let Some(operation) = goal_resume {
                record.goal_resume = Some(operation.id());
            }
            record.cancelled = Arc::clone(&cancelled);
            record.user_cancelled = false;
            record.provider_cancellation = token.clone();
            record.snapshot =
                super::snapshot::retry_snapshot(&record.request, record.resume.as_ref())?;
            record.progress.begin_attempt();
            record.settlement = None;
            record.interruption_cause.clear();
            if let Err(error) = persist_record(&self.inner.directory, record) {
                *record = previous;
                return Err(error);
            }
            (
                record.request.clone(),
                root,
                providers,
                cancelled,
                token,
                record.finding_state.clone(),
                record.resume.clone(),
                record.snapshot.clone(),
            )
        };
        self.spawn(request, root, providers, cancelled, token, finding_state, resume).await;
        Ok(snapshot)
    }
}
