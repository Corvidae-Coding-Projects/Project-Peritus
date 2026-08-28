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

use super::{ProductRunService, ProductRunServiceError};
use super::{persistence::persist_record, snapshot::initial_snapshot};
use super::{snapshot::replace_snapshot, snapshot::workspace_has_active_run};

impl ProductRunService {
    pub(crate) async fn shutdown(&self, timeout: Duration) {
        if let Ok(mut records) = self.inner.records.write() {
            for record in records.values_mut() {
                if !record.snapshot.phase().terminal()
                    && let Ok(snapshot) = replace_snapshot(
                        &record.snapshot,
                        ProductRunPhase::RecoveryRequired,
                        "Daemon is restarting; this goal will continue automatically",
                        record.snapshot.summary(),
                    )
                {
                    record.snapshot = snapshot;
                    let _ = persist_record(&self.inner.directory, record);
                }
                record.cancelled.store(true, Ordering::Release);
                let _ = record.provider_cancellation.cancel();
            }
        }
        let mut tasks = self.inner.tasks.lock().await;
        for task in tasks.drain(..) {
            let _ = tokio::time::timeout(timeout, task).await;
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
                let _ = record.conversation.append(
                    ProductConversationRole::Agent,
                    "The daemon restarted; I am continuing this goal from its preserved workspace."
                        .to_owned(),
                );
                let _ = persist_record(&self.inner.directory, record);
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
        let (request, root, providers, cancelled, token, conversation, finding_state, snapshot) = {
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
            if !record.snapshot.phase().retryable() {
                return Err(ProductRunServiceError::InvalidState);
            }
            let providers = self.resolve_providers(record.request.providers())?;
            let root = self
                .inner
                .workspaces
                .get(&record.request.workspace_id())
                .cloned()
                .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
            let cancelled = Arc::new(AtomicBool::new(false));
            let token = CancellationToken::new();
            record.cancelled = Arc::clone(&cancelled);
            record.provider_cancellation = token.clone();
            record.snapshot = initial_snapshot(&record.request)?;
            persist_record(&self.inner.directory, record)?;
            (
                record.request.clone(),
                root,
                providers,
                cancelled,
                token,
                Arc::clone(&record.conversation),
                record.finding_state.clone(),
                record.snapshot.clone(),
            )
        };
        self.spawn(request, root, providers, cancelled, token, conversation, finding_state).await;
        Ok(snapshot)
    }
}
