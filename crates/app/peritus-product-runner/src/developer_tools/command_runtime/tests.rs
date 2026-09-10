use super::identity::bounded_key;

#[cfg(unix)]
#[test]
#[ignore = "subprocess fixture invoked by the runtime restart test"]
fn command_restart_fixture() {
    use std::io::Write as _;

    let marker = std::fs::read_to_string("restart-marker.txt").expect("subprocess input");
    assert!(matches!(marker.as_str(), "first" | "second"));
    let mut effects = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("effects.txt")
        .expect("open command effects");
    writeln!(effects, "{marker}").expect("record command effect");
    effects.flush().expect("flush command effect");
    println!("restart-effect:{marker}");
}

#[test]
fn idempotency_keys_are_stable_and_fixed_width() {
    let first = bounded_key("provider-call-17");
    assert_eq!(first, bounded_key("provider-call-17"));
    assert_ne!(first, bounded_key("provider-call-18"));
    assert_eq!(first.len(), 64);
}

#[cfg(unix)]
#[test]
fn commands_after_runtime_reopen_have_distinct_identities_and_execute_once() {
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

        for marker in ["first", "second"] {
            let runtime = if direct {
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
            .expect("open runtime with the same run and durable state");
            std::fs::write(workspace.path().join("restart-marker.txt"), marker)
                .expect("subprocess input");
            let arguments = vec![
                "--exact".to_owned(),
                "developer_tools::command_runtime::tests::command_restart_fixture".to_owned(),
                "--ignored".to_owned(),
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
                    environment: Vec::new(),
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
            drop(runtime);
        }

        assert_ne!(handles[0], handles[1]);
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("effects.txt")).expect("command effects"),
            "first\nsecond\n",
        );
    }
}
