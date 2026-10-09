use super::{
    ActionConsumptionBinding, ActionId, ActionTerminalRecord, AppliedPatch, AuthorizationTarget,
    Generation, MutationOutcome, MutationRecoveryOutcome, PatchSet, RevisionNumber,
    WorkspaceAuthorizationRequest, WorkspaceCondition, WorkspaceError, WorkspaceGateway,
    consumption, patch_error, patch_payload, receipt_persistence_error, recovery_error,
    validate_authority,
};
use std::path::Path;

struct OpenTransactionRecovery<'a> {
    legacy: bool,
    binding: ActionConsumptionBinding,
    patch_identity: peritus_patch::PatchIdentity,
    patch_binding: peritus_patch::RecoveryBinding,
    transaction_root: &'a Path,
    transaction_directory: &'a Path,
}

impl WorkspaceGateway {
    /// Reconciles a consumed patch action against its exact transaction manifest and result.
    ///
    /// This operation never reapplies the patch. It records a conclusive transaction result in
    /// the existing action marker before recovery removes transaction files.
    ///
    /// # Errors
    ///
    /// Returns an error if the supplied authority or patch differs from the consumed action.
    pub fn recover_mutation(
        &mut self,
        authorization: &WorkspaceAuthorizationRequest<'_>,
        patch: PatchSet,
    ) -> Result<MutationRecoveryOutcome, WorkspaceError> {
        let payload = patch_payload(&patch, authorization.caller_binding());
        let permit = validate_authority(
            AuthorizationTarget::from_workspace(self.state()),
            authorization,
            &payload,
        )?;
        if permit.generation() != self.state().generation()
            || permit.revision() != self.state().revision()
        {
            return Err(patch_error("recovery authority differs from the current workspace"));
        }
        let patch_identity = patch.identity();
        let patch_binding = peritus_patch::RecoveryBinding::new(
            patch.workspace_id(),
            patch.expected_generation(),
            patch.expected_revision(),
        );
        patch
            .plan(
                self.state().binding().workspace_id(),
                self.state().generation(),
                self.state().revision(),
            )
            .map_err(|_| patch_error("recovery patch does not match current workspace state"))?;
        let binding = ActionConsumptionBinding::from_state(self.state());
        let transaction_root = self.workspace_mut().transaction_root().to_owned();
        let transaction_directory =
            transaction_root.join(format!("txn-{}", patch_identity.to_hex()));
        let Some(action) =
            consumption::action_record(&transaction_root, binding, permit.action_id())?
        else {
            return Ok(self.recover_absent_action(&transaction_directory));
        };
        if action.action_digest != permit.action_digest() {
            return Err(patch_error("recovery authority differs from the consumed action"));
        }
        if let Some(terminal) = action.terminal.as_ref() {
            return self.recover_recorded_terminal(
                terminal,
                &permit,
                patch_identity,
                patch_binding,
                &transaction_directory,
            );
        }
        self.recover_open_transaction(
            &permit,
            &OpenTransactionRecovery {
                legacy: action.legacy,
                binding,
                patch_identity,
                patch_binding,
                transaction_root: &transaction_root,
                transaction_directory: &transaction_directory,
            },
        )
    }

