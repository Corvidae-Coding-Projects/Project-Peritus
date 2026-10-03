//! Focused persistent-goal state-machine tests.

use super::*;

fn id(byte: u8) -> OperationId {
    OperationId::new([byte; 16]).expect("nonzero operation")
}

fn goal() -> GoalRecord {
    GoalRecord::start(
        id(1),
        [2; 16],
        ControlText::new("Ship the exact change".to_owned()).expect("objective"),
        vec![
            GoalCriterion::new(
                GoalCriterionKind::RunnerAcceptance,
                "Strict runner acceptance".to_owned(),
                true,
            )
            .expect("criterion"),
        ],
        7,
        10,
    )
    .expect("goal")
}

#[test]
fn accounting_above_former_goal_limits_never_stops_admission() {
    let mut goal = goal();
    for request in 0..4_097 {
        assert_eq!(goal.reserve_request(GoalRole::Writer, 1, 11), Ok(GoalAdmission::Accepted));
        let report = if request == 0 {
            GoalUsageReport { total_tokens: Some(100_000_001), ..GoalUsageReport::default() }
        } else {
            GoalUsageReport::default()
        };
        assert_eq!(
            goal.complete_request(GoalRole::Writer, 1, report, 12),
            Ok(GoalAdmission::Accepted)
        );
    }
    for _ in 0..20_001 {
        assert_eq!(goal.reserve_tool(GoalRole::Writer, 1, true, 13), Ok(GoalAdmission::Accepted));
        assert_eq!(goal.complete_tool(1, 14), Ok(GoalAdmission::Accepted));
    }
    goal.observe_progress(1, 86_400_000, 0, 0, 0, 0, 0, 0, 15).unwrap();
    assert_eq!(goal.state(), GoalState::Active);
    assert_eq!(goal.usage().requests(), 4_097);
    assert_eq!(goal.usage().tool_calls(), 20_001);
    assert_eq!(goal.usage().total_tokens(), 100_000_001);
    assert_eq!(goal.usage().active_millis(), 86_400_000);
}

#[test]
fn missing_usage_is_unknown_not_zero_and_cost_has_independent_provenance() {
    let mut goal = goal();
    goal.reserve_request(GoalRole::Writer, 1, 11).unwrap();
    goal.complete_request(
        GoalRole::Writer,
        1,
        GoalUsageReport {
            input_tokens: None,
            cached_input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            provider_cost_microunits: None,
        },
        12,
    )
    .unwrap();
    assert_eq!(goal.usage().total_tokens(), 0);
    assert!(!goal.usage().tokens_known());
    assert!(!goal.usage().cost_known());
}

#[test]
fn pause_boundaries_do_not_admit_mutation_or_complete_from_model_text() {
    let mut goal = goal();
    goal.pause(GoalPauseMode::BeforeEdit, 11).unwrap();
    assert_eq!(goal.reserve_tool(GoalRole::Writer, 1, false, 12), Ok(GoalAdmission::Accepted));
    assert_eq!(goal.reserve_tool(GoalRole::Writer, 1, true, 13), Ok(GoalAdmission::Paused));
    assert_eq!(goal.state(), GoalState::Paused);
    assert!(
        goal.criteria()
            .iter()
            .all(|criterion| { criterion.state() != GoalCriterionState::Satisfied })
    );
}

#[test]
fn pausing_an_idle_goal_produces_a_valid_resumable_record() {
    for mode in [GoalPauseMode::Now, GoalPauseMode::AfterOperation, GoalPauseMode::BeforeEdit] {
        let mut value = goal();
        value.settle(1, GoalSettlement::WaitingForUser, Some(7), false, 11).unwrap();
        assert_eq!(value.state(), GoalState::WaitingForUser);
        value.pause(mode, 12).unwrap();
        value.validate().expect("idle pause must persist");
        assert_eq!(value.state(), GoalState::Paused);
        assert_eq!(value.pause_mode(), None);
        value.resume(13).unwrap();
        value.validate().expect("resumed record");
        assert_eq!(value.state(), GoalState::Active);
    }
}

#[test]
fn only_fresh_strict_settlement_achieves_goal() {
    let mut stale = goal();
    stale.settle(1, GoalSettlement::Accepted, Some(6), false, 11).unwrap();
    assert_eq!(stale.state(), GoalState::Blocked);
    let mut current = goal();
    current.settle(1, GoalSettlement::Accepted, Some(7), false, 11).unwrap();
    assert_eq!(current.state(), GoalState::Achieved);
}

#[test]
fn graphical_evidence_is_independent_fresh_and_revision_fenced() {
    let mut goal = GoalRecord::start(
        id(1),
        [2; 16],
        ControlText::new("Ship a playable view".to_owned()).unwrap(),
        vec![
            GoalCriterion::new(
                GoalCriterionKind::RunnerAcceptance,
                "Strict runner acceptance".to_owned(),
                true,
            )
            .unwrap(),
            GoalCriterion::new(
                GoalCriterionKind::GraphicalPlaytest,
                "Native playtest".to_owned(),
                true,
            )
            .unwrap(),
        ],
        7,
        10,
    )
    .unwrap();
    goal.criteria.push(
        GoalCriterion::new(
            GoalCriterionKind::GraphicalPlaytest,
            "Independent second behavior".to_owned(),
            true,
        )
        .unwrap(),
    );
    goal.settle(1, GoalSettlement::Accepted, Some(7), false, 11).unwrap();
    assert_eq!(goal.state(), GoalState::WaitingForUser);
    assert_eq!(goal.criteria()[0].state(), GoalCriterionState::Satisfied);
    assert_eq!(goal.criteria()[1].state(), GoalCriterionState::Unavailable);
    assert_eq!(goal.observe_graphical_evidence(1, 1, 1, 6, 12), Err(ControlError::StaleRevision));
    assert_eq!(goal.criteria()[1].state(), GoalCriterionState::Unavailable);
    goal.pause(GoalPauseMode::AfterOperation, 13).unwrap();
    goal.resume(14).unwrap();
    assert_eq!(goal.observe_graphical_evidence(1, 2, 1, 7, 15), Err(ControlError::StaleRevision));
    goal.observe_graphical_evidence(1, 2, 3, 7, 16).unwrap();
    assert_eq!(goal.criteria()[0].state(), GoalCriterionState::Satisfied);
    assert_eq!(goal.criteria()[1].state(), GoalCriterionState::Satisfied);
    assert_eq!(goal.criteria()[1].evidence_revision(), Some(7));
    assert_eq!(goal.criteria()[2].state(), GoalCriterionState::Unavailable);
    assert_eq!(goal.state(), GoalState::Active);
    goal.observe_graphical_evidence(2, 2, 3, 7, 17).unwrap();
    assert_eq!(goal.state(), GoalState::Achieved);
    goal.requirements_changed(8, false, 18).unwrap();
    assert!(goal.criteria().iter().all(|criterion| criterion.state() == GoalCriterionState::Stale));
    assert_eq!(goal.state(), GoalState::Blocked);
}
