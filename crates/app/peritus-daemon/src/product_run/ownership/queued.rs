//! Recovery of a durably accepted first launch whose task owner was not published.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use peritus_app_protocol::ProductRunSnapshot;
use peritus_provider_core::CancellationToken;
use peritus_types::RunId;

use super::{PreparedRetry, PreparedRunLaunch};
use super::super::{
    ProductRunService, ProductRunServiceError, deliverable,
    snapshot::workspace_has_active_run,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InitialOwnerState {
    Missing,
    PendingLaunch,
    Live,
}

impl ProductRunService {
    /// Reacquires the exact first-attempt owner when C0 and the run record committed but task
    /// publication did not. This never replays the workbench queue or creates another run record.
    pub(super) async fn recover_queued_launch(
        &self,
        run_id: RunId,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let service = self.clone();
        let prepared = Self::await_blocking_owner(
            "recover accepted product-run launch",
            move || service.prepare_queued_recovery(run_id),
        )
        .await?;
        let prepared = match prepared {
            PreparedRetry::Existing(snapshot) => return Ok(snapshot),
            PreparedRetry::Launch(prepared) => prepared,
        };
        let snapshot = prepared.snapshot.clone();
        self.launch_prepared(prepared).await?;
        Ok(snapshot)
    }

    fn prepare_queued_recovery(
        &self,
        run_id: RunId,
    ) -> Result<PreparedRetry, ProductRunServiceError> {
        let (records_view, record_view) = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get(&run_id).ok_or(ProductRunServiceError::NotFound)?.clone();
            let workspace = record.request.workspace_id();
            if workspace_has_active_run(&records, workspace, Some(run_id)) {
                return Err(ProductRunServiceError::InvalidState);
            }
            (records.clone(), record)
        };
        deliverable::discard::workspace_available(
            &self.inner.directory,
            &records_view,
            record_view.request.workspace_id(),
        )?;
        if record_view.snapshot.phase() != peritus_app_protocol::ProductRunPhase::Queued {
            return Ok(PreparedRetry::Existing(record_view.snapshot));
        }
        match self.initial_owner_state(run_id, &record_view.interaction.workbench)? {
            InitialOwnerState::Live => {
                return Ok(PreparedRetry::Existing(record_view.snapshot));
            }
            InitialOwnerState::PendingLaunch => return Err(ProductRunServiceError::Unavailable),
            InitialOwnerState::Missing => {}
        }
        let workspace = record_view.request.workspace_id();
        self.validate_workspace_mode(workspace, record_view.interaction.mode)?;
        let providers = self.resolve_selected_providers(
            record_view.request.providers(),
            &record_view.interaction,
        )?;
        let workspace_root = self
            .inner
            .workspaces
            .get(&workspace)
            .cloned()
            .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let control_cancellation = peritus_journal::JournalCancellation::new();
        let provider_cancellation = CancellationToken::new();
        self.ensure_run_admission()?;
        if self.run_owner_active(run_id)? {
            return Err(ProductRunServiceError::Unavailable);
        }
        let prepared_owner = self.register_run_cancellation(
            run_id,
            &cancelled,
            &control_cancellation,
            &provider_cancellation,
            &record_view.interaction.workbench,
            None,
        )?;
        let prepared = (|| {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            self.ensure_run_admission()?;
            if workspace_has_active_run(&records, workspace, Some(run_id)) {
                return Err(ProductRunServiceError::InvalidState);
            }
            let record = records.get_mut(&run_id).ok_or(ProductRunServiceError::NotFound)?;
            if record.snapshot.phase() != peritus_app_protocol::ProductRunPhase::Queued
                || record.interaction.workbench != record_view.interaction.workbench
                || !Arc::ptr_eq(&record.cancelled, &record_view.cancelled)
            {
                return Err(ProductRunServiceError::InvalidState);
            }
            record.cancelled = Arc::clone(&cancelled);
            record.control_cancellation = control_cancellation.clone();
            record.provider_cancellation = provider_cancellation.clone();
            record.user_cancelled = false;
            Ok((
                record.request.clone(),
                record.finding_state.clone(),
                record.resume.clone(),
                record.snapshot.clone(),
            ))
        })();
        let (request, finding_state, resume, snapshot) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                drop(prepared_owner);
                return Err(error);
            }
        };
        Ok(PreparedRetry::Launch(PreparedRunLaunch {
            request,
            workspace_root,
            providers,
            cancelled,
            provider_cancellation,
            finding_state,
            resume,
            snapshot,
            owner: prepared_owner,
        }))
    }

    fn initial_owner_state(
        &self,
        run: RunId,
        binding: &peritus_product_runner::control::ControlOperation,
    ) -> Result<InitialOwnerState, ProductRunServiceError> {
        let cancellations = self
            .inner
            .run_cancellations
            .lock()
            .map_err(|_| ProductRunServiceError::Unavailable)?;
        let Some(owner) = cancellations.get(&run).filter(|owner| {
            owner.continuation.is_none()
                && owner.actor == *binding.actor_bytes()
                && owner.active.load(Ordering::Acquire)
        }) else {
            return Ok(InitialOwnerState::Missing);
        };
        Ok(if owner.launched.load(Ordering::Acquire) {
            InitialOwnerState::Live
        } else {
            InitialOwnerState::PendingLaunch
        })
    }
}
