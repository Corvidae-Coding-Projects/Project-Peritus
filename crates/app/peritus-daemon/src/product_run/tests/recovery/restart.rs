//! Daemon restart continuation and explicit cancellation regressions.

use super::*;

#[test]
fn daemon_restart_waits_for_explicit_retry_before_continuing_writer() {
    interaction::block_on(restart_scenario(false));
}

#[test]
fn explicitly_cancelled_writer_stays_stopped_after_daemon_restart() {
    interaction::block_on(restart_scenario(true));
}

#[test]
fn qualified_restart_reacquires_gates_without_replaying_a_provider() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0xb1, "writer", complete_writer(CORRECT));
        let reviewer = scripted(0xb2, "reviewer", clean_review());
        let fixer = scripted(0xb3, "fixer", Vec::new());
        let run_id = RunId::new([0xb4; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0xb5; 16]).expect("workspace");
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
        running.start(request).await.expect("start run");
        let completed = wait_for_terminal(&running, run_id).await;
        assert_eq!(completed.phase(), ProductRunPhase::Complete);
        let writer_requests = writer.requests.lock().expect("writer requests").len();
        let reviewer_requests = reviewer.requests.lock().expect("reviewer requests").len();
        running.shutdown().await.expect("shutdown product runs");
        drop(running);

        let restarted =
            service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
        let mut records = restarted.load_test_records().expect("restore runs");
        super::super::super::reconcile_restored_candidates(
            &restarted.inner.directory,
            &mut records,
            &restarted.inner.workspaces,
        )
        .expect("reconcile candidate");
        let restored = records.get(&run_id).expect("restored run");
        assert_eq!(restored.snapshot.phase(), ProductRunPhase::Failed);
        assert!(restored.resume.is_some(), "qualified continuation must remain retryable");
        assert_eq!(restored.checkpoint.expect("checkpoint").stage(), CandidateStage::SelfChecked);
        let model_requests = restored.progress.model_requests;
        let tool_calls = restored.progress.tool_calls;
        *restarted.inner.records.write().expect("run ownership") = records;

        let queued = restarted.retry(run_id).await.expect("retry stale gates");
        assert_eq!(queued.phase(), ProductRunPhase::Queued);
        assert_eq!(queued.status(), "Queued to reacquire stale checks");
        let requalified = wait_for_terminal(&restarted, run_id).await;
        assert_eq!(requalified.phase(), ProductRunPhase::Complete, "{}", requalified.summary());
        assert_eq!(
            requalified.deliverable().expect("deliverable").qualification(),
            CandidateStage::Qualified
        );
        assert_eq!(
            requalified.summary().matches("\n\nDeliverable: ").count(),
            1,
            "gate-only qualification must replace its generated handoff summary instead of appending another copy: {}",
            requalified.summary(),
        );
        assert_eq!(writer.requests.lock().expect("writer requests").len(), writer_requests);
        assert_eq!(reviewer.requests.lock().expect("reviewer requests").len(), reviewer_requests);
        {
            let records = restarted.inner.records.read().expect("run ownership");
            let progress = &records.get(&run_id).expect("requalified run").progress;
            assert_eq!(progress.model_requests, model_requests);
            assert_eq!(progress.tool_calls, tool_calls);
        }
        restarted.shutdown().await.expect("shutdown product runs");
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
    let before_shutdown = running
        .query(ProductRunQuery::exact(run_id))
        .expect("query stalled run")
        .into_iter()
        .next()
        .expect("stalled run");
    assert!(
        !before_shutdown.phase().terminal(),
        "the fixture must still be running before shutdown: {:?} / {}",
        before_shutdown.phase(),
        before_shutdown.status(),
    );
    if user_cancelled {
        running.cancel(run_id).expect("explicit user stop");
        assert_eq!(wait_for_terminal(&running, run_id).await.phase(), ProductRunPhase::Cancelled);
    }
    running.shutdown().await.expect("shutdown product runs");
    let after_shutdown =
        running.inner.records.read().expect("run ownership")[&run_id].snapshot.clone();
    let expected_after_shutdown =
        if user_cancelled { ProductRunPhase::Cancelled } else { ProductRunPhase::RecoveryRequired };
    assert_eq!(
        after_shutdown.phase(),
        expected_after_shutdown,
        "shutdown must distinguish daemon interruption from explicit user cancellation: {}",
        after_shutdown.status(),
    );
    drop(running);

    // Recreate service ownership from durable state, as daemon startup does, without sending
    // Retry or Continue and without appending a new user instruction.
    let restarted =
        service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
    let mut records = restarted.load_test_records().expect("restore runs");
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
    if user_cancelled {
        let terminal = wait_for_terminal(&restarted, run_id).await;
        assert_eq!(terminal.phase(), ProductRunPhase::Cancelled);
        assert_eq!(writer.requests.lock().expect("writer requests").len(), pending_request_count);
        assert_eq!(
            fs::read_to_string(repository.path().join("src/lib.rs")).expect("edit"),
            incorrect
        );
    } else {
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            writer.requests.lock().expect("writer requests").len(),
            pending_request_count,
            "restart must wait for an explicit retry before invoking the provider",
        );
        let stopped = restarted
            .query(ProductRunQuery::exact(run_id))
            .expect("query restored run")
            .into_iter()
            .next()
            .expect("restored snapshot");
        assert!(stopped.status().contains("explicit retry"), "{}", stopped.status());
        restarted.retry(run_id).await.expect("explicit retry");
        let terminal = wait_for_terminal(&restarted, run_id).await;
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
    restarted.shutdown().await.expect("shutdown product runs");
}
