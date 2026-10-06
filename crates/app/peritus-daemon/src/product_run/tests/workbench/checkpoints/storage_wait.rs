//! Actual `SQLite` exhaustion leaves the live task queryable, resumable and cancellable.

use super::*;
use peritus_product_runner::{
    WorkspaceMutationKind,
    control::{ControlIntent, ControlOperation, ControlText, OperationId},
};

#[test]
fn checkpoint_storage_wait_keeps_the_same_run_and_provider_request_until_space_returns() {
    interaction::block_on(scenario(false));
}

#[test]
fn checkpoint_storage_wait_accepts_public_cancellation_without_waiting_for_space() {
    interaction::block_on(scenario(true));
}

async fn scenario(cancel: bool) {
    let repository = repository();
    let state = tempfile::tempdir().unwrap();
    let writer = stalled(0xd1, "checkpoint-storage-wait");
    let workspace = WorkspaceId::new([0xd2; 16]).unwrap();
    let run = RunId::new([0xd3; 16]).unwrap();
    let service = service(state.path(), repository.path(), workspace, [&writer, &writer, &writer]);
    queue(&service, workspace).await;
    assert!(matches!(
        service
            .workbench_command(actor(), &start(workspace, run, [&writer, &writer, &writer]))
            .await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    tokio::time::timeout(Duration::from_secs(5), async {
        while writer.requests.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let conversation = DomainConversationId::new([2; 16]).unwrap();
    let original_limit = service
        .with_controls(false, |store| {
            let pages = store.storage_pages_for_test()?;
            store.limit_storage_pages_for_test(pages.page_count())?;
            for index in 1_u32..=512 {
                let revision = store.load(conversation)?.unwrap().revision();
                let mut id = [0xdb; 16];
                id[..4].copy_from_slice(&index.to_be_bytes());
                let operation = ControlOperation::new(
                    OperationId::new(id)?,
                    conversation,
                    actor(),
                    workspace,
                    revision,
                    ControlIntent::RenameConversation {
                        title: ControlText::new(format!(
                            "storage fixture {index}: {}",
                            "x".repeat(220)
                        ))?,
                    },
                );
                match store.accept(&operation) {
                    Ok(_) => {}
                    Err(error) => {
                        assert!(error.is_storage_exhausted(), "{error:?}");
                        assert!(store.resolve(&operation)?.is_none());
                        return Ok(pages.maximum_pages());
                    }
                }
            }
            panic!("fixture must reach real SQLite FULL");
        })
        .unwrap();
    let baseline = fs::read(repository.path().join("src/lib.rs")).unwrap();
    let start_record =
        service.inner.records.read().unwrap().get(&run).unwrap().interaction.workbench.clone();
    let error = service
        .capture_automatic_checkpoint(
            &start_record,
            run,
            std::path::Path::new("src/lib.rs"),
            WorkspaceMutationKind::File,
        )
        .unwrap_err();
    assert!(error.is_storage_exhausted(), "{error:?}");
    let pending_service = service.clone();
    let pending = tokio::spawn(async move {
        pending_service
            .capture_checkpoint_when_available_for_test(
                run,
                std::path::Path::new("src/lib.rs"),
                WorkspaceMutationKind::File,
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = service.query_interaction(ProductInteractionQuery::new(run)).unwrap();
            if snapshot.snapshot().status().starts_with("Waiting for storage to save a checkpoint")
            {
                assert!(!snapshot.snapshot().phase().terminal());
                assert!(
                    snapshot
                        .activities()
                        .iter()
                        .any(|activity| activity.text()
                            == "Waiting for storage to save a checkpoint")
                );
                break;
            }
            assert!(!pending.is_finished(), "capture failed rather than waiting");
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(writer.requests.lock().unwrap().len(), 1, "no provider reconnection or replay");
    assert_eq!(fs::read(repository.path().join("src/lib.rs")).unwrap(), baseline);
    let checkpoint = automatic::automatic_checkpoint_id(run, "src/lib.rs");
    assert!(
        service
            .with_controls(false, |store| store.load_checkpoint(conversation, checkpoint))
            .unwrap()
            .is_none()
    );

    if cancel {
        service
            .control(ProductRunControl::new(run, ProductRunControlAction::Cancel))
            .await
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), pending).await.unwrap().unwrap();
        assert!(result.unwrap_err().contains("cancelled"));
        assert!(
            service
                .with_controls(false, |store| store.load_checkpoint(conversation, checkpoint))
                .unwrap()
                .is_none()
        );
    } else {
        service
            .with_controls(false, |store| store.limit_storage_pages_for_test(original_limit))
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), pending).await.unwrap().unwrap().unwrap();
        assert_eq!(writer.requests.lock().unwrap().len(), 1);
        let retained = service
            .with_controls(false, |store| store.load_checkpoint(conversation, checkpoint))
            .unwrap()
            .unwrap();
        assert_eq!(retained.automatic_run(), Some(run.into_bytes()));
        assert_eq!(
            retained.paths()[0].checkpoint().digest(),
            Some(peritus_codec::sha256(&baseline))
        );
        service
            .control(ProductRunControl::new(run, ProductRunControlAction::Cancel))
            .await
            .unwrap();
    }
    assert_eq!(fs::read(repository.path().join("src/lib.rs")).unwrap(), baseline);
    service
        .with_controls(false, |store| store.limit_storage_pages_for_test(original_limit))
        .unwrap();
    service.shutdown(Duration::from_secs(5)).await;
}
