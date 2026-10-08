//! Semantic identity and replay comparison for host-authored goal observations.

use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, ConversationRecord, GoalRole,
};

use super::{ControlStore, Error, goal_key, host_operation_id};

struct GoalToolIdentity {
    role: GoalRole,
    current_reserve_key: Vec<u8>,
    current_complete_key: Vec<u8>,
    current_reserve: peritus_product_runner::control::OperationId,
    current_complete: peritus_product_runner::control::OperationId,
    legacy_reserve: peritus_product_runner::control::OperationId,
    legacy_complete: peritus_product_runner::control::OperationId,
}

pub(super) fn apply_versioned_tool_goal(
    store: &mut ControlStore,
    start: &ControlOperation,
    current: &ConversationRecord,
    role: GoalRole,
    invocation: &str,
    sequence: u32,
    intent: ControlIntent,
) -> Result<ConversationRecord, Error> {
    let identity = GoalToolIdentity::new(start, role, invocation, sequence, &intent)?;
    let current_id = identity.current_id(&intent)?;
    let current_operation = store.host_goal_operation(current_id)?;
    let migration = store.resolve_host_goal_migration(
        identity.legacy_reserve,
        identity.legacy_complete,
        identity.current_reserve,
        identity.current_complete,
    )?;
    match (current_operation, migration) {
        (Some(_), Some(_)) => {
            return Err(Error::Corrupt(
                "goal tool identity has both an operation and a migration",
            ));
        }
        (Some(_), None) => {
            return store.apply_host_goal(
                start,
                current,
                identity.current_key(&intent)?,
                intent,
            );
        }
        (None, Some((legacy_reserve, legacy_complete))) => {
            validate_legacy_pair(
                start,
                identity.role,
                &intent,
                &legacy_reserve,
                &legacy_complete,
            )?;
            return store
                .load(start.conversation())?
                .ok_or_else(|| ControlError::NotFound.into());
        }
        (None, None) => {}
    }

    let legacy_reserve = store.host_goal_operation(identity.legacy_reserve)?;
    let legacy_complete = store.host_goal_operation(identity.legacy_complete)?;
    match (legacy_reserve, legacy_complete) {
        (None, None) => store.apply_host_goal(
            start,
            current,
            identity.current_key(&intent)?,
            intent,
        ),
        (Some(_), None) => Err(ControlError::NotFound.into()),
        (None, Some(_)) => Err(Error::Corrupt(
            "legacy goal tool settlement has no reservation",
        )),
        (Some(legacy_reserve), Some(legacy_complete)) => {
            validate_legacy_pair(
                start,
                identity.role,
                &intent,
                &legacy_reserve,
                &legacy_complete,
            )?;
            store.migrate_host_goal_operations(
                start,
                current,
                identity.current_reserve,
                identity.current_complete,
                &legacy_reserve,
                &legacy_complete,
            )?;
            store
                .load(start.conversation())?
                .ok_or_else(|| ControlError::NotFound.into())
        }
    }
}

impl GoalToolIdentity {
    fn new(
        start: &ControlOperation,
        role: GoalRole,
        invocation: &str,
        sequence: u32,
        intent: &ControlIntent,
    ) -> Result<Self, Error> {
        let attempt = match intent {
            ControlIntent::ReserveGoalTool { attempt, .. }
            | ControlIntent::CompleteGoalTool { attempt, .. } => *attempt,
            _ => return Err(ControlError::InvalidInput.into()),
        };
        let current_reserve_key =
            goal_tool_key(b"tool-reserve-v2", attempt, role, invocation, sequence);
        let current_complete_key =
            goal_tool_key(b"tool-complete-v2", attempt, role, invocation, sequence);
        Ok(Self {
            role,
            current_reserve: host_operation_id(start.id(), &current_reserve_key)?,
            current_complete: host_operation_id(start.id(), &current_complete_key)?,
            current_reserve_key,
            current_complete_key,
            legacy_reserve: host_operation_id(
                start.id(),
                &goal_key(b"tool-reserve", attempt, &sequence.to_be_bytes()),
            )?,
            legacy_complete: host_operation_id(
                start.id(),
                &goal_key(b"tool-complete", attempt, &sequence.to_be_bytes()),
            )?,
        })
    }

