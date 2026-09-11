//! Owned process cleanup, controls, and resource-limit regressions.

use super::*;

#[test]
fn post_spawn_control_failure_reaps_and_persists_terminal_failure() {
    let root = TestRoot::new();
    let ids = Ids::new(121);
    let execution = plan(&root, &ids, control_options(IoMode::Pipes)).expect("execution plan");
    let (owned, store) = launch(&root, &ids, execution);
    let control = owned.control();
    control.close_stdin().expect("queue input close");
    control.write_stdin(vec![1]).expect("queue post-close write");
    let terminal = owned.wait().expect("durable supervisor failure");
    assert_eq!(terminal.disposition(), TerminalDisposition::SupervisorFailed);
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
    assert_eq!(store.terminal_result(ids.process).expect("persisted terminal"), terminal);
}

#[test]
fn root_exit_cleans_descendant_before_waiting_for_pipe_eof() {
    let root = TestRoot::new();
    let ids = Ids::new(132);
    let execution = plan(
        &root,
        &ids,
        PlanOptions {
            arguments: vec!["pipe-holder".to_owned()],
            environment: Vec::new(),
            io: IoMode::Pipes,
            stdin: StdinPolicy::Closed,
            output_limit: 64,
            wall_timeout: Some(2_000),
            graceful: GracefulAction::Terminate,
            grace_millis: 50,
            process_count: 2,
            descendants: 1,
            workspace_access: WorkspaceAccess::ReadOnly,
            resize_allowed: true,
            environment_authority: None,
            resource_fidelity: peritus_sandbox::ResourceFidelity::Reference,
        },
    )
    .expect("execution plan");
    let began = Instant::now();
    let (owned, _) = launch(&root, &ids, execution);
    let terminal = owned.wait().expect("bounded descendant cleanup");
    let elapsed = began.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "descendant cleanup exceeded its hosted-runner allowance: {elapsed:?}"
    );
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
}

#[test]
fn output_limit_terminates_and_reaps_the_owned_process() {
    let root = TestRoot::new();
    let ids = Ids::new(138);
    let mut options = control_options(IoMode::Pipes);
    options.arguments = vec!["output".to_owned()];
    options.stdin = StdinPolicy::Closed;
    options.output_limit = 4;
    let execution = plan(&root, &ids, options).expect("output-limit plan");
    let (owned, store) = launch(&root, &ids, execution);

    let terminal = owned.wait().expect("bounded output terminal");

    assert_eq!(terminal.disposition(), TerminalDisposition::OutputLimit);
    assert_eq!(
        terminal.first_trigger().map(peritus_process::StopTrigger::reason),
        Some(CancellationReason::OutputLimit)
    );
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
    let stdout = terminal
        .output()
        .streams()
        .iter()
        .find(|stream| stream.stream() == peritus_process::OutputStream::Stdout)
        .expect("stdout observation");
    assert_eq!(stdout.observed(), 8);
    assert_eq!(stdout.retained(), 4);
    assert_eq!(stdout.dropped(), 4);
    assert_eq!(store.terminal_result(ids.process).expect("persisted terminal"), terminal);
}

#[test]
fn output_limit_reaps_a_descendant_holding_the_stream_open() {
    let root = TestRoot::new();
    let ids = Ids::new(141);
    let mut options = control_options(IoMode::Pipes);
    options.arguments = vec!["output-tree".to_owned()];
    options.stdin = StdinPolicy::Closed;
    options.output_limit = 4;
    options.process_count = 2;
    options.descendants = 1;
    let execution = plan(&root, &ids, options).expect("output-tree plan");
    let began = Instant::now();
    let (owned, store) = launch(&root, &ids, execution);

    let terminal = owned.wait().expect("bounded descendant terminal");

    assert!(began.elapsed() < Duration::from_secs(10));
    assert_eq!(terminal.disposition(), TerminalDisposition::OutputLimit);
    assert_eq!(
        terminal.first_trigger().map(peritus_process::StopTrigger::reason),
        Some(CancellationReason::OutputLimit)
    );
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
    assert_eq!(store.terminal_result(ids.process).expect("persisted terminal"), terminal);
}

