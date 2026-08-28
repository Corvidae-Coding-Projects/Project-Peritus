//! Provider-neutral role request construction and reduction.

use peritus_model_protocol::{
    BoundedText, CachePolicy, Capability, ContentBlock, GenerationConfig, Message, ModelRequest,
    ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ReasoningPolicy, ReducedItem, RequestId,
    RequestOptions, RequestedCapabilities, ResponseReducer, Role, StructuredOutput,
    TerminalOutcome, ToolChoice, negotiate,
};
use peritus_provider_core::{CancellationToken, ModelProvider};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

pub async fn complete(
    provider: &dyn ModelProvider,
    request_name: String,
    system: String,
    user: String,
    cancellation: CancellationToken,
) -> Result<String, ProductRunnerError> {
    let profile = provider.profile();
    let limits = ProtocolLimits::PRODUCTION;
    let requested = RequestedCapabilities::new(&[], &[Capability::Streaming], profile.limits())
        .map_err(|error| protocol_error(&error))?;
    let negotiated = negotiate(profile, requested).map_err(|error| protocol_error(&error))?;
    let max_output = profile.limits().max_output_tokens().min(32_768);
    let request = ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(request_name).map_err(|error| protocol_error(&error))?,
        vec![message(Role::System, system, limits)?, message(Role::User, user, limits)?],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(max_output, Vec::new(), None, None, None)
                .map_err(|error| protocol_error(&error))?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        limits,
    )
    .map_err(|error| protocol_error(&error))?;
    let mut stream =
        provider.start(request, cancellation).await.map_err(|error| provider_error(&error))?;
    let mut reducer = ResponseReducer::new(profile.provider().clone(), limits);
    while let Some(event) = stream.pull().await.map_err(|error| provider_error(&error))? {
        reducer.push(event).map_err(|error| protocol_error(&error))?;
    }
    if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
        let detail = reducer.terminal().map_or_else(
            || "provider stream ended without a terminal outcome".to_owned(),
            |terminal| format!("provider terminal was {terminal:?}"),
        );
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Provider,
            "complete model role",
            detail,
        ));
    }
    let mut text = String::new();
    for item in reducer.completed_items() {
        match item {
            ReducedItem::Text { text: value, .. } => text.push_str(value.expose_for_wire()),
            ReducedItem::Refusal { .. } => {
                return Err(ProductRunnerError::new(
                    ProductRunnerErrorKind::Provider,
                    "complete model role",
                    "provider refused the role request",
                ));
            }
            _ => {}
        }
    }
    if text.trim().is_empty() {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Provider,
            "complete model role",
            "provider returned no usable text",
        ));
    }
    Ok(text)
}

fn message(
    role: Role,
    value: String,
    limits: ProtocolLimits,
) -> Result<Message, ProductRunnerError> {
    Message::new(
        role,
        vec![ContentBlock::Text(
            BoundedText::new(value, limits).map_err(|error| protocol_error(&error))?,
        )],
        limits,
    )
    .map_err(|error| protocol_error(&error))
}

fn protocol_error(error: &peritus_model_protocol::ProtocolError) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Provider,
        "construct or reduce model request",
        error.to_string(),
    )
}

fn provider_error(error: &peritus_provider_core::ProviderCoreError) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Provider,
        "execute model request",
        error.to_string(),
    )
}
