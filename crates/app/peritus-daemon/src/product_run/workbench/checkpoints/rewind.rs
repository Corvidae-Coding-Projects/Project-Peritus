//! Reviewed rewind preview, exact C1 application, and truthful durable settlement.

use super::capture::{check_protected, observe_path};
use super::{
    ActorId, AppErrorCode, AppResponsePayload, CheckpointFileVersion, CheckpointId, CheckpointPath,
    ControlError, ControlIntent, ControlOperation, ControlStore, ConversationId, Error, FinalFile,
    Generation, LineEndingPolicy, OperationId, PatchOperation, PatchSet, Preimage,
    ProductRunService, RestoreId, RestoreOperation, RestoreStatus, RevisionNumber, UserCheckpoint,
    WorkbenchCommand, WorkbenchIntent, WorkbenchRestoreProjection, WorkbenchRewindConfirmation,
    WorkbenchRewindDisposition, WorkbenchRewindPath, WorkbenchRewindPreview,
    WorkbenchRewindRequest, WorkspacePath, app_error, check_record, checkpoint_references,
    derived_id, error_response, external_effects, noop_manifest, patch_input, patch_mode,
    patch_preimage, public_restore, public_version,
};

mod plan;
mod preview;

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
    pub(crate) async fn apply_paged_workbench_rewind(
        &self,
        actor: ActorId,
        session: peritus_types::SessionId,
        command: &WorkbenchCommand,
        envelope: &peritus_app_protocol::AppRequestEnvelope,
        limits: peritus_app_protocol::AppProtocolLimits,
    ) -> AppResponsePayload {
        let command_copy = command.clone();
        let envelope = envelope.clone();
        let preflight = tokio::task::spawn_blocking(move || {
            let WorkbenchIntent::ConfirmRewind(confirmation) = command_copy.intent() else {
                return Err(Box::new(error_response(ControlError::InvalidInput.into())));
            };
            let request = confirmation.request();
            if request.query() != command_copy.query()
                || request.revision() != command_copy.expected_revision()
            {
                return Err(Box::new(error_response(ControlError::StaleRevision.into())));
            }
            let recovery = CheckpointId::new(derived_id(
                b"peritus-workbench-rewind-recovery-v1\0",
                command_copy.operation().as_bytes(),
            ))
            .map_err(|_| Box::new(error_response(ControlError::InvalidInput.into())))?;
            let summary = peritus_app_protocol::WorkbenchRestoreSummary::new(
                command_copy.operation(),
                request.checkpoint(),
                peritus_app_protocol::ControlOperationId::new(*recovery.as_bytes())
                    .map_err(|_| error_response(ControlError::InvalidInput.into()))?,
                request.query(),
                u64::MAX,
                peritus_app_protocol::WorkbenchRestoreStatus::Applied,
                u64::MAX,
                0,
                confirmation.preview_digest(),
            )
            .map_err(|error| Box::new(AppResponsePayload::Error(error)))?;
            if !super::pages::restore_summary_fits(&envelope, limits, &summary) {
                return Err(Box::new(AppResponsePayload::Error(
                    peritus_app_protocol::AppProtocolError::new(AppErrorCode::LimitExceeded, None),
                )));
            }
            Ok(())
        })
        .await;
        match preflight {
            Ok(Ok(())) => self.apply_workbench_rewind(actor, session, command).await,
            Ok(Err(response)) => *response,
            Err(_) => AppResponsePayload::Error(app_error(AppErrorCode::Internal)),
        }
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
            Ok(result) => result.map_or_else(error_response, |outcome| match outcome {
                WorkbenchRestoreProjection::Detailed(receipt) => {
                    AppResponsePayload::WorkbenchRestore(receipt)
                }
                WorkbenchRestoreProjection::Summary(summary) => {
                    AppResponsePayload::WorkbenchRestoreSummary(summary)
                }
            }),
            Err(_) => AppResponsePayload::Error(app_error(AppErrorCode::Internal)),
        }
    }

    fn apply_rewind(
        &self,
        actor: ActorId,
        session: peritus_types::SessionId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchRestoreProjection, Error> {
        let request = match command.intent() {
            WorkbenchIntent::ApplyRewind(preview) => preview.request(),
            WorkbenchIntent::ConfirmRewind(confirmation) => confirmation.request(),
            _ => return Err(ControlError::InvalidInput.into()),
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
        if request.query() != command.query() || request.revision() != command.expected_revision() {
            return Err(ControlError::StaleRevision.into());
        }
        let current = self.rewind_preview(actor, request)?;
        let (binding_digest, full_preview) = match command.intent() {
            WorkbenchIntent::ApplyRewind(confirmed) => {
                if &current != confirmed {
                    return Err(ControlError::StaleRevision.into());
                }
                (confirmed.preview_digest(), current)
            }
            WorkbenchIntent::ConfirmRewind(confirmation) => {
                let checkpoint_receipt = self.checkpoint_receipt(actor, request)?;
                let expected = WorkbenchRewindConfirmation::for_preview(
                    request,
                    &checkpoint_receipt,
                    &current,
                )
                .map_err(|_| ControlError::StaleRevision)?;
                if expected != *confirmation {
                    return Err(ControlError::StaleRevision.into());
                }
                (confirmation.preview_digest(), current)
            }
            _ => return Err(ControlError::InvalidInput.into()),
        };
        let confirmed = &full_preview;

        let checkpoint_id = CheckpointId::new(request.checkpoint().into_bytes())?;
        let record = self
            .with_controls(false, |store| store.load(conversation))?
            .ok_or(ControlError::NotFound)?;
        check_record(&record, actor, command.query(), Some(command.expected_revision()))?;
        let checkpoint = self
            .with_controls(false, |store| store.load_checkpoint(conversation, checkpoint_id))?
            .ok_or(ControlError::NotFound)?;
        let replay_only = !record.checkpoints().iter().any(|value| value.id() == checkpoint_id);
        let checkpoint_versions = checkpoint
            .paths()
            .iter()
            .map(|path| (path.path().to_owned(), path.checkpoint()))
            .collect::<Vec<_>>();

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
            binding_digest,
            patch_digest,
            recovery_id,
        )?;
        if let Some(branch) = branch {
            restore = restore.with_branch(branch)?;
        }
        let prepare_intent = if replay_only {
            ControlIntent::PrepareAutomaticRestore {
                restore,
                checkpoint: Box::new(checkpoint),
                recovery,
            }
        } else {
            ControlIntent::PrepareRestore { restore, recovery }
        };
        let prepare = ControlOperation::new(
            OperationId::new(command.operation().into_bytes())?,
            conversation,
            actor,
            command.query().workspace(),
            command.expected_revision(),
            prepare_intent,
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
                let settle_intent = if replay_only {
                    ControlIntent::SettleAutomaticRestore {
                        restore: restore_id,
                        checkpoint: checkpoint_id,
                        status,
                        conflicts: terminal_conflicts.clone(),
                        transaction_manifest_digest: manifest_digest,
                        seal_recovery: true,
                        checkpoint_versions: checkpoint_versions.clone(),
                    }
                } else {
                    ControlIntent::SettleRestore {
                        restore: restore_id,
                        status,
                        conflicts: terminal_conflicts.clone(),
                        transaction_manifest_digest: manifest_digest,
                        seal_recovery: true,
                    }
                };
                let settle = ControlOperation::new(
                    settle_id,
                    conversation,
                    actor,
                    command.query().workspace(),
                    preparation.accepted_revision(),
                    settle_intent,
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
            confirmed.external_effects().to_vec(),
        )
    }
}
