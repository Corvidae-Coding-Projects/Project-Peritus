use super::*;
use peritus_app_protocol::{
    WorkbenchImagePage, WorkbenchImageQuery, WorkbenchInputState, WorkbenchQueueIntent,
};

async fn inspect(client: &mut (AppFrameStream<UnixStream>, ProtocolContext)) -> WorkbenchImagePage {
    let query = WorkbenchImageQuery::new(query(), 0, 0).expect("query");
    let response = request(client, 20, AppRequestPayload::QueryWorkbenchImages(query)).await;
    let AppResponsePayload::WorkbenchImages(page) = response else {
        panic!("image page: {response:?}")
    };
    page
}

#[test]
fn image_inspection_exposes_exact_metadata_and_persistent_selection_without_inference() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let config = configuration(temporary.path(), 1);
        let runtime = DaemonRuntime::start(config.clone()).await.expect("start");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        let missing = WorkbenchImageQuery::new(query(), 0, 0).expect("query");
        assert_error(
            request(&mut client, 21, AppRequestPayload::QueryWorkbenchImages(missing)).await,
            AppErrorCode::InvalidIdentifier,
        );
        assert!(
            !temporary.path().join("state/workbench-v1").exists(),
            "inspection cannot initialize persistence"
        );
        create(&mut client).await;
        assert_eq!(inspect(&mut client).await.total(), 0);
        let artifact = upload(&mut client, 92, GIF, "image/gif").await;
        let preview = preview(&mut client, artifact).await;
        let metadata = preview.image();
        let confirm = command(
            93,
            1,
            WorkbenchIntent::AttachImage {
                preview,
                text: WorkbenchInputText::new("Inspect this exact image".to_owned())
                    .expect("caption"),
            },
        );
        assert!(matches!(
            request(&mut client, 6, AppRequestPayload::WorkbenchCommand(confirm.clone())).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let page = inspect(&mut client).await;
        assert_eq!(page.total(), 1);
        assert_eq!(page.query().revision(), 2);
        let row = &page.rows()[0];
        assert_eq!(row.operation(), confirm.operation());
        assert_eq!(row.artifact(), artifact);
        assert_eq!(row.image(), metadata);
        assert_eq!(row.label().as_str(), "explicit reference.gif");
        assert_eq!(row.source().text().as_str(), "Inspect this exact image");
        assert_eq!(row.source().state(), WorkbenchInputState::Queued);
        assert!(row.selected() && row.eligible());
        let deselect = command(
            94,
            2,
            WorkbenchIntent::SelectImage { attachment: row.operation(), selected: false },
        );
        let receipt =
            request(&mut client, 7, AppRequestPayload::WorkbenchCommand(deselect.clone())).await;
        assert!(matches!(receipt, AppResponsePayload::WorkbenchReceipt(_)));
        let deselected = inspect(&mut client).await;
        assert_eq!(deselected.query().revision(), 3);
        assert!(!deselected.rows()[0].selected() && !deselected.rows()[0].eligible());
        assert_error(
            request(&mut client, 8, AppRequestPayload::QueryWorkbenchImages(page.query())).await,
            AppErrorCode::StaleRevision,
        );
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        let runtime = DaemonRuntime::start(config).await.expect("restart");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        assert_eq!(inspect(&mut client).await, deselected);
        assert_eq!(
            request(&mut client, 9, AppRequestPayload::QueryWorkbenchReceipt(deselect)).await,
            receipt
        );
        excluded_caption_lifecycle(&mut client, row).await;
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        assert!(!temporary.path().join("never-execute-provider.invoked").exists());
    });
}

async fn excluded_caption_lifecycle(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
    row: &peritus_app_protocol::WorkbenchImageRow,
) {
    let selected = row.source().selected();
    let hold =
        command(95, 3, WorkbenchIntent::Queue(WorkbenchQueueIntent::Hold { selected, held: true }));
    assert!(matches!(
        request(client, 10, AppRequestPayload::WorkbenchCommand(hold)).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    let select = command(
        96,
        4,
        WorkbenchIntent::SelectImage { attachment: row.operation(), selected: true },
    );
    assert!(matches!(
        request(client, 11, AppRequestPayload::WorkbenchCommand(select)).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    let suspended = inspect(client).await;
    assert!(suspended.rows()[0].selected());
    assert!(!suspended.rows()[0].eligible());
    assert_eq!(suspended.rows()[0].source().state(), WorkbenchInputState::Held);
    let withdrawn =
        command(97, 5, WorkbenchIntent::Queue(WorkbenchQueueIntent::Withdraw(selected)));
    assert!(matches!(
        request(client, 12, AppRequestPayload::WorkbenchCommand(withdrawn)).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    let withdrawn = inspect(client).await;
    assert_eq!(withdrawn.rows()[0].source().state(), WorkbenchInputState::Withdrawn);
    assert!(withdrawn.rows()[0].selected());
    assert!(!withdrawn.rows()[0].eligible());
    assert_eq!(withdrawn.rows()[0].image(), row.image());
}
