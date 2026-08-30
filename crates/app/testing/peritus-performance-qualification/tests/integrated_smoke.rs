//! Operator-invoked smoke against a real `peritusd` executable.

#![cfg(unix)]

use std::path::PathBuf;

use peritus_benchmarks::{
    AccountingSummary, CapacityLimits, ConcurrencyLimits, MeasurementIngestor, PlanKind,
    QualificationPlan, QualificationRunner, QueueLimits, ResourceAccountant, ResourceEnvelope,
    RunContext, RunnerDescriptor, RunnerReceipt, ScenarioKind, Sha256Digest, StableId, Workload,
    WorkloadParameters,
};
use peritus_performance_qualification::{CancellationFlag, IntegratedSubject, PacedRunner};

#[test]
#[ignore = "set PERITUS_H3_DAEMON to a built peritusd and invoke explicitly"]
fn real_daemon_accepts_a_complete_scheduler_lifecycle() {
    let (receipt, accounting, measurement_count) =
        run_smoke(ScenarioKind::ConcurrentRuns, 1, 3, "smoke.scheduler-lifecycle");
    assert!(receipt.completed());
    assert_eq!(receipt.executed_steps(), 3);
    assert!(accounting.is_balanced());
    assert!(measurement_count >= 4);
}

#[test]
#[ignore = "set PERITUS_H3_DAEMON to a built peritusd and invoke explicitly"]
fn real_daemon_recovers_and_accepts_a_successor_event() {
    let (receipt, accounting, measurement_count) =
        run_smoke(ScenarioKind::Recovery, 2, 2, "smoke.recovery");
    assert!(receipt.completed());
    assert_eq!(receipt.executed_steps(), 4);
    assert!(accounting.is_balanced());
    assert!(measurement_count >= 5);
}

fn run_smoke(
    scenario: ScenarioKind,
    duration_seconds: u64,
    operations_per_second: u32,
    workload_id: &str,
) -> (RunnerReceipt, AccountingSummary, usize) {
    let mut authorized = IntegratedSubject::launch(&daemon_executable(), "operator-smoke")
        .expect("launch integrated subject");
    let envelope = ResourceEnvelope::new(
        ConcurrencyLimits::new(1, 1, 1).expect("concurrency"),
        CapacityLimits::new(1_048_576, 1_048_576, 1_024).expect("capacity"),
        QueueLimits::new(4, 4, 4, 4).expect("queues"),
    );
    let workload = Workload::new(
        id(workload_id),
        "real public-A3 integrated smoke",
        scenario,
        WorkloadParameters::load(duration_seconds, operations_per_second, 1).expect("parameters"),
    )
    .expect("workload");
    let plan = QualificationPlan::new(
        id("smoke-plan"),
        PlanKind::Load,
        id("smoke-profile"),
        envelope,
        workload,
    )
    .expect("plan");
    let context = RunContext::for_workload(
        id("smoke-run"),
        id("smoke-profile"),
        id("smoke-plan"),
        id(workload_id),
    );
    let mut measurements =
        MeasurementIngestor::new(id("smoke-run"), id("smoke-profile"), [id(workload_id)], 32)
            .expect("measurements");
    let mut accounting = ResourceAccountant::new(envelope);
    let mut runner = PacedRunner::new(
        RunnerDescriptor::new(
            id("peritus-h3-smoke"),
            env!("CARGO_PKG_VERSION"),
            Sha256Digest::of_bytes(b"operator-smoke"),
        )
        .expect("runner descriptor"),
        CancellationFlag::new(),
    );
    let (subject, authorization) = authorized.parts();
    let receipt = runner
        .run(subject, authorization, &context, &plan, &mut measurements, &mut accounting)
        .expect("run integrated smoke");
    let summary = accounting.summary();
    let measurement_count = measurements.finish().records().len();
    (receipt, summary, measurement_count)
}

fn daemon_executable() -> PathBuf {
    PathBuf::from(
        std::env::var_os("PERITUS_H3_DAEMON")
            .expect("PERITUS_H3_DAEMON must name the peritusd executable"),
    )
}

fn id(value: &str) -> StableId {
    StableId::new(value).expect("stable id")
}
