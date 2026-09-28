//! An exited preview must settle on the first observation even after unattended output.
use super::*;
use peritus_process::ProcessCursor;
use peritus_types::RunId;
use std::{io::Write as _, thread, time::Instant};

#[test]
#[ignore = "owned subprocess fixture"]
fn noisy_fixture() {
    for _ in 0..700 {
        println!("{}", "x".repeat(2048));
        std::io::stdout().flush().expect("flush");
        thread::sleep(Duration::from_millis(2));
    }
    println!("NATURAL_EXIT");
}

#[test]
fn terminal_preview_does_not_wait_for_progress_backlog_drain() {
    let workspace = tempfile::tempdir().expect("workspace");
    let runtime =
        CommandRuntime::open_for_test(workspace.path(), RunId::new([94; 16]).expect("run"));
    let command = PreviewCommand::new(
        std::env::current_exe().expect("executable").to_string_lossy().into_owned(),
        vec![
            "--ignored".to_owned(),
            "--exact".to_owned(),
            "developer_tools::command_runtime::preview::backlog::noisy_fixture".to_owned(),
            "--nocapture".to_owned(),
        ],
        workspace.path().to_path_buf(),
        Duration::from_secs(15),
        false,
        24,
        80,
        "terminal-backlog".to_owned(),
        Vec::new(),
    )
    .expect("command");
    let launch = runtime.launch_preview(&command).expect("launch");
    let control = runtime
        .inner
        .state
        .lock()
        .expect("state")
        .active
        .get(&launch.handle)
        .expect("active")
        .control
        .clone()
        .expect("control");
    let began = Instant::now();
    while control.terminal_result().is_none() {
        assert!(began.elapsed() < Duration::from_secs(10), "native exit must complete");
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        control.read_events(ProcessCursor::after(0), 1024).len() > 256,
        "fixture must exceed an observation page"
    );
    let observed = runtime.observe_preview(&launch).expect("observe native exit");
    assert_eq!(
        observed.state(),
        PreviewProcessState::Succeeded,
        "terminal state must not wait for historical progress pages"
    );
    assert!(observed.stdout().contains("NATURAL_EXIT"));
}
