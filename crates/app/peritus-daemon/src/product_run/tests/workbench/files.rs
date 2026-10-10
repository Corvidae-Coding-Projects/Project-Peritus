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
    let raster = service
        .inner
        .image_decodes
        .clone()
        .try_acquire_many_owned(
            u32::try_from(service.inner.image_decodes.available_permits()).expect("permits"),
        )
        .expect("occupy all raster decoding capacity");
    let AppResponsePayload::WorkbenchFilePreview(preview) = tokio::time::timeout(
        Duration::from_secs(2),
        service.preview_workbench_file(actor(), &selection),
    )
    .await
    .expect("text reads have separate fair admission") else {
        panic!("preview")
    };
    drop(raster);
    fs::write(&path, "changed after preview before confirmation").expect("edit after preview");
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

#[test]
fn refresh_retries_only_failed_sources_and_publishes_one_complete_snapshot() {
    interaction::block_on(async {
        let state = tempfile::tempdir().expect("state");
        let repository = repository();
        let writer = scripted(0x71, "chat", Vec::new());
        let reviewer = scripted(0x72, "review", Vec::new());
        let fixer = scripted(0x73, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x74; 16]).expect("workspace");
        let run = RunId::new([0x75; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        for (index, path) in ["first.txt", "second.txt"].into_iter().enumerate() {
            fs::write(repository.path().join(path), "ORIGINAL").expect("source");
            let selection = WorkbenchFileRequest::new(
                query(workspace),
                3 + index as u64,
                path.to_owned(),
                WorkbenchFileRange::All,
                WorkbenchFileMode::RefreshOnRequest,
                writer.profile.profile_id(),
                ProductModelChoice::default(),
            )
            .expect("selection");
            let AppResponsePayload::WorkbenchFilePreview(preview) =
                service.preview_workbench_file(actor(), &selection).await
            else {
                panic!("preview")
            };
            let attach = command(
                workspace,
                30 + u8::try_from(index).expect("id"),
                3 + index as u64,
                WorkbenchIntent::AttachFile {
                    preview,
                    text: WorkbenchInputText::new("Read source".to_owned()).expect("caption"),
                },
            );
            assert!(matches!(
                service.confirm_workbench_file(actor(), &attach).await,
                AppResponsePayload::WorkbenchReceipt(_)
            ));
        }
        let settings = start(workspace, run, [&writer, &reviewer, &fixer]);
        let start = command(workspace, 40, 5, settings.intent().clone());
        let WorkbenchIntent::StartExecution(settings) = start.intent() else { panic!("settings") };
        let operation = peritus_product_runner::control::ControlOperation::new(
            peritus_product_runner::control::OperationId::new(start.operation().into_bytes())
                .expect("operation"),
            peritus_product_runner::control::ConversationId::new(
                start.query().conversation().into_bytes(),
            )
            .expect("conversation"),
            actor(),
            workspace,
            start.expected_revision(),
            peritus_product_runner::control::ControlIntent::StartExecution {
                run: run.into_bytes(),
                settings_digest: settings.fingerprint().expect("settings digest").into_bytes(),
            },
        );
        service
            .with_controls(false, |store| store.accept(&operation))
            .expect("record start without executing");
        fs::write(repository.path().join("first.txt"), "FIRST_OBSERVATION").expect("change first");
        fs::remove_file(repository.path().join("second.txt")).expect("missing second");
        let error = service
            .refresh_request_files(
                &operation,
                writer.profile.profile_id(),
                &ProductModelChoice::default(),
            )
            .expect_err("failed second leaves all unpublished");
        assert!(error.to_string().contains("second.txt"), "{error}");
        let revision = service
            .with_controls(false, |store| store.execution_record(&operation))
            .expect("record")
            .revision();
        assert_eq!(revision, 6);
        fs::write(repository.path().join("first.txt"), "LATER_FIRST_OBSERVATION")
            .expect("edit successful source");
        fs::write(repository.path().join("second.txt"), "SECOND_OBSERVATION")
            .expect("repair failed source");
        service
            .refresh_request_files(
                &operation,
                writer.profile.profile_id(),
                &ProductModelChoice::default(),
            )
            .expect("retry only failed source");
        let record = service
            .with_controls(false, |store| store.execution_record(&operation))
            .expect("record");
        assert_eq!(record.revision(), 7, "two versions share one atomic event");
        let entries = record.files().entries();
        assert_eq!(
            entries[0].current().observation().digest(),
            peritus_codec::sha256(b"FIRST_OBSERVATION")
        );
        assert_eq!(
            entries[1].current().observation().digest(),
            peritus_codec::sha256(b"SECOND_OBSERVATION")
        );
        service
            .refresh_request_files(
                &operation,
                writer.profile.profile_id(),
                &ProductModelChoice::default(),
            )
            .expect("reuse complete construction snapshot");
        assert_eq!(
            service
                .with_controls(false, |store| store.execution_record(&operation))
                .expect("record")
                .revision(),
            7
        );
        service.finish_file_refresh(&operation).expect("request admitted");
        service
            .refresh_request_files(
                &operation,
                writer.profile.profile_id(),
                &ProductModelChoice::default(),
            )
            .expect("next boundary captures later source");
        let record = service
            .with_controls(false, |store| store.execution_record(&operation))
            .expect("record");
        assert_eq!(record.revision(), 8);
        assert_eq!(
            record.files().entries()[0].current().observation().digest(),
            peritus_codec::sha256(b"LATER_FIRST_OBSERVATION")
        );
    });
}

mod contract;
