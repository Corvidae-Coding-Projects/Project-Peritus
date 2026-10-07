//! Checked additive conversation and catalog compatibility frames.

use super::{FixtureClass, GeneratedFixtureCase};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ProductActivity,
    ProductActivityKind, ProductActivityPageQuery, ProductActivitySegment, ProductActivityWindow,
    ProductInteractionMode, ProductInteractionPage, ProductInteractionSnapshot,
    ProductModelCatalog, ProductModelInfo, ProductModelQuery, ProductProviderSelection,
    ProductRoleModels, ProductRunOperationState, ProductRunPhase, ProductRunSnapshot,
};
use peritus_codec::{CodecError, CodecLimits};
use peritus_types::{ProviderProfileId, RunId, Sha256Digest, WorkspaceId};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    use super::values::{context, encoded, id, request};
    let profile = id(33, ProviderProfileId::new);
    let run_id = id(31, RunId::new);
    let workspace_id = id(32, WorkspaceId::new);
    let providers = ProductProviderSelection::new(profile, profile, profile);
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
        super::product::run_operation(run_id, ProductRunOperationState::Running),
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
    let mut cases = binding_cases(interaction.clone(), limits)?;
    let history = Sha256Digest::new([41; 32]);
    let history_interaction = interaction
        .clone()
        .with_activity_window(
            ProductActivityWindow::new(history, 1, 0, Vec::new(), 0, None)
                .expect("activity window"),
        )
        .expect("history interaction");
    let page_query = ProductActivityPageQuery::first(run_id);
    let page = ProductInteractionPage::new(
        page_query,
        history_interaction,
        vec![ProductActivitySegment::new(
            1,
            0,
            ProductActivityKind::Assistant,
            0,
            "This is a coding assistant.".to_owned(),
            27,
            0,
            String::new(),
            0,
        )
        .expect("activity segment")],
        None,
    )
    .expect("interaction page");
    cases.extend([
        model_update(run_id, limits)?,
        model_update_with_effort(run_id, limits)?,
        encoded(
            "realistic-interaction-response",
            FixtureClass::Realistic,
            &response(AppResponsePayload::Interaction(interaction)),
            limits,
        )?,
        encoded(
            "minimal-interaction-page-query",
            FixtureClass::Minimal,
            &request(AppRequestPayload::QueryInteractionPage(page_query)),
            limits,
        )?,
        encoded(
            "realistic-interaction-page-response",
            FixtureClass::Realistic,
            &response(AppResponsePayload::InteractionPage(page)),
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
    ]);
    Ok(cases)
}

fn binding_cases(
    interaction: ProductInteractionSnapshot,
    limits: CodecLimits,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    use super::values::{context, encoded, id, request};
    let run_id = interaction.snapshot().run_id();
    let workspace_id = interaction.snapshot().workspace_id();
    let response = |payload| {
        AppResponseEnvelope::new(
            context(),
            id(10, crate::RequestId::new),
            id(11, crate::CorrelationId::new),
            payload,
        )
    };
    Ok(vec![
        encoded(
            "minimal-interaction-binding-query",
            FixtureClass::Minimal,
            &request(AppRequestPayload::QueryInteractionBinding(
                crate::ProductInteractionQuery::new(run_id),
            )),
            limits,
        )?,
        encoded(
            "realistic-interaction-binding",
            FixtureClass::Realistic,
            &response(AppResponsePayload::InteractionBinding(
                crate::ProductInteractionBinding::new(
                    interaction,
                    crate::WorkbenchQuery::new(id(34, crate::ConversationId::new), workspace_id),
                )
                .expect("binding"),
            )),
            limits,
        )?,
    ])
}

fn model_update_with_effort(
    run_id: RunId,
    limits: CodecLimits,
) -> Result<GeneratedFixtureCase, CodecError> {
    use super::values::{encoded, request};
    encoded(
        "realistic-model-effort-update",
        FixtureClass::Realistic,
        &request(AppRequestPayload::UpdateModels(crate::ProductModelUpdate::new(
            run_id,
            ProductRoleModels::new(
                crate::ProductModelChoice::default().with_effort(crate::ProductModelEffort::XHigh),
                crate::ProductModelChoice::default().with_effort(crate::ProductModelEffort::Low),
                crate::ProductModelChoice::default().with_effort(crate::ProductModelEffort::Max),
            ),
        ))),
        limits,
    )
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
