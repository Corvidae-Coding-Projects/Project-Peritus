//! Persistent-goal A3 projection, resume coordination, and safe cancellation signalling.

use super::{
    ProductRunService, domain_operation, error_response, receipt_projection, resolve_user_operation,
};
use crate::product_run::ProductRunServiceError;
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, WorkbenchCommand, WorkbenchGoalCriterion,
    WorkbenchGoalCriterionDefinition, WorkbenchGoalCriterionKind, WorkbenchGoalCriterionState,
    WorkbenchGoalPauseMode, WorkbenchGoalRole, WorkbenchGoalRoleUsage, WorkbenchGoalSnapshot,
    WorkbenchGoalState, WorkbenchGoalUsage, WorkbenchIntent, WorkbenchQuery,
};
use peritus_product_runner::control::{
    ControlError, ControlOperation, ConversationId, GoalCriterion, GoalCriterionKind,
    GoalCriterionState, GoalPauseMode, GoalRecord, GoalRoleUsage, GoalState,
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
        if replay_receipt.is_none() {
            // Never take the run-record lock from inside the serialized control owner. The prior
            // worker must have settled before durable resume admission. Waiting for user input
            // is a safe boundary, but only this explicit goal command may resume unchanged input.
            let retryable = match self
                .inner
                .records
                .read()
                .map_err(|_| ProductRunServiceError::Unavailable)
                .and_then(|records| {
                    let record = records.get(&run).ok_or(ProductRunServiceError::NotFound)?;
                    super::super::operation::may_start_execution(&self.inner.directory, record)
                }) {
                Ok(retryable) => retryable,
                Err(error) => return error.response(),
            };
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
        let accepted = match self.with_controls(false, |store| {
            store
                .operation(operation.conversation(), operation.id())?
                .ok_or_else(|| ControlError::NotFound.into())
        }) {
            Ok(accepted) => accepted,
            Err(error) => return error_response(error),
        };
        match self.retry_admitted(run, Some(&accepted)).await {
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

    /// Checks exact durable resume authority while the caller owns the run-record lock.
    pub(in crate::product_run) fn goal_resume_pending(
        &self,
        record: &crate::product_run::RunRecord,
        operation: &ControlOperation,
    ) -> Result<bool, ProductRunServiceError> {
        if record.goal_resume == Some(operation.id())
            && record.snapshot.phase() != peritus_app_protocol::ProductRunPhase::RecoveryRequired
        {
            return Ok(false);
        }
        let start = &record.interaction.workbench;
        let peritus_product_runner::control::ControlIntent::ResumeGoal { goal, .. } =
            operation.intent()
        else {
            return Err(ProductRunServiceError::Control(ControlError::InvalidInput));
        };
        if *goal != start.id()
            || operation.conversation() != start.conversation()
            || operation.actor_bytes() != start.actor_bytes()
            || operation.workspace_bytes() != start.workspace_bytes()
        {
            return Err(ProductRunServiceError::Control(ControlError::ScopeMismatch));
        }
        self.with_controls(false, |store| {
            let receipt = store.resolve(operation)?.ok_or(ControlError::NotFound)?;
            let resumed = store
                .load_revision(operation.conversation(), receipt.accepted_revision())?
                .ok_or(ControlError::NotFound)?;
            let admitted_goal = resumed.goal().ok_or(ControlError::NotFound)?;
            let current = store.load(operation.conversation())?.ok_or(ControlError::NotFound)?;
            Ok(current.goal().is_some_and(|current_goal| {
                current_goal.id() == *goal
                    && current_goal.run_bytes() == record.request.run_id().as_bytes()
                    && current_goal.attempt() == admitted_goal.attempt()
                    && current_goal.state() == GoalState::Active
            }))
        })
        .map_err(Into::into)
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
