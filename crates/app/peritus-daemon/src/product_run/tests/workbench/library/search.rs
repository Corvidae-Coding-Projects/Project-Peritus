//! Rebuildable search and metadata navigation coverage.

use super::*;

#[test]
fn durable_library_search_and_metadata_navigation_never_invoke_a_provider() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x11, "unused-writer", Vec::new());
        let reviewer = scripted(0x12, "unused-reviewer", Vec::new());
        let fixer = scripted(0x13, "unused-fixer", Vec::new());
        let workspace = WorkspaceId::new([0x14; 16]).expect("workspace");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        assert!(matches!(
            service
                .workbench_command(
                    actor(),
                    &command(
                        workspace,
                        20,
                        0,
                        WorkbenchIntent::CreateConversation(
                            ConversationTitle::new("Durable search fixture".to_owned()).unwrap(),
                        ),
                    ),
                )
                .await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        for offset in 0_u8..140 {
            let text = if offset == 0 {
                "ANCIENT_PUBLIC_NEEDLE from the first immutable message".to_owned()
            } else {
                format!("ordinary durable message {offset}")
            };
            let input = WorkbenchInputId::new([offset.saturating_add(1); 16]).unwrap();
            let response = service
                .workbench_command(
                    actor(),
                    &command(
                        workspace,
                        offset.saturating_add(21),
                        u64::from(offset) + 1,
                        WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                            WorkbenchNewInput::new(
                                input,
                                WorkbenchInputText::new(text).unwrap(),
                                WorkbenchInputOrder::new(Vec::new()).unwrap(),
                            )
                            .unwrap(),
                        )),
                    ),
                )
                .await;
            assert!(matches!(response, AppResponsePayload::WorkbenchReceipt(_)), "{response:?}");
        }
        let search = ConversationLibraryQuery::new(
            workspace,
            Some(ConversationSearchText::new("ANCIENT_PUBLIC_NEEDLE".to_owned()).unwrap()),
            false,
            0,
            16,
        )
        .unwrap();
        let page = match service.conversation_library(actor(), &search) {
            AppResponsePayload::ConversationLibrary(page) => page,
            response => panic!("expected library page, got {response:?}"),
        };
        assert_eq!(page.total(), 1);
        assert!(page.items()[0].snippet().unwrap().text().contains("ANCIENT_PUBLIC_NEEDLE"));
        assert!(matches!(
            service.workbench_query(actor(), query(workspace)),
            AppResponsePayload::Workbench(_)
        ));
        for (id, revision, intent) in [
            (
                180,
                141,
                WorkbenchIntent::RenameConversation(
                    ConversationTitle::new("Renamed durable conversation".to_owned()).unwrap(),
                ),
            ),
            (181, 142, WorkbenchIntent::PinConversation(true)),
            (182, 143, WorkbenchIntent::ArchiveConversation(true)),
        ] {
            assert!(matches!(
                service.workbench_command(actor(), &command(workspace, id, revision, intent)).await,
                AppResponsePayload::WorkbenchReceipt(_)
            ));
        }
        assert!(writer.requests.lock().unwrap().is_empty());
        drop(service);

        let reopened = super::service(
            state.path(),
            repository.path(),
            workspace,
            [&writer, &reviewer, &fixer],
        );
        *reopened.inner.controls.lock().unwrap() = Some(
            crate::product_control::ControlStore::open(
                &state.path().join("workbench-v1"),
                peritus_journal::StoreId::new([0x7f; 16]).unwrap(),
            )
            .unwrap(),
        );
        let archived_query = ConversationLibraryQuery::new(workspace, None, true, 0, 16).unwrap();
        let page = match reopened.conversation_library(actor(), &archived_query) {
            AppResponsePayload::ConversationLibrary(page) => page,
            response => panic!("expected reopened page, got {response:?}"),
        };
        assert_eq!(page.items().len(), 1);
        assert_eq!(page.items()[0].title().as_str(), "Renamed durable conversation");
        assert!(page.items()[0].pinned());
        assert!(page.items()[0].archived());
        assert!(matches!(
            reopened.workbench_query(actor(), query(workspace)),
            AppResponsePayload::Workbench(_)
        ));
        assert!(writer.requests.lock().unwrap().is_empty());
        reopened.shutdown(Duration::from_secs(5)).await;
    });
}
