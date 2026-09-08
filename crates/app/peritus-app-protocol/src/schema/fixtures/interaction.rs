//! Checked additive conversation and catalog compatibility frames.

use super::{FixtureClass, GeneratedFixtureCase};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ProductActivity,
    ProductActivityKind, ProductInteractionMode, ProductInteractionRequest,
    ProductInteractionSnapshot, ProductModelCatalog, ProductModelInfo, ProductModelQuery,
    ProductProviderSelection, ProductRoleModels, ProductRunPhase, ProductRunRequest,
    ProductRunSnapshot,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    use super::values::{context, encoded, id, request};
    let profile = id(33, ProviderProfileId::new);
    let run_id = id(31, RunId::new);
    let workspace_id = id(32, WorkspaceId::new);
    let providers = ProductProviderSelection::new(profile, profile, profile);
    let request_value =
        ProductRunRequest::new(run_id, workspace_id, providers, "Explain this project".to_owned())
            .expect("fixture request");
    let snapshot = ProductRunSnapshot::new(
        run_id,
        workspace_id,
        providers,
        ProductRunPhase::Writing,
        1,
        "Explain this project".to_owned(),
        "Responding".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .expect("snapshot");
    let interaction = ProductInteractionSnapshot::new(
        snapshot,
        ProductInteractionMode::Chat,
        ProductRoleModels::default(),
        2,
        1,
        vec![
            ProductActivity::new(
                1,
                ProductActivityKind::Assistant,
                "This is a coding assistant.".to_owned(),
                String::new(),
            )
            .expect("activity"),
        ],
        None,
    )
    .expect("interaction");
    let catalog = ProductModelCatalog::new(
        profile,
        "configured-exact-model".to_owned(),
        vec![
            ProductModelInfo::new(
                "provider-new-model".to_owned(),
                "Discovered model".to_owned(),
                None,
            )
            .expect("model"),
        ],
        1234,
        false,
        String::new(),
    )
    .expect("catalog");
    let response = |payload| {
        AppResponseEnvelope::new(
            context(),
            id(10, crate::RequestId::new),
            id(11, crate::CorrelationId::new),
            payload,
        )
    };
    Ok(vec![
        model_update(run_id, limits)?,
        encoded(
            "realistic-interaction-request",
            FixtureClass::Realistic,
            &request(AppRequestPayload::Interact(ProductInteractionRequest::new(
                request_value,
                ProductInteractionMode::Chat,
                ProductRoleModels::default(),
            ))),
            limits,
        )?,
        encoded(
            "realistic-interaction-response",
            FixtureClass::Realistic,
            &response(AppResponsePayload::Interaction(interaction)),
            limits,
        )?,
        encoded(
            "minimal-model-query",
            FixtureClass::Minimal,
            &request(AppRequestPayload::QueryModels(ProductModelQuery::new(profile, false))),
            limits,
        )?,
        encoded(
            "realistic-model-catalog-response",
            FixtureClass::Realistic,
            &response(AppResponsePayload::Models(catalog)),
            limits,
        )?,
    ])
}

fn model_update(run_id: RunId, limits: CodecLimits) -> Result<GeneratedFixtureCase, CodecError> {
    use super::values::{encoded, request};
    encoded(
        "realistic-model-update",
        FixtureClass::Realistic,
        &request(AppRequestPayload::UpdateModels(crate::ProductModelUpdate::new(
            run_id,
            ProductRoleModels::new(
                crate::ProductModelChoice::new("gpt-6-astra".to_owned(), true).expect("model"),
                crate::ProductModelChoice::default(),
                crate::ProductModelChoice::default(),
            ),
        ))),
        limits,
    )
}
