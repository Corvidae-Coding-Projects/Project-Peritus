//! Focused tests for campaign selection and fail-fast production admission.

use peritus_benchmarks::{
    DatasetLimits, QualificationDataset, RunnerDescriptor, ScenarioKind, Sha256Digest, StableId,
    Workload, WorkloadParameters,
};

use crate::campaign::classify_workloads;
use crate::{
    CampaignCoordinator, CampaignError, CampaignMode, CampaignRequest, MachineObservation,
    SubjectConfiguration,
};

#[test]
fn one_hour_is_the_long_horizon_boundary() {
    let load = workload("load", 3_599);
    let soak = workload("soak", 3_600);
    let (loads, soaks) = classify_workloads(&[load.clone(), soak.clone()]);
    assert_eq!(loads, vec![load]);
    assert_eq!(soaks, vec![soak]);
}

#[test]
fn machine_mismatch_is_rejected_before_daemon_launch() {
    let temporary = tempfile::tempdir().expect("scratch");
    let subject = SubjectConfiguration::new(
        &std::env::current_exe().expect("non-daemon test executable"),
        temporary.path(),
    )
    .expect("subject configuration");
    let dataset = QualificationDataset::from_json(
        include_str!("../../../../../benchmarks/profiles/qualification-candidate-v1.json"),
        include_str!("../../../../../benchmarks/workloads/production-v1.json"),
        DatasetLimits::production_defaults(),
    )
    .expect("checked-in dataset");
    let runner = RunnerDescriptor::new(
        id("runner"),
        "test",
        Sha256Digest::of_bytes(b"campaign-test-runner"),
    )
    .expect("runner");
    let machine = MachineObservation::new(
        id("different-os"),
        id("different-arch"),
        "different cpu",
        1,
        1,
        id("different-storage"),
    )
    .expect("observation");
    let request = CampaignRequest::new(
        dataset,
        subject,
        "revision",
        id("run"),
        runner,
        machine,
        CampaignMode::Load,
    );

    assert!(matches!(
        CampaignCoordinator::run(request),
        Err(CampaignError::ReferenceMachineMismatch { .. })
    ));
}

fn workload(id_value: &str, duration_seconds: u64) -> Workload {
    let parameters = WorkloadParameters::load(duration_seconds, 1, 1).expect("parameters");
    Workload::new(id(id_value), id_value, ScenarioKind::EventAppend, parameters).expect("workload")
}

fn id(value: &str) -> StableId {
    StableId::new(value).expect("stable id")
}
