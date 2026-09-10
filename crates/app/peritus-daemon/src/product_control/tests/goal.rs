use super::*;
use peritus_product_runner::control::{
    BriefField, GoalAdmission, GoalBudget, GoalCriterion, GoalCriterionKind, GoalRole,
};

fn active_goal(journal: &mut ControlStore) -> ControlOperation {
    journal.accept(&create()).expect("create");
    let objective = ControlText::new("Build the confirmed product".to_owned()).expect("objective");
    journal
        .accept(&operation(
            2,
            1,
            ControlIntent::SetBrief { field: BriefField::Objective, text: objective.clone() },
        ))
        .expect("set brief");
    let start = operation(
        3,
        2,
        ControlIntent::StartGoal {
            run: [5; 16],
            settings_digest: [6; 32],
            objective,
            criteria: vec![
                GoalCriterion::new(
                    GoalCriterionKind::RunnerAcceptance,
                    "Strict runner acceptance".to_owned(),
                    true,
                )
                .expect("criterion"),
            ],
            budget: GoalBudget::new(None, None, Some(8), None).expect("budget"),
            now_unix_millis: 1,
        },
    );
    journal.accept(&start).expect("start goal");
    start
}

#[test]
fn tool_sequence_identity_is_scoped_to_role_and_loop_invocation_with_exact_replay() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    let start = active_goal(&mut journal);
    let tools = [
        (GoalRole::Writer, "shared-invocation-1", false),
        (GoalRole::Writer, "implementation-invocation-1", true),
        (GoalRole::Reviewer, "shared-invocation-1", false),
    ];

    for (role, invocation, mutation_capable) in tools {
        assert_eq!(
            journal
                .reserve_goal_tool(&start, role, invocation, 1, mutation_capable)
                .expect("reserve tool"),
            GoalAdmission::Accepted
        );
        assert_eq!(
            journal.complete_goal_tool(&start, role, invocation, 1).expect("complete tool"),
            GoalAdmission::Accepted
        );
    }
    let exact_usage = journal
        .load(start.conversation())
        .expect("load")
        .expect("record")
        .goal()
        .expect("goal")
        .usage();
    assert_eq!(exact_usage.tool_calls(), 3);
    assert_eq!(exact_usage.roles()[0].tool_calls(), 2);
    assert_eq!(exact_usage.roles()[1].tool_calls(), 1);

    drop(journal);
    let mut journal = store(root.path());
    assert!(matches!(
        journal.reserve_goal_tool(&start, GoalRole::Writer, "shared-invocation-1", 1, true),
        Err(Error::Control(ControlError::IdempotencyConflict))
    ));
    for (role, invocation, mutation_capable) in tools {
        assert_eq!(
            journal
                .reserve_goal_tool(&start, role, invocation, 1, mutation_capable)
                .expect("replay reserve"),
            GoalAdmission::Accepted
        );
        assert_eq!(
            journal.complete_goal_tool(&start, role, invocation, 1).expect("replay complete"),
            GoalAdmission::Accepted
        );
    }
    assert_eq!(
        journal
            .load(start.conversation())
            .expect("replay load")
            .expect("record")
            .goal()
            .expect("goal")
            .usage(),
        exact_usage
    );
}

fn legacy_id(goal: OperationId, kind: &[u8], attempt: u32, sequence: u32) -> OperationId {
    let mut identity = b"peritus-workbench/goal-host-operation/v1".to_vec();
    identity.extend_from_slice(goal.as_bytes());
    identity.extend_from_slice(kind);
    identity.extend_from_slice(&attempt.to_be_bytes());
    identity.extend_from_slice(&sequence.to_be_bytes());
    let digest = sha256(&identity);
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    bytes[0] |= 1;
    OperationId::new(bytes).expect("legacy operation")
}

fn accept_legacy(
    journal: &mut ControlStore,
    start: &ControlOperation,
    kind: &[u8],
    intent: ControlIntent,
) {
    let current = journal.load(start.conversation()).expect("load").expect("record");
    let operation = ControlOperation::new(
        legacy_id(start.id(), kind, 1, 1),
        start.conversation(),
        ActorId::new(*start.actor_bytes()).expect("actor"),
        WorkspaceId::new(*start.workspace_bytes()).expect("workspace"),
        current.revision(),
        intent,
    );
    journal.accept_host_goal_operation(&operation).expect("accept legacy operation");
}

#[test]
fn legacy_tool_identity_fails_closed_without_a_v2_replay_debit() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    let start = active_goal(&mut journal);
    accept_legacy(
        &mut journal,
        &start,
        b"tool-reserve",
        ControlIntent::ReserveGoalTool {
            goal: start.id(),
            role: GoalRole::Writer,
            attempt: 1,
            mutation_capable: false,
            now_unix_millis: 2,
        },
    );
    accept_legacy(
        &mut journal,
        &start,
        b"tool-complete",
        ControlIntent::CompleteGoalTool { goal: start.id(), attempt: 1, now_unix_millis: 3 },
    );
    let usage = journal
        .load(start.conversation())
        .expect("load")
        .expect("record")
        .goal()
        .expect("goal")
        .usage();
    assert_eq!(usage.tool_calls(), 1);

    assert!(matches!(
        journal.reserve_goal_tool(&start, GoalRole::Writer, "designer-invocation-1", 1, false),
        Err(Error::Control(ControlError::UnsupportedSchema))
    ));
    assert!(matches!(
        journal.complete_goal_tool(&start, GoalRole::Writer, "designer-invocation-1", 1),
        Err(Error::Control(ControlError::UnsupportedSchema))
    ));
    assert_eq!(
        journal
            .load(start.conversation())
            .expect("unchanged load")
            .expect("record")
            .goal()
            .expect("goal")
            .usage(),
        usage
    );
}
