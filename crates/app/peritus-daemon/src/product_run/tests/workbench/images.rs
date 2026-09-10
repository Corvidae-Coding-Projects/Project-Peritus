use super::*;
use peritus_model_protocol::ContentBlock;
use peritus_product_runner::{
    attachment::ValidatedImage,
    control::{ControlIntent, ControlOperation, ControlText, ImageAttachment, OperationId},
};
use peritus_types::ArtifactId;

mod page;

#[test]
fn governed_image_reaches_the_real_runner_and_text_only_provider_never_receives_a_request() {
    for supported in [true, false] {
        interaction::block_on(image_scenario(supported));
    }
}

async fn image_scenario(supported: bool) {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let capable = support::scripted_images(
        0x51,
        "vision",
        vec![support::text_response(b"I received the image.")],
    );
    let writer = if supported { Arc::clone(&capable) } else { scripted(0x51, "text", Vec::new()) };
    let reviewer = scripted(0x52, "review", Vec::new());
    let fixer = scripted(0x53, "fix", Vec::new());
    let workspace = WorkspaceId::new([0x54; 16]).expect("workspace");
    let run = RunId::new([0x55; 16]).expect("run");
    let service = service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
    queue(&service, workspace).await;
    let bytes = vec![
        71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 44, 0, 0, 0, 0, 1,
        0, 1, 0, 0, 2, 2, 68, 1, 0, 59,
    ];
    let image = ValidatedImage::decode(bytes.clone(), &capable.profile).expect("decode");
    let operation = OperationId::new([8; 16]).expect("operation");
    let reference = ImageAttachment::from_validated(
        operation,
        ArtifactId::new([9; 16]).expect("artifact"),
        ControlText::new("explicit image.gif".to_owned()).expect("label"),
        &image,
    )
    .expect("reference");
    let import = ControlOperation::new(
        operation,
        peritus_product_runner::control::ConversationId::new([2; 16]).expect("conversation"),
        actor(),
        workspace,
        3,
        ControlIntent::AttachImage {
            image: reference,
            text: ControlText::new("Describe the explicitly attached image".to_owned())
                .expect("caption"),
        },
    );
    service
        .with_controls(false, |store| store.accept_image(&import, &image))
        .expect("atomic import");
    page::inspect_scope_fences(&service, workspace);
    let settings = start(workspace, run, [&writer, &reviewer, &fixer]);
    let start = command(workspace, 7, 4, settings.intent().clone());
    let accepted = service.workbench_command(actor(), &start).await;
    assert!(matches!(accepted, AppResponsePayload::WorkbenchReceipt(_)), "{accepted:?}");
    let terminal = wait_for_terminal(&service, run).await;
    if supported {
        assert_eq!(terminal.phase(), ProductRunPhase::WaitingForUser, "{}", terminal.summary());
        let requests = writer.requests.lock().expect("requests");
        assert_eq!(requests.len(), 1);
        let images: Vec<_> = requests[0]
            .messages()
            .iter()
            .flat_map(peritus_model_protocol::Message::content)
            .filter_map(|block| match block {
                ContentBlock::Image(image) => Some(image),
                _ => None,
            })
            .collect();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].inline_bytes_for_wire(), Some(bytes.as_slice()));
        assert_eq!(images[0].digest(), Some(image.digest()));
    } else {
        assert_eq!(terminal.phase(), ProductRunPhase::Failed);
        assert!(writer.requests.lock().expect("no provider sends").is_empty());
        let record = service
            .with_controls(false, |store| store.load(import.conversation()))
            .expect("load")
            .expect("record");
        assert!(
            record.inputs().invocations().is_empty(),
            "failed capability must not consume inputs"
        );
    }
    service.shutdown(Duration::from_secs(5)).await;
}
