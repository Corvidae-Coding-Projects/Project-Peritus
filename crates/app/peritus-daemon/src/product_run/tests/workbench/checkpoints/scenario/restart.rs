//! Durable checkpoint and branch assertions after the live service closes.
use super::*;

pub(super) async fn assert_legacy_manifest_admission(
    service: &ProductRunService,
    command: &WorkbenchCommand,
    folder: &std::path::Path,
) {
    if let WorkbenchIntent::CreateCheckpoint(name) = command.intent()
        && name.requires_manifest_feature()
    {
        let AppResponsePayload::Error(error) =
            service.workbench_command_with_checkpoint_features(actor(), command, true, false).await
        else {
            panic!("legacy peer must be rejected before publication");
        };
        assert_eq!(error.code(), peritus_app_protocol::AppErrorCode::UnsupportedSchema);
        let predecessor = service
            .with_controls(false, |store| store.load(DomainConversationId::new([2; 16])?))
            .unwrap()
            .unwrap();
        assert_eq!(predecessor.revision(), command.expected_revision());
        assert!(predecessor.checkpoints().is_empty());
        assert_eq!(fs::read(folder.join("note.txt")).unwrap(), b"checkpoint baseline\n");
    }
}

pub(super) async fn create_checkpoint_compatible(
    service: &ProductRunService,
    command: &WorkbenchCommand,
    selection: WorkbenchFileRange,
) -> AppResponsePayload {
    let legacy = service.workbench_command_negotiated(actor(), command, false).await;
    if selection == WorkbenchFileRange::All {
        return legacy;
    }
    let AppResponsePayload::Error(error) = legacy else {
        panic!("legacy peer accepted new schema");
    };
    assert_eq!(error.code(), peritus_app_protocol::AppErrorCode::UnsupportedSchema);
    service
        .with_controls(false, |store| {
            let record = store.load(DomainConversationId::new([2; 16])?)?.unwrap();
            assert_eq!(record.revision(), command.expected_revision());
            assert!(
                record.checkpoints().is_empty(),
                "unsupported schema must not publish a checkpoint"
            );
            Ok(())
        })
        .unwrap();
    service.workbench_command_negotiated(actor(), command, true).await
}

pub(super) fn verify(
    state: &std::path::Path,
    child: ConversationId,
    mode: WorkbenchRewindMode,
    expected_status: WorkbenchRestoreStatus,
    checkpoint_receipt: &peritus_app_protocol::WorkbenchCheckpointReceipt,
) {
    let store_id = peritus_journal::StoreId::new([0x7f; 16]).expect("control store");
    let store = crate::product_control::ControlStore::open(&state.join("workbench-v1"), store_id)
        .expect("reopen control journal");
    let durable = store
        .load(DomainConversationId::new([2; 16]).expect("conversation"))
        .expect("replay journal")
        .expect("durable record");
    let child_record = store.load(DomainConversationId::new(child.into_bytes()).unwrap()).unwrap();
    if mode != WorkbenchRewindMode::FilesOnly && expected_status == WorkbenchRestoreStatus::Applied
    {
        let child_record = child_record.expect("settled logical branch");
        let historical = store
            .load_revision(
                durable.id(),
                checkpoint_receipt.references().source_conversation_revision(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(child_record.inputs().order(), historical.inputs().order());
        assert!(child_record.inputs().capture().unwrap().pending().is_empty());
        assert!(child_record.execution().is_none());
        assert!(child_record.goal().is_none());
    } else {
        assert!(child_record.is_none());
    }
    let original = durable
        .checkpoints()
        .iter()
        .find(|checkpoint| {
            checkpoint.id()
                == CheckpointId::new(checkpoint_receipt.checkpoint().into_bytes())
                    .expect("checkpoint")
        })
        .expect("original checkpoint");
    assert_eq!(
        original.paths()[0].owned_postchange().expect("sealed").digest(),
        Some(peritus_codec::sha256(b"Peritus owned edit\n"))
    );
    assert_eq!(
        durable.restores().last().expect("restore journal").status(),
        if expected_status == WorkbenchRestoreStatus::Conflict {
            peritus_product_runner::control::RestoreStatus::Conflict
        } else {
            peritus_product_runner::control::RestoreStatus::Applied
        }
    );
}
