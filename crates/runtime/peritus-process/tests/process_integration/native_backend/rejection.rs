//! Native binding, preparation, and cleanup rejection cases.

use super::*;

#[test]
fn descriptor_drift_never_calls_prepare_or_consumes_authority() {
    let root = TestRoot::new();
    let ids = Ids::new(211);
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
    let drift_probe = LifecycleProbe::new();
    let drift = TestBackend {
        descriptor: BackendDescriptor::new(
            BackendName::new("different-native-test").expect("name"),
            BackendVersion::new("1").expect("version"),
            BackendKind::Native,
            native_path_semantics(),
            ResourceFidelity::Hard,
            FeatureSet::all(),
        ),
        probe: drift_probe.clone(),
        helper: native_helper_binary(),
        fail_prepare: false,
        fail_release: false,
        invalidate_prepared_observation: false,
        limit_on_poll: false,
        protected_payload: false,
    };
    let error = launch_error(gateway.launch_with_backend(
        &request,
        execution.clone(),
        &sandbox,
        &admission,
        drift,
    ));
    assert_eq!(error.code(), ErrorCode::PlanMismatch);
    assert_eq!(drift_probe.prepare_calls(), 0);

    let exact_probe = LifecycleProbe::new();
    let terminal = gateway
        .launch_with_backend(
            &request,
            execution,
            &sandbox,
            &admission,
            TestBackend::admitted(&admission, exact_probe.clone()),
        )
        .expect("authority was not consumed")
        .wait()
        .expect("terminal");
    assert_eq!(terminal.disposition(), TerminalDisposition::Exited);
    assert_eq!(exact_probe.prepare_calls(), 1);
}

#[test]
fn preparation_failure_is_durable_and_cannot_be_replayed() {
    let root = TestRoot::new();
    let ids = Ids::new(221);
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
    backend.fail_prepare = true;
    let error = launch_error(gateway.launch_with_backend(
        &request,
        execution.clone(),
        &sandbox,
        &admission,
        backend,
    ));
    assert_eq!(error.code(), ErrorCode::Unsupported);
    assert_eq!(probe.prepare_calls(), 1);
    let terminal = gateway.store().terminal_result(ids.process).expect("durable failure terminal");
    assert!(!terminal.support_tasks_joined());

    let retry = launch_error(gateway.launch_with_backend(
        &request,
        execution,
        &sandbox,
        &admission,
        TestBackend::admitted(&admission, LifecycleProbe::new()),
    ));
    assert_eq!(retry.code(), ErrorCode::ReceiptReused);
}

#[test]
fn prepared_session_is_released_before_spawn_failure_is_published() {
    let root = TestRoot::new();
    let ids = Ids::new(224);
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
    backend.helper = root.workspace().join("missing-native-helper").to_string_lossy().into_owned();

    let terminal = gateway
        .launch_with_backend(&request, execution, &sandbox, &admission, backend)
        .expect("owner starts before the asynchronous helper launch")
        .wait()
        .expect("durable spawn failure");

    assert_eq!(terminal.disposition(), TerminalDisposition::SpawnFailed);
    assert!(terminal.support_tasks_joined());
    assert_eq!(probe.events(), vec![ObservationKind::Prepared, ObservationKind::Released]);
}

#[test]
fn invalid_prepared_session_is_released_and_cannot_be_replayed() {
    let root = TestRoot::new();
    let ids = Ids::new(222);
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
    backend.invalidate_prepared_observation = true;
    let error = launch_error(gateway.launch_with_backend(
        &request,
        execution.clone(),
        &sandbox,
        &admission,
        backend,
    ));
    assert_eq!(error.code(), ErrorCode::PlanMismatch);
    assert_eq!(probe.events(), vec![ObservationKind::Prepared, ObservationKind::Released]);

    let retry = launch_error(gateway.launch_with_backend(
        &request,
        execution,
        &sandbox,
        &admission,
        TestBackend::admitted(&admission, LifecycleProbe::new()),
    ));
    assert_eq!(retry.code(), ErrorCode::ReceiptReused);
}

#[test]
fn invalid_prepared_session_cleanup_failure_remains_durably_incomplete() {
    let root = TestRoot::new();
    let ids = Ids::new(223);
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
    let mut backend = TestBackend::admitted(&admission, LifecycleProbe::new());
    backend.invalidate_prepared_observation = true;
    backend.fail_release = true;
    let error = launch_error(
        gateway.launch_with_backend(&request, execution, &sandbox, &admission, backend),
    );
    assert_eq!(error.code(), ErrorCode::Supervisor);
    let terminal = gateway.store().terminal_result(ids.process).expect("durable terminal");
    assert!(terminal.tree_cleanup_complete());
    assert!(!terminal.support_tasks_joined());
}

fn launch_error(result: Result<OwnedProcess, ProcessError>) -> ProcessError {
    match result {
        Ok(process) => {
            drop(process);
            panic!("native launch unexpectedly succeeded");
        }
        Err(error) => error,
    }
}

const fn native_path_semantics() -> PathSemantics {
    #[cfg(unix)]
    {
        PathSemantics::UnixNative
    }
    #[cfg(windows)]
    {
        PathSemantics::WindowsNative
    }
}
