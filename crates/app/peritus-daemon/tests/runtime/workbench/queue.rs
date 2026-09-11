use super::*;
use peritus_app_protocol::{
    WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputSelection, WorkbenchInputState,
    WorkbenchInputText, WorkbenchNewInput, WorkbenchQueueIntent, WorkbenchQueueQuery,
};

fn query() -> WorkbenchQuery {
    WorkbenchQuery::new(
        ConversationId::new([61; 16]).expect("conversation"),
        peritus_types::WorkspaceId::new([0x33; 16]).expect("workspace"),
    )
}
fn command(id: u8, revision: u64, intent: WorkbenchIntent) -> AppRequestPayload {
    AppRequestPayload::WorkbenchCommand(WorkbenchCommand::new(
        ControlOperationId::new([id; 16]).expect("operation"),
        query(),
        revision,
        intent,
    ))
}
fn input_text(text: &str) -> WorkbenchInputText {
    WorkbenchInputText::new(text.to_owned()).expect("text")
}

#[test]
fn queue_mutations_are_durable_and_feature_gated_and_never_start_execution() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let config = configuration(temporary.path());
        let runtime = DaemonRuntime::start(config.clone()).await.expect("start");
        let mut denied = connect(runtime.endpoint_address().clone(), false).await;
        let pending_query = AppRequestPayload::QueryWorkbenchQueue(
            WorkbenchQueueQuery::new(query(), 0, 0, false).expect("query"),
        );
        assert_error(
            request(&mut denied, 1, pending_query.clone()).await,
            AppErrorCode::MissingRequiredFeature,
        );
        drop(denied);
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        create_conversation(&mut client).await;
        let id = WorkbenchInputId::new([63; 16]).expect("input");
        let enqueue = command(
            64,
            1,
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                WorkbenchNewInput::new(
                    id,
                    input_text("original exact instruction"),
                    WorkbenchInputOrder::new(Vec::new()).expect("order"),
                )
                .expect("input"),
            )),
        );
        let receipt = request(&mut client, 2, enqueue.clone()).await;
        assert!(matches!(receipt, AppResponsePayload::WorkbenchReceipt(_)));
        let selected = WorkbenchInputSelection::new(id, 1).expect("selected");
        assert!(matches!(
            request(
                &mut client,
                3,
                command(
                    65,
                    2,
                    WorkbenchIntent::Queue(WorkbenchQueueIntent::Edit {
                        selected,
                        text: input_text("new exact instruction"),
                    })
                )
            )
            .await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_error(
            request(
                &mut client,
                4,
                command(66, 3, WorkbenchIntent::Queue(WorkbenchQueueIntent::Withdraw(selected))),
            )
            .await,
            AppErrorCode::StaleRevision,
        );
        let selected = WorkbenchInputSelection::new(id, 2).expect("selected");
        assert!(matches!(
            request(
                &mut client,
                5,
                command(
                    67,
                    3,
                    WorkbenchIntent::Queue(WorkbenchQueueIntent::Hold { selected, held: true })
                )
            )
            .await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let pending = request(&mut client, 6, pending_query.clone()).await;
        let AppResponsePayload::WorkbenchQueue(page) = &pending else {
            panic!("page: {pending:?}")
        };
        assert_eq!(page.rows().len(), 1);
        assert_eq!(page.rows()[0].state(), WorkbenchInputState::Held);
        assert_eq!(page.rows()[0].text().as_str(), "new exact instruction");
        assert_execution_unavailable(&mut client).await;
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        let runtime = DaemonRuntime::start(config).await.expect("restart");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        assert_eq!(request(&mut client, 1, pending_query).await, pending);
        assert_eq!(request(&mut client, 2, enqueue).await, receipt);
        assert_history_without_execution(&mut client).await;
        drop(client);
        runtime.shutdown().await.expect("shutdown");
    });
}

async fn create_conversation(client: &mut (AppFrameStream<UnixStream>, ProtocolContext)) {
    let title = ConversationTitle::new("Durable queue".to_owned()).expect("title");
    let create = command(62, 0, WorkbenchIntent::CreateConversation(title));
    assert!(matches!(request(client, 1, create).await, AppResponsePayload::WorkbenchReceipt(_)));
}

async fn assert_history_without_execution(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
) {
    let history = request(
        client,
        3,
        AppRequestPayload::QueryWorkbenchQueue(
            WorkbenchQueueQuery::new(query(), 4, 0, true).expect("history"),
        ),
    )
    .await;
    let AppResponsePayload::WorkbenchQueue(history) = history else { panic!("history page") };
    assert_eq!(history.rows().len(), 2);
    assert_eq!(history.rows()[0].state(), WorkbenchInputState::Superseded);
    assert_eq!(history.rows()[0].text().as_str(), "original exact instruction");
    assert!(matches!(request(client, 4, AppRequestPayload::QueryProductRuns(
        peritus_app_protocol::ProductRunQuery::recent(),
    )).await, AppResponsePayload::ProductRunSettlements(runs) if runs.is_empty()));
}

async fn assert_execution_unavailable(client: &mut (AppFrameStream<UnixStream>, ProtocolContext)) {
    let profile = peritus_types::ProviderProfileId::new([68; 16]).expect("profile");
    let start = command(
        69,
        4,
        WorkbenchIntent::StartExecution(peritus_app_protocol::WorkbenchExecutionSettings::new(
            peritus_types::RunId::new([70; 16]).expect("run"),
            peritus_app_protocol::ProductProviderSelection::new(profile, profile, profile),
            peritus_app_protocol::ProductInteractionMode::Chat,
            peritus_app_protocol::ProductRoleModels::default(),
        )),
    );
    assert_error(request(client, 7, start).await, AppErrorCode::MissingRequiredFeature);
}