#[test]
fn cancellation_after_output_preserves_first_reason_and_accounting() {
    let root = TestRoot::new();
    let ids = Ids::new(144);
    let mut options = control_options(IoMode::Pipes);
    options.arguments = vec!["output-control".to_owned()];
    options.stdin = StdinPolicy::Closed;
    let execution = plan(&root, &ids, options).expect("output-control plan");
    let (owned, store) = launch(&root, &ids, execution);
    let control = owned.control();
    for _ in 0..200 {
        if control.retained_stream_output(peritus_process::OutputStream::Stdout) == b"ready" {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        control.retained_stream_output(peritus_process::OutputStream::Stdout),
        b"ready",
        "output barrier was not reached"
    );

    control.cancel(CancellationReason::User).expect("cancel after output barrier");
    let terminal = owned.wait().expect("cancelled output terminal");

    assert_eq!(terminal.disposition(), TerminalDisposition::Cancelled);
    assert_eq!(
        terminal.first_trigger().map(peritus_process::StopTrigger::reason),
        Some(CancellationReason::User)
    );
    let stdout = terminal
        .output()
        .streams()
        .iter()
        .find(|stream| stream.stream() == peritus_process::OutputStream::Stdout)
        .expect("stdout accounting");
    assert_eq!((stdout.observed(), stdout.retained(), stdout.dropped()), (5, 5, 0));
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
    assert_eq!(store.terminal_result(ids.process).expect("persisted terminal"), terminal);
}

#[test]
fn resize_authority_rejects_pipe_and_denied_pty_without_stopping_owner() {
    let root = TestRoot::new();
    let pipe_ids = Ids::new(143);
    let pipe = plan(&root, &pipe_ids, control_options(IoMode::Pipes)).expect("pipe plan");
    let (pipe_owned, _) = launch(&root, &pipe_ids, pipe);
    reject_resize_then_cancel(pipe_owned);

    #[cfg(unix)]
    {
        let pty_root = TestRoot::new();
        let pty_ids = Ids::new(154);
        let io = IoMode::Pty(TerminalSize::new(24, 80, 0, 0).expect("PTY size"));
        let mut options = control_options(io);
        options.resize_allowed = false;
        let pty = plan(&pty_root, &pty_ids, options).expect("PTY plan");
        let (pty_owned, _) = launch(&pty_root, &pty_ids, pty);
        reject_resize_then_cancel(pty_owned);
    }
}

#[test]
fn inherited_environment_authority_cannot_bind_a_literal_value() {
    let root = TestRoot::new();
    let ids = Ids::new(165);
    let mut options = control_options(IoMode::Pipes);
    options.environment = vec![("PATH", "literal-replacement")];
    options.environment_authority = Some((vec!["PATH"], Vec::new()));
    let error = plan(&root, &ids, options).expect_err("literal use requires literal authority");
    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[cfg(unix)]
#[test]
fn process_count_overrun_is_observed_cancelled_and_classified() {
    let root = TestRoot::new();
    let ids = Ids::new(176);
    let execution = plan(
        &root,
        &ids,
        PlanOptions {
            arguments: vec!["tree".to_owned(), "2".to_owned()],
            environment: Vec::new(),
            io: IoMode::Pipes,
            stdin: StdinPolicy::Closed,
            output_limit: 64,
            wall_timeout: None,
            graceful: GracefulAction::Terminate,
            grace_millis: 50,
            process_count: 1,
            descendants: 2,
            workspace_access: WorkspaceAccess::ReadOnly,
            resize_allowed: true,
            environment_authority: None,
            resource_fidelity: peritus_sandbox::ResourceFidelity::Reference,
        },
    )
    .expect("execution plan");
    let (owned, _) = launch(&root, &ids, execution);
    let terminal = owned.wait().expect("resource terminal");
    assert_eq!(terminal.disposition(), TerminalDisposition::ResourceLimit);
    assert!(terminal.tree_cleanup_complete());
    assert!(terminal.support_tasks_joined());
    assert_eq!(terminal.resources().len(), 8);
    let processes = terminal
        .resources()
        .iter()
        .find(|value| value.dimension() == ProcessResourceDimension::ProcessCount)
        .expect("process-count observation");
    assert_eq!(processes.ceiling(), 1);
    assert_eq!(processes.fidelity(), ResourceFidelity::Sampled);
    let handles = terminal
        .resources()
        .iter()
        .find(|value| value.dimension() == ProcessResourceDimension::OpenHandles)
        .expect("open-handle observation");
    assert_eq!(handles.ceiling(), 32);
    #[cfg(target_os = "linux")]
    assert_eq!(handles.fidelity(), ResourceFidelity::Sampled);
    #[cfg(not(target_os = "linux"))]
    assert_eq!(handles.fidelity(), ResourceFidelity::Unsupported);
}
