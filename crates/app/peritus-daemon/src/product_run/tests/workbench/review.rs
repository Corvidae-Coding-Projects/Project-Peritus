//! Composed review behavior through the actual daemon service.

use super::*;

#[test]
fn selected_feedback_produces_a_read_only_reply_or_a_new_qualified_candidate() {
    interaction::block_on(async {
        selected_explanation_produces_a_reviewer_reply_without_writer_work().await;
        selected_revision_runs_the_qualified_pipeline().await;
    });
}

async fn selected_explanation_produces_a_reviewer_reply_without_writer_work() {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(0x71, "writer", complete_writer(CORRECT));
    let mut reviewer_responses = clean_review();
    reviewer_responses.push(support::text_response(
        b"The selected hunk defines the public answer and its focused regression test.",
    ));
    let reviewer = scripted(0x72, "reviewer", reviewer_responses);
    let fixer = scripted(0x73, "fixer", Vec::new());
    let workspace = WorkspaceId::new([0x74; 16]).expect("workspace");
    let run = RunId::new([0x75; 16]).expect("run");
    let service = service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
    queue(&service, workspace).await;
    assert!(matches!(
        service
            .workbench_command(actor(), &start_build(workspace, run, [&writer, &reviewer, &fixer]),)
            .await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    assert_eq!(wait_for_terminal(&service, run).await.phase(), ProductRunPhase::Complete);
    let writer_before = writer.requests.lock().expect("writer requests").len();
    let reviewer_before = reviewer.requests.lock().expect("reviewer requests").len();
    let AppResponsePayload::WorkbenchReview(page) =
        service.workbench_review(actor(), WorkbenchReviewQuery::new(query(workspace), run, 0, 0))
    else {
        panic!("review page")
    };
    let explain = command(
        workspace,
        8,
        page.query().revision(),
        WorkbenchIntent::AddReview {
            anchor: page.files()[0].hunks()[0].anchor().clone(),
            feedback: WorkbenchReviewFeedback::Explain,
            message: WorkbenchInputText::new(
                "Explain why this selected hunk is necessary.".to_owned(),
            )
            .expect("message"),
        },
    );
    assert!(matches!(
        service.workbench_command(actor(), &explain).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    let answered = wait_for_terminal(&service, run).await;
    assert_eq!(answered.phase(), ProductRunPhase::WaitingForUser, "{}", answered.summary());
    assert_eq!(writer.requests.lock().expect("writer requests").len(), writer_before);
    {
        let reviewer_requests = reviewer.requests.lock().expect("reviewer requests");
        assert_eq!(reviewer_requests.len(), reviewer_before + 1);
        let request = reviewer_requests.last().expect("explanation request");
        let bytes = request.canonical_bytes().expect("canonical request");
        assert!(
            bytes
                .windows(b"Explain why this selected hunk is necessary.".len())
                .any(|window| { window == b"Explain why this selected hunk is necessary." })
        );
    }
    let interaction =
        service.query_interaction(ProductRunConversationQuery::new(run)).expect("interaction");
    assert_eq!(interaction.mode(), ProductInteractionMode::Review);
    assert!(interaction.activities().iter().any(|activity| {
        activity.kind() == peritus_app_protocol::ProductActivityKind::Assistant
            && activity.text().contains("selected hunk defines the public answer")
    }));
    service.shutdown(Duration::from_secs(5)).await;
}

async fn selected_revision_runs_the_qualified_pipeline() {
    const REVISED: &str = "pub const fn answer() -> u32 {\n    43\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_43() {\n        assert_eq!(super::answer(), 43);\n    }\n}\n";

    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let mut writer_responses = complete_writer(CORRECT);
    writer_responses.extend(complete_writer(REVISED));
    let writer = scripted(0x81, "writer", writer_responses);
    let mut reviewer_responses = clean_review();
    reviewer_responses.extend(clean_review());
    let reviewer = scripted(0x82, "reviewer", reviewer_responses);
    let fixer = scripted(0x83, "fixer", Vec::new());
    let workspace = WorkspaceId::new([0x84; 16]).expect("workspace");
    let run = RunId::new([0x85; 16]).expect("run");
    let service = service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
    queue(&service, workspace).await;
    assert!(matches!(
        service
            .workbench_command(actor(), &start_build(workspace, run, [&writer, &reviewer, &fixer]),)
            .await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    assert_eq!(wait_for_terminal(&service, run).await.phase(), ProductRunPhase::Complete);
    let writer_before = writer.requests.lock().expect("writer requests").len();
    let reviewer_before = reviewer.requests.lock().expect("reviewer requests").len();
    let AppResponsePayload::WorkbenchReview(page) =
        service.workbench_review(actor(), WorkbenchReviewQuery::new(query(workspace), run, 0, 0))
    else {
        panic!("review page")
    };
    let revise = command(
        workspace,
        8,
        page.query().revision(),
        WorkbenchIntent::AddReview {
            anchor: page.files()[0].hunks()[0].anchor().clone(),
            feedback: WorkbenchReviewFeedback::RequestRevision,
            message: WorkbenchInputText::new(
                "Make this selected answer return 43 and update its test.".to_owned(),
            )
            .expect("message"),
        },
    );
    assert!(matches!(
        service.workbench_command(actor(), &revise).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    let revised = wait_for_terminal(&service, run).await;
    assert_eq!(revised.phase(), ProductRunPhase::Complete, "{}", revised.summary());
    assert_eq!(
        revised.deliverable().expect("qualified revised candidate").qualification(),
        CandidateStage::Qualified,
    );
    assert_eq!(
        fs::read_to_string(repository.path().join("src/lib.rs")).expect("revised source"),
        REVISED,
    );
    {
        let writer_requests = writer.requests.lock().expect("writer requests");
        assert!(writer_requests.len() > writer_before);
        assert!(writer_requests[writer_before..].iter().any(|request| {
            request.canonical_bytes().is_ok_and(|bytes| {
                bytes
                    .windows(b"Make this selected answer return 43".len())
                    .any(|window| window == b"Make this selected answer return 43")
            })
        }));
    }
    assert!(reviewer.requests.lock().expect("reviewer requests").len() > reviewer_before);
    service.shutdown(Duration::from_secs(5)).await;
}

#[test]
fn review_service_rejects_stale_add_then_projects_and_rebinds_a_persisted_stale_hunk() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x61, "writer", complete_writer(CORRECT));
        let reviewer = scripted(0x62, "reviewer", clean_review());
        let fixer = scripted(0x63, "fixer", Vec::new());
        let workspace = WorkspaceId::new([0x64; 16]).expect("workspace");
        let run = RunId::new([0x65; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        let settings = WorkbenchExecutionSettings::new(
            run,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            ProductInteractionMode::Build,
            ProductRoleModels::default(),
        );
        let start = command(workspace, 7, 3, WorkbenchIntent::StartExecution(settings));
        assert!(matches!(
            service.workbench_command(actor(), &start).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let settled = wait_for_terminal(&service, run).await;
        assert!(
            !settled.diff().is_empty(),
            "fixture must produce a structured candidate diff: {}",
            settled.summary()
        );
        let AppResponsePayload::WorkbenchReview(first) = service
            .workbench_review(actor(), WorkbenchReviewQuery::new(query(workspace), run, 0, 0))
        else {
            panic!("review page")
        };
        let anchor = first.files()[0].hunks()[0].anchor().clone();
        let outdated_anchor = WorkbenchReviewAnchor::new(
            anchor.run(),
            anchor.workspace(),
            peritus_types::Sha256Digest::new([0xee; 32]),
            anchor.diff_digest(),
            anchor.path().to_owned(),
            anchor.before_blob_digest(),
            anchor.after_blob_digest(),
            anchor.context_digest(),
            WorkbenchReviewTarget::Hunk,
            anchor.range(),
        )
        .expect("syntactically valid stale anchor");
        let stale_add = command(
            workspace,
            8,
            first.query().revision(),
            WorkbenchIntent::AddReview {
                anchor: outdated_anchor,
                feedback: WorkbenchReviewFeedback::LeaveAlone,
                message: WorkbenchInputText::new("Do not change this hunk.".to_owned())
                    .expect("message"),
            },
        );
        assert!(matches!(
            service.workbench_command(actor(), &stale_add).await,
            AppResponsePayload::Error(error)
                if error.code() == peritus_app_protocol::AppErrorCode::StaleRevision
        ));

        let add = command(
            workspace,
            9,
            first.query().revision(),
            WorkbenchIntent::AddReview {
                anchor: anchor.clone(),
                feedback: WorkbenchReviewFeedback::LeaveAlone,
                message: WorkbenchInputText::new("Do not change this hunk.".to_owned())
                    .expect("message"),
            },
        );
        assert!(matches!(
            service.workbench_command(actor(), &add).await,
            AppResponsePayload::WorkbenchReceipt(receipt) if receipt.accepted_revision() == first.query().revision() + 1
        ));

        fs::write(repository.path().join("src/lib.rs"), "pub const fn answer() -> u32 { 43 }\n")
            .expect("later candidate");
        let output = std::process::Command::new("git")
            .args(["diff", "--binary", "--no-ext-diff"])
            .current_dir(repository.path())
            .output()
            .expect("git diff");
        assert!(output.status.success());
        let later_diff = String::from_utf8(output.stdout).expect("utf8 diff");
        {
            let mut records = service.inner.records.write().expect("records");
            let record = records.get_mut(&run).expect("run record");
            let prior = record.snapshot.clone();
            record.snapshot = peritus_app_protocol::ProductRunSnapshot::new(
                run,
                workspace,
                prior.providers(),
                prior.phase(),
                prior.cycle(),
                prior.task().to_owned(),
                prior.status().to_owned(),
                later_diff,
                prior.gates().to_owned(),
                prior.review().to_owned(),
                prior.summary().to_owned(),
            )
            .expect("later projection");
            record.checkpoint = None;
            record.settlement = None;
            crate::product_run::persist_record(&service.inner.directory, record)
                .expect("persist later projection");
        }
        let AppResponsePayload::WorkbenchReview(later) = service
            .workbench_review(actor(), WorkbenchReviewQuery::new(query(workspace), run, 0, 0))
        else {
            panic!("later review page")
        };
        assert_eq!(later.comments()[0].state(), WorkbenchReviewCommentState::Stale);
        let rebound = later.files()[0].hunks()[0].anchor().clone();
        let rebind = command(
            workspace,
            10,
            later.query().revision(),
            WorkbenchIntent::RebindReview { comment: add.operation(), anchor: rebound.clone() },
        );
        assert!(matches!(
            service.workbench_command(actor(), &rebind).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let AppResponsePayload::WorkbenchReview(rebound_page) = service
            .workbench_review(actor(), WorkbenchReviewQuery::new(query(workspace), run, 0, 0))
        else {
            panic!("rebound review page")
        };
        assert_eq!(rebound_page.comments()[0].state(), WorkbenchReviewCommentState::Open);
        assert_eq!(rebound_page.comments()[0].anchor(), &rebound);

        service.shutdown(Duration::from_secs(5)).await;
        drop(service);
        let controls = crate::product_control::ControlStore::open(
            &state.path().join("workbench-v1"),
            peritus_journal::StoreId::new([0x7f; 16]).expect("store"),
        )
        .expect("reopen controls");
        let persisted = controls
            .load(peritus_product_runner::control::ConversationId::new([2; 16]).expect("id"))
            .expect("load")
            .expect("record");
        assert_eq!(persisted.reviews().comments().len(), 1);
        assert_eq!(
            persisted.reviews().comments()[0].anchor().candidate_digest(),
            rebound.candidate_digest()
        );
    });
}
