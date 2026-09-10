use super::*;
use peritus_app_protocol::{
    ProductModelChoice, WorkbenchImageLabel, WorkbenchImagePreview, WorkbenchImageRequest,
    WorkbenchImageUpload, WorkbenchInputText,
};
use peritus_types::{ArtifactId, ProviderProfileId, WorkspaceId};

#[path = "images/page.rs"]
mod page;
#[path = "images/rejections.rs"]
mod rejections;

const GIF: &[u8] = &[
    71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 44, 0, 0, 0, 0, 1, 0, 1,
    0, 0, 2, 2, 68, 1, 0, 59,
];

pub(super) fn configuration(root: &std::path::Path, revision: u64) -> DaemonConfig {
    use std::os::unix::fs::PermissionsExt as _;
    // Any accidental provider execution leaves evidence and fails. Preview must only inspect
    // the configured profile; it must not discover a CLI, log in, or perform inference.
    let executable = root.join("never-execute-provider");
    std::fs::write(&executable, "#!/bin/sh\n: > \"$0.invoked\"\nexit 91\n").expect("fixture");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).expect("mode");
    let text = format!(
        "{}\n[[providers]]\nkind = 'codex-runtime'\nexecutable = {:?}\n[providers.profile]\nprofile_id = '91919191919191919191919191919191'\nrevision = {revision}\nmodel = 'fixture-vision'\ncapabilities = ['image-input']\nmax_input_tokens = 32768\nmax_output_tokens = 1024\nmax_tools = 32\nmax_parallel_tool_calls = 1\nmax_inline_media_bytes = 4194304\n",
        configuration_text(root),
        executable.to_string_lossy()
    );
    DaemonConfig::parse(&text).expect("image fixture config")
}
pub(super) fn query() -> WorkbenchQuery {
    WorkbenchQuery::new(
        ConversationId::new([90; 16]).expect("conversation"),
        WorkspaceId::new([0x33; 16]).expect("workspace"),
    )
}
pub(super) fn command(id: u8, revision: u64, intent: WorkbenchIntent) -> WorkbenchCommand {
    WorkbenchCommand::new(
        ControlOperationId::new([id; 16]).expect("operation"),
        query(),
        revision,
        intent,
    )
}
fn selection(artifact: ArtifactId, revision: u64) -> WorkbenchImageRequest {
    WorkbenchImageRequest::new(
        query(),
        revision,
        artifact,
        ProviderProfileId::new([0x91; 16]).expect("profile"),
        ProductModelChoice::default(),
        WorkbenchImageLabel::new("explicit reference.gif".to_owned()).expect("label"),
    )
    .expect("selection")
}
pub(super) async fn create(client: &mut (AppFrameStream<UnixStream>, ProtocolContext)) {
    let create = command(
        91,
        0,
        WorkbenchIntent::CreateConversation(
            ConversationTitle::new("Image consent".to_owned()).expect("title"),
        ),
    );
    assert!(matches!(
        request(client, 1, AppRequestPayload::WorkbenchCommand(create)).await,
        AppResponsePayload::WorkbenchReceipt(_)
    ));
}
async fn upload(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
    id: u8,
    bytes: &[u8],
    mime: &str,
) -> ArtifactId {
    let metadata = ArtifactMetadata::new(
        TransferId::new([id; 16]).expect("transfer"),
        ArtifactId::new([id; 16]).expect("artifact"),
        bytes.len() as u64,
        CanonicalMediaType::new(mime.to_owned(), 128).expect("MIME"),
        peritus_codec::sha256(bytes),
        1024,
        1024,
    )
    .expect("metadata");
    let begin = WorkbenchImageUpload::new(query(), 1, metadata.clone()).expect("begin");
    let chunk = ArtifactChunk::new(
        metadata.transfer_id(),
        metadata.artifact_id(),
        0,
        0,
        bytes.to_vec(),
        1024,
    )
    .expect("chunk");
    let complete = ArtifactCompletion::new(
        metadata.transfer_id(),
        metadata.artifact_id(),
        metadata.byte_size(),
        metadata.digest(),
    );
    for (sequence, payload) in [
        (2, AppRequestPayload::BeginWorkbenchImageUpload(begin)),
        (3, AppRequestPayload::UploadArtifactChunk(chunk)),
        (4, AppRequestPayload::CompleteArtifactUpload(complete)),
    ] {
        let response = request(client, sequence, payload).await;
        assert!(matches!(response, AppResponsePayload::Acknowledged(_)), "{response:?}");
    }
    metadata.artifact_id()
}
async fn preview(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
    artifact: ArtifactId,
) -> WorkbenchImagePreview {
    let response =
        request(client, 5, AppRequestPayload::PreviewWorkbenchImage(selection(artifact, 1))).await;
    let AppResponsePayload::WorkbenchImagePreview(preview) = response else {
        panic!("preview: {response:?}")
    };
    assert_eq!(preview.image().digest(), peritus_codec::sha256(GIF));
    assert_eq!(preview.image().dimensions(), (1, 1));
    assert_eq!(preview.resolved_model(), "fixture-vision");
    preview
}

#[test]
fn image_upload_preview_confirm_and_exact_receipt_replay_survive_provider_revision_change() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let runtime =
            DaemonRuntime::start(configuration(temporary.path(), 1)).await.expect("start");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        create(&mut client).await;
        let artifact = upload(&mut client, 92, GIF, "application/octet-stream").await;
        let preview = preview(&mut client, artifact).await;
        let confirm = command(
            93,
            1,
            WorkbenchIntent::AttachImage {
                preview,
                text: WorkbenchInputText::new("Describe this exact image".to_owned())
                    .expect("caption"),
            },
        );
        let receipt =
            request(&mut client, 6, AppRequestPayload::WorkbenchCommand(confirm.clone())).await;
        assert!(
            matches!(&receipt, AppResponsePayload::WorkbenchReceipt(receipt) if receipt.accepted_revision() == 2),
            "{receipt:?}"
        );
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        let runtime =
            DaemonRuntime::start(configuration(temporary.path(), 2)).await.expect("restart");
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        for (sequence, payload) in [
            (1, AppRequestPayload::QueryWorkbenchReceipt(confirm.clone())),
            (2, AppRequestPayload::WorkbenchCommand(confirm)),
        ] {
            assert_eq!(request(&mut client, sequence, payload).await, receipt);
        }
        let response = request(&mut client, 3, AppRequestPayload::QueryWorkbench(query())).await;
        assert!(
            matches!(response, AppResponsePayload::Workbench(record) if record.revision() == 2)
        );
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        assert!(!temporary.path().join("never-execute-provider.invoked").exists());
    });
}
