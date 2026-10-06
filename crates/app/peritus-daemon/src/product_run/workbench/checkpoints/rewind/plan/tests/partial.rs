//! Cold daemon reopen keeps the selected capability and exact merged C1 transaction identity.

use super::*;
use peritus_product_runner::control::{CheckpointRange, FileRange};

#[test]
fn partial_restore_cold_reopens_before_and_after_c1_and_replays_the_same_receipt() {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        for applied in [false, true] {
            let workspace = tempfile::tempdir().unwrap();
            let state = tempfile::tempdir().unwrap();
            let workspace_id = WorkspaceId::new([4; 16]).unwrap();
            let actor = ActorId::new([1; 16]).unwrap();
            let conversation = DomainConversationId::new([2; 16]).unwrap();
            let query = WorkbenchQuery::new(ConversationId::new([2; 16]).unwrap(), workspace_id);
            let make_service = || {
                crate::product_run::tests::checkpoint_test_service(
                    state.path(),
                    workspace.path(),
                    workspace_id,
                )
            };
            let service = make_service();
            let saved = b"saved prefix\nold\nsaved suffix\n";
            let current = b"owned prefix\nlong edited line\nowned suffix\n";
            let merged = b"owned prefix\nold\nowned suffix\n";
            std::fs::write(workspace.path().join("note.txt"), current).unwrap();
            let version = |bytes: &[u8]| {
                CheckpointFileVersion::present(
                    peritus_codec::sha256(bytes),
                    bytes.len() as u64,
                    CheckpointFileMode::Regular,
                )
            };
            let checkpoint = UserCheckpoint::new(
                CheckpointId::new([11; 16]).unwrap(),
                "selected line".to_owned(),
                CheckpointReferences::new(1, 0, 0, None),
                vec![
                    CheckpointPath::selected_ranges(
                        "note.txt".to_owned(),
                        version(saved),
                        vec![
                            CheckpointRange::new(FileRange::Lines { first: 2, last: 2 }, 13, 17)
                                .unwrap(),
                        ],
                    )
                    .unwrap(),
                ],
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
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
            service
                .with_controls(true, |store| {
                    store.accept(&operation(
                        10,
                        0,
                        ControlIntent::CreateConversation {
                            title: ControlText::new("partial recovery".to_owned())?,
                        },
                    ))?;
                    store.accept_checkpoint(
                        &operation(11, 1, ControlIntent::CreateCheckpoint(checkpoint.clone())),
                        &[Some(saved.to_vec())],
                    )?;
                    store.accept(&operation(
                        12,
                        2,
                        ControlIntent::SealCheckpoint {
                            checkpoint: checkpoint.id(),
                            run: [5; 16],
                            versions: vec![("note.txt".to_owned(), version(current))],
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
                panic!("selected preview");
            };
            let command = WorkbenchCommand::new(
                ControlOperationId::new([13; 16]).unwrap(),
                query,
                3,
                WorkbenchIntent::ApplyRewind(preview),
            );
            service.inject_rewind_fault(
                command.operation().into_bytes(),
                if applied {
                    crate::product_run::workbench::RewindFaultPoint::AfterFolderPatch
                } else {
                    crate::product_run::workbench::RewindFaultPoint::AfterPrepare
                },
            );
            let session = SessionId::new([15; 16]).unwrap();
            assert!(matches!(
                service.workbench_folder_command(actor, session, &command).await,
                AppResponsePayload::Error(_)
            ));
            let retained = service
                .with_controls(false, |store| {
                    let identity = peritus_workspace::FolderIdentity::observe(workspace.path())?;
                    let observed = crate::product_run::workbench::checkpoints::capture::observe_path(&identity, "note.txt")?;
                    assert_eq!(selected_coverage_matches(store, checkpoint.id(), 0,
                        &checkpoint.paths()[0], &observed).is_ok(), applied,
                        "writable fork coverage follows selected content without claiming surrounding bytes");
                    let record = store.load(conversation)?.unwrap();
                    let restore = record.restores().last().unwrap();
                    assert_eq!(restore.targets().unwrap()[0].checkpoint(), version(merged));
                    Ok((restore.patch_digest(), restore.preview_digest()))
                })
                .unwrap();
            drop(service);
            let service = make_service();
            service.with_controls(true, |_| Ok(())).unwrap();
            let response = service.workbench_folder_command(actor, session, &command).await;
            let AppResponsePayload::WorkbenchRestore(receipt) = response else {
                panic!("recovered: {response:?}");
            };
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
                if applied { merged.as_slice() } else { current.as_slice() }
            );
            drop(service);
            let service = make_service();
            service
                .with_controls(true, |store| {
                    let record = store.load(conversation)?.unwrap();
                    let restore = record.restores().last().unwrap();
                    assert_eq!((restore.patch_digest(), restore.preview_digest()), retained);
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                service.workbench_receipt(actor, &command),
                AppResponsePayload::WorkbenchRestore(receipt.clone())
            );
            assert_eq!(
                service.workbench_folder_command(actor, session, &command).await,
                AppResponsePayload::WorkbenchRestore(receipt)
            );
        }
    });
}
