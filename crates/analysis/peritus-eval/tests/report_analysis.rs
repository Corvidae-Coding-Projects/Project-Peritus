//! End-to-end complete-ledger analysis and inert report validation.

mod support;

use peritus_eval::{
    EvaluationArm, EvaluationPlan, EvaluationReport, MetricAvailability, NeverCancelled,
    RolloutLedger, RolloutRecord, analyze_evaluation, execute_rollout,
};

use support::{FixturePort, PortMode, campaign_id, frozen_profile};

#[test]
fn complete_ledger_produces_deterministic_non_authoritative_report() {
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let mut ledger = RolloutLedger::from_plan(&plan, 3);
    for spec in plan.specs() {
        let mode = if spec.arm() == EvaluationArm::Candidate && spec.ordinal() == 1 {
            PortMode::TaskFail
        } else {
            PortMode::Pass
        };
        let mut port = FixturePort::new(mode);
        let executed =
            execute_rollout(&mut port, &NeverCancelled, spec, &profile, 1).expect("execute");
        let record = RolloutRecord::from_execution(spec, executed, None, None).expect("record");
        ledger.record_attempt(spec.id(), record.attempt()).expect("attempt");
        ledger.settle(record).expect("settle");
    }
    let analysis = analyze_evaluation(&plan, &profile, &ledger).expect("analysis");
    assert!(matches!(analysis.paired(), MetricAvailability::Available(_)));
    assert_eq!(analysis.reliability().counts().expected, 8);
    assert_eq!(analysis.reliability().counts().infrastructure_failed, 0);
    let report = EvaluationReport::new(
        campaign_id(),
        profile.dataset().digest(),
        profile.digest(),
        plan.id(),
        plan.digest(),
        analysis.clone(),
        None,
    )
    .expect("report")
    .validate()
    .expect("validated report");
    let repeated = EvaluationReport::new(
        campaign_id(),
        profile.dataset().digest(),
        profile.digest(),
        plan.id(),
        plan.digest(),
        analysis,
        None,
    )
    .expect("report")
    .validate()
    .expect("validated report");
    assert_eq!(report.id(), repeated.id());
    assert_eq!(report.bytes(), repeated.bytes());
    assert_eq!(report.digest(), repeated.digest());
}

#[test]
fn incomplete_ledger_cannot_begin_analysis() {
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let ledger = RolloutLedger::from_plan(&plan, 3);
    let error = analyze_evaluation(&plan, &profile, &ledger).expect_err("incomplete must reject");
    assert_eq!(error.kind(), peritus_eval::EvaluationErrorKind::Incomplete);
}
