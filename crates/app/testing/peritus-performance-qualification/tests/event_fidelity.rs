//! Native payload verification and explicitly invoked, unchanged production event acceptance.

#![cfg(unix)]

mod event_fidelity_support;

use std::fs;
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use peritus_benchmarks::{
    AccountingSummary, DatasetLimits, MeasurementIngestor, MeasurementSet, ObjectiveStatus,
    PlanKind, QualificationDataset, QualificationEvaluator, QualificationPlan, QualificationRunner,
    QualificationSubject, ResourceAccountant, RunContext, RunnerDescriptor, RunnerReceipt,
    StableId, SubjectDescriptor, Workload, WorkloadParameters,
};
use peritus_performance_qualification::{
    CancellationFlag, EventAppendRunner, IntegratedSubject, MachineProbe, StorageObservation,
    SubjectConfiguration, sha256_file,
};
use serde_json::json;

const PROFILE: &str = include_str!(
    "../../../../../benchmarks/profiles/qualification-intel-core-ultra-9-275hx-v1.json"
);
const CATALOG: &str = include_str!("../../../../../benchmarks/workloads/production-v1.json");

#[test]
#[ignore = "requires PERITUS_H3_DAEMON and PERITUS_H3_SCRATCH; short native fidelity check"]
fn native_event_bytes_and_streaming_evidence_match_the_journal() {
    let dataset = dataset();
    let original = event_workload(&dataset);
    let workload = Workload::new(
        original.id().clone(),
        "short native payload fidelity test",
        original.scenario(),
        WorkloadParameters::load(1, 10, 2)
            .expect("parameters")
            .with_payload_bytes(8192)
            .expect("payload")
            .with_queue_capacity(4)
            .expect("queue")
            .with_seed(1102),
    )
    .expect("workload");
    let temporary = tempfile::tempdir().expect("smoke evidence");
    let plan = make_plan(&dataset, workload.clone());
    let report = run(&dataset, workload, temporary.path());
    assert_eq!(report["receipt"]["termination"], "completed", "{report}");
    assert_eq!(report["durable_payloads_verified"], 10);
    let mut rows = event_fidelity_support::read_rows(&temporary.path().join("events.ndjson"));
    rows.iter_mut().find(|row| row["record"] == "completion").expect("completion")["payload_bytes"] =
        json!(1);
    let database = event_fidelity_support::open(&temporary.path().join("journal.sqlite3"));
    let initial: Vec<serde_json::Value> = serde_json::from_slice(
        &fs::read(temporary.path().join("initial-events.json")).expect("initial evidence"),
    )
    .expect("initial JSON");
    assert!(
        event_fidelity_support::verify(&rows, &database, &plan, &initial).is_err(),
        "altered expected payload must reject"
    );
}

#[test]
#[ignore = "120-second acceptance; requires explicit fresh PERITUS_H3_ACCEPTANCE_OUTPUT"]
fn production_event_append_acceptance() {
    let dataset = dataset();
    let machine = MachineProbe::observe(id("nvme-gen4")).expect("current host facts");
    assert!(
        machine.assess(dataset.profile().reference_machine()).matches(),
        "acceptance host mismatch"
    );
    let output = environment_path("PERITUS_H3_ACCEPTANCE_OUTPUT");
    fs::create_dir(&output)
        .expect("acceptance output must be a fresh directory under an existing parent");
    fs::set_permissions(&output, fs::Permissions::from_mode(0o700)).expect("private output");
    let sources = environment_path("PERITUS_H3_SOURCE_MANIFEST");
    fs::copy(sources, output.join("sources.sha256"))
        .expect("retain exact reviewed source manifest");
    fs::write(
        output.join("machine.json"),
        serde_json::to_vec_pretty(&machine).expect("machine JSON"),
    )
    .expect("retain machine");
    let report = run(&dataset, event_workload(&dataset), &output);
    // Retention and cleanup happen before either acceptance assertion, including on an SLO miss.
    assert_eq!(report["receipt"]["termination"], "completed", "{report}");
    assert_eq!(report["event_objective"]["status"], "met", "{report}");
}

