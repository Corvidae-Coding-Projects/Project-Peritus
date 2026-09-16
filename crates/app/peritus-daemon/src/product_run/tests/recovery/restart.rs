//! Daemon restart continuation and explicit cancellation regressions.

use super::*;

#[test]
fn daemon_restart_automatically_resumes_writer_and_accepts_its_new_effect() {
    interaction::block_on(restart_scenario(false));
}

#[test]
fn explicitly_cancelled_writer_stays_stopped_after_daemon_restart() {
    interaction::block_on(restart_scenario(true));
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
    let mut records =
        super::super::super::load_records(&restarted.inner.directory).expect("restore runs");
    super::super::super::reconcile_restored_candidates(
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
