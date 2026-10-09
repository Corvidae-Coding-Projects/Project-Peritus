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
    let state = tempfile::tempdir().expect("state");
    let source = repository();
    let repository =
        tempfile::tempdir_in(state.path()).expect("managed repository under daemon state");
    assert!(
        std::process::Command::new("git")
            .args(["clone", "--quiet"])
            .arg(source.path())
            .arg(repository.path())
            .status()
            .expect("clone managed repository")
            .success()
    );
    let path = repository.path().join("reference.txt");
    fs::write(&path, "UNSELECTED_FIRST\nORIGINAL_REFERENCE\nUNSELECTED_LAST\n").expect("source");
    let writer = support::scripted_attachment_reader(
        0x61,
        "chat",
        support::text_response(b"Reference received."),
    );
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
    let tool_result = {
        let requests = writer.requests.lock().expect("requests");
        assert_eq!(requests.len(), 2, "runner must retrieve the selected file before continuing");
        let first_request = &requests[0];
        let attachment_read_available =
            first_request.tools().iter().any(|tool| tool.name().as_str() == "attachment_read");
        assert!(
            attachment_read_available,
            "provider request must expose the scoped read-only tool"
        );
        let metadata = first_request
            .messages()
            .iter()
            .flat_map(peritus_model_protocol::Message::content)
            .filter_map(|block| match block {
                peritus_model_protocol::ContentBlock::Text(text) => Some(text.expose_for_wire()),
                _ => None,
            })
            .collect::<String>();
        assert!(metadata.contains("Explicit immutable file references"));
        assert!(metadata.contains("source_sha256"));
        assert!(metadata.contains("selected_sha256"));
        assert!(metadata.contains("range"));
        assert!(!metadata.contains("ORIGINAL_REFERENCE"));
        assert!(!metadata.contains("REFRESHED_REFERENCE"));
        assert!(!metadata.contains("UNSELECTED_FIRST"));
        assert!(!metadata.contains("UNSELECTED_LAST"));
        requests[1]
            .messages()
            .iter()
            .flat_map(peritus_model_protocol::Message::content)
            .find_map(|block| match block {
                peritus_model_protocol::ContentBlock::ToolResult(result) => {
                    Some(String::from_utf8_lossy(result.output().canonical_bytes()).into_owned())
                }
                _ => None,
            })
            .expect("tool result in actual provider continuation")
    };
    let (included, excluded) = if refresh {
        ("REFRESHED_REFERENCE", "ORIGINAL_REFERENCE")
    } else {
        ("ORIGINAL_REFERENCE", "REFRESHED_REFERENCE")
    };
    assert!(tool_result.contains(included), "expected exact selected source in tool result");
    assert!(!tool_result.contains(excluded));
    assert!(!tool_result.contains("UNSELECTED_FIRST"));
    assert!(!tool_result.contains("UNSELECTED_LAST"));
    let record = service
        .with_controls(false, |store| {
            store.load(peritus_product_runner::control::ConversationId::new([2; 16])?)
        })
        .expect("load")
        .expect("record");
    assert_eq!(record.files().entries()[0].refreshes().len(), usize::from(refresh));
    assert_eq!(record.inputs().invocations().len(), 2, "file retrieval stays in the governed turn");
    service.shutdown(Duration::from_secs(5)).await;
}
