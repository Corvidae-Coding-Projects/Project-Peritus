//! Reviewed rewind preview, exact C1 application, and truthful durable settlement.

use super::{
    ActorId, AppErrorCode, AppResponsePayload, CheckpointFileVersion, CheckpointId, CheckpointPath,
    ControlError, ControlIntent, ControlOperation, ControlStore, ConversationId, Error, FinalFile,
    Generation, LineEndingPolicy, OperationId, PatchOperation, PatchSet, Preimage,
    ProductRunService, RestoreId, RestoreOperation, RestoreStatus, RevisionNumber, UserCheckpoint,
    WorkbenchCommand, WorkbenchIntent, WorkbenchRestoreReceipt, WorkbenchRewindDisposition,
    WorkbenchRewindPreview, WorkspacePath, app_error, check_record, checkpoint_references,
    derived_id, error_response, external_effects, noop_manifest, patch_input, patch_mode,
    patch_preimage, public_restore, public_version,
};

mod plan;
mod preview;
pub(super) use plan::selected_coverage_matches;

#[cfg(test)]
mod faults;
#[cfg(test)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "crate-level tests inject exact crash boundaries"
)]
pub(crate) use faults::RewindFaultPoint;

impl ProductRunService {
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
            return Err(Error::StalePreimage);
        }

        let checkpoint_id = CheckpointId::new(request.checkpoint().into_bytes())?;
        let record = self
            .with_controls(false, |store| store.load(conversation))?
            .ok_or(ControlError::NotFound)?;
        check_record(&record, actor, command.query(), Some(command.expected_revision()))?;
        let checkpoint = self
            .with_controls(false, |store| store.load_checkpoint(conversation, checkpoint_id))?
            .ok_or(ControlError::NotFound)?;
        let replay_only = !record.checkpoints().iter().any(|value| value.id() == checkpoint_id);
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
            return Err(Error::StalePreimage);
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
                    WorkbenchRewindDisposition::Conflict
                        | WorkbenchRewindDisposition::Unsealed
                        | WorkbenchRewindDisposition::Unavailable
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
            self.with_controls(false, |store| {
                self.restore_plan_materialized(
                    store,
                    command.query(),
                    &checkpoint,
                    confirmed,
                    None,
                    Some((&recovery, Some(&recovery_bodies))),
                )
            })?
        } else {
            None
        };
        let patch_digest = plan
            .as_ref()
            .map_or_else(|| confirmed.preview_digest(), |patch| patch.identity().digest());
        let checkpoint_versions = planned_versions(&recovery, plan.as_ref());
        let mut restore = RestoreOperation::prepared(
            restore_id,
            checkpoint_id,
            confirmed.preview_digest(),
            patch_digest,
            recovery_id,
        )?
        .with_targets(checkpoint_versions.clone())?;
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
                let preparation = store.accept_restore_snapshots(&prepare, &recovery_bodies)?;
                #[cfg(test)]
                self.check_rewind_fault(command, RewindFaultPoint::AfterPrepare)?;
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
                self.check_rewind_fault(command, RewindFaultPoint::AfterFolderPatch)?;
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
        )
    }
}

pub(super) fn planned_versions(
    recovery: &UserCheckpoint,
    plan: Option<&PatchSet>,
) -> Vec<(String, CheckpointFileVersion)> {
    let mut operations = plan.map_or(&[][..], PatchSet::operations).iter().peekable();
    recovery
        .paths()
        .iter()
        .map(|path| {
            let version = if operations
                .peek()
                .is_some_and(|operation| operation.path().as_str() == path.path())
            {
                checkpoint_version(operations.next().expect("checked operation").postimage())
            } else {
                path.checkpoint()
            };
            (path.path().to_owned(), version)
        })
        .collect()
}

const fn checkpoint_version(preimage: Preimage) -> CheckpointFileVersion {
    match preimage {
        Preimage::Absent => CheckpointFileVersion::Absent,
        Preimage::EmptyDirectory { mode } => CheckpointFileVersion::empty_directory(mode),
        Preimage::Present { digest, size, mode } => CheckpointFileVersion::present(
            digest,
            size,
            match mode {
                peritus_patch::FileMode::Regular => super::CheckpointFileMode::Regular,
                peritus_patch::FileMode::Executable => super::CheckpointFileMode::Executable,
            },
        ),
    }
}