    fn current_id(
        &self,
        intent: &ControlIntent,
    ) -> Result<peritus_product_runner::control::OperationId, Error> {
        match intent {
            ControlIntent::ReserveGoalTool { .. } => Ok(self.current_reserve),
            ControlIntent::CompleteGoalTool { .. } => Ok(self.current_complete),
            _ => Err(ControlError::InvalidInput.into()),
        }
    }

    fn current_key(&self, intent: &ControlIntent) -> Result<Vec<u8>, Error> {
        match intent {
            ControlIntent::ReserveGoalTool { .. } => Ok(self.current_reserve_key.clone()),
            ControlIntent::CompleteGoalTool { .. } => Ok(self.current_complete_key.clone()),
            _ => return Err(ControlError::InvalidInput.into()),
        }
    }
}

fn validate_legacy_pair(
    start: &ControlOperation,
    role: GoalRole,
    requested: &ControlIntent,
    reserve: &ControlOperation,
    complete: &ControlOperation,
) -> Result<(), Error> {
    for operation in [reserve, complete] {
        if operation.conversation() != start.conversation()
            || operation.actor_bytes() != start.actor_bytes()
            || operation.workspace_bytes() != start.workspace_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
    }
    let (goal, attempt, mutation_capable) = match reserve.intent() {
        ControlIntent::ReserveGoalTool {
            goal,
            role: legacy_role,
            attempt,
            mutation_capable,
            ..
        } if *legacy_role == role => (*goal, *attempt, *mutation_capable),
        _ => return Err(ControlError::IdempotencyConflict.into()),
    };
    match complete.intent() {
        ControlIntent::CompleteGoalTool {
            goal: complete_goal,
            attempt: complete_attempt,
            ..
        } if (*complete_goal, *complete_attempt) == (goal, attempt) => {}
        _ => return Err(ControlError::IdempotencyConflict.into()),
    }
    match requested {
        ControlIntent::ReserveGoalTool {
            goal: requested_goal,
            role: requested_role,
            attempt: requested_attempt,
            mutation_capable: requested_mutation,
            ..
        } if (*requested_goal, *requested_role, *requested_attempt, *requested_mutation)
            == (goal, role, attempt, mutation_capable) =>
        {
            Ok(())
        }
        ControlIntent::CompleteGoalTool {
            goal: requested_goal,
            attempt: requested_attempt,
            ..
        } if (*requested_goal, *requested_attempt) == (goal, attempt) => Ok(()),
        _ => Err(ControlError::IdempotencyConflict.into()),
    }
}

pub(super) fn goal_tool_key(
    kind: &[u8],
    attempt: u32,
    role: GoalRole,
    invocation: &str,
    sequence: u32,
) -> Vec<u8> {
    let mut semantic = Vec::with_capacity(invocation.len() + 5);
    semantic.push(match role {
        GoalRole::Writer => 0,
        GoalRole::Reviewer => 1,
        GoalRole::Fixer => 2,
    });
    semantic.extend_from_slice(&sequence.to_be_bytes());
    semantic.extend_from_slice(invocation.as_bytes());
    goal_key(kind, attempt, &semantic)
}

