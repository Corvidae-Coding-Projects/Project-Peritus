//! Focused regressions for owned cleanup, checked controls, resources, and publication retries.

use std::{
    fs,
    time::{Duration, Instant},
};

use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_process::{
    CancellationReason, ErrorCode, ExecutionAuthorizationRequest, ExecutionGateway, GracefulAction,
    IoMode, ProcessCursor, ProcessStore, StdinPolicy, TerminalDisposition, TerminalSize,
    WorkspaceAccess,
};
#[cfg(unix)]
use peritus_process::{ProcessResourceDimension, ResourceFidelity};
use peritus_types::EventId;

use super::support::{Ids, PlanOptions, TestRoot, commit_authority, intent, open_journal, plan};

#[path = "regressions/process_lifecycle.rs"]
mod process_lifecycle;

#[cfg(unix)]
#[test]
fn disk_sampling_does_not_follow_workspace_symlink_cycles() {
    let root = TestRoot::new();
    std::os::unix::fs::symlink(root.workspace(), root.workspace().join("cycle"))
        .expect("workspace symlink cycle");
    let ids = Ids::new(179);
    let mut options = control_options(IoMode::Pipes);
    options.arguments = vec!["empty".to_owned()];
    options.stdin = StdinPolicy::Closed;
    let execution = plan(&root, &ids, options).expect("execution plan");
    let (owned, _) = launch(&root, &ids, execution);
    assert_eq!(
        owned.wait().expect("symlink-safe execution").disposition(),
        TerminalDisposition::Exited
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn unavailable_claimed_resource_enforcement_is_rejected_before_consumption() {
    let root = TestRoot::new();
    let ids = Ids::new(181);
    let mut options = control_options(IoMode::Pipes);
    options.resource_fidelity = peritus_sandbox::ResourceFidelity::Supervisor;
    let execution = plan(&root, &ids, options).expect("supervisor-fidelity plan");
    let action = intent(&ids, &execution);
    let mut journal = open_journal(&root);
    let receipts =
        commit_authority(&mut journal, &ids, &action, execution.resource_policy().wall_millis());
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).expect("process store"),
    );
    let request = ExecutionAuthorizationRequest::new(
        &action,
        &receipts.kernel,
        &receipts.capability,
        &receipts.budget,
        None,
        &receipts.epoch,
        ids.revision,
        ids.session,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        receipts.observed_at,
        execution.digest(),
    );
    let Err(error) = gateway.launch(&request, execution) else {
        panic!("unavailable claimed enforcement must fail before effect");
    };
    assert_eq!(error.code(), ErrorCode::Unsupported);
    assert_eq!(
        fs::read_dir(root.registry().join("claims-v1")).expect("claims directory").count(),
        0
    );
}

#[test]
fn artifact_failure_preserves_terminal_and_retry_resumes_per_stream() {
    let root = TestRoot::new();
    let ids = Ids::new(187);
    let mut options = control_options(IoMode::Pipes);
    options.arguments = vec!["dual-output".to_owned()];
    options.stdin = StdinPolicy::Closed;
    let execution = plan(&root, &ids, options).expect("execution plan");
    let (owned, store) = launch(&root, &ids, execution);
    let control = owned.control();
    wait_terminal(&control);
    let spool = only_spool_directory(&root);
    let stderr = spool.join("stderr.spool");
    let held = spool.join("stderr.held");
    fs::rename(&stderr, &held).expect("hold stderr spool");
    let artifact_root = root.path().join("artifacts");
    let artifacts = ArtifactStore::open(
        StoreConfig::new(&artifact_root, 1_024, 4_096).expect("artifact config"),
    )
    .expect("artifact store");
    let event = EventId::new([188; 16]).expect("publication event");
    let failure = owned
        .wait_and_publish(&artifacts, event)
        .expect_err("missing second spool must fail publication");
    let partial = failure.terminal_result().expect("completed process terminal preserved");
    assert_eq!(partial.disposition(), TerminalDisposition::Exited);
    assert_eq!(partial.artifacts().len(), 1);
    assert!(!partial.artifact_publication_complete());
    assert_eq!(store.terminal_result(ids.process).expect("durable partial"), *partial);
    fs::rename(&held, &stderr).expect("restore stderr spool");
    let complete = store
        .retry_artifact_publication(ids.process, &artifacts, event)
        .expect("retry remaining stream");
    assert_eq!(complete.artifacts().len(), 2);
    assert!(complete.artifact_publication_complete());
    assert_eq!(
        store.retry_artifact_publication(ids.process, &artifacts, event).expect("idempotent retry"),
        complete
    );
}

