//! Read-only rewind preview over exact stored checkpoint facts.

use super::{
    ActorId, AppErrorCode, AppResponsePayload, CheckpointId, ControlError, ConversationId, Error,
    ProductRunService, RestoreStatus, WorkbenchRewindDisposition, WorkbenchRewindPath,
    WorkbenchRewindPreview, WorkbenchRewindRequest, app_error, check_protected, check_record,
    error_response, observe_path, public_version,
};

impl ProductRunService {
    pub(crate) async fn preview_workbench_rewind(
        &self,
        actor: ActorId,
        request: &WorkbenchRewindRequest,
    ) -> AppResponsePayload {
        let service = self.clone();
        let request = *request;
        match tokio::task::spawn_blocking(move || service.rewind_preview(actor, request)).await {
            Ok(result) => {
                result.map_or_else(error_response, AppResponsePayload::WorkbenchRewindPreview)
            }
            Err(_) => AppResponsePayload::Error(app_error(AppErrorCode::Internal)),
        }
    }

    pub(in crate::product_run::workbench::checkpoints) fn rewind_preview(
        &self,
        actor: ActorId,
        request: WorkbenchRewindRequest,
    ) -> Result<WorkbenchRewindPreview, Error> {
        self.control_workspace(request.query())?;
        let conversation = ConversationId::new(request.query().conversation().into_bytes())?;
        let checkpoint_id = CheckpointId::new(request.checkpoint().into_bytes())?;
        let (record, checkpoint) = self.with_controls(false, |store| {
            let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
            let checkpoint = store
                .load_checkpoint(conversation, checkpoint_id)?
                .ok_or(ControlError::NotFound)?;
            Ok((record, checkpoint))
        })?;
        check_record(&record, actor, request.query(), Some(request.revision()))?;
        if record.restores().iter().any(|restore| {
            matches!(restore.status(), RestoreStatus::Prepared | RestoreStatus::RecoveryRequired)
        }) {
            return Err(Error::Corrupt(
                "a prepared rewind requires recovery before another preview",
            ));
        }
        if request.mode() == peritus_app_protocol::WorkbenchRewindMode::ConversationOnly {
            return WorkbenchRewindPreview::new(request, Vec::new(),
                vec!["Current files are unchanged; this branch is not a historical filesystem snapshot.".to_owned()],
                checkpoint.external_effects().map(str::to_owned).collect())
                .map_err(|_| ControlError::InvalidInput.into());
        }
        let root = self.workspace_root(request.query())?;
        let identity = self.checked_folder_identity(request.query(), root)?;
        let protected = self.protected_paths(request.query())?;
        let contract = record.inputs().capture()?.conversation().to_owned();
        let mut paths = Vec::with_capacity(checkpoint.paths().len());
        for checkpoint_path in checkpoint.paths() {
            check_protected(root, checkpoint_path.path(), &contract, &protected)?;
            let observed = observe_path(&identity, checkpoint_path.path())?.version;
            let disposition = if observed == checkpoint_path.checkpoint() {
                WorkbenchRewindDisposition::Unchanged
            } else if checkpoint_path.owned_postchange().is_none() {
                WorkbenchRewindDisposition::Unsealed
            } else if checkpoint_path.owned_postchange() == Some(observed) {
                WorkbenchRewindDisposition::Restore
            } else {
                WorkbenchRewindDisposition::Conflict
            };
            paths.push(
                WorkbenchRewindPath::new(
                    checkpoint_path.path().to_owned(),
                    public_version(checkpoint_path.checkpoint()),
                    checkpoint_path.owned_postchange().map(public_version),
                    public_version(observed),
                    disposition,
                )
                .map_err(|_| ControlError::InvalidInput)?,
            );
        }
        WorkbenchRewindPreview::new(
            request,
            paths,
            checkpoint.exclusions().collect(),
            checkpoint.external_effects().map(str::to_owned).collect(),
        )
        .map_err(|_| ControlError::InvalidInput.into())
    }
}
