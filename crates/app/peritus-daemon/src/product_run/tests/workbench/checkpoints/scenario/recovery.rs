use super::*;

pub(super) async fn verify_files_only_undo(
    service: &ProductRunService,
    workspace: WorkspaceId,
    folder: &std::path::Path,
    recovery_checkpoint: ControlOperationId,
) {
    let current = service
        .with_controls(false, |store| store.load(DomainConversationId::new([2; 16])?))
        .unwrap()
        .unwrap()
        .revision();
    let request =
        WorkbenchRewindRequest::new(query(workspace), current, recovery_checkpoint).unwrap();
    let AppResponsePayload::WorkbenchRewindPreview(preview) =
        service.preview_workbench_rewind(actor(), &request).await
    else {
        panic!("recovery checkpoint preview");
    };
    assert_eq!(preview.paths()[0].disposition(), WorkbenchRewindDisposition::Restore);
    let undo = command(workspace, 0xe1, current, WorkbenchIntent::ApplyRewind(preview));
    let AppResponsePayload::WorkbenchRestore(undone) =
        service.workbench_folder_command(actor(), SessionId::new([0xd1; 16]).unwrap(), &undo).await
    else {
        panic!("undo rewind receipt");
    };
    assert_eq!(undone.status(), WorkbenchRestoreStatus::Applied);
    assert_eq!(fs::read(folder.join("note.txt")).unwrap(), b"Peritus owned edit\n");
}
