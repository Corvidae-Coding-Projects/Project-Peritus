//! Focused bounded-goal state-machine tests.

use super::*;

fn id(byte: u8) -> OperationId {
    OperationId::new([byte; 16]).expect("nonzero operation")
}

fn goal(budget: GoalBudget) -> GoalRecord {
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
        budget,
        7,
        10,
    )
    .expect("goal")
}

#[test]
fn request_budget_is_reserved_before_admission_and_survives_resume() {
    let budget = GoalBudget::new(None, Some(1), None, None).expect("budget");
    let mut goal = goal(budget);
    assert_eq!(goal.reserve_request(GoalRole::Writer, 1, 11), Ok(GoalAdmission::Accepted));
    assert_eq!(
        goal.complete_request(GoalRole::Writer, 1, GoalUsageReport::default(), 12),
        Ok(GoalAdmission::Accepted)
    );
    assert_eq!(goal.reserve_request(GoalRole::Reviewer, 1, 13), Ok(GoalAdmission::BudgetReached));
    assert_eq!(goal.state(), GoalState::BudgetReached);
    assert!(goal.resume(14).is_err(), "resume cannot reset a reached limit");
    goal.update_budget(GoalBudget::new(None, Some(2), None, None).unwrap(), 15).unwrap();
    goal.resume(16).unwrap();
    assert_eq!(goal.usage().requests(), 1);
    assert_eq!(goal.attempt(), 2);
}

#[test]
fn missing_usage_is_unknown_not_zero_and_cost_has_independent_provenance() {
    let mut goal = goal(GoalBudget::default());
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
    let mut goal = goal(GoalBudget::default());
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
fn only_fresh_strict_settlement_achieves_goal() {
    let mut stale = goal(GoalBudget::default());
    stale.settle(1, GoalSettlement::Accepted, Some(6), false, 11).unwrap();
    assert_eq!(stale.state(), GoalState::Blocked);
    let mut current = goal(GoalBudget::default());
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
        GoalBudget::default(),
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
    goal.update_budget(GoalBudget::default(), 13).unwrap();
    assert_eq!(goal.observe_graphical_evidence(1, 1, 1, 7, 14), Err(ControlError::StaleRevision));
    goal.observe_graphical_evidence(1, 1, 2, 7, 15).unwrap();
    assert_eq!(goal.criteria()[0].state(), GoalCriterionState::Satisfied);
    assert_eq!(goal.criteria()[1].state(), GoalCriterionState::Satisfied);
    assert_eq!(goal.criteria()[1].evidence_revision(), Some(7));
    assert_eq!(goal.criteria()[2].state(), GoalCriterionState::Unavailable);
    assert_eq!(goal.state(), GoalState::WaitingForUser);
    goal.observe_graphical_evidence(2, 1, 2, 7, 15).unwrap();
    assert_eq!(goal.state(), GoalState::Achieved);
    goal.requirements_changed(8, false, 16).unwrap();
    assert!(goal.criteria().iter().all(|criterion| criterion.state() == GoalCriterionState::Stale));
    assert_eq!(goal.state(), GoalState::Blocked);
}
