//! Criterion benchmarks for deterministic H3 harness overhead.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group};
use peritus_benchmarks::{
    CapacityLimits, ConcurrencyLimits, MeasurementIngestor, MeasurementRecord, Metric,
    ObjectiveBound, PlanKind, QualificationEvaluator, QualificationPlan,
    QualificationProfileBuilder, QueueLimits, ReferenceMachine, RegressionPolicy,
    ResourceAccountant, ResourceEnvelope, RunnerReceipt, RunnerTermination, ScenarioKind,
    SloObjective, StableId, Statistic, Workload, WorkloadParameters,
};

fn envelope() -> ResourceEnvelope {
    ResourceEnvelope::new(
        ConcurrencyLimits::new(64, 64, 32).expect("concurrency"),
        CapacityLimits::new(16 << 30, 64 << 30, 100_000_000).expect("capacity"),
        QueueLimits::new(256, 512, 512, 128).expect("queues"),
    )
}

fn event_workload() -> Workload {
    Workload::new(
        StableId::new("criterion.event-append").expect("id"),
        "criterion harness event append workload",
        ScenarioKind::EventAppend,
        WorkloadParameters::load(10, 1_000, 32).expect("parameters").with_seed(42),
    )
    .expect("workload")
}

fn benchmark_lazy_plan(c: &mut Criterion) {
    let workload = event_workload();
    let plan = QualificationPlan::new(
        StableId::new("criterion-plan").expect("id"),
        PlanKind::Load,
        StableId::new("criterion-profile").expect("id"),
        envelope(),
        workload,
    )
    .expect("plan");
    let mut group = c.benchmark_group("qualification_plan");
    group.throughput(Throughput::Elements(10_000));
    group.bench_function("generate_10000_steps", |bencher| {
        bencher.iter(|| {
            for step in plan.iter().take(10_000) {
                black_box(step);
            }
        });
    });
    group.finish();
}

fn benchmark_ingestion(c: &mut Criterion) {
    let run = StableId::new("criterion-run").expect("id");
    let profile = StableId::new("criterion-profile").expect("id");
    let workload = StableId::new("criterion.event-append").expect("id");
    let mut group = c.benchmark_group("measurement_ingestion");
    group.throughput(Throughput::Elements(10_000));
    group.bench_function("validated_10000_records", |bencher| {
        bencher.iter(|| {
            let mut sink =
                MeasurementIngestor::new(run.clone(), profile.clone(), [workload.clone()], 10_000)
                    .expect("sink");
            for sequence in 0..10_000 {
                sink.record(
                    MeasurementRecord::new(
                        run.clone(),
                        profile.clone(),
                        workload.clone(),
                        Metric::EventAppendLatency,
                        sequence,
                        sequence,
                        sequence % 50_000,
                    )
                    .expect("record"),
                )
                .expect("ingest");
            }
            black_box(sink.finish());
        });
    });
    group.finish();
}

fn benchmark_evaluation(c: &mut Criterion) {
    let run = StableId::new("criterion-run").expect("id");
    let profile_id = StableId::new("criterion-profile").expect("id");
    let workload = event_workload();
    let workload_id = workload.id().clone();
    let machine = ReferenceMachine::new(
        StableId::new("linux").expect("id"),
        StableId::new("x86_64").expect("id"),
        "criterion reference",
        32,
        64 << 30,
        StableId::new("nvme").expect("id"),
    )
    .expect("machine");
    let profile = QualificationProfileBuilder::new(
        profile_id.clone(),
        "criterion evaluation profile",
        machine,
        envelope(),
        RegressionPolicy::new(500, 1_000, 1, false).expect("policy"),
    )
    .objective(
        SloObjective::new(
            StableId::new("append-p99").expect("id"),
            workload_id.clone(),
            Metric::EventAppendLatency,
            Statistic::P99,
            ObjectiveBound::AtMost,
            50_000,
            10_000,
        )
        .expect("objective"),
    )
    .build()
    .expect("profile");
    let mut sink = MeasurementIngestor::new(run.clone(), profile_id, [workload_id.clone()], 10_000)
        .expect("sink");
    for sequence in 0..10_000 {
        sink.record(
            MeasurementRecord::new(
                run.clone(),
                profile.id().clone(),
                workload_id.clone(),
                Metric::EventAppendLatency,
                sequence,
                sequence,
                sequence % 40_000,
            )
            .expect("record"),
        )
        .expect("ingest");
    }
    let measurements = sink.finish();
    let receipt = RunnerReceipt::new(
        run,
        StableId::new("criterion-plan").expect("id"),
        workload_id,
        workload.parameters().operation_count(),
        workload.parameters().operation_count(),
        RunnerTermination::Completed,
        Vec::new(),
    )
    .expect("receipt");
    let accounting = ResourceAccountant::new(profile.envelope()).summary();
    c.bench_function("evaluate_10000_samples", |bencher| {
        bencher.iter(|| {
            black_box(
                QualificationEvaluator::evaluate(
                    &profile,
                    std::slice::from_ref(&workload),
                    &measurements,
                    accounting.clone(),
                    std::slice::from_ref(&receipt),
                    None,
                )
                .expect("evaluation"),
            );
        });
    });
}

criterion_group!(qualification, benchmark_lazy_plan, benchmark_ingestion, benchmark_evaluation);

fn main() {
    if std::env::args_os()
        .skip(1)
        .any(|argument| argument.to_string_lossy().starts_with("--test-threads"))
    {
        return;
    }

    qualification();
    Criterion::default().configure_from_args().final_summary();
}
