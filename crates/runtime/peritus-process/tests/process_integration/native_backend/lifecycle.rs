//! Successful native lifecycle integration cases.

use super::*;

#[test]
fn exact_restricted_authority_runs_through_the_helper_and_releases_the_session() {
    let root = TestRoot::new();
    let ids = Ids::new(201);
    let (execution, sandbox, admission) = native_plan(
        &root,
        &ids,
        options(vec!["pipe".to_owned()], StdinPolicy::bounded(10, 10).expect("stdin policy")),
    )
    .expect("native execution plan");
    let action = intent(&ids, &execution);
    let mut journal = open_journal(&root);
    let receipts =
        commit_authority(&mut journal, &ids, &action, execution.resource_policy().wall_millis());
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).expect("store"),
    );
    let request = request(&ids, &execution, &action, &receipts);
    let probe = LifecycleProbe::new();
    let owned = gateway
        .launch_with_backend(
            &request,
            execution,
            &sandbox,
            &admission,
            TestBackend::admitted(&admission, probe.clone()),
        )
        .expect("native launch");
    let control = owned.control();
    control.write_stdin(b"pipe-input".to_vec()).expect("stdin");
    control.close_stdin().expect("close stdin");
    let terminal = owned.wait().expect("terminal");
    assert_eq!(terminal.disposition(), TerminalDisposition::Exited);
    assert!(terminal.tree_cleanup_complete());
    assert_eq!(
        probe.events(),
        vec![
            ObservationKind::Prepared,
            ObservationKind::Activated,
            ObservationKind::Terminated,
            ObservationKind::Released,
        ]
    );
}

#[cfg(unix)]
#[test]
fn restricted_pty_keeps_binary_helper_protocol_off_the_terminal_stream() {
    let root = TestRoot::new();
    let ids = Ids::new(206);
    let mut plan_options =
        options(vec!["pty".to_owned()], StdinPolicy::bounded(9, 9).expect("stdin policy"));
    plan_options.io = IoMode::Pty(TerminalSize::new(24, 80, 0, 0).expect("terminal size"));
    let (execution, sandbox, admission) =
        native_plan(&root, &ids, plan_options).expect("native PTY execution plan");
    let action = intent(&ids, &execution);
    let mut journal = open_journal(&root);
    let receipts =
        commit_authority(&mut journal, &ids, &action, execution.resource_policy().wall_millis());
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).expect("store"),
    );
    let request = request(&ids, &execution, &action, &receipts);
    let probe = LifecycleProbe::new();
    let owned = gateway
        .launch_with_backend(
            &request,
            execution,
            &sandbox,
            &admission,
            TestBackend::admitted(&admission, probe.clone()),
        )
        .expect("native PTY launch");
    let control = owned.control();
    control.write_stdin(b"pty-input".to_vec()).expect("PTY stdin");
    let terminal = owned.wait().expect("PTY terminal");
    assert_eq!(terminal.disposition(), TerminalDisposition::Exited);
    assert_eq!(
        probe.events(),
        vec![
            ObservationKind::Prepared,
            ObservationKind::Activated,
            ObservationKind::Terminated,
            ObservationKind::Released,
        ]
    );
}

#[cfg(unix)]
#[test]
fn restricted_helper_receives_the_exact_protected_anonymous_handle() {
    let root = TestRoot::new();
    let ids = Ids::new(207);
    let (execution, sandbox, admission) =
        native_plan(&root, &ids, options(Vec::new(), StdinPolicy::Closed))
            .expect("native execution plan");
    let action = intent(&ids, &execution);
    let mut journal = open_journal(&root);
    let receipts =
        commit_authority(&mut journal, &ids, &action, execution.resource_policy().wall_millis());
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).expect("store"),
    );
    let request = request(&ids, &execution, &action, &receipts);
    let probe = LifecycleProbe::new();
    let mut backend = TestBackend::admitted(&admission, probe.clone());
    backend.protected_payload = true;
    let terminal = gateway
        .launch_with_backend(&request, execution, &sandbox, &admission, backend)
        .expect("native protected-handle launch")
        .wait()
        .expect("terminal");
    assert_eq!(terminal.disposition(), TerminalDisposition::Exited);
    assert!(terminal.tree_cleanup_complete());
    assert_eq!(
        probe.events(),
        vec![
            ObservationKind::Prepared,
            ObservationKind::Activated,
            ObservationKind::Terminated,
            ObservationKind::Released,
        ]
    );
}

#[test]
fn native_supervisor_resource_poll_uses_the_owned_resource_limit_path() {
    let root = TestRoot::new();
    let ids = Ids::new(208);
    let (execution, sandbox, admission) =
        native_plan(&root, &ids, options(vec!["control".to_owned()], StdinPolicy::Closed))
            .expect("native execution plan");
    let action = intent(&ids, &execution);
    let mut journal = open_journal(&root);
    let receipts =
        commit_authority(&mut journal, &ids, &action, execution.resource_policy().wall_millis());
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).expect("store"),
    );
    let request = request(&ids, &execution, &action, &receipts);
    let probe = LifecycleProbe::new();
    let mut backend = TestBackend::admitted(&admission, probe.clone());
    backend.limit_on_poll = true;
    let terminal = gateway
        .launch_with_backend(&request, execution, &sandbox, &admission, backend)
        .expect("native launch")
        .wait()
        .expect("terminal");
    assert_eq!(terminal.disposition(), TerminalDisposition::ResourceLimit);
    assert_eq!(
        probe.events(),
        vec![
            ObservationKind::Prepared,
            ObservationKind::Activated,
            ObservationKind::ResourceCharged,
            ObservationKind::Cancellation,
            ObservationKind::Terminated,
            ObservationKind::Released,
        ]
    );
}
