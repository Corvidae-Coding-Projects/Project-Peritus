//! Production adapter integration coverage for authorized local process ownership.

#[path = "process_integration/control_lifecycle.rs"]
mod control_lifecycle;
#[path = "process_integration/native_backend.rs"]
mod native_backend;
#[path = "process_integration/regressions.rs"]
mod regressions;
mod support;

use peritus_leases::ReconciliationCorrelation;
use peritus_process::{
    CancellationReason, ExecutionAuthorizationRequest, ExecutionGateway, GracefulAction, IoMode,
    ProbeObservation, ProcessCursor, ProcessEventKind, ProcessProbe, ProcessStore,
    ProcessTreeIdentity, QuiescenceBlocker, StdinPolicy, TerminalDisposition,
};
#[cfg(unix)]
use peritus_process::{ProcessSignal, TerminalSize};
use peritus_types::{EvidenceId, Generation};

use support::authority::commit_authority_without_dispatch;
use support::{
    Ids, PlanOptions, TestRoot, commit_authority, commit_authority_with_lease, intent,
    open_journal, plan,
};

#[test]
fn exact_committed_authority_launches_separate_bounded_pipe_streams() {
    let root = TestRoot::new();
    let ids = Ids::new(1);
    let plan = plan(
        &root,
        &ids,
        PlanOptions {
            arguments: vec!["pipe".to_owned()],
            environment: Vec::new(),
            io: IoMode::Pipes,
            stdin: StdinPolicy::bounded(10, 10).expect("stdin policy"),
            output_limit: 64,
            wall_timeout: None,
            graceful: GracefulAction::Terminate,
            grace_millis: 100,
            process_count: 1,
            descendants: 0,
            workspace_access: peritus_process::WorkspaceAccess::ReadOnly,
            resize_allowed: true,
            environment_authority: None,
            resource_fidelity: peritus_sandbox::ResourceFidelity::Reference,
        },
    )
    .expect("execution plan");
    let intent = intent(&ids, &plan);
    let mut journal = open_journal(&root);
    let receipts =
        commit_authority(&mut journal, &ids, &intent, plan.resource_policy().wall_millis());
    let store = ProcessStore::open(root.registry(), root.workspace()).expect("process store");
    let gateway = ExecutionGateway::new(store);
    let request = ExecutionAuthorizationRequest::new(
        &intent,
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
        plan.digest(),
    );
    let owned = gateway.launch(&request, plan).expect("authorized launch");
    let control = owned.control();
    control.write_stdin(b"pipe-input".to_vec()).expect("pipe input");
    control.close_stdin().expect("close input");
    let terminal = owned.wait().expect("terminal result");
    assert_eq!(terminal.disposition(), TerminalDisposition::Exited);
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
    let events = control.read_events(ProcessCursor::after(0), 256);
    let stdout = output(&events, peritus_process::OutputStream::Stdout);
    let stderr = output(&events, peritus_process::OutputStream::Stderr);
    assert_eq!(stdout, b"pipe-out");
    assert_eq!(stderr, b"pipe-err");
    assert!(events.iter().any(|event| matches!(event.kind(), ProcessEventKind::StdinClosed)));
}

#[test]
fn non_dispatched_action_is_rejected_before_durable_consumption() {
    let root = TestRoot::new();
    let ids = Ids::new(45);
    let execution = plan(
        &root,
        &ids,
        PlanOptions {
            arguments: vec!["control".to_owned()],
            environment: Vec::new(),
            io: IoMode::Pipes,
            stdin: StdinPolicy::Closed,
            output_limit: 64,
            wall_timeout: None,
            graceful: GracefulAction::Terminate,
            grace_millis: 50,
            process_count: 1,
            descendants: 0,
            workspace_access: peritus_process::WorkspaceAccess::ReadOnly,
            resize_allowed: true,
            environment_authority: None,
            resource_fidelity: peritus_sandbox::ResourceFidelity::Reference,
        },
    )
    .expect("execution plan");
    let action = intent(&ids, &execution);
    let mut journal = open_journal(&root);
    let receipts = commit_authority_without_dispatch(
        &mut journal,
        &ids,
        &action,
        execution.resource_policy().wall_millis(),
    );
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
    assert!(gateway.launch(&request, execution).is_err());
    assert_eq!(
        std::fs::read_dir(root.registry().join("claims-v1")).expect("claims directory").count(),
        0
    );
}

