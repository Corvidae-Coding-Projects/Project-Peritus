//! Focused deterministic-plan unit tests.

use crate::{
    CapacityLimits, ConcurrencyLimits, PlanKind, PlannedOperation, QualificationPlan, QueueLimits,
    ResourceEnvelope, ScenarioKind, StableId, Workload, WorkloadParameters,
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

#[test]
fn partial_queue_cycle_ends_with_an_exact_drain() {
    let parameters = WorkloadParameters::load(10, 1, 3)
        .expect("parameters")
        .with_queue_capacity(3)
        .expect("queue capacity");
    let workload = Workload::new(
        StableId::new("queue").expect("id"),
        "partial queue cycle",
        ScenarioKind::QueueSaturation,
        parameters,
    )
    .expect("workload");
    let envelope = ResourceEnvelope::new(
        ConcurrencyLimits::new(3, 3, 3).expect("concurrency"),
        CapacityLimits::new(10, 10, 10).expect("capacity"),
        QueueLimits::new(3, 3, 3, 3).expect("queues"),
    );
    let plan = QualificationPlan::new(
        StableId::new("plan").expect("id"),
        PlanKind::Load,
        StableId::new("profile").expect("id"),
        envelope,
        workload,
    )
    .expect("plan");
    assert_eq!(
        plan.step(9).expect("last step").operation(),
        &PlannedOperation::DrainQueue { queue: crate::QueueKind::Command, count: 2 }
    );
}
