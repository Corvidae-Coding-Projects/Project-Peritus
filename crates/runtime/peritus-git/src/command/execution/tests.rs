//! Real subprocess cancellation verifies tree and pipe ownership, not only a flag.

#[cfg(unix)]
#[test]
fn cancellation_joins_a_running_git_helper_and_inherited_pipe_readers() {
    use super::{CommandAccess, run};
    use crate::{ErrorKind, GitCancellation, Operation};
    use std::{
        process::{Command, Stdio},
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    let root = tempfile::tempdir().expect("root");
    let marker = root.path().join("helper-ready");
    let mut command = Command::new("sh");
    command
        .args(["-c", "sleep 300 & printf '%s' \"$!\" > \"$1\"; printf ready; wait", "git-helper"])
        .arg(&marker)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let cancellation = GitCancellation::new();
    let worker_cancellation = cancellation.clone();
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result =
            run(command, None, &worker_cancellation, CommandAccess::Read, Operation::Status);
        sender.send(result).expect("owner remains present");
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() {
        if Instant::now() >= deadline {
            cancellation.cancel();
            worker.join().expect("worker cleanup");
            panic!("Git fixture did not start");
        }
        thread::sleep(Duration::from_millis(5));
    }
    cancellation.cancel();
    let result =
        receiver.recv_timeout(Duration::from_secs(5)).expect("tree and pipes finish promptly");
    worker.join().expect("joined operation");
    assert_eq!(result.expect_err("cancelled").kind(), ErrorKind::Cancelled);
}

#[test]
fn cancellation_before_spawn_does_not_start_the_selected_program() {
    let cancellation = crate::GitCancellation::new();
    cancellation.cancel();
    let command = std::process::Command::new("nonexistent-peritus-git-cancellation-fixture");
    let error = super::run(
        command,
        None,
        &cancellation,
        super::CommandAccess::Read,
        crate::Operation::Status,
    )
    .expect_err("cancel before executable lookup");
    assert_eq!(error.kind(), crate::ErrorKind::Cancelled);
}
