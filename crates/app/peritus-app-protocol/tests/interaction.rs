//! Additive interaction messages preserve exact model choices and input lifecycle revisions.

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope,
    AppResponsePayload, CorrelationId, ProductActivity, ProductActivityKind,
    ProductInteractionMode, ProductInteractionRequest, ProductInteractionSnapshot,
    ProductModelCatalog, ProductModelChoice, ProductModelInfo, ProductModelQuery,
    ProductProviderSelection, ProductRoleModels, ProductRunConversationQuery, ProductRunPhase,
    ProductRunRequest, ProductRunSnapshot, ProtocolContext, ProtocolId, ProtocolVersion, RequestId,
    decode_app_message, encode_app_message,
};
use peritus_types::{ProviderProfileId, RunId, SessionId, WorkspaceId};

fn context() -> ProtocolContext {
    ProtocolContext::new(
        ProtocolId::new([1; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0).expect("version"),
        SessionId::new([2; 16]).expect("session"),
    )
}
fn profile() -> ProviderProfileId {
    ProviderProfileId::new([3; 16]).expect("profile")
}
fn run() -> RunId {
    RunId::new([4; 16]).expect("run")
}
fn models() -> ProductRoleModels {
    ProductRoleModels::new(
        ProductModelChoice::new("arbitrary-writer".to_owned(), false).expect("writer"),
        ProductModelChoice::new("manual-reviewer".to_owned(), true).expect("reviewer"),
        ProductModelChoice::default(),
    )
}
fn snapshot() -> ProductRunSnapshot {
    ProductRunSnapshot::new(
        run(),
        WorkspaceId::new([5; 16]).expect("workspace"),
        ProductProviderSelection::new(profile(), profile(), profile()),
        ProductRunPhase::Writing,
        1,
        "Hello".to_owned(),
        "Responding".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .expect("snapshot")
}
fn roundtrip(message: &AppMessage) {
    let bytes = encode_app_message(message, AppProtocolLimits::PRODUCTION).expect("encode");
    assert_eq!(
        &decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).expect("decode"),
        message
    );
}

#[test]
fn every_mode_and_new_query_roundtrips() {
    let mut payloads = vec![
        AppRequestPayload::QueryInteraction(ProductRunConversationQuery::new(run())),
        AppRequestPayload::QueryModels(ProductModelQuery::new(profile(), true)),
    ];
    for mode in [
        ProductInteractionMode::Chat,
        ProductInteractionMode::Plan,
        ProductInteractionMode::Review,
        ProductInteractionMode::Build,
    ] {
        payloads.push(AppRequestPayload::Interact(ProductInteractionRequest::new(
            ProductRunRequest::new(
                run(),
                snapshot().workspace_id(),
                snapshot().providers(),
                "hello λ".to_owned(),
            )
            .expect("request"),
            mode,
            models(),
        )));
    }
    for payload in payloads {
        roundtrip(&AppMessage::Request(
            AppRequestEnvelope::new(
                context(),
                RequestId::new([6; 16]).expect("request"),
                CorrelationId::new([7; 16]).expect("correlation"),
                payload,
            )
            .expect("envelope"),
        ));
    }
}

#[test]
fn public_activity_and_catalog_provenance_roundtrip() {
    let interaction = ProductInteractionSnapshot::new(
        snapshot(),
        ProductInteractionMode::Chat,
        models(),
        2,
        1,
        vec![
            ProductActivity::new(1, ProductActivityKind::User, "Hello λ".to_owned(), String::new())
                .expect("user"),
            ProductActivity::new(
                2,
                ProductActivityKind::Tool,
                "Reading".to_owned(),
                "src/lib.rs".to_owned(),
            )
            .expect("tool"),
        ],
        None,
    )
    .expect("interaction");
    let catalog = ProductModelCatalog::new(
        profile(),
        "configured-original".to_owned(),
        vec![
            ProductModelInfo::new("arbitrary-writer".to_owned(), "Provider label".to_owned(), None)
                .expect("model"),
        ],
        1234,
        true,
        "Refresh unavailable; retained cache".to_owned(),
    )
    .expect("catalog");
    for payload in
        [AppResponsePayload::Interaction(interaction), AppResponsePayload::Models(catalog)]
    {
        roundtrip(&AppMessage::Response(AppResponseEnvelope::new(
            context(),
            RequestId::new([6; 16]).expect("request"),
            CorrelationId::new([7; 16]).expect("correlation"),
            payload,
        )));
    }
}

#[test]
fn impossible_input_lifecycle_and_duplicate_activity_sequences_are_rejected() {
    assert!(
        ProductInteractionSnapshot::new(
            snapshot(),
            ProductInteractionMode::Chat,
            models(),
            1,
            2,
            Vec::new(),
            None
        )
        .is_err()
    );
    let activity =
        ProductActivity::new(1, ProductActivityKind::User, "hello".to_owned(), String::new())
            .expect("activity");
    assert!(
        ProductInteractionSnapshot::new(
            snapshot(),
            ProductInteractionMode::Chat,
            models(),
            1,
            1,
            vec![activity.clone(), activity],
            None
        )
        .is_err()
    );
    assert!(ProductInteractionMode::from_tag(0).is_none());
    assert!(ProductInteractionMode::from_tag(5).is_none());
}