fn run(dataset: &QualificationDataset, workload: Workload, output: &Path) -> serde_json::Value {
    let configuration = SubjectConfiguration::new(
        &environment_path("PERITUS_H3_DAEMON"),
        &environment_path("PERITUS_H3_SCRATCH"),
    )
    .expect("configuration");
    let executable = std::env::current_exe().expect("exact native runner");
    let runner_identity = RunnerDescriptor::new(
        id("peritus-h3-event-acceptance"),
        env!("CARGO_PKG_VERSION"),
        sha256_file(&executable).expect("runner digest"),
    )
    .expect("runner identity");
    let plan = make_plan(dataset, workload);
    let context = RunContext::for_workload(
        id("event-acceptance-run"),
        dataset.profile().id().clone(),
        plan.id().clone(),
        plan.workload().id().clone(),
    );
    let mut measurements = MeasurementIngestor::new(
        context.run_id().clone(),
        dataset.profile().id().clone(),
        [plan.workload().id().clone()],
        dataset.profile().max_measurements(),
    )
    .expect("bounded measurements");
    let mut ledger = ResourceAccountant::new(dataset.profile().envelope());
    let mut runner = EventAppendRunner::new(runner_identity.clone(), CancellationFlag::new());
    fs::write(output.join("profile.json"), PROFILE).expect("retain profile");
    fs::write(output.join("catalog.json"), CATALOG).expect("retain catalog");
    fs::write(output.join("plan.json"), serde_json::to_vec_pretty(&plan).expect("plan JSON"))
        .expect("retain executed plan");
    fs::copy(&executable, output.join("runner")).expect("retain exact runner");
    fs::copy(configuration.executable(), output.join("peritusd")).expect("retain exact daemon");
    let mut authorized =
        IntegratedSubject::launch(&configuration, "bounded-event-fidelity-acceptance")
            .expect("fresh daemon");
    let (subject, authorization) = authorized.parts();
    let identity = subject.descriptor().clone();
    let storage = subject.storage().clone();
    fs::write(
        output.join("live-subject.json"),
        serde_json::to_vec_pretty(
            &json!({ "subject": identity, "storage": storage, "runner": runner_identity }),
        )
        .expect("live identity JSON"),
    )
    .expect("retain live identity");
    let initial_database =
        event_fidelity_support::open(&storage.path().join("state/peritus.sqlite3"));
    let initial = event_fidelity_support::initial_events(&initial_database);
    fs::write(
        output.join("initial-events.json"),
        serde_json::to_vec(&initial).expect("initial JSON"),
    )
    .expect("retain startup event identities");
    drop(initial_database);
    let result =
        runner.run(subject, authorization, &context, &plan, &mut measurements, &mut ledger);
    let evidence = runner.take_evidence();
    if let Some(evidence) = &evidence {
        let mut raw = BufReader::new(evidence.reader().expect("raw reader"));
        let mut destination =
            fs::File::create(output.join("events.ndjson")).expect("raw destination");
        std::io::copy(&mut raw, &mut destination).expect("retain raw unsampled evidence");
        destination.sync_all().expect("sync raw evidence");
    }
    // SQLite's backup API includes committed WAL contents in a consistent retained database.
    let source = event_fidelity_support::open(&storage.path().join("state/peritus.sqlite3"));
    source
        .backup(rusqlite::MAIN_DB, output.join("journal.sqlite3"), None)
        .expect("consistent database backup");
    drop(source);
    let cleanup = authorized.cleanup();
    drop(evidence);
    let receipt = result.expect("runner must retain a terminal receipt");
    cleanup.expect("owned processes and scratch cleanup");
    assert!(!storage.path().exists(), "scratch remains after cleanup");
    write_report(
        dataset,
        output,
        NativeResult {
            receipt,
            identity,
            runner_identity,
            storage,
            plan,
            measurements: measurements.finish(),
            accounting: ledger.summary(),
        },
    )
}