#[test]
fn zero_output_artifact_publication_is_complete_and_idempotent() {
    let root = TestRoot::new();
    let ids = Ids::new(198);
    let mut options = control_options(IoMode::Pipes);
    options.arguments = vec!["empty".to_owned()];
    options.stdin = StdinPolicy::Closed;
    let execution = plan(&root, &ids, options).expect("execution plan");
    let (owned, store) = launch(&root, &ids, execution);
    let artifact_root = root.path().join("empty-artifacts");
    let artifacts = ArtifactStore::open(
        StoreConfig::new(&artifact_root, 1_024, 4_096).expect("artifact config"),
    )
    .expect("artifact store");
    let event = EventId::new([199; 16]).expect("publication event");
    let complete = owned.wait_and_publish(&artifacts, event).expect("zero-output publication");
    assert!(complete.artifacts().is_empty());
    assert!(complete.artifact_publication_complete());
    assert_eq!(
        store.retry_artifact_publication(ids.process, &artifacts, event).expect("idempotent retry"),
        complete
    );
}

fn control_options(io: IoMode) -> PlanOptions<'static> {
    PlanOptions {
        arguments: vec!["control".to_owned()],
        environment: Vec::new(),
        io,
        stdin: StdinPolicy::bounded(16, 16).expect("stdin policy"),
        output_limit: 64,
        wall_timeout: None,
        graceful: GracefulAction::Terminate,
        grace_millis: 50,
        process_count: 1,
        descendants: 0,
        workspace_access: WorkspaceAccess::ReadOnly,
        resize_allowed: true,
        environment_authority: None,
        resource_fidelity: peritus_sandbox::ResourceFidelity::Reference,
    }
}

fn launch(
    root: &TestRoot,
    ids: &Ids,
    execution: peritus_process::ExecutionPlan,
) -> (peritus_process::OwnedProcess, ProcessStore) {
    let action = intent(ids, &execution);
    let mut journal = open_journal(root);
    let receipts =
        commit_authority(&mut journal, ids, &action, execution.resource_policy().wall_millis());
    let store = ProcessStore::open(root.registry(), root.workspace()).expect("process store");
    let gateway = ExecutionGateway::new(store.clone());
    let request = ExecutionAuthorizationRequest::new(
        &action,
        &receipts.kernel,
        &receipts.capability,
        &receipts.budget,
        None,
        &receipts.epoch,
        ids.revision,
        ids.session,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        receipts.observed_at,
        execution.digest(),
    );
    (gateway.launch(&request, execution).expect("authorized launch"), store)
}

fn reject_resize_then_cancel(owned: peritus_process::OwnedProcess) {
    let control = owned.control();
    let error = control
        .resize(TerminalSize::new(30, 100, 0, 0).expect("resize"))
        .expect_err("resize must be rejected before enqueue");
    assert_eq!(error.code(), ErrorCode::InvalidInput);
    control.cancel(CancellationReason::User).expect("owner remains controllable");
    assert_eq!(
        owned.wait().expect("cancelled terminal").disposition(),
        TerminalDisposition::Cancelled
    );
}

fn wait_terminal(control: &peritus_process::ProcessControl) {
    for _ in 0..200 {
        if control.terminal_result().is_some() {
            return;
        }
        let _ = control.read_events(ProcessCursor::after(0), 1);
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("process did not publish terminal result");
}

fn only_spool_directory(root: &TestRoot) -> std::path::PathBuf {
    let mut entries = fs::read_dir(root.registry().join("spools-v1")).expect("spool root");
    let directory = entries.next().expect("process spool").expect("spool entry").path();
    assert!(entries.next().is_none());
    directory
}
