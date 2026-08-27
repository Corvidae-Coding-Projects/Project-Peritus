//! Integration tests for the checked-in H3 profile and workload catalog.

use peritus_benchmarks::{
    DatasetLimits, PlanKind, QualificationDataset, QualificationPlan, StableId,
};

const PROFILE: &str =
    include_str!("../../../../../benchmarks/profiles/qualification-candidate-v1.json");
const WORKLOADS: &str = include_str!("../../../../../benchmarks/workloads/production-v1.json");

#[test]
fn checked_in_dataset_is_cross_referenced_and_bounded() {
    let dataset =
        QualificationDataset::from_json(PROFILE, WORKLOADS, DatasetLimits::production_defaults())
            .expect("stable dataset");
    assert_eq!(dataset.workloads().len(), 15);
    assert_eq!(dataset.profile().required_workloads().len(), 15);
}

#[test]
fn checked_in_soak_plan_is_lazy_and_deterministic() {
    let dataset =
        QualificationDataset::from_json(PROFILE, WORKLOADS, DatasetLimits::production_defaults())
            .expect("stable dataset");
    let workload = dataset
        .workload(&StableId::new("soak.recovery.8h.v1").expect("id"))
        .expect("workload")
        .clone();
    let plan = QualificationPlan::new(
        StableId::new("soak-recovery-plan").expect("id"),
        PlanKind::Soak,
        dataset.profile().id().clone(),
        dataset.profile().envelope(),
        workload,
    )
    .expect("plan");
    assert_eq!(plan.step(10_000), plan.step(10_000));
    assert!(plan.step_count() >= 28_800);
}
