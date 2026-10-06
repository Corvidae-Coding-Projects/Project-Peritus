//! Replay-only automatic before-images remain inspectable and rewindable across restart.

use super::*;

#[test]
fn automatic_checkpoint_rewinds_after_the_user_projection_is_bypassed() {
    interaction::block_on(run(false, false));
}

#[test]
fn automatic_checkpoint_above_inline_file_limit_runs_and_rewinds_exactly() {
    interaction::block_on(run(true, false));
}

#[test]
fn command_checkpoint_above_inline_file_limit_runs_and_rewinds_exactly() {
    interaction::block_on(run(true, true));
}

#[test]
fn command_checkpoint_fixture() {
    use std::io::Write as _;
    if !std::path::Path::new(".peritus-checkpoint-command-fixture").is_file() {
        return;
    }
    fs::OpenOptions::new()
        .append(true)
        .open("note.txt")
        .unwrap()
        .write_all(b"Peritus owned edit\n")
        .unwrap();
}

async fn run(large: bool, use_command: bool) {
    let container = tempfile::tempdir().expect("container");
    let folder = container.path().join("folder");
    let state = container.path().join("state");
    let managed = container.path().join("managed");
    fs::create_dir_all(&folder).unwrap();
    fs::create_dir_all(&state).unwrap();
    fs::create_dir_all(&managed).unwrap();
    fs::write(folder.join("note.txt"), b"checkpoint baseline\n").unwrap();
    fs::write(folder.join("peritus-workspace.toml"), b"schema_version = 1\nkind = \"artifact\"\n")
        .unwrap();

    let mut responses = pipeline_responses();
    if large {
        for index in [2, 5, 9] {
            responses[index] = crate::product_run::tests::support::named_tool_response(
                "workspace_read",
                br#"{"path":"note.txt","start_line":1,"end_line":1}"#.to_vec(),
            );
        }
    }
    if use_command {
        fs::write(folder.join(".peritus-checkpoint-command-fixture"), b"run").unwrap();
        responses[6] = crate::product_run::tests::support::named_tool_response(
            "run_command",
            serde_json::to_vec(&serde_json::json!({
                "program": std::env::current_exe().unwrap(),
                "args": ["--exact", "product_run::tests::workbench::checkpoints::automatic::command_checkpoint_fixture", "--nocapture"],
                "cwd": ".", "purpose": "verification"
            })).unwrap(),
        );
    }
    let writer = scripted(0xc1, "automatic-checkpoint-edit", responses);
    let workspace = WorkspaceId::new([0xc2; 16]).unwrap();
    let run = RunId::new([0xc3; 16]).unwrap();
    let mut service = service(&state, &managed, workspace, [&writer, &writer, &writer]);
    let folder = folder.canonicalize().unwrap();
    let identity = peritus_workspace::FolderIdentity::observe(&folder).unwrap();
    let identity_hex = identity.digest().as_bytes().iter().fold(String::new(), |mut text, byte| {
        use std::fmt::Write as _;
        write!(&mut text, "{byte:02x}").unwrap();
        text
    });
    let declaration = toml::from_str(&format!(
        "workspace_id = {:?}\nroot = {:?}\nidentity = {:?}\nwritable = true\nprotected_paths = []\n",
        "c2".repeat(16),
        folder.to_str().unwrap(),
        identity_hex,
    ))
    .unwrap();
    Arc::get_mut(&mut service.inner).unwrap().workspaces.insert(workspace, folder.clone());
    Arc::get_mut(&mut service.inner).unwrap().folders.insert(workspace, declaration);

    queue(&service, workspace).await;
    let file_request = WorkbenchFileRequest::new(
        query(workspace),
        3,
        "note.txt".to_owned(),
        WorkbenchFileRange::All,
        WorkbenchFileMode::Snapshot,
        writer.profile.profile_id(),
        ProductModelChoice::default(),
    )
    .unwrap();
    let AppResponsePayload::WorkbenchFilePreview(file_preview) =
        service.preview_workbench_file(actor(), &file_request).await
    else {
        panic!("whole-file preview")
    };
    let attach = command(
        workspace,
        8,
        3,
        WorkbenchIntent::AttachFile {
            preview: file_preview,
            text: WorkbenchInputText::new("Edit only this whole file.".to_owned()).unwrap(),
        },
    );
    assert!(matches!(
        service.confirm_workbench_file(actor(), &attach).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    let baseline = if large {
        let mut bytes = b"checkpoint baseline\n".to_vec();
        bytes.resize(peritus_patch::MAX_FILE_BYTES + 1, b'x');
        *bytes.last_mut().unwrap() = 0;
        bytes
    } else {
        b"checkpoint baseline\n".to_vec()
    };
    fs::write(folder.join("note.txt"), &baseline).unwrap();
    let start = command(
        workspace,
        10,
        4,
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
    let terminal = wait_for_terminal(&service, run).await;
    assert_eq!(terminal.phase(), ProductRunPhase::Complete, "{terminal:?}");
    let expected_postimage = if use_command {
        let mut bytes = baseline.clone();
        bytes.extend_from_slice(b"Peritus owned edit\n");
        assert!(bytes.len() > peritus_patch::MAX_FILE_BYTES);
        bytes
    } else {
        b"Peritus owned edit\n".to_vec()
    };
    assert_eq!(fs::read(folder.join("note.txt")).unwrap(), expected_postimage);

    let checkpoint = automatic_checkpoint_id(run, "note.txt");
    let revision = service
        .with_controls(false, |store| {
            let record = store.load(DomainConversationId::new([2; 16])?)?.unwrap();
            assert!(record.checkpoints().is_empty(), "automatic checkpoint bypasses projection");
            assert!(store.load_checkpoint(record.id(), checkpoint)?.is_some());
            Ok(record.revision())
        })
        .unwrap();
    let child = ConversationId::new([0xc6; 16]).unwrap();
    let request = WorkbenchRewindRequest::new(
        query(workspace),
        revision,
        ControlOperationId::new(*checkpoint.as_bytes()).unwrap(),
    )
    .unwrap()
    .with_branch(WorkbenchRewindMode::Combined, child)
    .unwrap();
    let AppResponsePayload::WorkbenchRewindPreview(preview) =
        service.preview_workbench_rewind(actor(), &request).await
    else {
        panic!("automatic rewind preview")
    };
    assert_eq!(preview.paths()[0].disposition(), WorkbenchRewindDisposition::Restore);
    let apply = command(workspace, 0xc4, revision, WorkbenchIntent::ApplyRewind(preview));
    let AppResponsePayload::WorkbenchRestore(receipt) = service
        .workbench_folder_command(actor(), SessionId::new([0xc5; 16]).unwrap(), &apply)
        .await
    else {
        panic!("automatic rewind receipt")
    };
    assert_eq!(receipt.status(), WorkbenchRestoreStatus::Applied);
    assert_eq!(fs::read(folder.join("note.txt")).unwrap(), baseline);

    service.shutdown(Duration::from_secs(5)).await;
    drop(service);
    let store_id = peritus_journal::StoreId::new([0x7f; 16]).unwrap();
    let journal =
        crate::product_control::ControlStore::open(&state.join("workbench-v1"), store_id).unwrap();
    let conversation = DomainConversationId::new([2; 16]).unwrap();
    let recovered = journal.load_checkpoint(conversation, checkpoint).unwrap().unwrap();
    let owned = recovered.paths()[0].owned_postchange().unwrap();
    assert_eq!(owned.digest(), Some(peritus_codec::sha256(&expected_postimage)));
    assert_eq!(owned.bytes(), Some(expected_postimage.len() as u64));
    assert_eq!(
        journal.load(conversation).unwrap().unwrap().restores().last().unwrap().status(),
        peritus_product_runner::control::RestoreStatus::Applied
    );
    assert!(
        journal.load(DomainConversationId::new(child.into_bytes()).unwrap()).unwrap().is_some()
    );
}

pub(super) fn automatic_checkpoint_id(run: RunId, path: &str) -> CheckpointId {
    let mut bytes = b"peritus-workbench-automatic-checkpoint-v1\0".to_vec();
    bytes.extend_from_slice(run.as_bytes());
    bytes.push(1);
    bytes.extend_from_slice(path.as_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    CheckpointId::new(id).unwrap()
}
