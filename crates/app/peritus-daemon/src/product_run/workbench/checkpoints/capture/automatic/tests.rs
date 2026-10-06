//! Production directory capture, archive replay, exact rewind and recovery-checkpoint undo.

use super::*;
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, WorkbenchCommand, WorkbenchIntent, WorkbenchQuery,
    WorkbenchRestoreStatus, WorkbenchRewindRequest,
};
use peritus_product_runner::control::{CheckpointId, CheckpointReferences, ControlText};
use peritus_types::SessionId;
use peritus_workspace::FolderIdentity;

#[test]
fn automatic_empty_directory_capture_restores_permissions_and_can_be_undone() {
    directory_scenario(false, OwnedNode::Absent);
}
#[test]
fn directory_restore_reopens_after_c1_apply_and_returns_the_same_exact_receipt() {
    directory_scenario(true, OwnedNode::Absent);
}

#[test]
fn directory_restore_replaces_an_owned_file_and_recovery_restores_that_file() {
    directory_scenario(false, OwnedNode::File);
}
#[test]
fn directory_replacement_cold_reopens_after_c1_without_losing_its_file_preimage() {
    directory_scenario(true, OwnedNode::File);
}
#[cfg(unix)]
#[test]
fn directory_restore_and_undo_restore_each_exact_permission_version() {
    directory_scenario(false, OwnedNode::Directory);
}

#[derive(Clone, Copy)]
enum OwnedNode {
    Absent,
    File,
    #[cfg(unix)]
    Directory,
}

fn directory_scenario(interrupted: bool, owned_node: OwnedNode) {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let target = workspace.path().join("empty");
        std::fs::create_dir(&target).unwrap();
        set_permissions(&target);
        let permissions = saved_permissions();
        let workspace_id = WorkspaceId::new([4; 16]).unwrap();
        let actor = ActorId::new([1; 16]).unwrap();
        let conversation = ConversationId::new([2; 16]).unwrap();
        let run = RunId::new([5; 16]).unwrap();
        let session = SessionId::new([6; 16]).unwrap();
        let query = WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new([2; 16]).unwrap(),
            workspace_id,
        );
        let make_service = || {
            crate::product_run::tests::checkpoint_test_service(
                state.path(),
                workspace.path(),
                workspace_id,
            )
        };
        let mut service = make_service();
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
                        title: ControlText::new("directory rewind".to_owned())?,
                    },
                ))
            })
            .unwrap();
        assert_eq!(
            service
                .capture_automatic_checkpoint_for_operation(
                    actor,
                    conversation,
                    workspace_id,
                    run,
                    "empty",
                    WorkspaceMutationKind::EmptyDirectory,
                    Some(1)
                )
                .unwrap(),
            2
        );
        let checkpoint =
            automatic_checkpoint_id(run, "empty", WorkspaceMutationKind::EmptyDirectory).unwrap();
        service
            .with_controls(false, |store| {
                let captured = store.load_checkpoint(conversation, checkpoint)?.unwrap();
                assert_eq!(captured.paths().len(), 1);
                assert_eq!(
                    captured.paths()[0].checkpoint(),
                    CheckpointFileVersion::EmptyDirectory { permissions }
                );
                assert!(captured.exclusions().next().is_none());
                Ok(())
            })
            .unwrap();
        std::fs::remove_dir(&target).unwrap();
        let owned = match owned_node {
            OwnedNode::Absent => CheckpointFileVersion::Absent,
            OwnedNode::File => {
                let bytes = b"owned file replacement";
                std::fs::write(&target, bytes).unwrap();
                CheckpointFileVersion::present(
                    peritus_codec::sha256(bytes),
                    bytes.len() as u64,
                    CheckpointFileMode::Regular,
                )
            }
            #[cfg(unix)]
            OwnedNode::Directory => {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::create_dir(&target).unwrap();
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
                CheckpointFileVersion::EmptyDirectory { permissions: 0o700 }
            }
        };
        service
            .with_controls(false, |store| {
                store.accept(&operation(
                    11,
                    2,
                    ControlIntent::SealAutomaticCheckpoint {
                        checkpoint,
                        run: run.into_bytes(),
                        versions: vec![("empty".to_owned(), owned)],
                    },
                ))
            })
            .unwrap();
        drop(service);
        service = make_service();
        service.with_controls(true, |_| Ok(())).unwrap();
        // Repeating capture reuses the accepted manifest even though the target is now absent.
        assert_eq!(
            service
                .capture_automatic_checkpoint_for_operation(
                    actor,
                    conversation,
                    workspace_id,
                    run,
                    "empty",
                    WorkspaceMutationKind::EmptyDirectory,
                    None
                )
                .unwrap(),
            3
        );
        let request = WorkbenchRewindRequest::new(
            query,
            3,
            ControlOperationId::new(*checkpoint.as_bytes()).unwrap(),
        )
        .unwrap();
        let response = service.preview_workbench_rewind(actor, &request).await;
        let AppResponsePayload::WorkbenchRewindPreview(preview) = response else {
            panic!("directory preview: {response:?}");
        };
        assert_eq!(
            preview.paths()[0].checkpoint(),
            peritus_app_protocol::WorkbenchCheckpointVersion::EmptyDirectory { permissions }
        );
        let command = WorkbenchCommand::new(
            ControlOperationId::new([12; 16]).unwrap(),
            query,
            3,
            WorkbenchIntent::ApplyRewind(preview),
        );
        if interrupted {
            crate::product_run::workbench::inject_rewind_fault(
                command.operation().into_bytes(),
                crate::product_run::workbench::RewindFaultPoint::AfterFolderPatch,
            );
            assert!(matches!(
                service.workbench_folder_command(actor, session, &command).await,
                AppResponsePayload::Error(_)
            ));
            drop(service);
            service = make_service();
            service.with_controls(true, |_| Ok(())).unwrap();
        }
        let response = service.workbench_folder_command(actor, session, &command).await;
        let AppResponsePayload::WorkbenchRestore(receipt) = response else {
            panic!("directory restore: {response:?}");
        };
        assert_eq!(receipt.status(), WorkbenchRestoreStatus::Applied);
        assert!(target.is_dir());
        assert!(std::fs::read_dir(&target).unwrap().next().is_none());
        assert_permissions(&target);
        assert_eq!(
            service.workbench_receipt(actor, &command),
            AppResponsePayload::WorkbenchRestore(receipt.clone())
        );
        let request = WorkbenchRewindRequest::new(
            query,
            receipt.accepted_revision(),
            receipt.recovery_checkpoint(),
        )
        .unwrap();
        let response = service.preview_workbench_rewind(actor, &request).await;
        let AppResponsePayload::WorkbenchRewindPreview(preview) = response else {
            panic!("directory undo preview: {response:?}");
        };
        let undo = WorkbenchCommand::new(
            ControlOperationId::new([13; 16]).unwrap(),
            query,
            receipt.accepted_revision(),
            WorkbenchIntent::ApplyRewind(preview),
        );
        let response = service.workbench_folder_command(actor, session, &undo).await;
        let AppResponsePayload::WorkbenchRestore(receipt) = response else {
            panic!("directory undo: {response:?}");
        };
        assert_eq!(receipt.status(), WorkbenchRestoreStatus::Applied);
        let identity = FolderIdentity::observe(workspace.path()).unwrap();
        assert_eq!(super::super::observe_version(&identity, "empty").unwrap(), owned);
    });
}

