//! Deterministic rollout planning and D3 work binding coverage.

mod support;

use peritus_eval::{EvaluationArm, EvaluationPlan};
use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerLimits,
};
use peritus_types::ActorId;

use support::{bytes, campaign_id, frozen_profile, revision};

#[test]
fn complete_plan_is_deterministic_and_pair_seeds_match() {
    let profile = frozen_profile();
    let left = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let right = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    assert_eq!(left, right);
    assert_eq!(left.specs().len(), 8);
    for pair in left.specs().chunks_exact(2) {
        assert_eq!(pair[0].arm(), EvaluationArm::Baseline);
        assert_eq!(pair[1].arm(), EvaluationArm::Candidate);
        assert_eq!(pair[0].task_id(), pair[1].task_id());
        assert_eq!(pair[0].ordinal(), pair[1].ordinal());
        assert_eq!(pair[0].seed(), pair[1].seed());
        assert_ne!(pair[0].id(), pair[1].id());
    }
    assert_eq!(left.dispatch_order(), right.dispatch_order());
}

#[test]
fn planned_rollout_builds_exact_d3_coordination_work() {
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let resources = ResourceVector::new(
        vec![ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(1).expect("quantity"))],
        4,
    )
    .expect("resources");
    let limits = SchedulerLimits::new(10, 10, 2, 4, 4, 2, 3, 8, 2, 1_024, 1_048_576)
        .expect("scheduler limits");
    let work = plan.specs()[0]
        .work_spec(ActorId::new(bytes(90)).expect("owner"), revision(), resources, 3, limits)
        .expect("work spec");
    assert_eq!(work.id(), plan.specs()[0].work_id());
    assert_eq!(work.payload_digest(), plan.specs()[0].request_digest());
    assert_eq!(work.class(), peritus_scheduler::ExecutionClass::Coordination);
    assert_eq!(work.recovery(), peritus_scheduler::RecoveryPolicy::Ambiguous);
}
