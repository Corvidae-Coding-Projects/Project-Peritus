//! Semantic identity and replay comparison for host-authored goal observations.

use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, ConversationRecord, GoalRole,
};

use super::{ControlStore, Error, goal_key, host_operation_id};

pub(super) fn apply_versioned_tool_goal(
    store: &mut ControlStore,
    start: &ControlOperation,
    current: &ConversationRecord,
    semantic_key: Vec<u8>,
    legacy_key: Vec<u8>,
    intent: ControlIntent,
) -> Result<ConversationRecord, Error> {
    let current_id = host_operation_id(start.id(), &semantic_key)?;
    if store.host_goal_operation(current_id)?.is_none() {
        let legacy_id = host_operation_id(start.id(), &legacy_key)?;
        if store.host_goal_operation(legacy_id)?.is_some() {
            return Err(ControlError::UnsupportedSchema.into());
        }
    }
    store.apply_host_goal(start, current, semantic_key, intent)
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
                evidence: left_evidence,
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
                evidence: right_evidence,
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
                left_evidence,
            ) == (
                right_criterion,
                right_goal,
                right_attempt,
                right_user_revision,
                right_input_generation,
                right_launch,
                right_capture,
                right_evidence,
            )
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::equivalent_host_intent;
    use peritus_product_runner::control::ControlIntent;
    use serde_json::json;

    fn graphical_intent(note_digest: u8) -> ControlIntent {
        serde_json::from_value(json!({
            "observe_graphical_goal_evidence": {
                "criterion_index": 0,
                "goal": vec![1; 16],
                "attempt": 1,
                "evidence_user_revision": 2,
                "evidence_input_generation": 3,
                "launch": vec![4; 16],
                "capture": vec![5; 16],
                "evidence": {
                    "check": vec![6; 16],
                    "observed": {
                        "stream": "stdout",
                        "start_byte": 2,
                        "end_byte": 4,
                        "observed_stream_bytes": 8,
                        "matched_bytes_digest": vec![7; 32],
                        "source": {"live_spool": {
                            "process_id": vec![8; 16],
                            "observed_prefix_digest": vec![9; 32]
                        }}
                    },
                    "note": {
                        "stream": "stderr",
                        "start_byte": 5,
                        "end_byte": 6,
                        "observed_stream_bytes": 8,
                        "matched_bytes_digest": vec![note_digest; 32],
                        "source": {"live_spool": {
                            "process_id": vec![8; 16],
                            "observed_prefix_digest": vec![10; 32]
                        }}
                    }
                },
                "now_unix_millis": 10
            }
        }))
        .expect("valid graphical evidence intent")
    }

    #[test]
    fn graphical_output_evidence_participates_in_host_replay_identity() {
        let first = graphical_intent(11);
        let same = graphical_intent(11);
        let changed_match = graphical_intent(12);

        assert!(equivalent_host_intent(&first, &same));
        assert!(!equivalent_host_intent(&first, &changed_match));
    }
}
