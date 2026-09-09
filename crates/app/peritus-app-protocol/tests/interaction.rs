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
fn effort_bearing_requests_and_snapshots_roundtrip_every_level_without_changing_legacy_tags() {
    use peritus_app_protocol::{ProductModelEffort, ProductModelUpdate};
    for effort in ProductModelEffort::ALL {
        let choices = ProductRoleModels::new(
            ProductModelChoice::default().with_effort(effort),
            ProductModelChoice::new("review-model".to_owned(), true)
                .expect("reviewer")
                .with_effort(ProductModelEffort::Low),
            ProductModelChoice::default().with_effort(ProductModelEffort::Max),
        );
        let request = ProductRunRequest::new(
            run(),
            snapshot().workspace_id(),
            snapshot().providers(),
            "hello".to_owned(),
        )
        .expect("request");
        for payload in [
            AppRequestPayload::UpdateModels(ProductModelUpdate::new(run(), choices.clone())),
            AppRequestPayload::Interact(ProductInteractionRequest::new(
                request,
                ProductInteractionMode::Chat,
                choices.clone(),
            )),
        ] {
            roundtrip(&AppMessage::Request(
                AppRequestEnvelope::new(
                    context(),
                    RequestId::new([6; 16]).expect("id"),
                    CorrelationId::new([7; 16]).expect("id"),
                    payload,
                )
                .expect("request"),
            ));
        }
        let interaction = ProductInteractionSnapshot::new(
            snapshot(),
            ProductInteractionMode::Chat,
            choices,
            1,
            1,
            Vec::new(),
            None,
        )
        .expect("interaction");
        roundtrip(&AppMessage::Response(AppResponseEnvelope::new(
            context(),
            RequestId::new([6; 16]).expect("id"),
            CorrelationId::new([7; 16]).expect("id"),
            AppResponsePayload::Interaction(interaction),
        )));
    }
}

#[test]
fn unknown_effort_and_noncanonical_explicit_default_payload_are_rejected() {
    use peritus_app_protocol::{ProductModelEffort, ProductModelUpdate};
    let update = AppMessage::Request(
        AppRequestEnvelope::new(
            context(),
            RequestId::new([6; 16]).expect("id"),
            CorrelationId::new([7; 16]).expect("id"),
            AppRequestPayload::UpdateModels(ProductModelUpdate::new(
                run(),
                ProductRoleModels::new(
                    ProductModelChoice::default(),
                    ProductModelChoice::default(),
                    ProductModelChoice::default().with_effort(ProductModelEffort::Low),
                ),
            )),
        )
        .expect("update"),
    );
    let bytes = encode_app_message(&update, AppProtocolLimits::PRODUCTION).expect("encoded");
    for tag in [0_u16, 99] {
        let mut corrupt = bytes.clone();
        let end = corrupt.len();
        corrupt[end - 2..].copy_from_slice(&tag.to_be_bytes());
        assert!(decode_app_message(&corrupt, AppProtocolLimits::PRODUCTION).is_err());
    }
}

#[test]
fn every_mode_and_new_query_roundtrips() {
    let mut payloads = vec![
        AppRequestPayload::QueryInteraction(ProductRunConversationQuery::new(run())),
        AppRequestPayload::QueryModels(ProductModelQuery::new(profile(), true)),
        AppRequestPayload::UpdateModels(peritus_app_protocol::ProductModelUpdate::new(
            run(),
            models(),
        )),
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
