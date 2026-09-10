use super::*;
use peritus_app_protocol::{
    WorkbenchContextDisposition as D, WorkbenchContextQuery, WorkbenchContextView as V,
    WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputText, WorkbenchNewInput,
    WorkbenchQueueIntent,
};

fn query() -> WorkbenchQuery {
    WorkbenchQuery::new(
        ConversationId::new([71; 16]).expect("conversation"),
        peritus_types::WorkspaceId::new([0x33; 16]).expect("workspace"),
    )
}
fn context(revision: u64, view: V) -> AppRequestPayload {
    AppRequestPayload::QueryWorkbenchContext(
        WorkbenchContextQuery::new(query(), revision, 0, view).expect("query"),
    )
}
async fn prepare(client: &mut (AppFrameStream<UnixStream>, ProtocolContext)) {
    for (sequence, revision, intent) in [
        (
            3,
            0,
            WorkbenchIntent::CreateConversation(
                ConversationTitle::new("Context inspection".to_owned()).expect("title"),
            ),
        ),
        (
            4,
            1,
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                WorkbenchNewInput::new(
                    WorkbenchInputId::new([72; 16]).expect("input"),
                    WorkbenchInputText::new(
                        "RAW_INPUT_MUST_NOT_APPEAR_IN_CONTEXT_RESPONSE".to_owned(),
                    )
                    .expect("text"),
                    WorkbenchInputOrder::new(Vec::new()).expect("order"),
                )
                .expect("input"),
            )),
        ),
    ] {
        let command = WorkbenchCommand::new(
            ControlOperationId::new([sequence + 80; 16]).expect("operation"),
            query(),
            revision,
            intent,
        );
        assert!(matches!(
            request(client, sequence, AppRequestPayload::WorkbenchCommand(command)).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
    }
}

#[test]
fn context_ipc_is_feature_gated_content_free_and_restart_stable_without_execution() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let config = configuration(temporary.path());
        let runtime = DaemonRuntime::start(config.clone()).await.expect("start");
        let mut denied = connect(runtime.endpoint_address().clone(), false).await;
        assert_error(
            request(&mut denied, 1, context(0, V::Next)).await,
            AppErrorCode::MissingRequiredFeature,
        );
        drop(denied);
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        assert_error(
            request(&mut client, 1, context(0, V::Next)).await,
            AppErrorCode::InvalidIdentifier,
        );
        assert!(!temporary.path().join("state/workbench-v1").exists());
        prepare(&mut client).await;
        let before = request(&mut client, 5, AppRequestPayload::QueryWorkbench(query())).await;
        let original = request(&mut client, 6, context(0, V::Next)).await;
        let AppResponsePayload::WorkbenchContext(page) = &original else {
            panic!("context: {original:?}");
        };
        assert_eq!(page.query().revision(), 2);
        assert_eq!(page.rows().len(), 1);
        assert_eq!(page.rows()[0].disposition(), D::Eligible);
        assert_eq!(
            page.rows()[0].digest(),
            peritus_codec::sha256(b"RAW_INPUT_MUST_NOT_APPEAR_IN_CONTEXT_RESPONSE")
        );
        assert!(!format!("{page:?}").contains("RAW_INPUT_MUST_NOT_APPEAR"));
        assert_error(
            request(&mut client, 7, context(1, V::Next)).await,
            AppErrorCode::StaleRevision,
        );
        let history = request(&mut client, 8, context(0, V::History)).await;
        assert!(matches!(history, AppResponsePayload::WorkbenchContext(page) if page.total() == 0));
        assert_eq!(
            request(&mut client, 9, AppRequestPayload::QueryWorkbench(query())).await,
            before
        );
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        let runtime = DaemonRuntime::start(config).await.expect("restart");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        assert_eq!(request(&mut client, 1, context(0, V::Next)).await, original);
        drop(client);
        runtime.shutdown().await.expect("shutdown");
    });
}
