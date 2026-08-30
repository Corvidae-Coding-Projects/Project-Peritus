//! Integration tests for the checked-in H3 profile and workload catalog.

use peritus_benchmarks::{
    DatasetLimits, PlanKind, QualificationDataset, QualificationPlan, StableId,
};

const PROFILE: &str =
    include_str!("../../../../../benchmarks/profiles/qualification-candidate-v1.json");
const INTEL_HOST_PROFILE: &str = include_str!(
    "../../../../../benchmarks/profiles/qualification-intel-core-ultra-9-275hx-v1.json"
);
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

#[test]
fn retained_host_profile_preserves_production_policy() {
    let production = dataset(PROFILE);
    let retained_host = dataset(INTEL_HOST_PROFILE);
    let expected_machine = retained_host.profile().reference_machine();

    assert_eq!(expected_machine.cpu_model(), "Intel(R) Core(TM) Ultra 9 275HX");
    assert_eq!(expected_machine.logical_cores(), 24);
    assert_eq!(expected_machine.memory_bytes(), 32 * 1024 * 1024 * 1024);
    assert_eq!(retained_host.profile().envelope(), production.profile().envelope());
    assert_eq!(retained_host.profile().objectives(), production.profile().objectives());
    assert_eq!(
        retained_host.profile().required_workloads(),
        production.profile().required_workloads()
    );
    assert_eq!(
        retained_host.profile().regression_policy(),
        production.profile().regression_policy()
    );
    assert_eq!(retained_host.profile().max_measurements(), production.profile().max_measurements());
}

fn dataset(profile: &str) -> QualificationDataset {
    QualificationDataset::from_json(profile, WORKLOADS, DatasetLimits::production_defaults())
        .expect("stable dataset")
}
