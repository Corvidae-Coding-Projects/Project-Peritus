//! Persistent-goal A3 projection, resume coordination, and safe cancellation signalling.

use super::{
    ProductRunService, domain_operation, error_response, receipt_projection, resolve_user_operation,
};
use crate::product_run::ProductRunServiceError;
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, WorkbenchCommand, WorkbenchGoalBudget,
    WorkbenchGoalCriterion, WorkbenchGoalCriterionDefinition, WorkbenchGoalCriterionKind,
    WorkbenchGoalCriterionState, WorkbenchGoalPauseMode, WorkbenchGoalRole, WorkbenchGoalRoleUsage,
    WorkbenchGoalSnapshot, WorkbenchGoalState, WorkbenchGoalUsage, WorkbenchIntent, WorkbenchQuery,
};
use peritus_product_runner::control::{
    ControlError, ConversationId, GoalBudget, GoalCriterion, GoalCriterionKind, GoalCriterionState,
    GoalPauseMode, GoalRecord, GoalRoleUsage, GoalState,
};
use peritus_types::{ActorId, RunId};
use std::sync::atomic::Ordering;

impl ProductRunService {
    pub(crate) fn workbench_goal(
        &self,
        actor: ActorId,
        query: WorkbenchQuery,
    ) -> AppResponsePayload {
        let result = self.control_workspace(query).and_then(|()| {
            let id = ConversationId::new(query.conversation().into_bytes())?;
            let record =
                self.with_controls(false, |store| store.load(id))?.ok_or(ControlError::NotFound)?;
            if record.owner_bytes() != actor.as_bytes()
                || record.workspace_bytes() != query.workspace().as_bytes()
            {
                return Err(ControlError::ScopeMismatch.into());
            }
            let goal = record.goal().ok_or(ControlError::NotFound)?;
            projection(query, record.revision(), goal)
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchGoal)
    }

    pub(super) async fn resume_workbench_goal(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let WorkbenchIntent::ResumeGoal { goal } = command.intent() else {
            return ProductRunServiceError::InvalidMessage.response();
        };
        let operation = match self
            .control_workspace(command.query())
            .and_then(|()| domain_operation(actor, command))
        {
            Ok(operation) => operation,
            Err(error) => return error_response(error),
        };
        let prepared = match self.with_controls(false, |store| {
            if let Some(receipt) = resolve_user_operation(store, &operation)? {
                let record = store.load(operation.conversation())?.ok_or(ControlError::NotFound)?;
                let run = record.goal().ok_or(ControlError::NotFound)?.run_bytes();
                return Ok((
                    RunId::new(*run).map_err(|_| ControlError::InvalidInput)?,
                    Some(receipt),
                ));
            }
            let current = store.load(operation.conversation())?.ok_or(ControlError::NotFound)?;
            let current_goal = current
                .goal()
                .filter(|current| current.id().as_bytes() == goal.as_bytes())
                .ok_or(ControlError::InvalidInput)?;
            let run =
                RunId::new(*current_goal.run_bytes()).map_err(|_| ControlError::InvalidInput)?;
            Ok((run, None))
        }) {
            Ok(prepared) => prepared,
            Err(error) => return error_response(error),
        };
        let (run, replay_receipt) = prepared;
        if let Some(receipt) = replay_receipt {
            return receipt_projection(command, &receipt)
                .map_or_else(error_response, AppResponsePayload::WorkbenchReceipt);
        }
        {
            // Never take the run-record lock from inside the serialized control owner. The prior
            // worker must be at a retryable terminal projection before durable resume admission.
            let retryable = self
                .inner
                .records
                .read()
                .ok()
                .and_then(|records| {
                    records.get(&run).map(|record| record.snapshot.phase().retryable())
                })
                .unwrap_or(false);
            if !retryable {
                return error_response(ControlError::InvalidInput.into());
            }
            if let Err(error) = self.with_controls(false, |store| {
                if resolve_user_operation(store, &operation)?.is_none() {
                    store.accept(&operation)?;
                }
                Ok(())
            }) {
                return error_response(error);
            }
        }
        match self.retry(run).await {
            Ok(_) => self
                .with_controls(false, |store| {
                    resolve_user_operation(store, &operation)?
                        .ok_or_else(|| ControlError::NotFound.into())
                })
                .and_then(|receipt| receipt_projection(command, &receipt))
                .map_or_else(error_response, AppResponsePayload::WorkbenchReceipt),
            Err(error) => error.response(),
        }
    }

    pub(super) fn signal_goal_cancellation(&self, command: &WorkbenchCommand) {
        let goal = match command.intent() {
            WorkbenchIntent::PauseGoal { goal, .. } | WorkbenchIntent::ClearGoal { goal } => *goal,
            _ => return,
        };
        let run = self
            .with_controls(false, |store| {
                let id = ConversationId::new(command.query().conversation().into_bytes())?;
                let record = store.load(id)?.ok_or(ControlError::NotFound)?;
                let goal = record
                    .goal()
                    .filter(|current| current.id().as_bytes() == goal.as_bytes())
                    .ok_or(ControlError::NotFound)?;
                RunId::new(*goal.run_bytes()).map_err(|_| ControlError::InvalidInput.into())
            })
            .ok();
        let Some(run) = run else { return };
        let Ok(mut records) = self.inner.records.write() else { return };
        let Some(record) = records.get_mut(&run) else { return };
        record.cancelled.store(true, Ordering::Release);
        let _ = record.provider_cancellation.cancel();
    }
}

pub(super) fn domain_budget(value: WorkbenchGoalBudget) -> Result<GoalBudget, ControlError> {
    GoalBudget::new(
        value.max_active_millis(),
        value.max_requests(),
        value.max_tool_calls(),
        value.max_total_tokens(),
    )
}

pub(super) fn domain_criterion(
    value: &WorkbenchGoalCriterionDefinition,
) -> Result<GoalCriterion, ControlError> {
    GoalCriterion::new(
        match value.kind() {
            WorkbenchGoalCriterionKind::RunnerAcceptance => GoalCriterionKind::RunnerAcceptance,
            WorkbenchGoalCriterionKind::GraphicalPlaytest => GoalCriterionKind::GraphicalPlaytest,
            WorkbenchGoalCriterionKind::HumanValidation => GoalCriterionKind::HumanValidation,
        },
        value.description().as_str().to_owned(),
        value.mandatory(),
    )
}

pub(super) const fn domain_pause(value: WorkbenchGoalPauseMode) -> GoalPauseMode {
    match value {
        WorkbenchGoalPauseMode::Now => GoalPauseMode::Now,
        WorkbenchGoalPauseMode::AfterOperation => GoalPauseMode::AfterOperation,
        WorkbenchGoalPauseMode::BeforeEdit => GoalPauseMode::BeforeEdit,
    }
}

pub(super) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn projection(
    query: WorkbenchQuery,
    aggregate_revision: u64,
    goal: &GoalRecord,
) -> Result<WorkbenchGoalSnapshot, crate::product_control::ControlStoreError> {
    let criteria = goal
        .criteria()
        .iter()
        .map(|criterion| {
            let definition = WorkbenchGoalCriterionDefinition::new(
                match criterion.kind() {
                    GoalCriterionKind::RunnerAcceptance => {
                        WorkbenchGoalCriterionKind::RunnerAcceptance
                    }
                    GoalCriterionKind::GraphicalPlaytest => {
                        WorkbenchGoalCriterionKind::GraphicalPlaytest
                    }
                    GoalCriterionKind::HumanValidation => {
                        WorkbenchGoalCriterionKind::HumanValidation
                    }
                },
                peritus_app_protocol::WorkbenchInputText::new(criterion.description().to_owned())
                    .map_err(|_| ControlError::InvalidInput)?,
                criterion.mandatory(),
            );
            Ok(WorkbenchGoalCriterion::new(
                definition,
                match criterion.state() {
                    GoalCriterionState::Pending => WorkbenchGoalCriterionState::Pending,
                    GoalCriterionState::Satisfied => WorkbenchGoalCriterionState::Satisfied,
                    GoalCriterionState::Unavailable => WorkbenchGoalCriterionState::Unavailable,
                    GoalCriterionState::Stale => WorkbenchGoalCriterionState::Stale,
                },
                criterion.evidence_revision(),
            ))
        })
        .collect::<Result<Vec<_>, ControlError>>()?;
    let usage = goal.usage();
    let domain_roles = usage.roles();
    let roles = [
        role_projection(WorkbenchGoalRole::Writer, domain_roles[0]),
        role_projection(WorkbenchGoalRole::Reviewer, domain_roles[1]),
        role_projection(WorkbenchGoalRole::Fixer, domain_roles[2]),
    ];
    let public_usage = WorkbenchGoalUsage::new(
        roles,
        usage.active_millis(),
        now_millis().saturating_sub(goal.created_unix_millis()),
        usage.retries(),
        usage.provider_failovers(),
        usage.compactions(),
        usage.workspace_bytes(),
        usage.workspace_growth_bytes(),
        usage.peak_rss_bytes(),
    );
    WorkbenchGoalSnapshot::new(
        query,
        aggregate_revision,
        ControlOperationId::new(*goal.id().as_bytes()).map_err(|_| ControlError::InvalidInput)?,
        RunId::new(*goal.run_bytes()).map_err(|_| ControlError::InvalidInput)?,
        peritus_app_protocol::WorkbenchInputText::new(goal.objective().to_owned())
            .map_err(|_| ControlError::InvalidInput)?,
        match goal.state() {
            GoalState::Active => WorkbenchGoalState::Active,
            GoalState::WaitingForUser => WorkbenchGoalState::WaitingForUser,
            GoalState::Pausing => WorkbenchGoalState::Pausing,
            GoalState::Paused => WorkbenchGoalState::Paused,
            GoalState::Blocked => WorkbenchGoalState::Blocked,
            GoalState::BudgetReached => WorkbenchGoalState::BudgetReached,
            GoalState::Achieved => WorkbenchGoalState::Achieved,
            GoalState::Cancelled => WorkbenchGoalState::Cancelled,
        },
        goal.reason().to_owned(),
        goal.user_revision(),
        goal.attempt(),
        goal.restart_eligible(),
        goal.pause_mode().map(|mode| match mode {
            GoalPauseMode::Now => WorkbenchGoalPauseMode::Now,
            GoalPauseMode::AfterOperation => WorkbenchGoalPauseMode::AfterOperation,
            GoalPauseMode::BeforeEdit => WorkbenchGoalPauseMode::BeforeEdit,
        }),
        criteria,
        WorkbenchGoalBudget::new(
            goal.budget().max_active_millis(),
            goal.budget().max_requests(),
            goal.budget().max_tool_calls(),
            goal.budget().max_total_tokens(),
        )
        .map_err(|_| ControlError::InvalidInput)?,
        public_usage,
    )
    .map_err(|_| ControlError::InvalidInput.into())
}

fn role_projection(role: WorkbenchGoalRole, usage: GoalRoleUsage) -> WorkbenchGoalRoleUsage {
    WorkbenchGoalRoleUsage::new(
        role,
        usage.requests(),
        usage.completed_requests(),
        usage.tool_calls(),
        usage.tokens_known().then_some(usage.total_tokens()),
        usage.cost_known().then_some(usage.provider_cost_microunits()),
    )
}
