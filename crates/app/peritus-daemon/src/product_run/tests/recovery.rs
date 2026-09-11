//! Automatic continuation after a daemon interruption, with preserved completed effects.

use super::*;

#[test]
fn daemon_restart_automatically_resumes_writer_and_accepts_its_new_effect() {
    interaction::block_on(restart_scenario(false));
}

#[test]
fn explicitly_cancelled_writer_stays_stopped_after_daemon_restart() {
    interaction::block_on(restart_scenario(true));
}

#[test]
#[ignore = "subprocess fixture; invoked by abrupt_cancel_survives_process_termination"]
fn abrupt_cancel_child_fixture() {
    let Ok(state) = std::env::var("PERITUS_ABRUPT_CANCEL_STATE") else { return };
    let repository = std::env::var("PERITUS_ABRUPT_CANCEL_REPOSITORY").expect("repository path");
    let barrier = std::env::var("PERITUS_ABRUPT_CANCEL_BARRIER").expect("barrier path");
    interaction::block_on(async {
        let writer = stalled(0xf1, "writer");
        let reviewer = scripted(0xf2, "reviewer", clean_review());
        let fixer = scripted(0xf3, "fixer", Vec::new());
        let run_id = RunId::new([0xf4; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0xf5; 16]).expect("workspace");
        let running = service(
            std::path::Path::new(&state),
            std::path::Path::new(&repository),
            workspace_id,
            [&writer, &reviewer, &fixer],
        );
        let request = ProductRunRequest::new(
            run_id,
            workspace_id,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            "Remain stopped after an abrupt daemon death.".to_owned(),
        )
        .expect("request");
        running.start(request).await.expect("start writer");
        tokio::time::timeout(Duration::from_secs(5), async {
            while writer.requests.lock().expect("writer requests").is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("provider boundary");
        running.cancel(run_id).expect("persist explicit cancellation");
        fs::write(&barrier, b"cancel-persisted\n").expect("publish crash barrier");
        std::future::pending::<()>().await;
    });
}

#[test]
fn abrupt_cancel_survives_process_termination() {
    struct OwnedChild(std::process::Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let barrier = state.path().join("cancel-persisted.barrier");
        let executable = std::env::current_exe().expect("current test executable");
        let child = std::process::Command::new(executable)
            .args([
                "--exact",
                "product_run::tests::recovery::abrupt_cancel_child_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("PERITUS_ABRUPT_CANCEL_STATE", state.path())
            .env("PERITUS_ABRUPT_CANCEL_REPOSITORY", repository.path())
            .env("PERITUS_ABRUPT_CANCEL_BARRIER", &barrier)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn owned crash fixture");
        let mut child = OwnedChild(child);
        tokio::time::timeout(Duration::from_secs(10), async {
            while !barrier.exists() {
                assert!(
                    child.0.try_wait().expect("inspect child").is_none(),
                    "fixture exited early"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fixture reached durable cancellation barrier");
        child.0.kill().expect("terminate exact owned fixture");
        let status = child.0.wait().expect("reap exact owned fixture");
        assert!(!status.success());

        let writer = stalled(0xf1, "writer");
        let reviewer = scripted(0xf2, "reviewer", clean_review());
        let fixer = scripted(0xf3, "fixer", Vec::new());
        let run_id = RunId::new([0xf4; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0xf5; 16]).expect("workspace");
        let restarted =
            service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
        let records = super::super::load_records(&restarted.inner.directory).expect("reload runs");
        assert_eq!(
            records.get(&run_id).expect("durable run").snapshot.phase(),
            ProductRunPhase::Cancelled,
        );
        *restarted.inner.records.write().expect("restore ownership") = records;
        restarted.resume_interrupted().await;
        assert!(writer.requests.lock().expect("no restarted requests").is_empty());
        restarted.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn repeated_failed_recovery_admission_does_not_duplicate_restart_narration() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = stalled(0xa1, "writer");
        let reviewer = scripted(0xa2, "reviewer", clean_review());
        let fixer = scripted(0xa3, "fixer", Vec::new());
        let run_id = RunId::new([0xa4; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0xa5; 16]).expect("workspace");
        let running =
            service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
        let request = ProductRunRequest::new(
            run_id,
            workspace_id,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            "Exercise failed restart admission.".to_owned(),
        )
        .expect("request");
        running.start(request).await.expect("start writer");
        tokio::time::timeout(Duration::from_secs(5), async {
            while writer.requests.lock().expect("writer requests").is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("provider boundary");
        running.shutdown(Duration::from_secs(5)).await;
        let records = super::super::load_records(&running.inner.directory).expect("restore runs");
        drop(running);

        let unavailable_workspace = WorkspaceId::new([0xa6; 16]).expect("other workspace");
        let restarted = service(
            state.path(),
            repository.path(),
            unavailable_workspace,
            [&writer, &reviewer, &fixer],
        );
        *restarted.inner.records.write().expect("run ownership") = records;
        restarted.resume_interrupted().await;
        restarted.resume_interrupted().await;

        let messages = restarted
            .inner
            .records
            .read()
            .expect("records")
            .get(&run_id)
            .expect("run")
            .conversation
            .messages()
            .expect("conversation");
        let recovery_messages = messages
            .iter()
            .filter(|message| {
                message.content()
                    == "The daemon restarted; I am continuing this goal from its preserved workspace."
            })
            .count();
        assert_eq!(recovery_messages, 1);
        assert_eq!(writer.requests.lock().expect("writer requests").len(), 1);
        restarted.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn explicit_cancel_racing_shutdown_remains_cancelled_in_durable_state() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = stalled(0xe1, "writer");
        let reviewer = scripted(0xe2, "reviewer", clean_review());
        let fixer = scripted(0xe3, "fixer", Vec::new());
        let run_id = RunId::new([0xe4; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0xe5; 16]).expect("workspace");
        let running =
            service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
        let durable_directory = running.inner.directory.clone();
        let request = ProductRunRequest::new(
            run_id,
            workspace_id,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            "Hold this run at the provider boundary.".to_owned(),
        )
        .expect("request");
        running.start(request).await.expect("start writer");

        tokio::time::timeout(Duration::from_secs(5), async {
            while writer.requests.lock().expect("writer requests").is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("writer reached the provider boundary");
        running.cancel(run_id).expect("explicit user stop");
        running.shutdown(Duration::from_secs(5)).await;
        drop(running);

        let records = super::super::load_records(&durable_directory).expect("reload durable runs");
        assert_eq!(
            records.get(&run_id).expect("durable run").snapshot.phase(),
            ProductRunPhase::Cancelled,
            "shutdown must not convert an explicitly cancelled run into restartable work",
        );
        assert_eq!(writer.requests.lock().expect("writer requests").len(), 1);
    });
}

async fn restart_scenario(user_cancelled: bool) {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let incorrect = CORRECT.replace("42", "41");
    let mut partial = complete_writer(&incorrect);
    partial.pop();
    let pending_request_count = partial.len() + 1;
    let writer = support::stalled_after(0xd1, "writer", partial);
    let reviewer = scripted(0xd2, "reviewer", clean_review());
    let fixer = scripted(0xd3, "fixer", Vec::new());
    let run_id = RunId::new([0xd4; 16]).expect("run");
    let workspace_id = WorkspaceId::new([0xd5; 16]).expect("workspace");
    let running =
        service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
    let request = ProductRunRequest::new(
        run_id,
        workspace_id,
        ProductProviderSelection::new(
            writer.profile.profile_id(),
            reviewer.profile.profile_id(),
            fixer.profile.profile_id(),
        ),
        "Add a tested answer function that returns 42.".to_owned(),
    )
    .expect("request");
    running.start(request).await.expect("start writer");
    tokio::time::timeout(Duration::from_secs(30), async {
        while writer.requests.lock().expect("writer requests").len() < pending_request_count {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("writer waits after its completed edit");
    assert_eq!(fs::read_to_string(repository.path().join("src/lib.rs")).expect("edit"), incorrect);
    if user_cancelled {
        running.cancel(run_id).expect("explicit user stop");
        assert_eq!(wait_for_terminal(&running, run_id).await.phase(), ProductRunPhase::Cancelled);
    }
    running.shutdown(Duration::from_secs(5)).await;
    drop(running);

    // Recreate service ownership from durable state, as daemon startup does, without sending
    // Retry or Continue and without appending a new user instruction.
    let restarted =
        service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
    let mut records = super::super::load_records(&restarted.inner.directory).expect("restore runs");
    super::super::reconcile_restored_candidates(
        &restarted.inner.directory,
        &mut records,
        &restarted.inner.workspaces,
    )
    .expect("restore candidate");
    let record = records.get(&run_id).expect("restored run");
    let expected =
        if user_cancelled { ProductRunPhase::Cancelled } else { ProductRunPhase::RecoveryRequired };
    assert_eq!(record.snapshot.phase(), expected);
    *restarted.inner.records.write().expect("run ownership") = records;
    writer
        .responses
        .lock()
        .expect("writer scripts")
        .extend(complete_writer(CORRECT).into_iter().skip(4));
    restarted.resume_interrupted().await;
    let terminal = wait_for_terminal(&restarted, run_id).await;
    if user_cancelled {
        assert_eq!(terminal.phase(), ProductRunPhase::Cancelled);
        assert_eq!(writer.requests.lock().expect("writer requests").len(), pending_request_count);
        assert_eq!(
            fs::read_to_string(repository.path().join("src/lib.rs")).expect("edit"),
            incorrect
        );
    } else {
        assert_eq!(terminal.phase(), ProductRunPhase::Complete, "{}", terminal.summary());
        assert_eq!(
            fs::read_to_string(repository.path().join("src/lib.rs")).expect("edit"),
            CORRECT
        );
        assert_eq!(
            terminal.deliverable().expect("qualified result").qualification(),
            CandidateStage::Qualified
        );
    }
    restarted.shutdown(Duration::from_secs(5)).await;
}
