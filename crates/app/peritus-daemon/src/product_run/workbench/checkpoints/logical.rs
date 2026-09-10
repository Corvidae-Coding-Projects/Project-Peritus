//! Reserved logical rewind branches, published atomically only after successful settlement.
use super::{ControlError, ControlStore, Error, ProductRunService, derived_id, public_restore};
use peritus_app_protocol::{
    ControlOperationId, ConversationTitle, WorkbenchCommand, WorkbenchForkMode,
    WorkbenchForkRequest, WorkbenchIntent, WorkbenchQuery, WorkbenchRestoreReceipt,
    WorkbenchRestoreStatus, WorkbenchRewindRequest,
};
use peritus_product_runner::control::{
    CheckpointId, ControlIntent, ControlOperation, ConversationBranch, ConversationId,
    ConversationRecord, OperationId, RestoreId, RestoreStatus,
};
use peritus_types::ActorId;

impl ProductRunService {
    pub(super) fn logical_rewind_branch(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        request: WorkbenchRewindRequest,
        record: &ConversationRecord,
    ) -> Result<Option<ConversationBranch>, Error> {
        let Some(child) = request.child() else { return Ok(None) };
        if record.goal().is_some() != request.allocation().is_some() {
            return Err(ControlError::InvalidInput.into());
        }
        self.with_controls(false, |store| {
            if store.load(ConversationId::new(child.into_bytes())?)?.is_some() {
                return Err(ControlError::IdempotencyConflict.into());
            }
            let checkpoint = record
                .checkpoints()
                .iter()
                .find(|value| value.id().as_bytes() == request.checkpoint().as_bytes())
                .ok_or(ControlError::NotFound)?;
            let refs = checkpoint.references();
            let fork = WorkbenchForkRequest::new(
                WorkbenchQuery::new(child, request.query().workspace()),
                ConversationTitle::new(format!("Rewind of {}", record.title()))
                    .map_err(|_| ControlError::Capacity)?,
                request.checkpoint(),
                refs.source_conversation_revision(),
                refs.context_generation(),
                refs.brief_revision(),
                refs.goal_revision().unwrap_or(0),
                WorkbenchForkMode::ReadOnlyCurrentWorkspace,
                request.allocation(),
            )
            .map_err(|_| ControlError::InvalidInput)?;
            let operation = ControlOperationId::new(derived_id(
                b"peritus-workbench-rewind-branch-v1\0",
                command.operation().as_bytes(),
            ))
            .map_err(|_| ControlError::InvalidInput)?;
            let public = WorkbenchCommand::new(
                operation,
                command.query(),
                command.expected_revision(),
                WorkbenchIntent::ForkConversation(fork.clone()),
            );
            let source = store
                .load_revision(record.id(), refs.source_conversation_revision())?
                .ok_or(ControlError::NotFound)?;
            super::super::fork::validate_fork_governance(record, &source, &fork)?;
            let branch = super::super::fork::branch(actor, &public, &fork, &source)?;
            // Check the complete reservation before admitting any filesystem work.
            let proposal = ControlOperation::new(
                branch.operation(),
                record.id(),
                actor,
                command.query().workspace(),
                record.revision(),
                ControlIntent::ReserveFork {
                    branch: branch.clone(),
                    now_unix_millis: super::super::goal::now_millis(),
                },
            );
            ConversationRecord::apply(Some(record), &proposal)?;
            Ok(Some(branch))
        })
    }

    pub(super) fn finish_logical_rewind(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        receipt: WorkbenchRestoreReceipt,
    ) -> Result<WorkbenchRestoreReceipt, Error> {
        self.logical_rewind_receipt(actor, command, receipt, true)
    }

    pub(super) fn observe_logical_rewind(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        receipt: WorkbenchRestoreReceipt,
    ) -> Result<WorkbenchRestoreReceipt, Error> {
        self.logical_rewind_receipt(actor, command, receipt, false)
    }

    fn logical_rewind_receipt(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
        receipt: WorkbenchRestoreReceipt,
        publish: bool,
    ) -> Result<WorkbenchRestoreReceipt, Error> {
        if receipt.status() != WorkbenchRestoreStatus::Applied {
            return Ok(receipt);
        }
        let source = ConversationId::new(command.query().conversation().into_bytes())?;
        let restore_id = RestoreId::new(command.operation().into_bytes())?;
        let published = self.with_controls(false, |store| {
            publish_branch(store, actor, command, source, restore_id, publish)
        })?;
        let Some(revision) = published else { return Ok(receipt) };
        public_restore(
            command,
            CheckpointId::new(receipt.recovery_checkpoint().into_bytes())?,
            RestoreStatus::Applied,
            revision,
            receipt.restored().to_vec(),
            receipt.conflicts().to_vec(),
        )
    }
}

fn publish_branch(
    store: &mut ControlStore,
    actor: ActorId,
    command: &WorkbenchCommand,
    source: ConversationId,
    restore_id: RestoreId,
    publish: bool,
) -> Result<Option<u64>, Error> {
    let record = store.load(source)?.ok_or(ControlError::NotFound)?;
    if record.owner_bytes() != actor.as_bytes()
        || record.workspace_bytes() != command.query().workspace().as_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    let restore = record
        .restores()
        .iter()
        .find(|value| value.id() == restore_id)
        .ok_or(ControlError::NotFound)?;
    if restore.status() != RestoreStatus::Applied {
        return Err(ControlError::InvalidInput.into());
    }
    let Some(branch) = restore.branch() else { return Ok(None) };
    let child = super::super::fork::child_operation(actor, branch)?;
    let intent =
        ControlIntent::PublishRestoreBranch { restore: restore_id, branch: branch.clone() };
    let operation = if let Some(existing) = store.operation(source, branch.operation())? {
        if existing.intent() != &intent || existing.actor_bytes() != actor.as_bytes() {
            return Err(ControlError::IdempotencyConflict.into());
        }
        existing
    } else {
        if !publish {
            return Err(ControlError::NotFound.into());
        }
        ControlOperation::new(
            OperationId::new(*branch.operation().as_bytes())?,
            source,
            actor,
            command.query().workspace(),
            record.revision(),
            intent,
        )
    };
    let receipt = if publish {
        store.accept_fork(&operation, &child, branch)?
    } else {
        store.resolve_fork(&operation, &child, branch)?.ok_or(ControlError::NotFound)?
    };
    Ok(Some(receipt.accepted_revision()))
}
