use super::{CommandRuntime, StartCommand, identity::bounded_key};
use peritus_process::ProcessStore;
use peritus_types::RunId;
use std::io::Write as _;
use std::time::{Duration, Instant};

#[test]
fn command_restart_fixture() {
    let Ok(marker) = std::env::var("PERITUS_COMMAND_RESTART_FIXTURE") else { return };
    assert!(matches!(marker.as_str(), "first" | "second"));
    let mut effects = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("effects.txt")
        .expect("open command effects");
    effects.write_all(format!("{marker}\n").as_bytes()).expect("record command effect");
    effects.flush().expect("flush command effect");
    let mut output = std::io::stdout();
    // Start a separate line even when the serial test harness leaves its prefix open.
    output.write_all(format!("\nrestart-effect:{marker}\n").as_bytes()).expect("report effect");
    output.flush().expect("flush effect observation");
}

#[test]
fn command_poll_attachment_fixture() {
    let Ok(barrier_path) = std::env::var("PERITUS_COMMAND_POLL_ATTACHMENT_BARRIER") else {
        return;
    };
    let barrier = std::path::PathBuf::from(barrier_path);
    std::fs::create_dir_all(&barrier).expect("create command barrier");
    std::fs::write(barrier.join("started"), b"started\n").expect("signal command started");
    let deadline = Instant::now() + Duration::from_mins(1);
    while !barrier.join("release").exists() {
        assert!(Instant::now() < deadline, "parent did not release command barrier");
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut effects = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(barrier.join("effects.txt"))
        .expect("open command effect record");
    effects.write_all(b"effect\n").expect("record command effect");
    effects.flush().expect("flush command effect");
    let mut output = std::io::stdout();
    output.write_all(b"\nretained-command-output\n").expect("write command output");
    output.flush().expect("flush command output");
}

#[test]
fn no_deadline_execution_fixture() {
    let Ok(marker) = std::env::var("PERITUS_NO_DEADLINE_FIXTURE_MARKER") else { return };
    std::thread::sleep(Duration::from_millis(50));
    std::fs::write(marker, b"completed without wall deadline\n").expect("write fixture marker");
    println!("no-deadline-fixture-completed");
}

#[test]
fn idempotency_keys_are_stable_and_fixed_width() {
    let first = bounded_key("provider-call-17");
    assert_eq!(first, bounded_key("provider-call-17"));
    assert_ne!(first, bounded_key("provider-call-18"));
    assert_eq!(first.len(), 64);
}

#[test]
fn command_without_wall_deadline_has_complete_execution_authority() {
    let executable = std::env::current_exe().expect("current executable");
    let workspace = tempfile::tempdir().expect("workspace");
    let runtime = CommandRuntime::open_for_test(
        workspace.path(),
        RunId::new([19; 16]).expect("run identity"),
    );
    let marker = workspace.path().join("no-deadline-completed.txt");
    let result = runtime
        .run(StartCommand {
            program: executable.to_str().expect("executable path"),
            arguments: &[
                "--exact".to_owned(),
                "developer_tools::command_runtime::tests::no_deadline_execution_fixture".to_owned(),
                "--nocapture".to_owned(),
            ],
            cwd: workspace.path(),
            timeout: None,
            interactive: false,
            rows: 24,
            columns: 80,
            idempotency_key: "without-deadline",
            environment: vec![(
                "PERITUS_NO_DEADLINE_FIXTURE_MARKER".to_owned(),
                marker.to_string_lossy().into_owned(),
            )],
            owner_registered: None,
        })
        .expect("run command without a wall deadline");
    assert_eq!(result["success"].as_bool(), Some(true), "{result}");
    assert_eq!(
        std::fs::read(&marker).expect("no-deadline fixture ran"),
        b"completed without wall deadline\n"
    );
    assert!(
        result["stdout"]
            .as_str()
            .is_some_and(|output| output.contains("no-deadline-fixture-completed"))
    );
}

#[test]
fn repeated_receipt_attachment_keeps_active_router_owner_pollable() {
    let executable = std::env::current_exe().expect("current test executable");
    let program = executable.to_str().expect("test executable path");
    let workspace = tempfile::tempdir().expect("workspace");
    let barrier = workspace.path().join("poll-barrier");
    let run = RunId::new([18; 16]).expect("run identity");
    let runtime = CommandRuntime::open_for_test(workspace.path(), run);
    let arguments = vec![
        "--exact".to_owned(),
        "developer_tools::command_runtime::tests::command_poll_attachment_fixture".to_owned(),
        "--nocapture".to_owned(),
    ];
    let mut owner = None;
    let started = {
        let mut register_owner = |registered| {
            owner = Some(registered);
            Ok(())
        };
        runtime
            .start(StartCommand {
                program,
                arguments: &arguments,
                cwd: workspace.path(),
                timeout: Some(Duration::from_secs(10)),
                interactive: false,
                rows: 24,
                columns: 80,
                idempotency_key: "poll-attachment",
                environment: vec![(
                    "PERITUS_COMMAND_POLL_ATTACHMENT_BARRIER".to_owned(),
                    barrier.to_string_lossy().into_owned(),
                )],
                owner_registered: Some(&mut register_owner),
            })
            .expect("start command with durable owner registration")
    };
    let owner = owner.expect("registered native command owner");
    let handle = started["handle"].as_str().expect("command handle");
    wait_for_file(&barrier.join("started"));
    runtime.attach_native_owner(owner).expect("attach exact receipt owner");
    let live = runtime.poll(handle).expect("poll active router owner");
    assert_eq!(live["state"], "running");
    std::fs::write(barrier.join("release"), b"release\n").expect("release command barrier");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        runtime.attach_native_owner(owner).expect("attach exact receipt owner");
        let observation = runtime.poll(handle).expect("poll active router owner");
        if observation["state"] == "completed" {
            let state = runtime.inner.state.lock().expect("command runtime state");
            assert!(!state.active.contains_key(handle));
            assert!(state.terminal.contains_key(handle));
            drop(state);
            return;
        }
        assert!(Instant::now() < deadline, "attached command did not complete after release");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn live_native_command_survives_runtime_reopen_and_recovers_its_output() {
    let executable = std::env::current_exe().expect("current test executable");
    let program = executable.to_str().expect("test executable path");
    let workspace = tempfile::tempdir().expect("workspace");
    let state = tempfile::tempdir().expect("command state");
    let barrier = workspace.path().join("recovery-barrier");
    let run = RunId::new([19; 16]).expect("run identity");
    let processes = ProcessStore::open(state.path().join("processes"), workspace.path())
        .expect("daemon-owned process store");
    let open_runtime = || {
        CommandRuntime::open(state.path().join("router"), workspace.path(), run, processes.clone())
            .expect("open command runtime")
    };
    let runtime = open_runtime();
    let arguments = vec![
        "--exact".to_owned(),
        "developer_tools::command_runtime::tests::command_poll_attachment_fixture".to_owned(),
        "--nocapture".to_owned(),
    ];
    let mut owner = None;
    let started = {
        let mut register_owner = |registered| {
            owner = Some(registered);
            Ok(())
        };
        runtime
            .start(StartCommand {
                program,
                arguments: &arguments,
                cwd: workspace.path(),
                timeout: Some(Duration::from_secs(45)),
                interactive: false,
                rows: 24,
                columns: 80,
                idempotency_key: "runtime-reopen-live-command",
                environment: vec![(
                    "PERITUS_COMMAND_POLL_ATTACHMENT_BARRIER".to_owned(),
                    barrier.to_string_lossy().into_owned(),
                )],
                owner_registered: Some(&mut register_owner),
            })
            .expect("start command with durable owner registration")
    };
    let owner = owner.expect("registered native command owner");
    let handle = started["handle"].as_str().expect("command handle").to_owned();
    wait_for_file(&barrier.join("started"));
    drop(runtime);

    let runtime = open_runtime();
    runtime.attach_native_owner(owner).expect("reattach exact receipt owner");
    let live = runtime.poll(&handle).expect("poll live command after runtime reopen");
    assert_eq!(live["state"], "running", "ordinary polling must preserve the live process");
    std::fs::write(barrier.join("release"), b"release\n").expect("release command barrier");

    let deadline = Instant::now() + Duration::from_secs(30);
    let terminal = loop {
        runtime.attach_native_owner(owner).expect("reattach exact receipt owner");
        let observation = runtime.poll(&handle).expect("poll recovered command");
        if observation["state"] == "completed" {
            break observation;
        }
        assert!(Instant::now() < deadline, "reopened command did not complete after release");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        terminal["stdout"]
            .as_str()
            .is_some_and(|output| { output.lines().any(|line| line == "retained-command-output") })
    );
    let recovered = runtime.recover(&handle).expect("recover terminal command output");
    assert_eq!(recovered["state"], "completed");
    assert!(
        recovered["stdout"]
            .as_str()
            .is_some_and(|output| { output.lines().any(|line| line == "retained-command-output") })
    );
    assert_eq!(
        std::fs::read_to_string(barrier.join("effects.txt")).expect("command effects"),
        "effect\n",
    );
}

fn wait_for_file(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(Instant::now() < deadline, "timed out waiting for {}", path.display());
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn commands_after_failed_start_and_runtime_reopen_execute_once_with_fresh_identities() {
    exercise_restarted_runtime(false);
    exercise_restarted_runtime(true);
}

fn exercise_restarted_runtime(direct: bool) {
    let executable = std::env::current_exe().expect("current test executable");
    let program = executable.to_str().expect("test executable path");
    let workspace = tempfile::tempdir().expect("workspace");
    let state = tempfile::tempdir().expect("command state");
    let run = RunId::new([17; 16]).expect("run identity");
    let processes = ProcessStore::open(state.path().join("processes"), workspace.path())
        .expect("process store");
    let mut handles = Vec::new();

    let open_runtime = || {
        if direct {
            CommandRuntime::open_direct(
                state.path().join("router"),
                workspace.path(),
                run,
                processes.clone(),
            )
        } else {
            CommandRuntime::open(
                state.path().join("router"),
                workspace.path(),
                run,
                processes.clone(),
            )
        }
        .expect("open runtime with the same run and durable state")
    };
    {
        let runtime = open_runtime();
        let missing = workspace.path().join("missing-command-executable");
        assert!(
            runtime
                .run(StartCommand {
                    program: missing.to_str().expect("missing executable path"),
                    arguments: &[],
                    cwd: workspace.path(),
                    timeout: Some(Duration::from_secs(10)),
                    interactive: false,
                    rows: 24,
                    columns: 80,
                    idempotency_key: "failed-start",
                    environment: vec![],
                    owner_registered: None,
                })
                .is_err()
        );
    }
    assert!(!workspace.path().join("effects.txt").exists());

    for (marker, ordinal) in [("first", 2), ("second", 3)] {
        let runtime = open_runtime();
        let arguments = vec![
            "--exact".to_owned(),
            "developer_tools::command_runtime::tests::command_restart_fixture".to_owned(),
            "--nocapture".to_owned(),
        ];
        let result = runtime
            .run(StartCommand {
                program,
                arguments: &arguments,
                cwd: workspace.path(),
                timeout: Some(Duration::from_secs(10)),
                interactive: false,
                rows: 24,
                columns: 80,
                idempotency_key: marker,
                environment: vec![(
                    "PERITUS_COMMAND_RESTART_FIXTURE".to_owned(),
                    marker.to_owned(),
                )],
                owner_registered: None,
            })
            .expect("execute command across runtime restart");
        assert_eq!(result["success"].as_bool(), Some(true), "{result}");
        assert_eq!(result["exit_code"].as_i64(), Some(0), "{result}");
        let expected_output = format!("restart-effect:{marker}");
        assert!(
            result["stdout"]
                .as_str()
                .is_some_and(|output| output.lines().any(|line| line == expected_output)),
            "{result}"
        );
        handles.push(result["handle"].as_str().expect("command handle").to_owned());
        let action = peritus_types::ActionId::new(super::contract::id(run, ordinal, "action"))
            .expect("expected fresh action");
        assert_eq!(handles.last(), Some(&super::identity::action_hex(action)));
        drop(runtime);
    }

    assert_ne!(handles[0], handles[1]);
    let recovered = open_runtime()
        .recover(&handles[0])
        .expect("recover completed command projection after runtime reopen");
    assert_eq!(recovered["state"], "completed");
    assert_eq!(recovered["success"], true);
    assert!(
        recovered["stdout"].as_str().is_some_and(|output| output.contains("restart-effect:first"))
    );
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("effects.txt")).expect("command effects"),
        "first\nsecond\n",
    );
}