pub(super) fn equivalent_host_intent(left: &ControlIntent, right: &ControlIntent) -> bool {
    match (left, right) {
        (
            ControlIntent::ReserveGoalRequest {
                goal: left_goal,
                role: left_role,
                attempt: left_attempt,
                ..
            },
            ControlIntent::ReserveGoalRequest {
                goal: right_goal,
                role: right_role,
                attempt: right_attempt,
                ..
            },
        ) => (left_goal, left_role, left_attempt) == (right_goal, right_role, right_attempt),
        (
            ControlIntent::CompleteGoalRequest {
                goal: left_goal,
                role: left_role,
                attempt: left_attempt,
                report: left_report,
                ..
            },
            ControlIntent::CompleteGoalRequest {
                goal: right_goal,
                role: right_role,
                attempt: right_attempt,
                report: right_report,
                ..
            },
        ) => {
            (left_goal, left_role, left_attempt, left_report)
                == (right_goal, right_role, right_attempt, right_report)
        }
        (
            ControlIntent::ReserveGoalTool {
                goal: left_goal,
                role: left_role,
                attempt: left_attempt,
                mutation_capable: left_mutation,
                ..
            },
            ControlIntent::ReserveGoalTool {
                goal: right_goal,
                role: right_role,
                attempt: right_attempt,
                mutation_capable: right_mutation,
                ..
            },
        ) => {
            (left_goal, left_role, left_attempt, left_mutation)
                == (right_goal, right_role, right_attempt, right_mutation)
        }
        (
            ControlIntent::CompleteGoalTool { goal: left_goal, attempt: left_attempt, .. },
            ControlIntent::CompleteGoalTool { goal: right_goal, attempt: right_attempt, .. },
        ) => (left_goal, left_attempt) == (right_goal, right_attempt),
        (
            ControlIntent::ObserveGoalProgress {
                goal: left_goal,
                attempt: left_attempt,
                elapsed_millis: left_elapsed,
                retries: left_retries,
                provider_failovers: left_failovers,
                compactions: left_compactions,
                workspace_bytes: left_workspace,
                workspace_growth_bytes: left_growth,
                peak_rss_bytes: left_rss,
                ..
            },
            ControlIntent::ObserveGoalProgress {
                goal: right_goal,
                attempt: right_attempt,
                elapsed_millis: right_elapsed,
                retries: right_retries,
                provider_failovers: right_failovers,
                compactions: right_compactions,
                workspace_bytes: right_workspace,
                workspace_growth_bytes: right_growth,
                peak_rss_bytes: right_rss,
                ..
            },
        ) => {
            (
                left_goal,
                left_attempt,
                left_elapsed,
                left_retries,
                left_failovers,
                left_compactions,
                left_workspace,
                left_growth,
                left_rss,
            ) == (
                right_goal,
                right_attempt,
                right_elapsed,
                right_retries,
                right_failovers,
                right_compactions,
                right_workspace,
                right_growth,
                right_rss,
            )
        }
        (
            ControlIntent::SettleGoal {
                goal: left_goal,
                attempt: left_attempt,
                settlement: left_settlement,
                evidence_input_generation: left_generation,
                unresolved_effects: left_unresolved,
                ..
            },
            ControlIntent::SettleGoal {
                goal: right_goal,
                attempt: right_attempt,
                settlement: right_settlement,
                evidence_input_generation: right_generation,
                unresolved_effects: right_unresolved,
                ..
            },
        ) => {
            (left_goal, left_attempt, left_settlement, left_generation, left_unresolved)
                == (right_goal, right_attempt, right_settlement, right_generation, right_unresolved)
        }
        (
            ControlIntent::ObserveGraphicalGoalEvidence {
                criterion_index: left_criterion,
                goal: left_goal,
                attempt: left_attempt,
                evidence_user_revision: left_user_revision,
                evidence_input_generation: left_input_generation,
                launch: left_launch,
                capture: left_capture,
                ..
            },
            ControlIntent::ObserveGraphicalGoalEvidence {
                criterion_index: right_criterion,
                goal: right_goal,
                attempt: right_attempt,
                evidence_user_revision: right_user_revision,
                evidence_input_generation: right_input_generation,
                launch: right_launch,
                capture: right_capture,
                ..
            },
        ) => {
            (
                left_criterion,
                left_goal,
                left_attempt,
                left_user_revision,
                left_input_generation,
                left_launch,
                left_capture,
            ) == (
                right_criterion,
                right_goal,
                right_attempt,
                right_user_revision,
                right_input_generation,
                right_launch,
                right_capture,
            )
        }
        _ => false,
    }
}
