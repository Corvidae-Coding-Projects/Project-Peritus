//! Host-only durable goal admission, accounting, progress, and settlement operations.

mod replay;

use super::{ControlStore, ControlStoreError as Error};
use peritus_model_protocol::UsageCounters;
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, ConversationRecord, GoalAdmission, GoalRole,
    GoalSettlement, GoalState, GoalUsageReport, OperationId,
};
use peritus_types::{ActorId, WorkspaceId};

use replay::{apply_versioned_tool_goal, equivalent_host_intent, goal_tool_key};

impl ControlStore {
    /// Returns the current attempt for a governed goal, or `None` for an ordinary execution.
    pub fn goal_attempt(&self, start: &ControlOperation) -> Result<Option<u32>, Error> {
        Ok(self.goal_record(start)?.map(|(_, goal)| goal.attempt()))
    }

    /// Returns remaining active time for a governed goal, preserving the runner ceiling otherwise.
    pub fn goal_remaining_active_millis(
        &self,
        start: &ControlOperation,
    ) -> Result<Option<u64>, Error> {
        Ok(self.goal_record(start)?.map(|(_, goal)| goal.remaining_active_millis()))
    }

    /// Reserves one exact provider request before the adapter may observe it.
    pub fn reserve_goal_request(
        &mut self,
        start: &ControlOperation,
        role: GoalRole,
        request_id: &str,
    ) -> Result<GoalAdmission, Error> {
        let Some((record, goal)) = self.goal_record(start)? else {
            return Ok(GoalAdmission::Accepted);
        };
        let attempt = goal.attempt();
        let before = goal.usage().requests();
        let goal_id = goal.id();
        let intent = ControlIntent::ReserveGoalRequest {
            goal: goal_id,
            role,
            attempt,
            now_unix_millis: now_millis(),
        };
        let next = self.apply_host_goal(
            start,
            &record,
            goal_key(b"request-reserve", attempt, request_id.as_bytes()),
            intent,
        )?;
        let next_goal = exact_goal(&next, goal_id)?;
        if next_goal.usage().requests() > before {
            Ok(GoalAdmission::Accepted)
        } else {
            Ok(state_admission(next_goal.state()))
        }
    }

    /// Reconciles the exact reserved provider request with optional normalized usage.
    pub fn complete_goal_request(
        &mut self,
        start: &ControlOperation,
        role: GoalRole,
        request_id: &str,
        usage: UsageCounters,
    ) -> Result<GoalAdmission, Error> {
        let Some((record, goal)) = self.goal_record(start)? else {
            return Ok(GoalAdmission::Accepted);
        };
        let attempt = goal.attempt();
        let goal_id = goal.id();
        let intent = ControlIntent::CompleteGoalRequest {
            goal: goal_id,
            role,
            attempt,
            report: GoalUsageReport {
                input_tokens: usage.input_tokens(),
                cached_input_tokens: usage.cached_input_tokens(),
                output_tokens: usage.output_tokens(),
                total_tokens: usage.total_tokens(),
                provider_cost_microunits: usage.provider_cost_microunits(),
            },
            now_unix_millis: now_millis(),
        };
        let next = self.apply_host_goal(
            start,
            &record,
            goal_key(b"request-complete", attempt, request_id.as_bytes()),
            intent,
        )?;
        Ok(state_admission(exact_goal(&next, goal_id)?.state()))
    }

    /// Reserves one exact tool operation before dispatching any effect. The stable loop invocation
    /// scopes its local sequence across independently constructed developer loops.
    pub fn reserve_goal_tool(
        &mut self,
        start: &ControlOperation,
        role: GoalRole,
        invocation: &str,
        sequence: u32,
        mutation_capable: bool,
    ) -> Result<GoalAdmission, Error> {
        let Some((record, goal)) = self.goal_record(start)? else {
            return Ok(GoalAdmission::Accepted);
        };
        let attempt = goal.attempt();
        let before = goal.usage().tool_calls();
        let goal_id = goal.id();
        let intent = ControlIntent::ReserveGoalTool {
            goal: goal_id,
            role,
            attempt,
            mutation_capable,
            now_unix_millis: now_millis(),
        };
        let next = apply_versioned_tool_goal(
            self,
            start,
            &record,
            goal_tool_key(b"tool-reserve-v2", attempt, role, invocation, sequence),
            goal_key(b"tool-reserve", attempt, &sequence.to_be_bytes()),
            intent,
        )?;
        let next_goal = exact_goal(&next, goal_id)?;
        if next_goal.usage().tool_calls() > before {
            Ok(GoalAdmission::Accepted)
        } else {
            Ok(state_admission(next_goal.state()))
        }
    }

