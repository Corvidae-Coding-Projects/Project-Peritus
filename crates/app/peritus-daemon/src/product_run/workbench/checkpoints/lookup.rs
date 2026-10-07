//! Exact restore receipt lookup and retry resolution.

use super::{
    ActorId, ControlError, ControlIntent, ControlOperation, ConversationId, Error, OperationId,
    ProductRunService, RestoreId, RestoreStatus, Sha256Digest, WorkbenchCommand, WorkbenchIntent,
    WorkbenchRestoreReceipt, WorkbenchRewindDisposition, check_record, derived_id, public_restore,
};

impl ProductRunService {
    pub(crate) fn resolve_workbench_restore(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchRestoreReceipt, Error> {
        self.restore_receipt(actor, command, true)
    }

    pub(crate) fn observe_workbench_restore(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> Result<WorkbenchRestoreReceipt, Error> {
        self.restore_receipt(actor, command, false)
    }

    fn restore_receipt(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        resume: bool,
    ) -> Result<WorkbenchRestoreReceipt, Error> {
        let WorkbenchIntent::ApplyRewind(confirmed) = command.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        if confirmed.request().query() != command.query()
            || confirmed.request().revision() != command.expected_revision()
        {
            return Err(ControlError::StaleRevision.into());
        }
        self.control_workspace(command.query())?;
        let conversation = ConversationId::new(command.query().conversation().into_bytes())?;
        let receipt = self.with_control_conversation(conversation, |store| {
            let record = store.load(conversation)?.ok_or(ControlError::NotFound)?;
            check_record(&record, actor, command.query(), None)?;
            let restore_id = RestoreId::new(command.operation().into_bytes())?;
            let restore = record
                .restores()
                .iter()
                .find(|restore| restore.id() == restore_id)
                .cloned()
                .ok_or(ControlError::NotFound)?;
            if restore.checkpoint().as_bytes() != confirmed.request().checkpoint().as_bytes()
                || restore.preview_digest() != confirmed.preview_digest()
            {
                return Err(ControlError::IdempotencyConflict.into());
            }
            let recovery = record
                .checkpoints()
                .iter()
                .find(|checkpoint| checkpoint.id() == restore.recovery_checkpoint())
                .cloned()
                .ok_or(Error::Corrupt("restore recovery checkpoint missing"))?;
            let checkpoint = store
                .load_checkpoint(conversation, restore.checkpoint())?
                .ok_or(Error::Corrupt("restore source checkpoint missing"))?;
            let replay_only =
                !record.checkpoints().iter().any(|value| value.id() == restore.checkpoint());
            let checkpoint_versions = restore
                .targets()
                .unwrap_or_else(|| checkpoint.paths())
                .iter()
                .map(|path| (path.path().to_owned(), path.checkpoint()))
                .collect::<Vec<_>>();
            let prepared = restore.prepared_manifest();
            let prepare_intent = if replay_only {
                ControlIntent::PrepareAutomaticRestore {
                    restore: prepared,
                    checkpoint: Box::new(checkpoint.clone()),
                    recovery: recovery.capture_manifest(),
                }
            } else {
                ControlIntent::PrepareRestore {
                    restore: prepared,
                    recovery: recovery.capture_manifest(),
                }
            };
            let prepare = ControlOperation::new(
                OperationId::new(command.operation().into_bytes())?,
                conversation,
                actor,
                command.query().workspace(),
                command.expected_revision(),
                prepare_intent,
            );
            let _preparation = store.resolve(&prepare)?.ok_or(ControlError::NotFound)?;
            let (status, conflicts, accepted_revision) = if restore.status()
                == RestoreStatus::Prepared
            {
                if !resume {
                    return Err(ControlError::NotFound.into());
                }
                if confirmed.request().mode()
                    != peritus_app_protocol::WorkbenchRewindMode::ConversationOnly
                {
                    self.require_folder_write(store, actor, command, record.revision())?;
                }
                let recovered = self.recover_prepared_restore(
                    store,
                    &record,
                    actor,
                    command,
                    &restore,
                    (&checkpoint, &recovery),
                )?;
                let evidence_digest = peritus_codec::sha256(&recovered.evidence).into_bytes();
                let settle_intent = if replay_only {
                    ControlIntent::SettleAutomaticRestore {
                        restore: restore.id(),
                        checkpoint: checkpoint.id(),
                        status: recovered.status,
                        conflicts: recovered.conflicts.clone(),
                        transaction_manifest_digest: Some(evidence_digest),
                        seal_recovery: true,
                        checkpoint_versions,
                    }
                } else {
                    ControlIntent::SettleRestore {
                        restore: restore.id(),
                        status: recovered.status,
                        conflicts: recovered.conflicts.clone(),
                        transaction_manifest_digest: Some(evidence_digest),
                        seal_recovery: true,
                    }
                };
                let settle = ControlOperation::new(
                    OperationId::new(derived_id(
                        b"peritus-workbench-rewind-settle-v1\0",
                        command.operation().as_bytes(),
                    ))?,
                    conversation,
                    actor,
                    command.query().workspace(),
                    record.revision(),
                    settle_intent,
                );
                let receipt = store.accept_restore_settlement(&settle, Some(recovered.evidence))?;
                (recovered.status, recovered.conflicts, receipt.accepted_revision())
            } else {
                let settle_id = OperationId::new(derived_id(
                    b"peritus-workbench-rewind-settle-v1\0",
                    command.operation().as_bytes(),
                ))?;
                let settle = store
                    .operation(conversation, settle_id)?
                    .ok_or(Error::Corrupt("terminal restore settlement operation missing"))?;
                let (settled_restore, status, conflicts, transaction_manifest_digest) =
                    match settle.intent() {
                        ControlIntent::SettleRestore {
                            restore,
                            status,
                            conflicts,
                            transaction_manifest_digest,
                            ..
                        }
                        | ControlIntent::SettleAutomaticRestore {
                            restore,
                            status,
                            conflicts,
                            transaction_manifest_digest,
                            ..
                        } => (restore, status, conflicts, transaction_manifest_digest),
                        _ => {
                            return Err(Error::Corrupt(
                                "restore settlement identity has another intent",
                            ));
                        }
                    };
                if *settled_restore != restore.id()
                    || *status != restore.status()
                    || conflicts.iter().map(String::as_str).ne(restore.conflicts())
                    || transaction_manifest_digest.as_ref()
                        != restore
                            .transaction_manifest_digest()
                            .as_ref()
                            .map(Sha256Digest::as_bytes)
                {
                    return Err(Error::Corrupt("restore settlement differs from durable state"));
                }
                let receipt = store.resolve(&settle)?.ok_or(ControlError::NotFound)?;
                (restore.status(), conflicts.clone(), receipt.accepted_revision())
            };
            let restored = confirmed
                .paths()
                .iter()
                .filter(|path| path.disposition() == WorkbenchRewindDisposition::Restore)
                .map(|path| path.path().to_owned())
                .collect();
            public_restore(
                command,
                restore.recovery_checkpoint(),
                status,
                accepted_revision,
                restored,
                conflicts,
            )
        })?;
        if resume {
            self.finish_logical_rewind(actor, command, receipt)
        } else {
            self.observe_logical_rewind(actor, command, receipt)
        }
    }
}
