use super::*;

fn write(text: &str) -> std::collections::VecDeque<peritus_model_protocol::EventEnvelope> {
    named_tool_response(
        "workspace_write",
        serde_json::to_vec(&serde_json::json!({"path":"note.txt", "content":text}))
            .expect("write arguments"),
    )
}

fn complete() -> std::collections::VecDeque<peritus_model_protocol::EventEnvelope> {
    text_response(br#"{"kind":"complete","run_instructions":"cat note.txt","summary":"Updated the requested note."}"#)
}

#[test]
fn folder_provider_failure_retains_unqualified_effects_and_durable_retry_reuses_the_pipeline() {
    block_on(async {
        let root = tempfile::tempdir().expect("folder");
        fs::write(root.path().join("note.txt"), "original").expect("source");
        artifact_contract(root.path());
        let responses =
            pipeline_prefix().into_iter().chain([write("requested text"), complete()]).collect();
        let writer = scripted(0x68, "interrupted-pipeline", responses);
        let (service, request) = folder_service(root.path(), &writer, true);
        let id = request.run_id();
        service
            .interact(ProductInteractionRequest::new(
                request,
                Mode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start");
        let failed = wait_for_terminal(&service, id).await;
        assert_eq!(failed.phase(), ProductRunPhase::Failed, "{}", failed.summary());
        assert!(failed.status().contains("in-place effects retained"));
        assert!(failed.deliverable().is_none());
        assert_eq!(
            fs::read_to_string(root.path().join("note.txt")).expect("retained effect"),
            "requested text"
        );
        service.query_interaction(ProductRunConversationQuery::new(id)).expect("wire-safe failure");
        for action in [ProductRunControlAction::Accept, ProductRunControlAction::Discard] {
            assert!(service.control(ProductRunControl::new(id, action)).await.is_err());
        }
        let restored = crate::product_run::persistence::load_records(&service.inner.directory)
            .expect("durable restore");
        let resume = restored
            .get(&id)
            .expect("restored record")
            .resume
            .clone()
            .expect("durable pipeline continuation");
        assert_eq!(resume.next_phase(), peritus_product_runner::ProductRunPhase::Checking);
        service.inner.records.write().expect("records").get_mut(&id).expect("record").resume =
            Some(resume);
        writer.responses.lock().expect("scripts").extend(
            std::iter::once(named_tool_response("run_pipeline", b"{}".to_vec()))
                .chain(pipeline_review("note.txt")),
        );
        service
            .control(ProductRunControl::new(id, ProductRunControlAction::Retry))
            .await
            .expect("retry");
        let completed = wait_for_terminal(&service, id).await;
        assert_eq!(completed.phase(), ProductRunPhase::Complete, "{}", completed.summary());
        assert!(completed.diff().contains("-original"));
        assert!(completed.diff().contains("+requested text"));
        assert!(completed.deliverable().is_none());
        assert!(writer.responses.lock().expect("scripts").is_empty());
        service
            .query_interaction(ProductRunConversationQuery::new(id))
            .expect("wire-safe completion");
        assert!(!root.path().join(".git").exists());
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn completed_followup_uses_a_new_in_place_baseline() {
    block_on(async {
        let root = tempfile::tempdir().expect("folder");
        fs::write(root.path().join("note.txt"), "original").expect("source");
        artifact_contract(root.path());
        let writer = scripted(
            0x6a,
            "followup-pipeline",
            pipeline_prefix()
                .into_iter()
                .chain([write("requested text"), complete()])
                .chain(pipeline_review("note.txt"))
                .collect(),
        );
        let (service, request) = folder_service(root.path(), &writer, true);
        let id = request.run_id();
        service
            .interact(ProductInteractionRequest::new(
                request,
                Mode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start");
        assert_eq!(wait_for_terminal(&service, id).await.phase(), ProductRunPhase::Complete);
        writer.responses.lock().expect("scripts").extend(
            pipeline_prefix()
                .into_iter()
                .chain([write("second requested text"), complete()])
                .chain(pipeline_review("note.txt")),
        );
        service
            .continue_run(
                &ProductRunContinuation::new(
                    id,
                    "Now replace note.txt with second requested text.".to_owned(),
                )
                .expect("followup"),
            )
            .await
            .expect("continue");
        let completed = wait_for_terminal(&service, id).await;
        assert_eq!(completed.phase(), ProductRunPhase::Complete, "{}", completed.summary());
        assert!(completed.diff().contains("-requested text"), "{}", completed.diff());
        assert!(!completed.diff().contains("-original"));
        assert!(completed.diff().contains("+second requested text"));
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn unsupported_folder_checks_do_not_become_success_from_clean_reviewer_prose() {
    block_on(async {
        let root = tempfile::tempdir().expect("folder");
        fs::write(root.path().join("note.txt"), "original").expect("source");
        // No project or artifact verification contract exists here.
        let writer = scripted(
            0x6b,
            "uncovered-pipeline",
            pipeline_prefix()
                .into_iter()
                .chain([write("requested text"), complete()])
                .chain(pipeline_review("note.txt"))
                .collect(),
        );
        let (service, request) = folder_service(root.path(), &writer, true);
        let id = request.run_id();
        service
            .interact(ProductInteractionRequest::new(
                request,
                Mode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start");
        let failed = wait_for_terminal(&service, id).await;
        assert_eq!(failed.phase(), ProductRunPhase::Failed, "{}", failed.summary());
        assert!(failed.gates().contains("Uncovered candidate files"));
        assert!(failed.gates().contains("Exact-target acceptance: FAIL"));
        assert!(failed.deliverable().is_none());
        assert_eq!(
            fs::read_to_string(root.path().join("note.txt")).expect("retained effect"),
            "requested text"
        );
        assert!(!root.path().join("peritus-workspace.toml").exists());
        assert!(!root.path().join(".git").exists());
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn cancellation_during_folder_review_retains_effects_without_qualification_or_controls() {
    block_on(async {
        let root = tempfile::tempdir().expect("folder");
        fs::write(root.path().join("note.txt"), "original").expect("source");
        artifact_contract(root.path());
        let writer = stalled(0x6c, "cancelled-folder-review");
        {
            let mut scripts = writer.responses.lock().expect("scripts");
            let stalled_review = scripts.pop_front().expect("stalled response");
            scripts.extend(pipeline_prefix().into_iter().chain([
                write("requested text"),
                complete(),
                stalled_review,
            ]));
        }
        let (service, request) = folder_service(root.path(), &writer, true);
        let id = request.run_id();
        service
            .interact(ProductInteractionRequest::new(
                request,
                Mode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start");
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let phase = service.query(ProductRunQuery::exact(id)).expect("snapshot")[0].phase();
                if phase == ProductRunPhase::Reviewing {
                    break;
                }
                assert!(!phase.terminal(), "stopped before review: {phase:?}");
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("entered review");
        service
            .control(ProductRunControl::new(id, ProductRunControlAction::Cancel))
            .await
            .expect("cancel");
        let cancelled = wait_for_terminal(&service, id).await;
        assert_eq!(cancelled.phase(), ProductRunPhase::Cancelled);
        assert!(cancelled.deliverable().is_none());
        assert_eq!(
            fs::read_to_string(root.path().join("note.txt")).expect("retained"),
            "requested text"
        );
        {
            let records = service.inner.records.read().expect("records");
            let record = records.get(&id).expect("record");
            assert!(!record.candidate_actionable);
            assert!(!record.settlement.expect("settlement").is_accepted());
        }
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn a_question_about_interrupted_folder_work_preserves_its_continuation_without_running_it() {
    block_on(async {
        let root = tempfile::tempdir().expect("folder");
        fs::write(root.path().join("note.txt"), "original").expect("source");
        artifact_contract(root.path());
        let writer = scripted(
            0x6d,
            "discuss-interruption",
            pipeline_prefix().into_iter().chain([write("requested text"), complete()]).collect(),
        );
        let (service, request) = folder_service(root.path(), &writer, true);
        let id = request.run_id();
        service
            .interact(ProductInteractionRequest::new(
                request,
                Mode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start");
        assert_eq!(wait_for_terminal(&service, id).await.phase(), ProductRunPhase::Failed);
        let before = writer.requests.lock().expect("requests").len();
        writer.responses.lock().expect("scripts").push_back(text_response(
            b"The edit is retained, but review was interrupted. No new work has run.",
        ));
        service
            .continue_run(
                &ProductRunContinuation::new(
                    id,
                    "Why did it stop? Explain only; do not change anything.".to_owned(),
                )
                .expect("question"),
            )
            .await
            .expect("discuss");
        let idle = wait_for_terminal(&service, id).await;
        assert_eq!(idle.phase(), ProductRunPhase::WaitingForUser, "{}", idle.summary());
        assert_eq!(writer.requests.lock().expect("requests").len(), before + 1);
        assert_eq!(
            fs::read_to_string(root.path().join("note.txt")).expect("retained"),
            "requested text"
        );
        let restored = crate::product_run::persistence::load_records(&service.inner.directory)
            .expect("reload");
        let record = restored.get(&id).expect("record");
        assert!(record.resume.is_some());
        assert!(record.checkpoint.is_some());
        assert!(!record.settlement.expect("settlement").is_accepted());
        assert!(!record.candidate_actionable);
        service
            .query_interaction(ProductRunConversationQuery::new(id))
            .expect("wire-safe question");
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn folder_review_finding_runs_the_existing_fixer_and_fresh_review() {
    block_on(async {
        let root = tempfile::tempdir().expect("folder");
        fs::write(root.path().join("note.txt"), "original").expect("source");
        artifact_contract(root.path());
        let mut review = pipeline_review("note.txt");
        review.pop();
        review.push(text_response(br#"{"findings":[{"category":"requested_behavior","description":"The note says wrong text rather than requested text.","location":"note.txt","remediation":"Write requested text exactly.","reproduction":"Read note.txt and compare to the user request.","severity":"high","title":"Wrong note contents"}],"summary":"The literal request is not met."}"#));
        let responses = pipeline_prefix()
            .into_iter()
            .chain([write("wrong text"), complete()])
            .chain(review)
            .chain([
                named_tool_response("workspace_list", br#"{"path":"","depth":1}"#.to_vec()),
                named_tool_response("workspace_read", br#"{"path":"note.txt"}"#.to_vec()),
                write("requested text"),
                complete(),
            ])
            .chain(pipeline_review("note.txt"))
            .collect();
        let writer = scripted(0x69, "fixer-pipeline", responses);
        let (service, request) = folder_service(root.path(), &writer, true);
        let id = request.run_id();
        service
            .interact(ProductInteractionRequest::new(
                request,
                Mode::Chat,
                ProductRoleModels::default(),
            ))
            .await
            .expect("start");
        let completed = wait_for_terminal(&service, id).await;
        assert_eq!(completed.phase(), ProductRunPhase::Complete, "{}", completed.summary());
        assert!(completed.cycle() >= 2);
        assert_eq!(
            fs::read_to_string(root.path().join("note.txt")).expect("fixed"),
            "requested text"
        );
        let snapshot =
            service.query_interaction(ProductRunConversationQuery::new(id)).expect("snapshot");
        assert!(
            snapshot
                .activities()
                .iter()
                .any(|activity| activity.text().contains("review found issues"))
        );
        assert!(writer.responses.lock().expect("scripts").is_empty());
        service.shutdown(Duration::from_secs(5)).await;
    });
}