    /// Marks one reserved tool operation settled before the next operation may start using the
    /// exact role, loop invocation, and local sequence from its admission.
    pub fn complete_goal_tool(
        &mut self,
        start: &ControlOperation,
        role: GoalRole,
        invocation: &str,
        sequence: u32,
    ) -> Result<GoalAdmission, Error> {
        let Some((record, goal)) = self.goal_record(start)? else {
            return Ok(GoalAdmission::Accepted);
        };
        let attempt = goal.attempt();
        let goal_id = goal.id();
        let intent = ControlIntent::CompleteGoalTool {
            goal: goal_id,
            attempt,
            now_unix_millis: now_millis(),
        };
        let next = apply_versioned_tool_goal(
            self,
            start,
            &record,
            goal_tool_key(b"tool-complete-v2", attempt, role, invocation, sequence),
            goal_key(b"tool-complete", attempt, &sequence.to_be_bytes()),
            intent,
        )?;
        Ok(state_admission(exact_goal(&next, goal_id)?.state()))
    }

    /// Publishes one attempt's monotonic runner/resource high-water observations.
    #[allow(clippy::too_many_arguments, reason = "one exact progress observation stays explicit")]
    pub fn observe_goal_progress(
        &mut self,
        start: &ControlOperation,
        elapsed_millis: u64,
        retries: u32,
        provider_failovers: u32,
        compactions: u32,
        workspace_bytes: u64,
        workspace_growth_bytes: u64,
        peak_rss_bytes: u64,
    ) -> Result<(), Error> {
        let Some((record, goal)) = self.goal_record(start)? else {
            return Ok(());
        };
        let attempt = goal.attempt();
        let goal_id = goal.id();
        let mut semantic = Vec::with_capacity(44);
        semantic.extend_from_slice(&elapsed_millis.to_be_bytes());
        semantic.extend_from_slice(&retries.to_be_bytes());
        semantic.extend_from_slice(&provider_failovers.to_be_bytes());
        semantic.extend_from_slice(&compactions.to_be_bytes());
        semantic.extend_from_slice(&workspace_bytes.to_be_bytes());
        semantic.extend_from_slice(&workspace_growth_bytes.to_be_bytes());
        semantic.extend_from_slice(&peak_rss_bytes.to_be_bytes());
        let intent = ControlIntent::ObserveGoalProgress {
            goal: goal_id,
            attempt,
            elapsed_millis,
            retries,
            provider_failovers,
            compactions,
            workspace_bytes,
            workspace_growth_bytes,
            peak_rss_bytes,
            now_unix_millis: now_millis(),
        };
        self.apply_host_goal(start, &record, goal_key(b"progress", attempt, &semantic), intent)?;
        Ok(())
    }

    /// Publishes strict runner settlement evidence for the current goal attempt.
    pub fn settle_goal(
        &mut self,
        start: &ControlOperation,
        settlement: GoalSettlement,
        evidence_input_generation: Option<u64>,
        unresolved_effects: bool,
    ) -> Result<(), Error> {
        let Some((record, goal)) = self.goal_record(start)? else {
            return Ok(());
        };
        if matches!(goal.state(), GoalState::Paused | GoalState::BudgetReached) {
            return Ok(());
        }
        let attempt = goal.attempt();
        let goal_id = goal.id();
        let mut semantic = vec![settlement as u8, u8::from(unresolved_effects)];
        semantic.extend_from_slice(&evidence_input_generation.unwrap_or(0).to_be_bytes());
        let intent = ControlIntent::SettleGoal {
            goal: goal_id,
            attempt,
            settlement,
            evidence_input_generation,
            unresolved_effects,
            now_unix_millis: now_millis(),
        };
        self.apply_host_goal(start, &record, goal_key(b"settlement", attempt, &semantic), intent)?;
        Ok(())
    }

