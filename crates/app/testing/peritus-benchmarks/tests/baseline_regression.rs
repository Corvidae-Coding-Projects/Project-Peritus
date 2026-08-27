//! Boundary tests for baseline regression classification and readiness.

use peritus_benchmarks::{
    BaselineEntry, BaselineManifest, CapacityLimits, ConcurrencyLimits, MeasurementIngestor,
    MeasurementRecord, Metric, ObjectiveBound, QualificationEvaluator, QualificationProfileBuilder,
    QualificationVerdict, QueueLimits, ReferenceMachine, RegressionClass, RegressionPolicy,
    ResourceAccountant, ResourceEnvelope, RunnerReceipt, RunnerTermination, ScenarioKind,
    Sha256Digest, SloObjective, StableId, Statistic, Workload, WorkloadParameters,
};

#[test]
fn blocking_baseline_regression_prevents_readiness_even_when_slo_is_met() {
    let workload = Workload::new(
        StableId::new("append").expect("id"),
        "append workload",
        ScenarioKind::EventAppend,
        WorkloadParameters::load(1, 10, 1).expect("parameters"),
    )
    .expect("workload");
    let envelope = ResourceEnvelope::new(
        ConcurrencyLimits::new(1, 1, 1).expect("concurrency"),
        CapacityLimits::new(1024, 1024, 1024).expect("capacity"),
        QueueLimits::new(1, 1, 1, 1).expect("queues"),
    );
    let profile_id = StableId::new("profile").expect("id");
    let profile = QualificationProfileBuilder::new(
        profile_id.clone(),
        "regression profile",
        reference_machine(),
        envelope,
        RegressionPolicy::new(500, 1_000, 1, true).expect("policy"),
    )
    .objective(
        SloObjective::new(
            StableId::new("append-p99").expect("id"),
            workload.id().clone(),
            Metric::EventAppendLatency,
            Statistic::P99,
            ObjectiveBound::AtMost,
            200,
            10,
        )
        .expect("objective"),
    )
    .build()
    .expect("profile");
    let run_id = StableId::new("run").expect("id");
    let mut sink =
        MeasurementIngestor::new(run_id.clone(), profile_id.clone(), [workload.id().clone()], 10)
            .expect("sink");
    for sequence in 0..10 {
        sink.record(
            MeasurementRecord::new(
                run_id.clone(),
                profile_id.clone(),
                workload.id().clone(),
                Metric::EventAppendLatency,
                sequence,
                sequence,
                120,
            )
            .expect("measurement"),
        )
        .expect("ingest");
    }
    let baseline = BaselineManifest::new(
        StableId::new("baseline").expect("id"),
        profile_id,
        "previous",
        Sha256Digest::of_bytes(b"evidence"),
        vec![
            BaselineEntry::new(
                workload.id().clone(),
                Metric::EventAppendLatency,
                Statistic::P99,
                100,
                10,
            )
            .expect("entry"),
        ],
    )
    .expect("baseline");
    let steps = workload.parameters().operation_count();
    let receipt = RunnerReceipt::new(
        run_id,
        StableId::new("plan").expect("id"),
        workload.id().clone(),
        steps,
        steps,
        RunnerTermination::Completed,
        Vec::new(),
    )
    .expect("receipt");
    let evaluation = QualificationEvaluator::evaluate(
        &profile,
        std::slice::from_ref(&workload),
        &sink.finish(),
        ResourceAccountant::new(envelope).summary(),
        &[receipt],
        Some(&baseline),
    )
    .expect("evaluation");
    assert_eq!(evaluation.objectives()[0].status(), peritus_benchmarks::ObjectiveStatus::Met);
    assert_eq!(evaluation.regressions()[0].class(), RegressionClass::Blocking);
    assert_eq!(evaluation.verdict(), QualificationVerdict::NotReady);
}

fn reference_machine() -> ReferenceMachine {
    ReferenceMachine::new(
        StableId::new("linux").expect("id"),
        StableId::new("x86_64").expect("id"),
        "test cpu",
        1,
        1024,
        StableId::new("test-disk").expect("id"),
    )
    .expect("machine")
}
