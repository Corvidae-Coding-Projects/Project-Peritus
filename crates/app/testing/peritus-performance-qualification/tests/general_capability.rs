//! Generic performance qualification fixtures derived from observed failure classes.

use peritus_benchmarks::{
    BaselineEntry, BaselineManifest, CapacityLimits, ConcurrencyLimits, MeasurementIngestor,
    MeasurementRecord, Metric, ObjectiveBound, QualificationEvaluator, QualificationProfile,
    QualificationProfileBuilder, QualificationVerdict, QueueLimits, ReferenceMachine,
    RegressionClass, RegressionPolicy, ResourceAccountant, ResourceEnvelope, RunnerReceipt,
    RunnerTermination, ScenarioKind, Sha256Digest, SloObjective, StableId, Statistic, Workload,
    WorkloadParameters,
};
use serde::Deserialize;

const CASES: &str = include_str!("fixtures/general-capability/performance/cases.json");

#[derive(Deserialize)]
struct FixtureSet {
    cases: Vec<FixtureCase>,
}

#[derive(Deserialize)]
struct FixtureCase {
    name: String,
    candidate: u64,
    baseline: u64,
    include_baseline: bool,
    expected: Expected,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Expected {
    Ready,
    NotReady,
    BlockingRegression,
}

#[test]
fn measured_results_decide_performance_acceptance() {
    let fixtures: FixtureSet = serde_json::from_str(CASES).expect("performance fixtures");
    for fixture in fixtures.cases {
        let evaluation = evaluate(&fixture);
        match fixture.expected {
            Expected::Ready => {
                assert_eq!(evaluation.verdict(), QualificationVerdict::Ready, "{}", fixture.name);
                assert_eq!(
                    evaluation.regressions()[0].class(),
                    RegressionClass::Improvement,
                    "{}",
                    fixture.name
                );
            }
            Expected::NotReady => {
                assert_eq!(
                    evaluation.verdict(),
                    QualificationVerdict::NotReady,
                    "{}",
                    fixture.name
                );
                assert!(!evaluation.not_ready_reasons().is_empty(), "{}", fixture.name);
            }
            Expected::BlockingRegression => {
                assert_eq!(
                    evaluation.verdict(),
                    QualificationVerdict::NotReady,
                    "{}",
                    fixture.name
                );
                assert_eq!(
                    evaluation.regressions()[0].class(),
                    RegressionClass::Blocking,
                    "{}",
                    fixture.name
                );
            }
        }
    }
}

fn evaluate(fixture: &FixtureCase) -> peritus_benchmarks::QualificationEvaluation {
    let workload = workload();
    let envelope = ResourceEnvelope::new(
        ConcurrencyLimits::new(1, 1, 1).expect("concurrency"),
        CapacityLimits::new(1024, 1024, 1024).expect("capacity"),
        QueueLimits::new(1, 1, 1, 1).expect("queues"),
    );
    let profile = profile(&workload, envelope);
    let run_id = StableId::new(format!("run-{}", fixture.name)).expect("run id");
    let measurements = measurements(&run_id, &profile, &workload, fixture.candidate);
    let receipt = receipt(run_id, &workload);
    let baseline =
        fixture.include_baseline.then(|| baseline(&profile, &workload, fixture.baseline));
    QualificationEvaluator::evaluate(
        &profile,
        std::slice::from_ref(&workload),
        &measurements,
        ResourceAccountant::new(envelope).summary(),
        &[receipt],
        baseline.as_ref(),
    )
    .expect("qualification evaluation")
}

fn workload() -> Workload {
    Workload::new(
        StableId::new("generic-operation").expect("id"),
        "generic repeated operation",
        ScenarioKind::EventAppend,
        WorkloadParameters::load(1, 10, 1).expect("parameters"),
    )
    .expect("workload")
}

fn profile(workload: &Workload, envelope: ResourceEnvelope) -> QualificationProfile {
    QualificationProfileBuilder::new(
        StableId::new("generic-profile").expect("id"),
        "generic performance profile",
        ReferenceMachine::new(
            StableId::new("generic-os").expect("id"),
            StableId::new("generic-arch").expect("id"),
            "fixture cpu",
            1,
            1024,
            StableId::new("generic-disk").expect("id"),
        )
        .expect("machine"),
        envelope,
        RegressionPolicy::new(500, 1_000, 1, true).expect("policy"),
    )
    .objective(
        SloObjective::new(
            StableId::new("operation-p99").expect("id"),
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
    .expect("profile")
}

fn measurements(
    run_id: &StableId,
    profile: &QualificationProfile,
    workload: &Workload,
    value: u64,
) -> peritus_benchmarks::MeasurementSet {
    let mut sink =
        MeasurementIngestor::new(run_id.clone(), profile.id().clone(), [workload.id().clone()], 10)
            .expect("measurement sink");
    for sequence in 0..10 {
        sink.record(
            MeasurementRecord::new(
                run_id.clone(),
                profile.id().clone(),
                workload.id().clone(),
                Metric::EventAppendLatency,
                sequence,
                sequence,
                value,
            )
            .expect("measurement"),
        )
        .expect("ingest measurement");
    }
    sink.finish()
}

fn baseline(profile: &QualificationProfile, workload: &Workload, value: u64) -> BaselineManifest {
    BaselineManifest::new(
        StableId::new("generic-baseline").expect("id"),
        profile.id().clone(),
        "fixture-revision",
        Sha256Digest::of_bytes(b"generic baseline evidence"),
        vec![
            BaselineEntry::new(
                workload.id().clone(),
                Metric::EventAppendLatency,
                Statistic::P99,
                value,
                10,
            )
            .expect("baseline entry"),
        ],
    )
    .expect("baseline")
}

fn receipt(run_id: StableId, workload: &Workload) -> RunnerReceipt {
    let steps = workload.parameters().operation_count();
    RunnerReceipt::new(
        run_id,
        StableId::new("generic-plan").expect("id"),
        workload.id().clone(),
        steps,
        steps,
        RunnerTermination::Completed,
        Vec::new(),
    )
    .expect("runner receipt")
}
