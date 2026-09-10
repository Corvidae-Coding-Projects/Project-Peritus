use super::*;
use peritus_app_protocol::{
    WorkbenchGuidanceContent, WorkbenchGuidanceForget, WorkbenchGuidanceReason,
    WorkbenchGuidanceSave, WorkbenchGuidanceScope, WorkbenchGuidanceSelection,
    WorkbenchGuidanceSource, WorkbenchGuidanceText, WorkbenchMemoryQuery, WorkbenchMemoryRow,
};

fn scoped_command(
    query: WorkbenchQuery,
    operation: u8,
    revision: u64,
    intent: WorkbenchIntent,
) -> WorkbenchCommand {
    WorkbenchCommand::new(
        ControlOperationId::new([operation; 16]).expect("operation"),
        query,
        revision,
        intent,
    )
}

async fn create_and_start(
    service: &ProductRunService,
    query: WorkbenchQuery,
    run: RunId,
    base: u8,
    providers: [&Arc<ScriptedProvider>; 3],
) {
    let create = scoped_command(
        query,
        base,
        0,
        WorkbenchIntent::CreateConversation(
            ConversationTitle::new(format!("guidance fixture {base}")).expect("title"),
        ),
    );
    assert!(matches!(
        service.workbench_command(actor(), &create).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    let input = WorkbenchInputId::new([base.saturating_add(1); 16]).expect("input");
    let enqueue = scoped_command(
        query,
        base.saturating_add(2),
        1,
        WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
            WorkbenchNewInput::new(
                input,
                WorkbenchInputText::new("Use the current project guidance.".to_owned())
                    .expect("text"),
                WorkbenchInputOrder::new(Vec::new()).expect("dependencies"),
            )
            .expect("input"),
        )),
    );
    assert!(matches!(
        service.workbench_command(actor(), &enqueue).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    let start = scoped_command(
        query,
        base.saturating_add(3),
        2,
        WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
            run,
            ProductProviderSelection::new(
                providers[0].profile.profile_id(),
                providers[1].profile.profile_id(),
                providers[2].profile.profile_id(),
            ),
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
        )),
    );
    assert!(matches!(
        service.workbench_command(actor(), &start).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
}

fn request_contains(request: &peritus_model_protocol::ModelRequest, needle: &str) -> bool {
    request
        .messages()
        .iter()
        .flat_map(peritus_model_protocol::Message::content)
        .any(|block| {
            matches!(block, peritus_model_protocol::ContentBlock::Text(text) if text.expose_for_wire().contains(needle))
        })
}

#[test]
fn saved_guidance_enters_future_provider_request_and_restart_tombstone_excludes_it() {
    interaction::block_on(async {
        const GUIDANCE: &str = "P7_GUIDANCE_MARKER: run the focused acceptance gate.";
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0xa1,
            "chat",
            vec![
                support::text_response(b"Guidance observed."),
                support::text_response(b"Tombstone observed."),
            ],
        );
        let reviewer = scripted(0xa2, "review", Vec::new());
        let fixer = scripted(0xa3, "fix", Vec::new());
        let workspace = WorkspaceId::new([0xa4; 16]).expect("workspace");
        let admin = WorkbenchQuery::new(
            ConversationId::new([0xa5; 16]).expect("admin conversation"),
            workspace,
        );
        let save_operation = ControlOperationId::new([0xa6; 16]).expect("save operation");
        let initial =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);

        let create_admin = scoped_command(
            admin,
            0xa7,
            0,
            WorkbenchIntent::CreateConversation(
                ConversationTitle::new("guidance administration".to_owned()).expect("title"),
            ),
        );
        assert!(matches!(
            initial.workbench_command(actor(), &create_admin).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let content = WorkbenchGuidanceContent::new(
            WorkbenchGuidanceText::new(GUIDANCE.to_owned()).expect("guidance text"),
            WorkbenchGuidanceSource::UserAuthored,
            WorkbenchGuidanceScope::Project,
        )
        .expect("guidance content");
        let save = scoped_command(
            admin,
            0xa6,
            1,
            WorkbenchIntent::SaveGuidance(WorkbenchGuidanceSave::new(0, content, false)),
        );
        assert_eq!(save.operation(), save_operation);
        assert!(matches!(
            initial.workbench_command(actor(), &save).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));

        let first_query = WorkbenchQuery::new(
            ConversationId::new([0xb1; 16]).expect("first conversation"),
            workspace,
        );
        let first_run = RunId::new([0xb2; 16]).expect("first run");
        create_and_start(&initial, first_query, first_run, 0xb3, [&writer, &reviewer, &fixer])
            .await;
        let first = wait_for_terminal(&initial, first_run).await;
        assert_eq!(first.phase(), ProductRunPhase::WaitingForUser, "{}", first.summary());
        {
            let requests = writer.requests.lock().expect("provider requests");
            assert_eq!(requests.len(), 1);
            assert!(request_contains(&requests[0], "PERITUS_PROJECT_GUIDANCE_V1"));
            assert!(request_contains(&requests[0], GUIDANCE));
        }

        let forget = scoped_command(
            admin,
            0xa8,
            2,
            WorkbenchIntent::ForgetGuidance(WorkbenchGuidanceForget::new(
                WorkbenchGuidanceSelection::new(save_operation, 1).expect("selection"),
                1,
                WorkbenchGuidanceReason::new("Superseded by current project policy.".to_owned())
                    .expect("reason"),
            )),
        );
        assert!(matches!(
            initial.workbench_command(actor(), &forget).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        initial.shutdown(Duration::from_secs(5)).await;
        drop(initial);

        let restarted =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        let recovered = crate::product_control::ControlStore::open(
            &state.path().join("workbench-v1"),
            peritus_journal::StoreId::new([0x7f; 16]).expect("control store"),
        )
        .expect("recover control store");
        *restarted.inner.controls.lock().expect("control owner") = Some(recovered);
        let history_response = restarted.workbench_memory(
            actor(),
            WorkbenchMemoryQuery::new(admin, 0, 0, true).expect("history query"),
        );
        let AppResponsePayload::WorkbenchMemory(history) = history_response else {
            panic!("history projection: {history_response:?}")
        };
        assert_eq!(history.dependency_revision(), 2);
        assert!(matches!(history.rows(), [WorkbenchMemoryRow::Forgotten(_)]));

        let second_query = WorkbenchQuery::new(
            ConversationId::new([0xc1; 16]).expect("second conversation"),
            workspace,
        );
        let second_run = RunId::new([0xc2; 16]).expect("second run");
        create_and_start(&restarted, second_query, second_run, 0xc3, [&writer, &reviewer, &fixer])
            .await;
        let second = wait_for_terminal(&restarted, second_run).await;
        assert_eq!(second.phase(), ProductRunPhase::WaitingForUser, "{}", second.summary());
        {
            let requests = writer.requests.lock().expect("provider requests");
            assert_eq!(requests.len(), 2);
            assert!(!request_contains(&requests[1], GUIDANCE));
            assert!(!request_contains(&requests[1], "PERITUS_PROJECT_GUIDANCE_V1"));
        }
        restarted.shutdown(Duration::from_secs(5)).await;
    });
}
