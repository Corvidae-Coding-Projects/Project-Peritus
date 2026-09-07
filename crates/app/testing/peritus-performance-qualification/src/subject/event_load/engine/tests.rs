//! Adversarial producer/worker tests; these are not performance acceptance evidence.

use std::io::{BufRead as _, BufReader};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::*;
use peritus_benchmarks::{
    CapacityLimits, ConcurrencyLimits, PlanKind, QueueLimits, ResourceAccountant, ResourceEnvelope,
    ScenarioKind, StableId, Workload, WorkloadParameters,
};

struct SlowWorker {
    live: Arc<AtomicUsize>,
    panic: bool,
}
impl Drop for SlowWorker {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}
impl Executor for SlowWorker {
    fn execute(&mut self, worker: usize, job: &Job) -> Completion {
        assert!(!self.panic, "intentional worker panic");
        let started_us = micros(job.origin.elapsed());
        thread::sleep(Duration::from_millis(100));
        Completion {
            worker,
            sequence: job.sequence,
            scheduled_us: job.scheduled_us,
            started_us,
            finished_us: micros(job.origin.elapsed()),
            payload_bytes: job.payload_bytes,
            event_id: Some([1; 16]),
            expected_event_digest: None,
            committed_commands: 3,
            failure: None,
        }
    }
}

fn plan() -> (QualificationPlan, ResourceEnvelope) {
    let envelope = ResourceEnvelope::new(
        ConcurrencyLimits::new(1, 1, 1).expect("concurrency"),
        CapacityLimits::new(1_048_576, 1_048_576, 1_024).expect("capacity"),
        QueueLimits::new(1, 1, 1, 1).expect("queues"),
    );
    let workload = Workload::new(
        id("events"),
        "slow worker independent-arrival regression",
        ScenarioKind::EventAppend,
        WorkloadParameters::load(1, 100, 1).expect("parameters"),
    )
    .expect("workload");
    (
        QualificationPlan::new(id("plan"), PlanKind::Load, id("profile"), envelope, workload)
            .expect("plan"),
        envelope,
    )
}

#[test]
fn slow_worker_does_not_throttle_arrivals_or_drain_expired_backlog() {
    let (plan, envelope) = plan();
    let temporary = tempfile::tempdir().expect("evidence root");
    let mut evidence = EvidenceWriter::new(temporary.path()).expect("evidence");
    let mut ledger = ResourceAccountant::new(envelope);
    let live = Arc::new(AtomicUsize::new(1));
    let mut latencies = Vec::new();
    let mut complete = |row: &Completion| {
        latencies.push(row.finished_us - row.scheduled_us);
        Ok(())
    };
    let summary = run(
        &plan,
        &CancellationFlag::new(),
        vec![SlowWorker { live: live.clone(), panic: false }],
        Sinks { accounting: &mut ledger, evidence: &mut evidence, on_completion: &mut complete },
    )
    .expect("bounded load");
    assert_eq!(summary.offered + summary.schedule_missed, 100);
    assert!(summary.queue_full > 50);
    assert!(summary.committed < 20);
    assert_eq!(summary.peak_active, 1);
    assert_eq!(summary.peak_queued, 1);
    assert_eq!(
        summary.offered,
        summary.committed + summary.queue_full + summary.expired + summary.failed
    );
    assert!(summary.elapsed_us >= 1_000_000);
    assert!(
        latencies.iter().any(|latency| *latency >= 150_000),
        "scheduled latency must include client queue waiting"
    );
    assert!(ledger.summary().is_balanced());
    assert_eq!(live.load(Ordering::SeqCst), 0);
    let evidence = evidence.finish().expect("finish");
    let rows = BufReader::new(evidence.reader().expect("read"))
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(&line.expect("line")).expect("json"))
        .collect::<Vec<_>>();
    assert_eq!(rows.iter().filter(|row| row["record"] == "arrival").count(), 100);
    assert!(
        rows.iter()
            .filter(|row| row["record"] == "completion")
            .all(|row| row["started_us"].as_u64().expect("start") < 1_000_000)
    );
}

#[test]
fn panicked_worker_is_reported_and_every_owned_worker_is_joined() {
    let (plan, envelope) = plan();
    let temporary = tempfile::tempdir().expect("evidence root");
    let mut evidence = EvidenceWriter::new(temporary.path()).expect("evidence");
    let mut ledger = ResourceAccountant::new(envelope);
    let live = Arc::new(AtomicUsize::new(1));
    let result = run(
        &plan,
        &CancellationFlag::new(),
        vec![SlowWorker { live: live.clone(), panic: true }],
        Sinks { accounting: &mut ledger, evidence: &mut evidence, on_completion: &mut |_| Ok(()) },
    );
    assert!(matches!(result, Err(SubjectError::EventWorker("worker panicked"))));
    assert_eq!(live.load(Ordering::SeqCst), 0);
}

#[test]
fn cancellation_does_not_offer_work_and_releases_idle_workers() {
    let (plan, envelope) = plan();
    let temporary = tempfile::tempdir().expect("evidence root");
    let mut evidence = EvidenceWriter::new(temporary.path()).expect("evidence");
    let mut ledger = ResourceAccountant::new(envelope);
    let live = Arc::new(AtomicUsize::new(1));
    let cancel = CancellationFlag::new();
    cancel.cancel();
    let summary = run(
        &plan,
        &cancel,
        vec![SlowWorker { live: live.clone(), panic: false }],
        Sinks { accounting: &mut ledger, evidence: &mut evidence, on_completion: &mut |_| Ok(()) },
    )
    .expect("cancelled load");
    assert_eq!(summary.offered, 0);
    assert_eq!(live.load(Ordering::SeqCst), 0);
    assert!(ledger.summary().is_balanced());
}

fn id(value: &str) -> StableId {
    StableId::new(value).expect("id")
}