struct NativeResult {
    receipt: RunnerReceipt,
    identity: SubjectDescriptor,
    runner_identity: RunnerDescriptor,
    storage: StorageObservation,
    plan: QualificationPlan,
    measurements: MeasurementSet,
    accounting: AccountingSummary,
}

fn write_report(
    dataset: &QualificationDataset,
    output: &Path,
    result: NativeResult,
) -> serde_json::Value {
    let NativeResult {
        receipt,
        identity,
        runner_identity,
        storage,
        plan,
        measurements,
        accounting,
    } = result;
    let rows = event_fidelity_support::read_rows(&output.join("events.ndjson"));
    let snapshot = event_fidelity_support::open(&output.join("journal.sqlite3"));
    let initial: Vec<serde_json::Value> = serde_json::from_slice(
        &fs::read(output.join("initial-events.json")).expect("initial evidence"),
    )
    .expect("initial JSON");
    let verified = event_fidelity_support::verify(&rows, &snapshot, &plan, &initial)
        .expect("independent durable payload and arrival audit");
    let evaluation = QualificationEvaluator::evaluate(
        dataset.profile(),
        dataset.workloads(),
        &measurements,
        accounting.clone(),
        std::slice::from_ref(&receipt),
        None,
    )
    .expect("unchanged evaluator");
    let objective = evaluation
        .objectives()
        .iter()
        .find(|objective| objective.objective_id().as_str() == "event-append-p99")
        .expect("production event SLO");
    let summary = rows.last().expect("terminal raw summary");
    let report = json!({
        "scope": "event-append acceptance only; not full H3, baseline acceptance, or release qualification",
        "fixture": "one real D2 submission per offered operation; two prerequisite commits included in latency",
        "subject": identity, "runner": runner_identity, "storage": storage,
        "profile_sha256": sha256_file(&output.join("profile.json")).expect("profile digest"),
        "catalog_sha256": sha256_file(&output.join("catalog.json")).expect("catalog digest"),
        "events_sha256": sha256_file(&output.join("events.ndjson")).expect("raw digest"),
        "database_sha256": sha256_file(&output.join("journal.sqlite3")).expect("database digest"),
        "receipt": receipt, "arrival_summary": summary, "durable_payloads_verified": verified,
        "event_objective": objective, "accounting": accounting, "cleanup_verified": true,
        "initial_events": initial,
        "acceptance_passed": receipt.completed() && objective.status() == ObjectiveStatus::Met,
    });
    fs::write(
        output.join("measurements.json"),
        serde_json::to_vec(&measurements).expect("measurements JSON"),
    )
    .expect("retain all measurements");
    fs::write(
        output.join("evaluation.json"),
        serde_json::to_vec_pretty(&evaluation).expect("evaluation JSON"),
    )
    .expect("retain full honest evaluation");
    fs::write(output.join("report.json"), serde_json::to_vec_pretty(&report).expect("report JSON"))
        .expect("retain acceptance outcome");
    report
}

fn make_plan(dataset: &QualificationDataset, workload: Workload) -> QualificationPlan {
    QualificationPlan::new(
        id("event-acceptance-plan"),
        PlanKind::Load,
        dataset.profile().id().clone(),
        dataset.profile().envelope(),
        workload,
    )
    .expect("exact workload plan")
}

fn dataset() -> QualificationDataset {
    QualificationDataset::from_json(PROFILE, CATALOG, DatasetLimits::production_defaults())
        .expect("production documents")
}
fn event_workload(dataset: &QualificationDataset) -> Workload {
    dataset
        .workloads()
        .iter()
        .find(|workload| workload.id().as_str() == "load.event-append.v1")
        .expect("production event case")
        .clone()
}
fn environment_path(name: &str) -> PathBuf {
    std::env::var_os(name).map(PathBuf::from).expect(name)
}
fn id(value: &str) -> StableId {
    StableId::new(value).expect("id")
}
