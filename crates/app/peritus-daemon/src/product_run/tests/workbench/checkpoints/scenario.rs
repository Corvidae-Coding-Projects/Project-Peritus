//! End-to-end covered-file and logical rewind fixture.

use super::*;

mod defaults;
mod restart;
pub(super) use defaults::{checkpoint_scenario, checkpoint_scenario_with_selection};

pub(super) async fn checkpoint_scenario_with_name(
    user_conflict: bool,
    crash: Option<crate::product_run::workbench::RewindFaultPoint>,
    mode: WorkbenchRewindMode,
    selection: WorkbenchFileRange,
    name: String,
) {
    let container = tempfile::tempdir().expect("container");
    let folder = container.path().join("folder");
    let state = container.path().join("state");
    let managed = container.path().join("managed");
    fs::create_dir_all(&folder).expect("folder");
    fs::create_dir_all(&state).expect("state");
    fs::create_dir_all(&managed).expect("managed");
    fs::write(folder.join("note.txt"), b"checkpoint baseline\n").expect("covered baseline");
    fs::write(folder.join("unrelated.txt"), b"unrelated exact bytes\n")
        .expect("unrelated baseline");
    fs::write(folder.join("peritus-workspace.toml"), b"schema_version = 1\nkind = \"artifact\"\n")
        .expect("artifact contract");

    let writer = scripted(0xb1, "checkpoint-edit", pipeline_responses());
    let workspace = WorkspaceId::new([0xb2; 16]).expect("workspace");
    let run = RunId::new([0xb3; 16]).expect("run");
    let mut service = service(&state, &managed, workspace, [&writer, &writer, &writer]);
    let folder = folder.canonicalize().expect("canonical folder");
    let identity = peritus_workspace::FolderIdentity::observe(&folder).expect("folder identity");
    let identity_hex = identity.digest().as_bytes().iter().fold(String::new(), |mut text, byte| {
        use std::fmt::Write as _;
        write!(&mut text, "{byte:02x}").expect("hex");
        text
    });
    let declaration = toml::from_str(&format!(
        "workspace_id = {:?}\nroot = {:?}\nidentity = {:?}\nwritable = true\nprotected_paths = []\n",
        "b2".repeat(16),
        folder.to_str().expect("UTF-8 folder"),
        identity_hex,
    ))
    .expect("folder declaration");
    Arc::get_mut(&mut service.inner)
        .expect("unique service")
        .workspaces
        .insert(workspace, folder.clone());
    Arc::get_mut(&mut service.inner)
        .expect("unique service")
        .folders
        .insert(workspace, declaration);

    queue(&service, workspace).await;
    let file_request = WorkbenchFileRequest::new(
        query(workspace),
        3,
        "note.txt".to_owned(),
        selection,
        WorkbenchFileMode::Snapshot,
        writer.profile.profile_id(),
        ProductModelChoice::default(),
    )
    .expect("whole-file selection");
    let AppResponsePayload::WorkbenchFilePreview(file_preview) =
        service.preview_workbench_file(actor(), &file_request).await
    else {
        panic!("whole-file preview was not returned")
    };
    let attach = command(
        workspace,
        8,
        3,
        WorkbenchIntent::AttachFile {
            preview: file_preview,
            text: WorkbenchInputText::new("Edit only this whole file.".to_owned())
                .expect("caption"),
        },
    );
    assert!(matches!(
        service.confirm_workbench_file(actor(), &attach).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));

    let checkpoint_command = command(
        workspace,
        9,
        4,
        WorkbenchIntent::CreateCheckpoint(WorkbenchCheckpointName::new(name).expect("name")),
    );
    restart::assert_legacy_manifest_admission(&service, &checkpoint_command, &folder).await;
    let AppResponsePayload::WorkbenchCheckpoint(checkpoint_receipt) =
        restart::create_checkpoint_compatible(&service, &checkpoint_command, selection).await
    else {
        panic!("checkpoint receipt was not returned")
    };
    assert_eq!(checkpoint_receipt.accepted_revision(), 5);
    assert_eq!(checkpoint_receipt.paths().len(), 1);
    assert_eq!(checkpoint_receipt.paths()[0].path(), "note.txt");
    assert_eq!(
        checkpoint_receipt.paths()[0].ranges().is_empty(),
        selection == WorkbenchFileRange::All
    );
    assert!(!checkpoint_receipt.external_effects().is_empty());

    let start = command(
        workspace,
        10,
        5,
        WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
            run,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                writer.profile.profile_id(),
                writer.profile.profile_id(),
            ),
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
        )),
    );
    assert!(matches!(
        service.workbench_command(actor(), &start).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    let completed = wait_for_terminal(&service, run).await;
    assert_eq!(completed.phase(), ProductRunPhase::Complete, "{}", completed.summary());
    assert_eq!(fs::read(folder.join("note.txt")).expect("owned edit"), b"Peritus owned edit\n");
    assert_eq!(
        fs::read(folder.join("unrelated.txt")).expect("unrelated after owned edit"),
        b"unrelated exact bytes\n"
    );

    // A verification command observes all changed scoped files again without editing them.
    let start_binding =
        service.inner.records.read().unwrap().get(&run).unwrap().interaction.workbench.clone();
    let postimage = peritus_product_runner::control::CheckpointFileVersion::present(
        peritus_codec::sha256(b"Peritus owned edit\n"),
        19,
        peritus_product_runner::control::CheckpointFileMode::Regular,
    );
    service
        .seal_automatic_checkpoint(
            &start_binding,
            run,
            std::path::Path::new("note.txt"),
            peritus_product_runner::WorkspaceMutationKind::File,
            postimage,
        )
        .expect("repeated unchanged command postimage remains idempotent");

    if user_conflict {
        fs::write(folder.join("note.txt"), b"independent user edit\n")
            .expect("independent user edit");
    }
    let mut revision = service
        .with_controls(false, |store| store.load(DomainConversationId::new([2; 16])?))
        .expect("load")
        .expect("record")
        .revision();
    if mode == WorkbenchRewindMode::ConversationOnly {
        let narrowed = command(
            workspace,
            0xed,
            revision,
            WorkbenchIntent::SetPermissions(peritus_app_protocol::WorkbenchPermissionChange::new(
                0,
                peritus_app_protocol::WorkbenchPermissionCapability::Write,
                false,
            )),
        );
        assert!(matches!(
            service.workbench_command(actor(), &narrowed).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        revision += 1;
    }
    let mut rewind_request =
        WorkbenchRewindRequest::new(query(workspace), revision, checkpoint_receipt.checkpoint())
            .expect("rewind request");
    let child = ConversationId::new([0xef; 16]).unwrap();
    if mode != WorkbenchRewindMode::FilesOnly {
        rewind_request = rewind_request.with_branch(mode, child).unwrap();
    }
    let AppResponsePayload::WorkbenchRewindPreview(rewind_preview) =
        service.preview_workbench_rewind(actor(), &rewind_request).await
    else {
        panic!("rewind preview was not returned")
    };
    assert_eq!(
        rewind_preview.paths().len(),
        usize::from(mode != WorkbenchRewindMode::ConversationOnly)
    );
    if let Some(path) = rewind_preview.paths().first() {
        assert_eq!(
            path.disposition(),
            if user_conflict {
                WorkbenchRewindDisposition::Conflict
            } else {
                WorkbenchRewindDisposition::Restore
            }
        );
    }

    let restore_seed = match (user_conflict, crash) {
        (true, _) => 11,
        (false, None) => 12,
        (false, Some(crate::product_run::workbench::RewindFaultPoint::AfterPrepare)) => 13,
        (false, Some(crate::product_run::workbench::RewindFaultPoint::AfterFolderPatch)) => 14,
        (false, Some(crate::product_run::workbench::RewindFaultPoint::InsideFolderPatch)) => 15,
    };
    let apply = WorkbenchCommand::new(
        ControlOperationId::new(
            [restore_seed
                + match mode {
                    WorkbenchRewindMode::FilesOnly => 0,
                    WorkbenchRewindMode::ConversationOnly => 10,
                    WorkbenchRewindMode::Combined => 20,
                }; 16],
        )
        .expect("restore operation"),
        query(workspace),
        revision,
        WorkbenchIntent::ApplyRewind(rewind_preview),
    );
    service
        .authorize_workbench_request(
            actor(),
            &peritus_app_protocol::AppRequestPayload::WorkbenchCommand(apply.clone()),
        )
        .expect("A3 rewind permission admission");
    let checkpoint_count = service
        .with_controls(false, |store| {
            Ok(store
                .load(DomainConversationId::new([2; 16])?)?
                .ok_or(ControlError::NotFound)?
                .checkpoints()
                .len())
        })
        .expect("checkpoint count before rewind");
    if let Some(point) = crash {
        service.inject_rewind_fault(apply.operation().into_bytes(), point);
        assert!(matches!(
            service
                .workbench_folder_command(actor(), SessionId::new([0xd1; 16]).unwrap(), &apply)
                .await,
            AppResponsePayload::Error(_)
        ));
        let prepared = service
            .with_controls(false, |store| store.load(DomainConversationId::new([2; 16])?))
            .expect("load prepared restore")
            .expect("record");
        assert_eq!(
            prepared.restores().last().expect("prepared restore").status(),
            peritus_product_runner::control::RestoreStatus::Prepared
        );
        support::assert_receipt_is_observational(&service, &folder, &apply);
        if point == crate::product_run::workbench::RewindFaultPoint::InsideFolderPatch {
            support::remove_injected_incomplete_transaction(&service.inner.directory);
        }
        service
            .with_controls(false, |store| {
                assert!(store.load(DomainConversationId::new(child.into_bytes())?)?.is_none());
                Ok(())
            })
            .unwrap();
    }
    let response = service
        .workbench_folder_command(actor(), SessionId::new([0xd1; 16]).unwrap(), &apply)
        .await;
    let AppResponsePayload::WorkbenchRestore(restore) = response else {
        panic!("durable restore receipt was not returned: {response:?}")
    };
    let expected_status = if mode != WorkbenchRewindMode::ConversationOnly
        && (user_conflict
            || matches!(
                crash,
                Some(
                    crate::product_run::workbench::RewindFaultPoint::AfterPrepare
                        | crate::product_run::workbench::RewindFaultPoint::InsideFolderPatch
                )
            )) {
        WorkbenchRestoreStatus::Conflict
    } else {
        WorkbenchRestoreStatus::Applied
    };
    assert_eq!(restore.status(), expected_status);
    let restored_expected = match selection {
        WorkbenchFileRange::Bytes { .. } => b"checkpointned edit\n".as_slice(),
        _ => b"checkpoint baseline\n".as_slice(),
    };
    let expected = if user_conflict {
        b"independent user edit\n".as_slice()
    } else if mode == WorkbenchRewindMode::ConversationOnly
        || matches!(
            crash,
            Some(
                crate::product_run::workbench::RewindFaultPoint::AfterPrepare
                    | crate::product_run::workbench::RewindFaultPoint::InsideFolderPatch
            )
        )
    {
        b"Peritus owned edit\n".as_slice()
    } else {
        restored_expected
    };
    assert_eq!(fs::read(folder.join("note.txt")).expect("terminal covered bytes"), expected);
    assert_eq!(
        fs::read(folder.join("unrelated.txt")).expect("terminal unrelated bytes"),
        b"unrelated exact bytes\n"
    );
    if mode == WorkbenchRewindMode::ConversationOnly {
        service
            .with_controls(false, |store| {
                let record = store
                    .load(DomainConversationId::new([2; 16])?)?
                    .ok_or(ControlError::NotFound)?;
                assert_eq!(record.checkpoints().len(), checkpoint_count + 1);
                let recovery = record.checkpoints().last().expect("rewind recovery metadata");
                assert!(recovery.paths().is_empty());
                Ok(())
            })
            .expect("conversation-only rewind retained no filesystem capture");
    }
    if expected_status == WorkbenchRestoreStatus::Conflict {
        assert_eq!(restore.conflicts(), &["note.txt".to_owned()]);
        assert!(restore.restored().is_empty());
    } else {
        if mode == WorkbenchRewindMode::ConversationOnly {
            assert!(restore.restored().is_empty());
        } else {
            assert_eq!(restore.restored(), &["note.txt".to_owned()]);
        }
        assert!(restore.conflicts().is_empty());
    }
    assert_eq!(
        service.workbench_receipt(actor(), &checkpoint_command),
        AppResponsePayload::WorkbenchCheckpoint(checkpoint_receipt.clone()),
        "checkpoint receipt lookup must replay the originally accepted revision"
    );
    assert_eq!(
        service.workbench_command(actor(), &checkpoint_command).await,
        AppResponsePayload::WorkbenchCheckpoint(checkpoint_receipt.clone()),
        "checkpoint retry must be idempotent after later revisions"
    );
    assert_eq!(
        service.workbench_receipt(actor(), &apply),
        AppResponsePayload::WorkbenchRestore(restore.clone()),
        "restore receipt lookup must replay its terminal journal result"
    );
    assert_eq!(
        service
            .workbench_folder_command(actor(), SessionId::new([0xd1; 16]).unwrap(), &apply)
            .await,
        AppResponsePayload::WorkbenchRestore(restore.clone()),
        "restore retry must not reapply filesystem effects"
    );

    if mode == WorkbenchRewindMode::FilesOnly && expected_status == WorkbenchRestoreStatus::Applied
    {
        let current = service
            .with_controls(false, |store| store.load(DomainConversationId::new([2; 16])?))
            .unwrap()
            .unwrap()
            .revision();
        let request =
            WorkbenchRewindRequest::new(query(workspace), current, restore.recovery_checkpoint())
                .unwrap();
        let AppResponsePayload::WorkbenchRewindPreview(preview) =
            service.preview_workbench_rewind(actor(), &request).await
        else {
            panic!("recovery checkpoint preview");
        };
        assert_eq!(preview.paths()[0].disposition(), WorkbenchRewindDisposition::Restore);
        let undo = command(workspace, 0xe1, current, WorkbenchIntent::ApplyRewind(preview));
        let AppResponsePayload::WorkbenchRestore(undone) = service
            .workbench_folder_command(actor(), SessionId::new([0xd1; 16]).unwrap(), &undo)
            .await
        else {
            panic!("undo rewind receipt");
        };
        assert_eq!(undone.status(), WorkbenchRestoreStatus::Applied);
        assert_eq!(fs::read(folder.join("note.txt")).unwrap(), b"Peritus owned edit\n");
    }

    service.shutdown().await.expect("shutdown product runs");
    drop(service);
    restart::verify(&state, child, mode, expected_status, &checkpoint_receipt);
}
