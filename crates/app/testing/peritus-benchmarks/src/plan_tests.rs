//! Focused deterministic-plan unit tests.

use crate::{
    CapacityLimits, ConcurrencyLimits, PlanKind, QualificationPlan, QueueLimits, ResourceEnvelope,
    ScenarioKind, StableId, Workload, WorkloadParameters,
};

#[test]
fn plan_is_reproducible_without_materializing_soak() {
    let parameters = WorkloadParameters::load(3_600, 10, 2).expect("parameters");
    let workload = Workload::new(
        StableId::new("soak").expect("id"),
        "long horizon",
        ScenarioKind::EventAppend,
        parameters,
    )
    .expect("workload");
    let envelope = ResourceEnvelope::new(
        ConcurrencyLimits::new(2, 2, 2).expect("concurrency"),
        CapacityLimits::new(1_000, 1_000, 1_000).expect("capacity"),
        QueueLimits::new(2, 2, 2, 2).expect("queues"),
    );
    let plan = QualificationPlan::new(
        StableId::new("plan").expect("id"),
        PlanKind::Soak,
        StableId::new("profile").expect("id"),
        envelope,
        workload,
    )
    .expect("plan");
    assert_eq!(plan.step(19), plan.step(19));
    assert_eq!(plan.step_count(), 36_000);
}