    fn recover_absent_action(&mut self, transaction_directory: &Path) -> MutationRecoveryOutcome {
        match std::fs::symlink_metadata(transaction_directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                MutationRecoveryOutcome::NotAttempted
            }
            _ => {
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
                MutationRecoveryOutcome::Indeterminate
            }
        }
    }

    fn recover_recorded_terminal(
        &mut self,
        terminal: &ActionTerminalRecord,
        permit: &super::MutationPermit,
        patch_identity: peritus_patch::PatchIdentity,
        patch_binding: peritus_patch::RecoveryBinding,
        transaction_directory: &Path,
    ) -> Result<MutationRecoveryOutcome, WorkspaceError> {
        match terminal {
            ActionTerminalRecord::Applied { patch_identity: recorded, installed_manifest } => {
                if *recorded != patch_identity {
                    return Err(patch_error("retained patch result differs from this request"));
                }
                let retained = AppliedPatch::from_installed_manifest(
                    *recorded,
                    installed_manifest.clone(),
                    true,
                )
                .map_err(|_| recovery_error("retained patch result is malformed"))?;
                let cleanup_pending = peritus_patch::cleanup_applied_transaction(
                    self.workspace_mut().root(),
                    transaction_directory,
                    &retained,
                )
                .is_err();
                let applied = AppliedPatch::from_installed_manifest(
                    *recorded,
                    retained.installed_manifest().to_vec(),
                    cleanup_pending,
                )
                .map_err(|_| recovery_error("retained patch result is malformed"))?;
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Dirty);
                Ok(MutationRecoveryOutcome::AlreadyApplied(self.recovered_outcome(
                    permit.action_id(),
                    permit.generation(),
                    permit.revision(),
                    applied,
                )))
            }
            ActionTerminalRecord::RolledBack => {
                Ok(self.recover_recorded_rollback(patch_binding, transaction_directory))
            }
            ActionTerminalRecord::Candidate { .. }
            | ActionTerminalRecord::WorkspaceRollback { .. } => {
                Err(patch_error("action receipt belongs to a Git mutation"))
            }
        }
    }

    fn recover_recorded_rollback(
        &mut self,
        patch_binding: peritus_patch::RecoveryBinding,
        transaction_directory: &Path,
    ) -> MutationRecoveryOutcome {
        if transaction_directory.try_exists().unwrap_or(true) {
            let recovery = peritus_patch::recover_transaction(
                self.workspace_mut().root(),
                transaction_directory,
                patch_binding,
            );
            match recovery {
                Ok(recovery)
                    if recovery.state() == peritus_patch::RecoveryState::RolledBackCleanly => {}
                Ok(recovery) if recovery.state() == peritus_patch::RecoveryState::Dirty => {
                    self.workspace_mut()
                        .state_mut()
                        .set_condition(WorkspaceCondition::Indeterminate);
                    return MutationRecoveryOutcome::Dirty;
                }
                Ok(_) | Err(_) => {
                    self.workspace_mut()
                        .state_mut()
                        .set_condition(WorkspaceCondition::Indeterminate);
                    return MutationRecoveryOutcome::Indeterminate;
                }
            }
        }
        self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Clean);
        MutationRecoveryOutcome::RolledBack
    }

    fn recover_open_transaction(
        &mut self,
        permit: &super::MutationPermit,
        recovery: &OpenTransactionRecovery<'_>,
    ) -> Result<MutationRecoveryOutcome, WorkspaceError> {
        match std::fs::symlink_metadata(recovery.transaction_directory) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !recovery.legacy {
                    return Ok(MutationRecoveryOutcome::NotAttempted);
                }
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
                return Ok(MutationRecoveryOutcome::Indeterminate);
            }
            Ok(_) | Err(_) => {
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
                return Ok(MutationRecoveryOutcome::Indeterminate);
            }
        }
        let workspace_root = self.workspace_mut().root().to_owned();
        let mut recovered_patch = None;
        let patch_recovery = peritus_patch::recover_transaction_with_completion(
            workspace_root,
            recovery.transaction_directory,
            recovery.patch_binding,
            recovery.patch_identity,
            |applied| {
                let terminal = applied.map_or(ActionTerminalRecord::RolledBack, |applied| {
                    recovered_patch = Some(applied.clone());
                    ActionTerminalRecord::Applied {
                        patch_identity: applied.identity(),
                        installed_manifest: applied.installed_manifest().to_vec(),
                    }
                });
                consumption::complete_action(
                    recovery.transaction_root,
                    recovery.binding,
                    permit.action_id(),
                    permit.action_digest(),
                    &terminal,
                )
                .map_err(|_| receipt_persistence_error())
            },
        );
        let Ok(patch_recovery) = patch_recovery else {
            self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
            return Ok(MutationRecoveryOutcome::Indeterminate);
        };
        match patch_recovery.state() {
            peritus_patch::RecoveryState::AlreadyApplied => {
                let applied = recovered_patch
                    .ok_or_else(|| recovery_error("recovery omitted its installed result"))?;
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Dirty);
                Ok(MutationRecoveryOutcome::AlreadyApplied(self.recovered_outcome(
                    permit.action_id(),
                    permit.generation(),
                    permit.revision(),
                    applied,
                )))
            }
            peritus_patch::RecoveryState::RolledBackCleanly => {
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Clean);
                Ok(MutationRecoveryOutcome::RolledBack)
            }
            peritus_patch::RecoveryState::Dirty => {
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
                Ok(MutationRecoveryOutcome::Dirty)
            }
            peritus_patch::RecoveryState::Indeterminate => {
                self.workspace_mut().state_mut().set_condition(WorkspaceCondition::Indeterminate);
                Ok(MutationRecoveryOutcome::Indeterminate)
            }
        }
    }

    const fn recovered_outcome(
        &self,
        action_id: ActionId,
        generation: Generation,
        revision: RevisionNumber,
        patch: AppliedPatch,
    ) -> MutationOutcome {
        MutationOutcome {
            action_id,
            workspace_id: self.state().binding().workspace_id(),
            resource_id: self.state().binding().resource_id(),
            generation,
            revision,
            patch,
        }
    }
}
