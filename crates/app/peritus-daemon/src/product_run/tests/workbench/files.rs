//! Preview/confirm through the same service used by A3, then actual governed runner admission.
use super::*;
use peritus_app_protocol::{
    ProductModelChoice, WorkbenchFileMode, WorkbenchFileRange, WorkbenchFileRequest,
};

#[test]
fn file_snapshot_and_refresh_modes_deliver_the_right_version_through_the_real_runner() {
    for refresh in [false, true] {
        interaction::block_on(scenario(refresh));
    }
}
async fn scenario(refresh: bool) {
    let repository = repository();
    let path = repository.path().join("reference.txt");
    fs::write(&path, "UNSELECTED_FIRST\nORIGINAL_REFERENCE\nUNSELECTED_LAST\n").expect("source");
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(0x61, "chat", vec![support::text_response(b"Reference received.")]);
    let reviewer = scripted(0x62, "review", Vec::new());
    let fixer = scripted(0x63, "fix", Vec::new());
    let workspace = WorkspaceId::new([0x64; 16]).expect("workspace");
    let run = RunId::new([0x65; 16]).expect("run");
    let service = service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
    queue(&service, workspace).await;
    let selection = WorkbenchFileRequest::new(
        query(workspace),
        3,
        "reference.txt".to_owned(),
        WorkbenchFileRange::Lines { first: 2, last: 2 },
        if refresh { WorkbenchFileMode::RefreshOnRequest } else { WorkbenchFileMode::Snapshot },
        writer.profile.profile_id(),
        ProductModelChoice::default(),
    )
    .expect("selection");
    let denied = service
        .preview_workbench_file(ActorId::new([99; 16]).expect("other actor"), &selection)
        .await;
    assert!(matches!(denied, AppResponsePayload::Error(_)));
    let AppResponsePayload::WorkbenchFilePreview(preview) =
        service.preview_workbench_file(actor(), &selection).await
    else {
        panic!("preview")
    };
    assert!(writer.requests.lock().expect("requests").is_empty());
    let attach = command(
        workspace,
        8,
        3,
        WorkbenchIntent::AttachFile {
            preview,
            text: WorkbenchInputText::new("Use this selected reference.".to_owned())
                .expect("caption"),
        },
    );
    assert!(matches!(
        service.confirm_workbench_file(actor(), &attach).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    fs::write(&path, "UNSELECTED_FIRST\nREFRESHED_REFERENCE\nUNSELECTED_LAST\n")
        .expect("edit source");
    let settings = start(workspace, run, [&writer, &reviewer, &fixer]);
    let start = command(workspace, 7, 4, settings.intent().clone());
    assert!(matches!(
        service.workbench_command(actor(), &start).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
    let terminal = wait_for_terminal(&service, run).await;
    assert_eq!(terminal.phase(), ProductRunPhase::WaitingForUser, "{}", terminal.summary());
    let text = {
        let requests = writer.requests.lock().expect("requests");
        assert_eq!(requests.len(), 1, "stale preparation must not send an obsolete request");
        requests[0]
            .messages()
            .iter()
            .flat_map(peritus_model_protocol::Message::content)
            .filter_map(|block| match block {
                peritus_model_protocol::ContentBlock::Text(text) => Some(text.expose_for_wire()),
                _ => None,
            })
            .collect::<String>()
    };
    let (included, excluded) = if refresh {
        ("REFRESHED_REFERENCE", "ORIGINAL_REFERENCE")
    } else {
        ("ORIGINAL_REFERENCE", "REFRESHED_REFERENCE")
    };
    assert!(text.contains(included), "expected selected source in actual provider request");
    assert!(!text.contains(excluded));
    assert!(!text.contains("UNSELECTED_FIRST"));
    assert!(!text.contains("UNSELECTED_LAST"));
    let record = service
        .with_controls(false, |store| {
            store.load(peritus_product_runner::control::ConversationId::new([2; 16])?)
        })
        .expect("load")
        .expect("record");
    assert_eq!(record.files().entries()[0].refreshes().len(), usize::from(refresh));
    assert_eq!(record.inputs().invocations().len(), 1);
    service.shutdown(Duration::from_secs(5)).await;
}