#[test]
fn holder_quiescence_requires_exact_terminal_correlated_manifests() {
    let root = TestRoot::new();
    let ids = Ids::new(60);
    let (owned, claim) = writable_launch(&root, &ids);
    let control = owned.control();
    wait_started(&control);
    let store = ProcessStore::open(root.registry(), root.workspace()).expect("reopen store");
    let correlation =
        ReconciliationCorrelation::new(claim.scope(), claim.generation(), claim.holder());
    let evidence = EvidenceId::new([90; 16]).expect("evidence");
    assert_eq!(
        store.inspect_holder_quiescence(correlation, claim, evidence),
        Err(QuiescenceBlocker::LiveProcess(ids.process))
    );
    control.cancel(CancellationReason::User).expect("cancel writable process");
    owned.wait().expect("terminal writable process");
    let reopened = ProcessStore::open(root.registry(), root.workspace()).expect("terminal store");
    let observation = reopened
        .inspect_holder_quiescence(correlation, claim, evidence)
        .expect("exact terminal holder quiescence");
    assert_eq!(observation.correlation(), correlation);
    assert_eq!(observation.claim(), claim);
    assert_eq!(observation.evidence().claim(), claim);
    assert_eq!(observation.inspected_records(), 1);
    let mismatched = ReconciliationCorrelation::new(
        claim.scope(),
        Generation::new(2).expect("mismatched generation"),
        claim.holder(),
    );
    assert_eq!(
        reopened.inspect_holder_quiescence(mismatched, claim, evidence),
        Err(QuiescenceBlocker::CorrelationMismatch)
    );

    let unresolved_root = TestRoot::new();
    let unresolved_ids = Ids::new(100);
    let (unresolved, unresolved_claim) = writable_launch(&unresolved_root, &unresolved_ids);
    let unresolved_control = unresolved.control();
    wait_started(&unresolved_control);
    let unresolved_store =
        ProcessStore::open(unresolved_root.registry(), unresolved_root.workspace()).expect("store");
    unresolved_store.reconcile(&mut MismatchProbe).expect("mark unresolved recovery");
    let unresolved_correlation = ReconciliationCorrelation::new(
        unresolved_claim.scope(),
        unresolved_claim.generation(),
        unresolved_claim.holder(),
    );
    assert_eq!(
        unresolved_store.inspect_holder_quiescence(
            unresolved_correlation,
            unresolved_claim,
            EvidenceId::new([91; 16]).expect("unresolved evidence"),
        ),
        Err(QuiescenceBlocker::UnresolvedProcess(unresolved_ids.process))
    );
    let _ = unresolved_control.cancel(CancellationReason::SupervisorShutdown);
    let _ = unresolved.wait();
}

fn writable_launch(
    root: &TestRoot,
    ids: &Ids,
) -> (peritus_process::OwnedProcess, peritus_leases::LeaseClaim) {
    let execution = plan(
        root,
        ids,
        PlanOptions {
            arguments: vec!["control".to_owned()],
            environment: Vec::new(),
            io: IoMode::Pipes,
            stdin: StdinPolicy::Closed,
            output_limit: 64,
            wall_timeout: None,
            graceful: GracefulAction::Terminate,
            grace_millis: 50,
            process_count: 1,
            descendants: 0,
            workspace_access: peritus_process::WorkspaceAccess::Writable,
            resize_allowed: true,
            environment_authority: None,
            resource_fidelity: peritus_sandbox::ResourceFidelity::Reference,
        },
    )
    .expect("execution plan");
    let action = intent(ids, &execution);
    let mut journal = open_journal(root);
    let receipts = commit_authority_with_lease(
        &mut journal,
        ids,
        &action,
        execution.resource_policy().wall_millis(),
    );
    let claim = receipts.lease.transition().record().binding().as_use().expect("lease use").claim();
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).expect("process store"),
    );
    let request = ExecutionAuthorizationRequest::new(
        &action,
        &receipts.kernel,
        &receipts.capability,
        &receipts.budget,
        Some(&receipts.lease),
        &receipts.epoch,
        ids.revision,
        ids.session,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        receipts.observed_at,
        execution.digest(),
    );
    (gateway.launch(&request, execution).expect("writable launch"), claim)
}

fn wait_started(control: &peritus_process::ProcessControl) {
    for _ in 0..100 {
        if control
            .read_events(ProcessCursor::after(0), 32)
            .iter()
            .any(|event| matches!(event.kind(), ProcessEventKind::Started { .. }))
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("process did not start");
}

struct MismatchProbe;

impl ProcessProbe for MismatchProbe {
    fn observe(
        &mut self,
        _identity: ProcessTreeIdentity,
    ) -> Result<ProbeObservation, peritus_process::ProcessError> {
        Ok(ProbeObservation::Mismatched)
    }

    fn terminate(
        &mut self,
        _identity: ProcessTreeIdentity,
    ) -> Result<(), peritus_process::ProcessError> {
        panic!("mismatched identity must never be signalled");
    }
}

fn output(
    events: &[peritus_process::ProcessEvent],
    stream: peritus_process::OutputStream,
) -> Vec<u8> {
    events
        .iter()
        .filter_map(|event| {
            matches!(event.kind(), ProcessEventKind::Output(value) if *value == stream)
                .then_some(event.data())
        })
        .flatten()
        .copied()
        .collect()
}
