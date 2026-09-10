//! Composed admission behavior through the actual daemon service.

use super::*;

#[test]
fn governed_queue_reaches_real_runner_and_exact_binding_survives_fenced_restart() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0x31,
            "chat",
            vec![support::text_response(b"Hello from the scripted fixture.")],
        );
        let reviewer = scripted(0x32, "review", Vec::new());
        let fixer = scripted(0x33, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x34; 16]).expect("workspace");
        let run = RunId::new([0x35; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        let start = start(workspace, run, [&writer, &reviewer, &fixer]);
        let accepted = service.workbench_command(actor(), &start).await;
        assert!(matches!(accepted, AppResponsePayload::WorkbenchReceipt(_)), "{accepted:?}");
        let final_state = wait_for_terminal(&service, run).await;
        assert_eq!(
            final_state.phase(),
            ProductRunPhase::WaitingForUser,
            "{}",
            final_state.summary()
        );
        let snapshot =
            service.query_interaction(ProductRunConversationQuery::new(run)).expect("interaction");
        assert_eq!((snapshot.received(), snapshot.incorporated()), (2, 2));
        let request_digest = {
            let requests = writer.requests.lock().expect("requests");
            assert_eq!(requests.len(), 1, "own incorporation must not look like new steering");
            let bytes = requests[0].canonical_bytes().expect("canonical request");
            assert!(
                !bytes
                    .windows(b"OBSOLETE_NEVER_SEND".len())
                    .any(|part| part == b"OBSOLETE_NEVER_SEND")
            );
            assert!(
                bytes
                    .windows(b"CURRENT_EXACT_INSTRUCTION".len())
                    .any(|part| part == b"CURRENT_EXACT_INSTRUCTION")
            );
            requests[0].fingerprint().expect("fingerprint").digest()
        };
        service.shutdown(Duration::from_secs(5)).await;
        drop(service);
        assert!(
            crate::product_run::persistence::load_records(&state.path().join("product-runs"))
                .expect("legacy generation")
                .is_empty()
        );
        let controls = crate::product_control::ControlStore::open(
            &state.path().join("workbench-v1"),
            peritus_journal::StoreId::new([0x7f; 16]).expect("store"),
        )
        .expect("restart controls");
        let records = crate::product_run::persistence::load_workbench_records(
            &state.path().join("workbench-v1"),
            Some(&controls),
        )
        .expect("restart runs");
        let restored =
            self::service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        *restored.inner.controls.lock().expect("owner") = Some(controls);
        *restored.inner.records.write().expect("records") = records;
        assert_eq!(restored.workbench_command(actor(), &start).await, accepted);
        let record = restored
            .with_controls(false, |store| {
                store.load(peritus_product_runner::control::ConversationId::new([2; 16])?)
            })
            .expect("load")
            .expect("record");
        assert_eq!(record.inputs().invocations()[0].request_digest(), request_digest);
        assert_eq!(record.inputs().revisions()[0].state(), InputState::Superseded);
        assert_eq!(record.inputs().revisions()[1].state(), InputState::Incorporated);
        assert_eq!(
            restored.control(ProductRunControl::new(run, ProductRunControlAction::Retry)).await,
            Err(crate::product_run::ProductRunServiceError::Control(
                ControlError::UnsupportedSchema
            ))
        );
        assert_eq!(
            restored
                .update_models(&peritus_app_protocol::ProductModelUpdate::new(
                    run,
                    ProductRoleModels::default()
                ))
                .await,
            Err(crate::product_run::ProductRunServiceError::Control(
                ControlError::UnsupportedSchema
            ))
        );
        assert_eq!(
            writer.requests.lock().expect("requests").len(),
            1,
            "opening, receipt replay and legacy retry cannot start work"
        );
        restored.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn failed_run_staging_preserves_the_pending_queue_and_does_not_call_a_provider() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x41, "chat", Vec::new());
        let reviewer = scripted(0x42, "review", Vec::new());
        let fixer = scripted(0x43, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x44; 16]).expect("workspace");
        let run = RunId::new([0x45; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        let blocked =
            state.path().join("workbench-v1/runs/45454545454545454545454545454545.json.new");
        fs::create_dir_all(blocked).expect("inject write failure");
        let start = start(workspace, run, [&writer, &reviewer, &fixer]);
        assert!(matches!(
            service.workbench_command(actor(), &start).await,
            AppResponsePayload::Error(_)
        ));
        let record = service
            .with_controls(false, |store| {
                store.load(peritus_product_runner::control::ConversationId::new([2; 16])?)
            })
            .expect("load")
            .expect("record");
        assert!(record.execution().is_none());
        assert_eq!(record.inputs().capture().expect("capture").pending().len(), 1);
        assert!(writer.requests.lock().expect("requests").is_empty());
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn governed_textual_image_mentions_do_not_discover_or_cache_ambient_workspace_images() {
    interaction::block_on(async {
        let repository = repository();
        fs::write(
            repository.path().join("reference.png"),
            b"\x89PNG\r\n\x1a\nprivate unrelated image",
        )
        .expect("workspace image");
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0x51,
            "chat",
            vec![support::text_response(b"No explicit attachment was selected.")],
        );
        let reviewer = scripted(0x52, "review", Vec::new());
        let fixer = scripted(0x53, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x54; 16]).expect("workspace");
        let run = RunId::new([0x55; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        let edit = command(
            workspace,
            8,
            3,
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Edit {
                selected: WorkbenchInputSelection::new(
                    WorkbenchInputId::new([4; 16]).expect("id"),
                    2,
                )
                .expect("selection"),
                text: WorkbenchInputText::new(
                    "Describe the screenshot reference.png, but I have not attached it.".to_owned(),
                )
                .expect("text"),
            }),
        );
        assert!(matches!(
            service.workbench_command(actor(), &edit).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let initial = start(workspace, run, [&writer, &reviewer, &fixer]);
        let start = WorkbenchCommand::new(
            initial.operation(),
            initial.query(),
            4,
            initial.intent().clone(),
        );
        assert!(matches!(
            service.workbench_command(actor(), &start).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let outcome = wait_for_terminal(&service, run).await;
        assert_eq!(outcome.phase(), ProductRunPhase::WaitingForUser, "{}", outcome.summary());
        let requests = writer.requests.lock().expect("requests").clone();
        assert_eq!(requests.len(), 1);
        assert!(
            !requests[0]
                .messages()
                .iter()
                .flat_map(peritus_model_protocol::Message::content)
                .any(|block| matches!(block, peritus_model_protocol::ContentBlock::Image(_)))
        );
        service.shutdown(Duration::from_secs(5)).await;
    });
}
