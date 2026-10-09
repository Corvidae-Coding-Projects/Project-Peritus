use super::*;

#[test]
fn oversized_diff_review_starts_bounded_and_keeps_raw_and_anchor_paths_exact() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x91, "writer", complete_writer(CORRECT));
        let reviewer = scripted(0x92, "reviewer", clean_review());
        let fixer = scripted(0x93, "fixer", Vec::new());
        let workspace = WorkspaceId::new([0x94; 16]).expect("workspace");
        let run = RunId::new([0x95; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        assert!(matches!(
            service
                .workbench_command(
                    actor(),
                    &start_build(workspace, run, [&writer, &reviewer, &fixer]),
                )
                .await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_eq!(wait_for_terminal(&service, run).await.phase(), ProductRunPhase::Complete);
        let AppResponsePayload::WorkbenchReview(base) = service
            .workbench_review(actor(), WorkbenchReviewQuery::new(query(workspace), run, 0, 0))
        else {
            panic!("initial review page")
        };
        let initial_revision = base.query().revision();
        let stale_anchor = base.files()[0].hunks()[0].anchor().clone();
        assert!(matches!(
            service
                .workbench_command(
                    actor(),
                    &command(
                        workspace,
                        0x9a,
                        initial_revision,
                        WorkbenchIntent::AddReview {
                            anchor: stale_anchor,
                            feedback: WorkbenchReviewFeedback::KeepBehavior,
                            message: WorkbenchInputText::new(
                                "Retain this review across diff changes.".to_owned()
                            )
                            .expect("message"),
                        },
                    )
                )
                .await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let AppResponsePayload::WorkbenchReviewSummary(before_change) = service
            .workbench_review_summary(
                actor(),
                WorkbenchReviewQuery::new(query(workspace), run, 0, 0),
            )
        else {
            panic!("initial summary with an existing comment")
        };
        assert!(before_change.structured_available());
        let revision = before_change.query().revision();

        let malformed_diff = "diff --git a/../secret b/../secret\n--- a/../secret\n+++ b/../secret\n@@ -1 +1 @@\n-old\n+new\n";
        replace_snapshot_diff(&service, run, malformed_diff);
        let AppResponsePayload::WorkbenchReviewSummary(malformed_summary) = service
            .workbench_review_summary(
                actor(),
                WorkbenchReviewQuery::new(query(workspace), run, revision, 0),
            )
        else {
            panic!("malformed diff retains raw inspection binding")
        };
        assert!(!malformed_summary.structured_available());
        assert_eq!(malformed_summary.comments().len(), 1);
        assert_eq!(malformed_summary.comments()[0].state(), WorkbenchReviewCommentState::Stale);
        let raw_request = peritus_app_protocol::WorkbenchReviewDiffBytesQuery::new(
            query(workspace),
            run,
            revision,
            malformed_summary.candidate_digest(),
            malformed_summary.diff_digest(),
            0,
            32,
        );
        let AppResponsePayload::WorkbenchReviewDiffBytes(raw_page) =
            service.workbench_review_diff_bytes(actor(), raw_request)
        else {
            panic!("raw range does not require structured parsing")
        };
        assert_eq!(raw_page.bytes(), &malformed_diff.as_bytes()[..32]);

        let mut large_diff = String::new();
        for index in 0..513 {
            use std::fmt::Write as _;
            writeln!(large_diff, "diff --git a/file-{index:03}.rs b/file-{index:03}.rs").unwrap();
            writeln!(large_diff, "--- a/file-{index:03}.rs").unwrap();
            writeln!(large_diff, "+++ b/file-{index:03}.rs").unwrap();
            large_diff.push_str("@@ -1 +1 @@\n-old\n+new\n");
        }
        replace_snapshot_diff(&service, run, &large_diff);
        assert!(matches!(
            service.workbench_review(
                actor(),
                WorkbenchReviewQuery::new(query(workspace), run, revision, 0),
            ),
            AppResponsePayload::Error(_)
        ));
        let AppResponsePayload::WorkbenchReviewSummary(initial) = service.workbench_review_summary(
            actor(),
            WorkbenchReviewQuery::new(query(workspace), run, revision, 0),
        ) else {
            panic!("large diff has a complete bounded metadata summary")
        };
        assert_eq!(initial.total_files(), 513);
        assert!(initial.structured_available());

        let AppResponsePayload::WorkbenchReviewDiff(last) = service.workbench_review_diff(
            actor(),
            peritus_app_protocol::WorkbenchReviewDiffQuery::new(
                query(workspace),
                run,
                initial.query().revision(),
                512,
                0,
                0,
            ),
        ) else {
            panic!("last bounded diff page")
        };
        assert_eq!(last.total_files(), 513);
        assert_eq!(last.file_anchor().path(), "file-512.rs");
        assert!(last.next().is_none());

        let accepted = command(
            workspace,
            0x96,
            revision,
            WorkbenchIntent::AddReview {
                anchor: last.hunk().expect("hunk").anchor().clone(),
                feedback: WorkbenchReviewFeedback::KeepBehavior,
                message: WorkbenchInputText::new("Keep this exact behavior.".to_owned())
                    .expect("message"),
            },
        );
        assert!(matches!(
            service.workbench_command(actor(), &accepted).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        service.shutdown(Duration::from_secs(5)).await;
    });
}

fn replace_snapshot_diff(service: &ProductRunService, run: RunId, diff: &str) {
    let mut records = service.inner.records.write().expect("run records");
    let run_record = records.get_mut(&run).expect("run record");
    let previous = run_record.snapshot.clone();
    run_record.snapshot = peritus_app_protocol::ProductRunSnapshot::new(
        previous.run_id(),
        previous.workspace_id(),
        previous.providers(),
        previous.phase(),
        previous.cycle(),
        previous.task().to_owned(),
        previous.status().to_owned(),
        diff.to_owned(),
        previous.gates().to_owned(),
        previous.review().to_owned(),
        previous.summary().to_owned(),
        previous.operation().clone(),
    )
    .expect("replacement snapshot");
}
