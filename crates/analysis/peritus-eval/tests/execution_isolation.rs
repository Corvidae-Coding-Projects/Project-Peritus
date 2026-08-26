//! Candidate/evaluator isolation and failure-class coverage.

mod support;

use peritus_eval::{
    EvaluationPlan, InfrastructureFailureClass, NeverCancelled, RolloutOutcome, execute_rollout,
};

use support::{FixturePort, PortMode, campaign_id, frozen_profile};

#[test]
fn evaluator_runs_only_after_finalized_candidate_output() {
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let mut port = FixturePort::new(PortMode::Pass);
    let executed = execute_rollout(&mut port, &NeverCancelled, &plan.specs()[0], &profile, 1)
        .expect("execute rollout");
    assert_eq!(port.candidate_calls, 1);
    assert_eq!(port.evaluator_calls, 1);
    assert!(matches!(executed.attempt().terminal(), RolloutOutcome::TaskPassed { .. }));
    assert!(executed.candidate().is_some());
    assert!(executed.evaluator().is_some());
}

#[test]
fn candidate_failure_is_infrastructure_and_skips_evaluator() {
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let mut port = FixturePort::new(PortMode::CandidateInfrastructure);
    let executed = execute_rollout(&mut port, &NeverCancelled, &plan.specs()[0], &profile, 1)
        .expect("truthful infrastructure terminal");
    assert_eq!(port.candidate_calls, 1);
    assert_eq!(port.evaluator_calls, 0);
    assert!(matches!(
        executed.attempt().terminal(),
        RolloutOutcome::InfrastructureFailed { class: InfrastructureFailureClass::Provider, .. }
    ));
}

#[test]
fn evaluator_outage_never_masquerades_as_task_failure() {
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let mut port = FixturePort::new(PortMode::EvaluatorInfrastructure);
    let executed = execute_rollout(&mut port, &NeverCancelled, &plan.specs()[0], &profile, 1)
        .expect("truthful evaluator outage");
    assert!(matches!(
        executed.attempt().terminal(),
        RolloutOutcome::InfrastructureFailed { class: InfrastructureFailureClass::Evaluator, .. }
    ));
}
