//! Exact parent/checkpoint lineage validation and atomic non-running child creation.

use super::{ProductRunService, error_response, receipt_projection};
use crate::product_control::ControlStoreError;
use peritus_app_protocol::{
    AppResponsePayload, WorkbenchCommand, WorkbenchForkMode, WorkbenchForkRequest, WorkbenchIntent,
};
use peritus_product_runner::control::{
    ChildBudgetAllocation, ControlError, ControlIntent, ControlOperation, ConversationBranch,
    ConversationBranchMode, ConversationId, GoalCriterion, OperationId,
};
use peritus_types::{ActorId, WorkspaceId};

impl ProductRunService {
    pub(super) fn fork_workbench(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let WorkbenchIntent::ForkConversation(request) = command.intent() else {
            return error_response(ControlError::InvalidInput.into());
        };
        let result = self.control_workspace(command.query()).and_then(|()| {
            self.validate_fork_workspaces(command, request)?;
            self.with_controls(false, |store| {
                let source_id = ConversationId::new(command.query().conversation().into_bytes())?;
                let operation_id = OperationId::new(command.operation().into_bytes())?;
                if let Some(existing) = store.operation(source_id, operation_id)? {
                    if existing.actor_bytes() != actor.as_bytes() {
                        return Err(ControlError::ScopeMismatch.into());
                    }
                    if existing.expected_revision() != command.expected_revision() {
                        return Err(ControlError::IdempotencyConflict.into());
                    }
                    let ControlIntent::ReserveFork { branch, .. } = existing.intent() else {
                        return Err(ControlError::IdempotencyConflict.into());
                    };
                    if !same_public_request(command, request, branch) {
                        return Err(ControlError::IdempotencyConflict.into());
                    }
                    let child = child_operation(actor, branch)?;
                    let receipt = store
                        .resolve_fork(&existing, &child, branch)?
                        .ok_or(ControlStoreError::Corrupt("accepted fork receipt missing"))?;
                    return receipt_projection(command, &receipt);
                }
                let source = store.load(source_id)?.ok_or(ControlError::NotFound)?;
                if source.owner_bytes() != actor.as_bytes()
                    || source.workspace_bytes() != command.query().workspace().as_bytes()
                {
                    return Err(ControlError::ScopeMismatch.into());
                }
                if source.revision() != command.expected_revision() {
                    return Err(ControlError::StaleRevision.into());
                }
                validate_checkpoint_reference(&source, request)?;
                self.validate_fork_coverage(&source, request)?;
                let historical = store
                    .load_revision(source_id, request.source_revision())?
                    .ok_or(ControlError::NotFound)?;
                validate_fork_governance(&source, &historical, request)?;
                let branch = branch(actor, command, request, &historical)?;
                let source_operation = ControlOperation::new(
                    operation_id,
                    source_id,
                    actor,
                    command.query().workspace(),
                    command.expected_revision(),
                    ControlIntent::ReserveFork {
                        branch: branch.clone(),
                        now_unix_millis: super::goal::now_millis(),
                    },
                );
                let child_operation = child_operation(actor, &branch)?;
                let receipt = store.accept_fork(&source_operation, &child_operation, &branch)?;
                receipt_projection(command, &receipt)
            })
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchReceipt)
    }

    fn validate_fork_workspaces(
        &self,
        command: &WorkbenchCommand,
        request: &WorkbenchForkRequest,
    ) -> Result<(), ControlStoreError> {
        let source = command.query().workspace();
        let child = request.child().workspace();
        match request.mode() {
            WorkbenchForkMode::ReadOnlyCurrentWorkspace if child == source => Ok(()),
            WorkbenchForkMode::IsolatedWritableWorkspace if child != source => {
                let source_root =
                    self.inner.workspaces.get(&source).ok_or(ControlError::ScopeMismatch)?;
                let child_root =
                    self.inner.workspaces.get(&child).ok_or(ControlError::ScopeMismatch)?;
                let source_root = source_root.canonicalize().map_err(ControlStoreError::Io)?;
                let child_root = child_root.canonicalize().map_err(ControlStoreError::Io)?;
                if source_root.starts_with(&child_root) || child_root.starts_with(&source_root) {
                    Err(ControlError::ScopeMismatch.into())
                } else {
                    Ok(())
                }
            }
            _ => Err(ControlError::InvalidInput.into()),
        }
    }
}

pub(super) fn validate_fork_governance(
    current: &peritus_product_runner::control::ConversationRecord,
    historical: &peritus_product_runner::control::ConversationRecord,
    request: &WorkbenchForkRequest,
) -> Result<(), ControlStoreError> {
    match (current.goal(), historical.goal(), request.allocation()) {
        (None, None, None) => Ok(()),
        (Some(current), Some(historical), Some(_)) if current.id() == historical.id() => Ok(()),
        _ => Err(ControlError::InvalidInput.into()),
    }
}

fn validate_checkpoint_reference(
    source: &peritus_product_runner::control::ConversationRecord,
    request: &WorkbenchForkRequest,
) -> Result<(), ControlStoreError> {
    let checkpoint = source
        .checkpoints()
        .iter()
        .find(|value| value.id().as_bytes() == request.checkpoint().as_bytes())
        .ok_or(ControlError::NotFound)?;
    let references = checkpoint.references();
    if references.source_conversation_revision() != request.source_revision()
        || references.context_generation() != request.context_generation()
        || references.brief_revision() != request.brief_revision()
        || references.goal_revision().unwrap_or(0) != request.goal_revision()
    {
        return Err(ControlError::InvalidInput.into());
    }
    Ok(())
}

pub(super) fn branch(
    _actor: ActorId,
    command: &WorkbenchCommand,
    request: &WorkbenchForkRequest,
    source: &peritus_product_runner::control::ConversationRecord,
) -> Result<ConversationBranch, ControlStoreError> {
    let source_id = ConversationId::new(command.query().conversation().into_bytes())?;
    let child_id = ConversationId::new(request.child().conversation().into_bytes())?;
    let operation = OperationId::new(command.operation().into_bytes())?;
    let checkpoint = OperationId::new(request.checkpoint().into_bytes())?;
    let (objective, criteria) = if let Some(goal) = source.goal() {
        let criteria = goal
            .criteria()
            .iter()
            .map(|criterion| {
                GoalCriterion::new(
                    criterion.kind(),
                    criterion.description().to_owned(),
                    criterion.mandatory(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        (Some(goal.objective().to_owned()), criteria)
    } else {
        let objective = source
            .brief()
            .active_source(peritus_product_runner::control::BriefField::Objective, source.inputs())
            .map(|value| value.text().to_owned());
        (objective, Vec::new())
    };
    let allocation = request.allocation().map(domain_allocation).transpose()?;
    if source.goal().is_some() != allocation.is_some() {
        return Err(ControlError::InvalidInput.into());
    }
    let mode = match request.mode() {
        WorkbenchForkMode::ReadOnlyCurrentWorkspace => {
            ConversationBranchMode::ReadOnlyCurrentWorkspace
        }
        WorkbenchForkMode::IsolatedWritableWorkspace => {
            ConversationBranchMode::IsolatedWritableWorkspace
        }
    };
    ConversationBranch::new(
        operation,
        source_id,
        command.query().workspace(),
        request.source_revision(),
        checkpoint,
        request.context_generation(),
        request.brief_revision(),
        request.goal_revision(),
        child_id,
        request.child().workspace(),
        mode,
        request.title().as_str().to_owned(),
        objective,
        criteria,
        allocation,
    )?
    .with_seed(source.historical_seed()?)
    .map_err(Into::into)
}

pub(super) fn child_operation(
    actor: ActorId,
    branch: &ConversationBranch,
) -> Result<ControlOperation, ControlStoreError> {
    Ok(ControlOperation::new(
        branch.operation(),
        branch.child(),
        actor,
        WorkspaceId::new(*branch.child_workspace_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
        0,
        ControlIntent::CreateFork { branch: branch.clone() },
    ))
}

fn domain_allocation(
    value: peritus_app_protocol::WorkbenchForkBudget,
) -> Result<ChildBudgetAllocation, ControlStoreError> {
    ChildBudgetAllocation::new(
        value.active_millis(),
        value.requests(),
        value.tool_calls(),
        value.total_tokens(),
    )
    .map_err(Into::into)
}

fn same_public_request(
    command: &WorkbenchCommand,
    request: &WorkbenchForkRequest,
    branch: &ConversationBranch,
) -> bool {
    let mode = match request.mode() {
        WorkbenchForkMode::ReadOnlyCurrentWorkspace => {
            ConversationBranchMode::ReadOnlyCurrentWorkspace
        }
        WorkbenchForkMode::IsolatedWritableWorkspace => {
            ConversationBranchMode::IsolatedWritableWorkspace
        }
    };
    branch.source().as_bytes() == command.query().conversation().as_bytes()
        && branch.child().as_bytes() == request.child().conversation().as_bytes()
        && branch.source_workspace_bytes() == command.query().workspace().as_bytes()
        && branch.child_workspace_bytes() == request.child().workspace().as_bytes()
        && branch.source_revision() == request.source_revision()
        && branch.checkpoint().as_bytes() == request.checkpoint().as_bytes()
        && branch.context_generation() == request.context_generation()
        && branch.brief_revision() == request.brief_revision()
        && branch.goal_revision() == request.goal_revision()
        && branch.mode() == mode
        && branch.title() == request.title().as_str()
        && branch.allocation().map(|value| {
            (value.active_millis(), value.requests(), value.tool_calls(), value.total_tokens())
        }) == request.allocation().map(|value| {
            (value.active_millis(), value.requests(), value.tool_calls(), value.total_tokens())
        })
}
