//! Cancellation and signal lifecycle coverage.

use super::*;

#[test]
fn cancellation_publishes_one_terminal_owned_tree_result() {
    let root = TestRoot::new();
    let ids = Ids::new(30);
    let plan = plan(
        &root,
        &ids,
        PlanOptions {
            arguments: vec!["control".to_owned()],
            environment: Vec::new(),
            io: IoMode::Pty(TerminalSize::new(24, 80, 0, 0).expect("terminal size")),
            stdin: StdinPolicy::bounded(16, 16).expect("stdin policy"),
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
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).expect("process store"),
    );
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
    control.cancel(CancellationReason::User).expect("cancel");
    let terminal = owned.wait().expect("terminal result");
    assert_eq!(terminal.disposition(), TerminalDisposition::Cancelled);
    assert_eq!(control.terminal_result(), Some(terminal));
}

#[cfg(unix)]
#[test]
fn interrupt_signal_is_ordered_and_does_not_become_cancellation() {
    let root = TestRoot::new();
    let ids = Ids::new(35);
    let plan = plan(
        &root,
        &ids,
        PlanOptions {
            arguments: vec!["control".to_owned()],
            environment: Vec::new(),
            io: IoMode::Pty(TerminalSize::new(24, 80, 0, 0).expect("terminal size")),
            stdin: StdinPolicy::bounded(16, 16).expect("stdin policy"),
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
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).expect("process store"),
    );
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
    wait_started(&control);
    control.signal(ProcessSignal::Interrupt).expect("interrupt");
    let terminal = owned.wait().expect("terminal result");
    assert_eq!(terminal.disposition(), TerminalDisposition::Signalled);
    assert!(terminal.first_trigger().is_none());
    let events = control.read_events(ProcessCursor::after(0), 256);
    let signal = events
        .iter()
        .position(|event| {
            matches!(event.kind(), ProcessEventKind::Signalled(ProcessSignal::Interrupt))
        })
        .expect("ordered signal event");
    let exit = events
        .iter()
        .position(|event| matches!(event.kind(), ProcessEventKind::OsExit))
        .expect("ordered exit event");
    assert!(signal < exit);
    assert!(
        !events.iter().any(|event| { matches!(event.kind(), ProcessEventKind::Cancellation(_)) })
    );
}
