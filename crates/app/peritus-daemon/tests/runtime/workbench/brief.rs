use super::*;
use peritus_app_protocol::{
    WorkbenchBrief, WorkbenchBriefField as F, WorkbenchInputState as S, WorkbenchInputText,
    WorkbenchQueueIntent, WorkbenchQueueQuery,
};

fn query() -> WorkbenchQuery {
    WorkbenchQuery::new(
        ConversationId::new([73; 16]).expect("id"),
        peritus_types::WorkspaceId::new([0x33; 16]).expect("workspace"),
    )
}
fn command(id: u8, revision: u64, intent: WorkbenchIntent) -> WorkbenchCommand {
    WorkbenchCommand::new(
        ControlOperationId::new([id; 16]).expect("operation"),
        query(),
        revision,
        intent,
    )
}
fn edit(id: u8, revision: u64, field: F, text: &str) -> WorkbenchCommand {
    command(
        id,
        revision,
        WorkbenchIntent::SetBrief {
            field,
            text: WorkbenchInputText::new(text.to_owned()).expect("text"),
        },
    )
}
async fn inspect(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
    sequence: u8,
) -> WorkbenchBrief {
    let response = request(client, sequence, AppRequestPayload::QueryWorkbenchBrief(query())).await;
    let AppResponsePayload::WorkbenchBrief(brief) = response else { panic!("brief: {response:?}") };
    brief
}

#[test]
fn brief_ipc_is_durable_feature_gated_and_retains_exact_held_sources_without_execution() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let config = configuration(temporary.path());
        let runtime = DaemonRuntime::start(config.clone()).await.expect("start");
        let mut denied = connect(runtime.endpoint_address().clone(), false).await;
        let first = edit(74, 1, F::Objective, "Original requirement");
        for (sequence, payload) in [
            (1, AppRequestPayload::QueryWorkbenchBrief(query())),
            (2, AppRequestPayload::WorkbenchCommand(first.clone())),
            (3, AppRequestPayload::QueryWorkbenchReceipt(first.clone())),
        ] {
            assert_error(
                request(&mut denied, sequence, payload).await,
                AppErrorCode::MissingRequiredFeature,
            );
        }
        drop(denied);
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        assert_error(
            request(&mut client, 1, AppRequestPayload::QueryWorkbenchBrief(query())).await,
            AppErrorCode::InvalidIdentifier,
        );
        assert!(!temporary.path().join("state/workbench-v1").exists());
        let create = command(
            75,
            0,
            WorkbenchIntent::CreateConversation(
                ConversationTitle::new("Brief inspection".to_owned()).expect("title"),
            ),
        );
        assert!(matches!(
            request(&mut client, 2, AppRequestPayload::WorkbenchCommand(create)).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let initial_receipt =
            request(&mut client, 3, AppRequestPayload::WorkbenchCommand(first.clone())).await;
        assert!(matches!(initial_receipt, AppResponsePayload::WorkbenchReceipt(_)));
        let brief = inspect(&mut client, 4).await;
        assert_eq!(brief.revision(), 2);
        assert_eq!(brief.entries()[0].source().text().as_str(), "Original requirement");
        let selected = brief.entries()[0].source().selected();
        let hold = command(
            76,
            2,
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Hold { selected, held: true }),
        );
        assert!(matches!(
            request(&mut client, 5, AppRequestPayload::WorkbenchCommand(hold)).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let changed = edit(77, 3, F::Objective, "Revised held requirement");
        assert!(matches!(
            request(&mut client, 6, AppRequestPayload::WorkbenchCommand(changed)).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert_error(
            request(
                &mut client,
                7,
                AppRequestPayload::WorkbenchCommand(edit(78, 3, F::Acceptance, "stale edit")),
            )
            .await,
            AppErrorCode::StaleRevision,
        );
        let before = inspect(&mut client, 8).await;
        assert_eq!(before.entries().len(), 1);
        assert_eq!(before.entries()[0].source().selected().id(), selected.id());
        assert_eq!(before.entries()[0].source().selected().revision(), 2);
        assert_eq!(before.entries()[0].source().state(), S::Held);
        assert_eq!(before.entries()[0].source().text().as_str(), "Revised held requirement");
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        let runtime = DaemonRuntime::start(config).await.expect("restart");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        assert_eq!(inspect(&mut client, 1).await, before);
        assert_eq!(
            request(&mut client, 2, AppRequestPayload::QueryWorkbenchReceipt(first)).await,
            initial_receipt
        );
        assert_history_and_no_runs(&mut client).await;
        drop(client);
        runtime.shutdown().await.expect("shutdown");
    });
}
async fn assert_history_and_no_runs(client: &mut (AppFrameStream<UnixStream>, ProtocolContext)) {
    let response = request(
        client,
        3,
        AppRequestPayload::QueryWorkbenchQueue(
            WorkbenchQueueQuery::new(query(), 4, 0, true).expect("query"),
        ),
    )
    .await;
    let AppResponsePayload::WorkbenchQueue(page) = response else { panic!("queue") };
    assert_eq!(page.rows().len(), 2);
    assert_eq!(page.rows()[0].text().as_str(), "Original requirement");
    assert_eq!(page.rows()[0].state(), S::Superseded);
    assert_eq!(page.rows()[1].state(), S::Held);
    assert!(
        matches!(request(client, 4, AppRequestPayload::QueryProductRuns(peritus_app_protocol::ProductRunQuery::recent())).await, AppResponsePayload::ProductRunSettlements(runs) if runs.is_empty())
    );
}
