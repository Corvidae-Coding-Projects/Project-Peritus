//! Previously prepared v1 restores retain their original C1 authority after migration.

use super::*;
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, ConversationId, WorkbenchCommand, WorkbenchIntent,
    WorkbenchQuery, WorkbenchRestoreStatus, WorkbenchRewindRequest,
};
use peritus_product_runner::control::{
    CheckpointFileMode, CheckpointId, CheckpointPath, CheckpointReferences, ControlIntent,
    ControlOperation, ControlText, ConversationId as DomainConversationId, OperationId, RestoreId,
    RestoreOperation,
};
use peritus_types::{ActorId, SessionId, WorkspaceId};
mod partial;

#[test]
fn prepared_legacy_restore_recovers_the_same_identity_before_and_after_c1_apply() {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        for applied in [false, true] {
            let workspace = tempfile::tempdir().unwrap();
            let state = tempfile::tempdir().unwrap();
            std::fs::write(workspace.path().join("note.txt"), b"postchange").unwrap();
            let workspace_id = WorkspaceId::new([4; 16]).unwrap();
            let actor = ActorId::new([1; 16]).unwrap();
            let conversation = DomainConversationId::new([2; 16]).unwrap();
            let query = WorkbenchQuery::new(ConversationId::new([2; 16]).unwrap(), workspace_id);
            let service = crate::product_run::tests::checkpoint_test_service(
                state.path(),
                workspace.path(),
                workspace_id,
            );
            let operation = |id, revision, intent| {
                ControlOperation::new(
                    OperationId::new([id; 16]).unwrap(),
                    conversation,
                    actor,
                    workspace_id,
                    revision,
                    intent,
                )
            };
            let version = |bytes: &[u8]| {
                CheckpointFileVersion::present(
                    peritus_codec::sha256(bytes),
                    bytes.len() as u64,
                    CheckpointFileMode::Regular,
                )
            };
            let checkpoint = UserCheckpoint::new(
                CheckpointId::new([11; 16]).unwrap(),
                "legacy".to_owned(),
                CheckpointReferences::new(1, 0, 0, None),
                vec![CheckpointPath::new("note.txt".to_owned(), version(b"saved")).unwrap()],
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
            service
                .with_controls(true, |store| {
                    store.accept(&operation(
                        10,
                        0,
                        ControlIntent::CreateConversation {
                            title: ControlText::new("legacy restore".to_owned())?,
                        },
                    ))?;
                    store.accept_checkpoint(
                        &operation(11, 1, ControlIntent::CreateCheckpoint(checkpoint.clone())),
                        &[Some(b"saved".to_vec())],
                    )?;
                    store.accept(&operation(
                        12,
                        2,
                        ControlIntent::SealCheckpoint {
                            checkpoint: checkpoint.id(),
                            run: [5; 16],
                            versions: vec![("note.txt".to_owned(), version(b"postchange"))],
                        },
                    ))?;
                    Ok(())
                })
                .unwrap();
            let request =
                WorkbenchRewindRequest::new(query, 3, ControlOperationId::new([11; 16]).unwrap())
                    .unwrap();
            let AppResponsePayload::WorkbenchRewindPreview(preview) =
                service.preview_workbench_rewind(actor, &request).await
            else {
                panic!("legacy preview");
            };
            let patch = PatchSet::new(
                workspace_id,
                Generation::first(),
                RevisionNumber::new(3).unwrap(),
                vec![
                    PatchOperation::replace(
                        WorkspacePath::new("note.txt").unwrap(),
                        Preimage::from_bytes(b"postchange", peritus_patch::FileMode::Regular),
                        FinalFile::new(
                            b"saved".to_vec(),
                            peritus_patch::FileMode::Regular,
                            LineEndingPolicy::Preserve,
                        )
                        .unwrap(),
                    )
                    .unwrap(),
                ],
            )
            .unwrap();
            let patch_digest = patch.identity().digest();
            let restore_id = RestoreId::new([13; 16]).unwrap();
            let recovery = UserCheckpoint::new(
                CheckpointId::new([14; 16]).unwrap(),
                "recovery".to_owned(),
                CheckpointReferences::new(3, 0, 0, None),
                vec![CheckpointPath::new("note.txt".to_owned(), version(b"postchange")).unwrap()],
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
            let restore = RestoreOperation::prepared(
                restore_id,
                checkpoint.id(),
                preview.preview_digest(),
                patch_digest,
                recovery.id(),
            )
            .unwrap();
            let command = WorkbenchCommand::new(
                ControlOperationId::new([13; 16]).unwrap(),
                query,
                3,
                WorkbenchIntent::ApplyRewind(preview),
            );
            service
                .with_controls(false, |store| {
                    let receipt = store.accept_restore_preparation(
                        &operation(13, 3, ControlIntent::PrepareRestore { restore, recovery }),
                        &[Some(b"postchange".to_vec())],
                    )?;
                    if applied {
                        service.apply_authorized_folder_patch(
                            store,
                            actor,
                            SessionId::new([15; 16]).unwrap(),
                            &command,
                            patch,
                            receipt.accepted_revision(),
                        )?;
                    }
                    Ok(())
                })
                .unwrap();
            drop(service);
            let service = crate::product_run::tests::checkpoint_test_service(
                state.path(),
                workspace.path(),
                workspace_id,
            );
            service.with_controls(true, |_| Ok(())).unwrap();
            let recovered = service.resolve_workbench_restore(actor, &command).unwrap();
            let result = service
                .workbench_folder_command(actor, SessionId::new([15; 16]).unwrap(), &command)
                .await;
            let AppResponsePayload::WorkbenchRestore(receipt) = result else {
                panic!("legacy reconciliation: {result:?}");
            };
            assert_eq!(receipt, recovered);
            assert_eq!(
                receipt.status(),
                if applied {
                    WorkbenchRestoreStatus::Applied
                } else {
                    WorkbenchRestoreStatus::Conflict
                }
            );
            assert_eq!(
                std::fs::read(workspace.path().join("note.txt")).unwrap(),
                if applied { b"saved".as_slice() } else { b"postchange".as_slice() }
            );
            *service.inner.controls.lock().unwrap() = None;
            service
                .with_controls(true, |store| {
                    let record = store.load(conversation)?.unwrap();
                    assert_eq!(
                        record.restores()[0].patch_digest(),
                        patch_digest,
                        "migration must not replace previously admitted authority"
                    );
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                service
                    .workbench_folder_command(actor, SessionId::new([15; 16]).unwrap(), &command)
                    .await,
                AppResponsePayload::WorkbenchRestore(receipt)
            );
        }
    });
}
