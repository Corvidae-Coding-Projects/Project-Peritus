use super::*;
use std::io::Write as _;

#[test]
#[ignore = "owned subprocess fixture"]
fn owned_output_fixture() {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(b"EARLY RETAINED OUTPUT\n").unwrap();
    let chunk = vec![b'x'; 65_536];
    for _ in 0..144 {
        stdout.write_all(&chunk).unwrap();
    }
    stdout.write_all(b"\nFINAL RETAINED OUTPUT\n").unwrap();
    stdout.flush().unwrap();
    drop(stdout);
    loop {
        std::thread::park();
    }
}

fn options(root: &Path, state: &Path) -> crate::config::Options {
    crate::config::Options {
        port: 0,
        root: root.into(),
        assets: state.join("assets"),
        config_file: state.join("config.toml"),
        state_file: state.join("workspace.json"),
        daemon_config_root: state.join("daemon"),
        product_state_root: state.join("product"),
        daemon_config: None,
        endpoint: None,
        cli: std::env::current_exe().unwrap(),
    }
}

#[test]
fn owned_command_retains_large_output_cancels_and_reopens_without_replay() {
    exercise_retained_output(false);
}

#[test]
fn owned_console_retains_large_terminal_output_across_reopen() {
    exercise_retained_output(true);
}

fn exercise_retained_output(interactive: bool) {
    let stream = if interactive { OutputStream::Terminal } else { OutputStream::Stdout };
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let app = App::open(options(workspace.path(), state.path()), 0).unwrap();
    let args = vec![
        "--ignored".into(),
        "--exact".into(),
        "processes::tests::owned_output_fixture".into(),
        "--nocapture".into(),
    ];
    let command = ManagedCommand::start(
        &app,
        "large-output",
        workspace.path(),
        std::env::current_exe().unwrap().to_string_lossy().into_owned(),
        args.clone(),
        interactive,
    )
    .unwrap();
    let wrong_process = PreviewOwner::new(
        command.owner.source_run(),
        command.owner.execution_run(),
        command.owner.action(),
        ProcessId::new([0x7e; 16]).unwrap(),
    );
    assert!(command.runtime.observe_retained_preview(wrong_process).is_err());
    assert!(command.runtime.retained_preview_range(wrong_process, stream, 0, 64).is_err());
    assert!(
        ManagedCommand::start(&app, "large-output", workspace.path(), "unused".into(), args, false)
            .is_err()
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let observed = command.observe().unwrap();
        assert_eq!(observed.state(), PreviewProcessState::Running);
        let (total, _) = command.output(stream, 0, 64).unwrap();
        if total > 9 * 1024 * 1024 {
            let (_, tail) = command.output(stream, total.saturating_sub(64), 64).unwrap();
            if String::from_utf8_lossy(&tail).contains("FINAL RETAINED OUTPUT") {
                break;
            }
        }
        assert!(std::time::Instant::now() < deadline, "fixture output deadline");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    command.cancel().unwrap();
    while !settled(command.observe().unwrap().state()) {
        assert!(std::time::Instant::now() < deadline, "cancellation deadline");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    command.cancel().unwrap();
    let expected = complete_output(&command, stream).unwrap();
    assert!(expected.contains("EARLY RETAINED OUTPUT"));
    assert!(expected.contains("FINAL RETAINED OUTPUT"));
    reap(&app).unwrap();
    assert!(app.processes.lock().unwrap().is_empty());
    drop(command);
    drop(app);
    let reopened = App::open(options(workspace.path(), state.path()), 0).unwrap();
    let command = ManagedCommand::get(&reopened, "large-output").unwrap();
    assert!(settled(command.observe().unwrap().state()));
    assert_eq!(complete_output(&command, stream).unwrap(), expected);
    assert!(!command.input_available());
    assert!(command.input(b"not replayed").is_err());
}

#[cfg(unix)]
#[test]
fn failed_durable_registration_never_starts_the_native_effect() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let app = App::open(options(workspace.path(), state.path()), 0).unwrap();
    let marker = workspace.path().join("must-not-exist");
    std::fs::remove_file(&app.options.state_file).unwrap();
    std::fs::create_dir(&app.options.state_file).unwrap();
    let result = ManagedCommand::start(
        &app,
        "registration-failure",
        workspace.path(),
        "sh".into(),
        vec!["-c".into(), "printf effect > must-not-exist".into()],
        false,
    );
    assert!(result.is_err());
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!marker.exists());
    assert!(app.snapshot().unwrap().commands.is_empty());
}

#[test]
fn direct_local_user_folder_is_explicit_and_does_not_weaken_managed_store_isolation() {
    let workspace = tempfile::tempdir().unwrap();
    let state = workspace.path().join("application-state");
    std::fs::create_dir(&state).unwrap();
    assert!(ProcessStore::open(state.join("protected-processes"), workspace.path()).is_err());
    let app = App::open(options(workspace.path(), &state), 0).unwrap();
    assert!(open_runtime(&app, workspace.path(), scope("direct").unwrap()).is_ok());
    assert!(ProcessStore::open_direct(workspace.path(), &state).is_err());
}

#[test]
fn exited_consoles_are_reaped_without_a_count_gate_and_remain_discoverable() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let app = App::open(options(workspace.path(), state.path()), 0).unwrap();
    let project = app.snapshot().unwrap().projects[0].id.clone();
    for index in 0..26 {
        crate::terminal::Terminal::start(
            &app,
            crate::consoles::Console {
                id: format!("console-{index}"),
                project: project.clone(),
                session: None,
                title: format!("Console {index}"),
                suggestion: String::new(),
            },
            vec!["--list".into()],
        )
        .unwrap();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        reap(&app).unwrap();
        if app.processes.lock().unwrap().is_empty() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "ended console retirement deadline");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let consoles = crate::consoles::list(&app).unwrap();
    assert_eq!(consoles.as_array().unwrap().len(), 26);
    assert!(consoles.as_array().unwrap().iter().all(|console| console["ended"] == true));
    crate::consoles::close(&app, "console-0").unwrap();
    assert_eq!(crate::consoles::list(&app).unwrap().as_array().unwrap().len(), 25);
}