#[test]
fn previously_accepted_directory_exclusion_is_reused_without_claiming_new_coverage() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let workspace_id = WorkspaceId::new([4; 16]).unwrap();
    let actor = ActorId::new([1; 16]).unwrap();
    let conversation = ConversationId::new([2; 16]).unwrap();
    let run = RunId::new([5; 16]).unwrap();
    let service = crate::product_run::tests::checkpoint_test_service(
        state.path(),
        workspace.path(),
        workspace_id,
    );
    let checkpoint =
        automatic_checkpoint_id(run, "empty", WorkspaceMutationKind::EmptyDirectory).unwrap();
    service
        .with_controls(true, |store| {
            store.accept(&ControlOperation::new(
                OperationId::new([10; 16])?,
                conversation,
                actor,
                workspace_id,
                0,
                ControlIntent::CreateConversation {
                    title: ControlText::new("legacy directory".to_owned())?,
                },
            ))?;
            let legacy = UserCheckpoint::automatic(
                checkpoint,
                AUTOMATIC_CHECKPOINT_NAME.to_owned(),
                CheckpointReferences::new(1, 0, 0, None),
                Vec::new(),
                vec![super::super::empty_directory_exclusion("empty")],
                Vec::new(),
                run.into_bytes(),
            )?;
            store.accept_checkpoint(
                &ControlOperation::new(
                    OperationId::new(*checkpoint.as_bytes())?,
                    conversation,
                    actor,
                    workspace_id,
                    1,
                    ControlIntent::CreateAutomaticCheckpoint(legacy),
                ),
                &[],
            )
        })
        .unwrap();
    drop(service);
    let service = crate::product_run::tests::checkpoint_test_service(
        state.path(),
        workspace.path(),
        workspace_id,
    );
    service.with_controls(true, |_| Ok(())).unwrap();
    assert_eq!(
        service
            .capture_automatic_checkpoint_for_operation(
                actor,
                conversation,
                workspace_id,
                run,
                "empty",
                WorkspaceMutationKind::EmptyDirectory,
                None
            )
            .unwrap(),
        2
    );
    service
        .with_controls(false, |store| {
            let legacy = store
                .load_checkpoint(conversation, CheckpointId::new(*checkpoint.as_bytes())?)?
                .unwrap();
            assert!(legacy.paths().is_empty());
            assert_eq!(
                legacy.exclusions().next(),
                Some("empty: empty directory removal cannot restore the directory")
            );
            Ok(())
        })
        .unwrap();
}

#[cfg(unix)]
const fn saved_permissions() -> u16 {
    0o750
}
#[cfg(not(unix))]
const fn saved_permissions() -> u16 {
    0o777
}
#[cfg(unix)]
fn set_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(u32::from(saved_permissions())))
        .unwrap();
}
#[cfg(not(unix))]
fn set_permissions(_path: &Path) {}
#[cfg(unix)]
fn assert_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777,
        u32::from(saved_permissions())
    );
}
#[cfg(not(unix))]
fn assert_permissions(path: &Path) {
    assert!(!std::fs::metadata(path).unwrap().permissions().readonly());
}
