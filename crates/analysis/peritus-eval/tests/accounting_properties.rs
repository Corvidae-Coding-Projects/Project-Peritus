//! Complete rollout accounting, idempotency, and conflict coverage.

mod support;

use peritus_eval::{EvaluationPlan, NeverCancelled, RolloutLedger, RolloutRecord, execute_rollout};

use support::{FixturePort, PortMode, artifact, campaign_id, frozen_profile};

#[test]
fn ledger_conserves_every_expected_rollout_and_retains_attempts() {
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let mut ledger = RolloutLedger::from_plan(&plan, 3);
    for (index, spec) in plan.specs().iter().enumerate() {
        let mode = if index % 3 == 0 { PortMode::TaskFail } else { PortMode::Pass };
        let mut port = FixturePort::new(mode);
        let executed = execute_rollout(&mut port, &NeverCancelled, spec, &profile, 1)
            .expect("executed rollout");
        let record = RolloutRecord::from_execution(spec, executed, Some(artifact(90)), None)
            .expect("terminal record");
        ledger.record_attempt(spec.id(), record.attempt()).expect("attempt");
        ledger.settle(record).expect("settle");
    }
    let counts = ledger.counts();
    assert!(counts.complete());
    assert_eq!(counts.expected, 8);
    assert_eq!(counts.passed + counts.task_failed, 8);
    assert_eq!(ledger.records().count(), 8);
}

#[test]
fn exact_duplicates_are_idempotent_and_conflicting_terminals_quarantine() {
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let spec = &plan.specs()[0];
    let mut ledger = RolloutLedger::from_plan(&plan, 3);
    let mut pass_port = FixturePort::new(PortMode::Pass);
    let pass = execute_rollout(&mut pass_port, &NeverCancelled, spec, &profile, 1)
        .expect("pass execution");
    let pass = RolloutRecord::from_execution(spec, pass, None, None).expect("pass record");
    ledger.record_attempt(spec.id(), pass.attempt()).expect("attempt");
    ledger.record_attempt(spec.id(), pass.attempt()).expect("exact duplicate attempt");
    ledger.settle(pass).expect("terminal");
    ledger.settle(pass).expect("exact duplicate terminal");

    let mut fail_port = FixturePort::new(PortMode::TaskFail);
    let failed = execute_rollout(&mut fail_port, &NeverCancelled, spec, &profile, 1)
        .expect("failure execution");
    let failed = RolloutRecord::from_execution(spec, failed, None, None).expect("failure record");
    let error = ledger.settle(failed).expect_err("conflicting terminal must reject");
    assert_eq!(error.recovery(), peritus_eval::EvaluationRecovery::Quarantine);
}
