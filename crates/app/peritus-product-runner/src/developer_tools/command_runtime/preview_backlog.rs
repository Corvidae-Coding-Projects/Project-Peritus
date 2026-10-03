//! An exited preview must settle on the first observation even after unattended output.
use super::*;
use peritus_process::ProcessCursor;
use peritus_types::RunId;
use std::{io::Write as _, thread, time::Instant};

#[test]
#[ignore = "owned subprocess fixture"]
fn noisy_fixture() {
    for index in 0..300 {
        println!("PROGRESS {index}");
        std::io::stdout().flush().expect("flush");
        let expected = index.to_string();
        while std::fs::read_to_string("progress-ack").ok().as_deref() != Some(&expected) {
            thread::sleep(Duration::from_millis(1));
        }
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
        Duration::from_mins(1),
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
    let mut cursor = ProcessCursor::after(0);
    let mut output = String::new();
    for index in 0..300 {
        let expected = format!("PROGRESS {index}\n");
        while !output.contains(&expected) {
            for event in control.read_events(cursor, 1024) {
                cursor = ProcessCursor::after(event.sequence());
                output.push_str(&String::from_utf8_lossy(event.data()));
            }
            assert!(began.elapsed() < Duration::from_secs(45), "fixture output must arrive");
            thread::sleep(Duration::from_millis(1));
        }
        // Each acknowledgement follows an observed C2 event. This guarantees more than one
        // observation page without depending on platform timer resolution or pipe coalescing.
        std::fs::write(workspace.path().join("progress-ack"), index.to_string())
            .expect("next output event");
    }
    while control.terminal_result().is_none() {
        assert!(began.elapsed() < Duration::from_secs(45), "native exit must complete");
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
