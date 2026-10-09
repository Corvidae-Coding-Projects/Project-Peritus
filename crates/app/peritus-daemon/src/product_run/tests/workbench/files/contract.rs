//! Exact user instructions govern every workspace-read surface, regardless of storage size.
use super::*;
use peritus_product_runner::{
    attachment::ValidatedFileText,
    control::{
        ControlIntent, ControlOperation, ControlText, FileAttachment, FileObservation, FileRange,
        FileSource, FileVersion, InputId, OperationId,
    },
};
const CONTRACT: &str = "Query the system by importing `forward.py` and calling forward(x). You do not know the shape of A1.";

fn add_reference(service: &ProductRunService, workspace: WorkspaceId, user_message: bool) {
    let text =
        ValidatedFileText::new(format!("{}\n{CONTRACT}", "long input\n".repeat(1000)).into_bytes())
            .expect("text");
    let id = OperationId::new([41; 16]).expect("operation");
    let input = InputId::new(*id.as_bytes()).expect("input");
    let source = if user_message {
        FileSource::user_message()
    } else {
        FileSource::imported(ControlText::new("data.txt".to_owned()).unwrap(), FileRange::All)
            .unwrap()
    };
    let version = FileVersion::new(
        id,
        peritus_types::ArtifactId::new([42; 16]).unwrap(),
        FileObservation::new(text.digest(), text.bytes(), (0, text.bytes()), text.digest())
            .unwrap(),
        peritus_codec::sha256(b"consent"),
    )
    .unwrap();
    let operation = ControlOperation::new(
        id,
        peritus_product_runner::control::ConversationId::new([2; 16]).unwrap(),
        actor(),
        workspace,
        3,
        ControlIntent::SubmitMessage {
            text: ControlText::new("Read the selected reference.".to_owned()).unwrap(),
            files: vec![FileAttachment::for_message(source, version, input).unwrap()],
        },
    );
    service
        .with_controls(false, |store| store.accept_files(&operation, &[(&text, b"consent")]))
        .expect("authenticated exact message reference");
}

#[test]
fn exact_inline_and_referenced_user_contracts_block_preview_and_init_but_file_data_has_no_authority()
 {
    interaction::block_on(async {
        for variant in 0..3 {
            let state = tempfile::tempdir().unwrap();
            let repository = repository();
            fs::write(repository.path().join("forward.py"), "SECRET_IMPLEMENTATION_BYTES").unwrap();
            let writer = scripted(0x71, "chat", Vec::new());
            let workspace = WorkspaceId::new([0x72; 16]).unwrap();
            let service =
                service(state.path(), repository.path(), workspace, [&writer, &writer, &writer]);
            queue(&service, workspace).await;
            if variant == 0 {
                let inline = command(
                    workspace,
                    41,
                    3,
                    WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                        WorkbenchNewInput::new(
                            WorkbenchInputId::new([41; 16]).unwrap(),
                            WorkbenchInputText::new(CONTRACT.to_owned()).unwrap(),
                            WorkbenchInputOrder::new(Vec::new()).unwrap(),
                        )
                        .unwrap(),
                    )),
                );
                assert!(matches!(
                    service.workbench_command(actor(), &inline).await,
                    AppResponsePayload::WorkbenchReceipt(_)
                ));
            } else {
                add_reference(&service, workspace, variant == 1);
            }
            let selection = WorkbenchFileRequest::new(
                query(workspace),
                4,
                "forward.py".to_owned(),
                WorkbenchFileRange::All,
                WorkbenchFileMode::Snapshot,
                writer.profile.profile_id(),
                ProductModelChoice::default(),
            )
            .unwrap();
            let preview = service.preview_workbench_file(actor(), &selection).await;
            if variant == 2 {
                assert!(
                    matches!(preview, AppResponsePayload::WorkbenchFilePreview(_)),
                    "{preview:?}"
                );
            } else {
                assert!(
                    matches!(preview, AppResponsePayload::Error(ref error) if error.code() == peritus_app_protocol::AppErrorCode::ReadOnly),
                    "{preview:?}"
                );
            }
            check_init_contract(&service, workspace, variant != 2).await;
            assert!(writer.requests.lock().unwrap().is_empty());
            service.shutdown(Duration::from_secs(5)).await;
        }
    });
}

async fn check_init_contract(service: &ProductRunService, workspace: WorkspaceId, denied: bool) {
    use peritus_app_protocol::{
        InitArtifactDiscovery, InitArtifactPageRequest, InitDiscoveryRequest, InitSourceKind,
        InitSourceSelection,
    };
    let discovery = InitArtifactDiscovery::new(
        InitDiscoveryRequest::new(query(workspace), 4).unwrap(),
        None,
        Some(
            InitSourceSelection::new("forward.py".to_owned(), InitSourceKind::Documentation)
                .unwrap(),
        ),
        None,
    )
    .unwrap();
    let AppResponsePayload::InitArtifactProposal(proposal) =
        service.discover_init_artifacts(actor(), discovery).await
    else {
        panic!("init proposal")
    };
    let page = InitArtifactPageRequest::new(proposal, 0, 32 * 1024).unwrap();
    let AppResponsePayload::InitArtifactPage(page) =
        service.init_artifact_page(actor(), page).await
    else {
        panic!("review page")
    };
    let review = String::from_utf8_lossy(page.bytes());
    assert_eq!(
        review.contains("source forward.py: source is outside the current read policy"),
        denied,
        "{review}"
    );
    assert!(!review.contains("SECRET_IMPLEMENTATION_BYTES"));
}