    /// Publishes exact daemon-qualified graphical evidence for the current goal revision.
    #[allow(
        clippy::too_many_arguments,
        reason = "current goal and immutable preview evidence fences stay explicit"
    )]
    #[allow(
        clippy::too_many_arguments,
        reason = "criterion and evidence identity bindings stay explicit"
    )]
    pub fn observe_graphical_goal_evidence(
        &mut self,
        start: &ControlOperation,
        criterion_index: u32,
        evidence_user_revision: u64,
        evidence_input_generation: u64,
        launch: OperationId,
        capture: OperationId,
    ) -> Result<(), Error> {
        let Some((record, goal)) = self.goal_record(start)? else {
            return Ok(());
        };
        if goal.user_revision() != evidence_user_revision
            || goal.required_input_generation() != evidence_input_generation
        {
            return Err(ControlError::StaleRevision.into());
        }
        if !goal.criteria().iter().any(|criterion| {
            criterion.kind()
                == peritus_product_runner::control::GoalCriterionKind::GraphicalPlaytest
        }) {
            return Ok(());
        }
        let attempt = goal.attempt();
        let goal_id = goal.id();
        let mut semantic = Vec::with_capacity(56);
        semantic.extend_from_slice(&evidence_user_revision.to_be_bytes());
        semantic.extend_from_slice(&evidence_input_generation.to_be_bytes());
        semantic.extend_from_slice(launch.as_bytes());
        semantic.extend_from_slice(capture.as_bytes());
        semantic.extend_from_slice(&criterion_index.to_be_bytes());
        let intent = ControlIntent::ObserveGraphicalGoalEvidence {
            criterion_index,
            goal: goal_id,
            attempt,
            evidence_user_revision,
            evidence_input_generation,
            launch,
            capture,
            now_unix_millis: now_millis(),
        };
        self.apply_host_goal(
            start,
            &record,
            goal_key(b"graphical-evidence", attempt, &semantic),
            intent,
        )?;
        Ok(())
    }

    fn goal_record(
        &self,
        start: &ControlOperation,
    ) -> Result<Option<(ConversationRecord, peritus_product_runner::control::GoalRecord)>, Error>
    {
        let record = self.execution_record(start)?;
        let Some(goal) = record.goal().cloned() else {
            return Ok(None);
        };
        if goal.id() != start.id() || goal.run_bytes() != execution_run(start)? {
            return Err(ControlError::ScopeMismatch.into());
        }
        Ok(Some((record, goal)))
    }

    fn apply_host_goal(
        &mut self,
        start: &ControlOperation,
        current: &ConversationRecord,
        semantic_key: Vec<u8>,
        intent: ControlIntent,
    ) -> Result<ConversationRecord, Error> {
        let id = host_operation_id(start.id(), &semantic_key)?;
        if let Some(existing) = self.host_goal_operation(id)? {
            if existing.conversation() != start.conversation()
                || existing.actor_bytes() != start.actor_bytes()
                || existing.workspace_bytes() != start.workspace_bytes()
                || !equivalent_host_intent(existing.intent(), &intent)
                || self.resolve(&existing)?.is_none()
            {
                return Err(ControlError::IdempotencyConflict.into());
            }
            return self.load(start.conversation())?.ok_or_else(|| ControlError::NotFound.into());
        }
        let operation = ControlOperation::new(
            id,
            start.conversation(),
            ActorId::new(*start.actor_bytes()).map_err(|_| ControlError::InvalidInput)?,
            WorkspaceId::new(*start.workspace_bytes()).map_err(|_| ControlError::InvalidInput)?,
            current.revision(),
            intent,
        );
        self.accept_host_goal_operation(&operation)?;
        self.load(start.conversation())?.ok_or_else(|| ControlError::NotFound.into())
    }
}

fn execution_run(start: &ControlOperation) -> Result<&[u8; 16], Error> {
    match start.intent() {
        ControlIntent::StartExecution { run, .. } | ControlIntent::StartGoal { run, .. } => Ok(run),
        _ => Err(ControlError::InvalidInput.into()),
    }
}

fn exact_goal(
    record: &ConversationRecord,
    goal: OperationId,
) -> Result<&peritus_product_runner::control::GoalRecord, Error> {
    record
        .goal()
        .filter(|current| current.id() == goal)
        .ok_or_else(|| ControlError::NotFound.into())
}

const fn state_admission(state: GoalState) -> GoalAdmission {
    match state {
        GoalState::Active | GoalState::Pausing => GoalAdmission::Accepted,
        GoalState::Paused | GoalState::Cancelled => GoalAdmission::Paused,
        GoalState::BudgetReached => GoalAdmission::BudgetReached,
        GoalState::WaitingForUser | GoalState::Blocked | GoalState::Achieved => {
            GoalAdmission::Inactive
        }
    }
}

fn goal_key(kind: &[u8], attempt: u32, semantic: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(kind.len() + semantic.len() + 4);
    key.extend_from_slice(kind);
    key.extend_from_slice(&attempt.to_be_bytes());
    key.extend_from_slice(semantic);
    key
}

fn host_operation_id(goal: OperationId, semantic_key: &[u8]) -> Result<OperationId, ControlError> {
    let mut identity = b"peritus-workbench/goal-host-operation/v1".to_vec();
    identity.extend_from_slice(goal.as_bytes());
    identity.extend_from_slice(semantic_key);
    let digest = peritus_codec::sha256(&identity);
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    bytes[0] |= 1;
    OperationId::new(bytes)
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}
