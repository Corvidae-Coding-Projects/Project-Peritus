use super::*;
use peritus_app_protocol::{
    AppRequestPayload, CorrelationId, ProtocolContext, ProtocolId, ProtocolVersion, RequestId,
    WorkbenchCheckpointName, WorkbenchCheckpointReferences, WorkbenchRewindMode,
};

#[test]
fn empty_checkpoint_coverage_starts_with_a_terminal_page() {
    assert_eq!(
        start_position(None, peritus_types::Sha256Digest::new([0x31; 32]), [0; 3],)
            .expect("empty terminal page position"),
        (WorkbenchCoverageSection::Paths, 0),
    );
}

#[test]
fn empty_checkpoint_and_conversation_only_routes_encode_terminal_pages() {
    let conversation = ConversationId::new([0x41; 16]).expect("conversation");
    let workspace = peritus_types::WorkspaceId::new([0x42; 16]).expect("workspace");
    let query = peritus_app_protocol::WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new(*conversation.as_bytes())
            .expect("app conversation"),
        workspace,
    );
    let checkpoint = ControlOperationId::new([0x43; 16]).expect("checkpoint");
    let receipt = WorkbenchCheckpointReceipt::new(
        checkpoint,
        query,
        3,
        WorkbenchCheckpointName::new("empty".to_owned()).expect("name"),
        WorkbenchCheckpointReferences::new(2, 1, 0, None),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("empty receipt");
    let context = ProtocolContext::new(
        ProtocolId::new([0x44; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0).expect("version"),
        peritus_types::SessionId::new([0x45; 16]).expect("session"),
    );
    let limits = AppProtocolLimits::PRODUCTION;
    let checkpoint_request = WorkbenchCheckpointPageRequest::new(query, 3, checkpoint, None)
        .expect("checkpoint page request");
    let checkpoint_envelope = AppRequestEnvelope::new(
        context,
        RequestId::new([0x46; 16]).expect("request"),
        CorrelationId::new([0x47; 16]).expect("correlation"),
        AppRequestPayload::QueryWorkbenchCheckpointPage(checkpoint_request),
    )
    .expect("checkpoint envelope");
    let checkpoint_page =
        build_checkpoint_page(&receipt, checkpoint_request, &checkpoint_envelope, limits)
            .expect("empty checkpoint page");
    assert!(checkpoint_page.paths().is_empty());
    assert!(checkpoint_page.next().is_none());

    let rewind_request = WorkbenchRewindRequest::new(query, 3, checkpoint)
        .expect("conversation-only request")
        .with_branch(
            WorkbenchRewindMode::ConversationOnly,
            peritus_app_protocol::ConversationId::new([0x48; 16]).expect("child"),
        )
        .expect("conversation-only scope");
    let preview = WorkbenchRewindPreview::new(rewind_request, Vec::new(), Vec::new(), Vec::new())
        .expect("empty conversation-only preview");
    let confirmation = WorkbenchRewindConfirmation::for_preview(rewind_request, &receipt, &preview)
        .expect("confirmation");
    let rewind_page_request = WorkbenchRewindPageRequest::new(rewind_request, None);
    let rewind_envelope = AppRequestEnvelope::new(
        context,
        RequestId::new([0x49; 16]).expect("request"),
        CorrelationId::new([0x4a; 16]).expect("correlation"),
        AppRequestPayload::QueryWorkbenchRewindPage(rewind_page_request),
    )
    .expect("rewind envelope");
    let rewind_page = build_rewind_page(&preview, confirmation, None, &rewind_envelope, limits)
        .expect("empty rewind page");
    assert!(rewind_page.paths().is_empty());
    assert!(rewind_page.next().is_none());
}
