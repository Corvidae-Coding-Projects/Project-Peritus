//! Reviewed rewind preview, exact C1 application, and truthful durable settlement.

use super::{
    ActorId, AppErrorCode, AppResponsePayload, CheckpointFileVersion, CheckpointId, CheckpointPath,
    ControlError, ControlIntent, ControlOperation, ControlStore, ConversationId, Error, FinalFile,
    Generation, LineEndingPolicy, OperationId, PatchOperation, PatchSet, Preimage,
    ProductRunService, RestoreId, RestoreOperation, RestoreStatus, RevisionNumber, UserCheckpoint,
    WorkbenchCommand, WorkbenchIntent, WorkbenchRestoreReceipt, WorkbenchRewindDisposition,
    WorkbenchRewindPath, WorkbenchRewindPreview, WorkbenchRewindRequest, WorkspacePath, app_error,
    check_protected, check_record, checkpoint_references, derived_id, error_response,
    external_effects, noop_manifest, observe_path, patch_input, patch_mode, patch_preimage,
    public_restore, public_version,
};

#[cfg(test)]
mod faults;
#[cfg(test)]
use faults::check_rewind_fault;
#[cfg(test)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "crate-level tests inject exact crash boundaries"
)]
pub(crate) use faults::{RewindFaultPoint, inject_rewind_fault, obstruct_folder_patch};

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

    pub(super) fn rewind_preview(
        &self,
        actor: ActorId,
        request: WorkbenchRewindRequest,
    ) -> Result<WorkbenchRewindPreview, Error> {
        self.control_workspace(request.query())?;
        let conversation = ConversationId::new(request.query().conversation().into_bytes())?;
        let checkpoint_id = CheckpointId::new(request.checkpoint().into_bytes())?;
        let record = self
            .with_controls(false, |store| store.load(conversation))?
            .ok_or(ControlError::NotFound)?;
        check_record(&record, actor, request.query(), Some(request.revision()))?;
        let checkpoint = record
            .checkpoints()
            .iter()
            .find(|value| value.id() == checkpoint_id)
            .ok_or(ControlError::NotFound)?;
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
            checkpoint.exclusions().map(str::to_owned).collect(),
            checkpoint.external_effects().map(str::to_owned).collect(),
        )
        .map_err(|_| ControlError::InvalidInput.into())
    }

    pub(crate) async fn apply_workbench_rewind(
        &self,
        actor: ActorId,
        session: peritus_types::SessionId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let service = self.clone();
        let command = command.clone();
        match tokio::task::spawn_blocking(move || {
            let receipt = service.apply_rewind(actor, session, &command)?;
            service.finish_logical_rewind(actor, &command, receipt)
        })
        .await
        {
            Ok(result) => result.map_or_else(error_response, AppResponsePayload::WorkbenchRestore),
            Err(_) => AppResponsePayload::Error(app_error(AppErrorCode::Internal)),
        }
    }

    fn apply_rewind(
        &self,
        actor: ActorId,
        session: peritus_types::SessionId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchRestoreReceipt, Error> {
        let WorkbenchIntent::ApplyRewind(confirmed) = command.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        self.control_workspace(command.query())?;
        let conversation = ConversationId::new(command.query().conversation().into_bytes())?;
        let restore_id = RestoreId::new(command.operation().into_bytes())?;
        let before = self
            .with_controls(false, |store| store.load(conversation))?
            .ok_or(ControlError::NotFound)?;
        check_record(&before, actor, command.query(), None)?;
        if before.restores().iter().any(|restore| restore.id() == restore_id) {
            return self.resolve_workbench_restore(actor, command);
        }
        let request = confirmed.request();
        if request.query() != command.query() || request.revision() != command.expected_revision() {
            return Err(ControlError::StaleRevision.into());
        }
        let current = self.rewind_preview(actor, request)?;
        if &current != confirmed {
            return Err(ControlError::StaleRevision.into());
        }

        let checkpoint_id = CheckpointId::new(request.checkpoint().into_bytes())?;
        let record = self
            .with_controls(false, |store| store.load(conversation))?
            .ok_or(ControlError::NotFound)?;
        check_record(&record, actor, command.query(), Some(command.expected_revision()))?;
        let checkpoint = record
            .checkpoints()
            .iter()
            .find(|value| value.id() == checkpoint_id)
            .cloned()
            .ok_or(ControlError::NotFound)?;

        let conversation_only =
            request.mode() == peritus_app_protocol::WorkbenchRewindMode::ConversationOnly;
        let observed = if conversation_only {
            Vec::new()
        } else {
            self.capture_checkpoint_paths(&record, command.query(), &checkpoint)?
        };
        if observed.iter().zip(confirmed.paths()).any(|(captured, preview)| {
            public_version(captured.version) != preview.observed_current()
        }) {
            return Err(ControlError::StaleRevision.into());
        }
        let recovery_id = CheckpointId::new(derived_id(
            b"peritus-workbench-rewind-recovery-v1\0",
            command.operation().as_bytes(),
        ))?;
        let recovery = UserCheckpoint::new(
            recovery_id,
            format!("Recovery before rewind {restore_id}"),
            checkpoint_references(&record),
            observed
                .iter()
                .map(|path| CheckpointPath::new(path.path.clone(), path.version))
                .collect::<Result<Vec<_>, _>>()?,
            Vec::new(),
            external_effects(),
        )?;
        let recovery_bodies = observed.into_iter().map(|path| path.body).collect::<Vec<_>>();
        let conflicts = confirmed
            .paths()
            .iter()
            .filter(|path| {
                matches!(
                    path.disposition(),
                    WorkbenchRewindDisposition::Conflict | WorkbenchRewindDisposition::Unsealed
                )
            })
            .map(|path| path.path().to_owned())
            .collect::<Vec<_>>();
        let branch = if conflicts.is_empty() {
            self.logical_rewind_branch(actor, command, request, &record)?
        } else {
            None
        };
        let plan = if conflicts.is_empty() && !conversation_only {
            self.restore_plan(command.query(), &checkpoint, confirmed)?
        } else {
            None
        };
        let patch_digest = plan
            .as_ref()
            .map_or_else(|| confirmed.preview_digest(), |patch| patch.identity().digest());
        let mut restore = RestoreOperation::prepared(
            restore_id,
            checkpoint_id,
            confirmed.preview_digest(),
            patch_digest,
            recovery_id,
        )?;
        if let Some(branch) = branch {
            restore = restore.with_branch(branch)?;
        }
        let prepare = ControlOperation::new(
            OperationId::new(command.operation().into_bytes())?,
            conversation,
            actor,
            command.query().workspace(),
            command.expected_revision(),
            ControlIntent::PrepareRestore { restore, recovery },
        );
        let settle_id = OperationId::new(derived_id(
            b"peritus-workbench-rewind-settle-v1\0",
            command.operation().as_bytes(),
        ))?;
        let restored = confirmed
            .paths()
            .iter()
            .filter(|path| path.disposition() == WorkbenchRewindDisposition::Restore)
            .map(|path| path.path().to_owned())
            .collect::<Vec<_>>();

        let (status, terminal_conflicts, _transaction_manifest, accepted_revision) = self
            .with_controls(false, |store| {
                if !conversation_only {
                    self.require_folder_write(store, actor, command, command.expected_revision())?;
                }
                let preparation = store.accept_restore_preparation(&prepare, &recovery_bodies)?;
                #[cfg(test)]
                check_rewind_fault(command, RewindFaultPoint::AfterPrepare)?;
                let (status, terminal_conflicts, transaction_manifest) = if !conflicts.is_empty() {
                    (RestoreStatus::Conflict, conflicts, None)
                } else if let Some(plan) = plan {
                    match self.apply_authorized_folder_patch(
                        store,
                        actor,
                        session,
                        command,
                        plan,
                        preparation.accepted_revision(),
                    ) {
                        Ok(applied) => (
                            RestoreStatus::Applied,
                            Vec::new(),
                            Some(applied.applied_patch().installed_manifest().to_vec()),
                        ),
                        Err(Error::Workspace(error))
                            if error.recovery() == peritus_workspace::RecoveryClass::Reobserve =>
                        {
                            (RestoreStatus::Conflict, restored.clone(), None)
                        }
                        // Uncertain C1 outcomes remain retryable Prepared records. Only the
                        // explicit reconciler can establish their durable terminal outcome.
                        Err(error) => return Err(error),
                    }
                } else {
                    (
                        RestoreStatus::Applied,
                        Vec::new(),
                        Some(noop_manifest(confirmed.preview_digest())),
                    )
                };
                #[cfg(test)]
                check_rewind_fault(command, RewindFaultPoint::AfterFolderPatch)?;
                let manifest_digest = transaction_manifest
                    .as_ref()
                    .map(|bytes| peritus_codec::sha256(bytes).into_bytes());
                let settle = ControlOperation::new(
                    settle_id,
                    conversation,
                    actor,
                    command.query().workspace(),
                    preparation.accepted_revision(),
                    ControlIntent::SettleRestore {
                        restore: restore_id,
                        status,
                        conflicts: terminal_conflicts.clone(),
                        transaction_manifest_digest: manifest_digest,
                    },
                );
                let receipt =
                    store.accept_restore_settlement(&settle, transaction_manifest.clone())?;
                Ok((status, terminal_conflicts, transaction_manifest, receipt.accepted_revision()))
            })?;
        public_restore(
            command,
            recovery_id,
            status,
            accepted_revision,
            restored,
            terminal_conflicts,
        )
    }

    fn restore_plan(
        &self,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
        preview: &WorkbenchRewindPreview,
    ) -> Result<Option<PatchSet>, Error> {
        self.with_controls(false, |store| {
            self.restore_plan_with_store(store, query, checkpoint, preview)
        })
    }

    pub(super) fn restore_plan_with_store(
        &self,
        store: &ControlStore,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
        preview: &WorkbenchRewindPreview,
    ) -> Result<Option<PatchSet>, Error> {
        let mut operations = Vec::new();
        for (index, (path, preview_path)) in
            checkpoint.paths().iter().zip(preview.paths()).enumerate()
        {
            if preview_path.disposition() != WorkbenchRewindDisposition::Restore {
                continue;
            }
            let workspace_path = patch_input(WorkspacePath::new(path.path()))?;
            let preimage =
                patch_preimage(path.owned_postchange().ok_or(ControlError::InvalidInput)?);
            let operation = match path.checkpoint() {
                CheckpointFileVersion::Absent => {
                    patch_input(PatchOperation::delete(workspace_path, preimage))?
                }
                target @ CheckpointFileVersion::Present { .. } => {
                    let body = store
                        .checkpoint_body(checkpoint.id(), index, target)?
                        .ok_or(Error::Corrupt("present checkpoint target has no retained bytes"))?;
                    let final_file = patch_input(FinalFile::new(
                        body,
                        patch_mode(target.mode().ok_or(ControlError::InvalidInput)?),
                        LineEndingPolicy::Preserve,
                    ))?;
                    match preimage {
                        Preimage::Absent => PatchOperation::create(workspace_path, final_file),
                        Preimage::Present { .. } => patch_input(PatchOperation::replace(
                            workspace_path,
                            preimage,
                            final_file,
                        ))?,
                    }
                }
            };
            operations.push(operation);
        }
        if operations.is_empty() {
            return Ok(None);
        }
        let patch = patch_input(PatchSet::new(
            query.workspace(),
            Generation::first(),
            RevisionNumber::new(preview.request().revision())
                .map_err(|_| ControlError::InvalidInput)?,
            operations,
        ))?;
        Ok(Some(patch))
    }
}
