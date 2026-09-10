use super::identity::bounded_key;

#[test]
fn command_restart_fixture() {
    use std::io::Write as _;

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
    output.write_all(format!("restart-effect:{marker}\n").as_bytes()).expect("report effect");
    output.flush().expect("flush effect observation");
}

#[test]
fn idempotency_keys_are_stable_and_fixed_width() {
    let first = bounded_key("provider-call-17");
    assert_eq!(first, bounded_key("provider-call-17"));
    assert_ne!(first, bounded_key("provider-call-18"));
    assert_eq!(first.len(), 64);
}

#[test]
fn commands_after_failed_start_and_runtime_reopen_execute_once_with_fresh_identities() {
    use super::{CommandRuntime, StartCommand};
    use peritus_process::ProcessStore;
    use peritus_types::RunId;
    use std::time::Duration;

    let executable = std::env::current_exe().expect("current test executable");
    let program = executable.to_str().expect("test executable path");

    for direct in [false, true] {
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
                        timeout: Duration::from_secs(10),
                        interactive: false,
                        rows: 24,
                        columns: 80,
                        idempotency_key: "failed-start",
                        environment: vec![],
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
                    timeout: Duration::from_secs(10),
                    interactive: false,
                    rows: 24,
                    columns: 80,
                    idempotency_key: marker,
                    environment: vec![(
                        "PERITUS_COMMAND_RESTART_FIXTURE".to_owned(),
                        marker.to_owned(),
                    )],
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
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("effects.txt")).expect("command effects"),
            "first\nsecond\n",
        );
    }
}
