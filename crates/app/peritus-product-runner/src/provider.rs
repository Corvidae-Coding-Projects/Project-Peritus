//! Provider-neutral role request construction and reduction.

use peritus_model_protocol::{
    BoundedText, CachePolicy, Capability, ContentBlock, GenerationConfig, Message, ModelRequest,
    ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ReasoningPolicy, ReducedItem, RequestId,
    RequestOptions, RequestedCapabilities, ResponseReducer, Retryability, Role, StructuredOutput,
    TerminalOutcome, ToolChoice, negotiate,
};
use peritus_provider_core::{CancellationToken, ModelProvider, ProviderCoreErrorKind};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAX_ATTEMPTS: u8 = 3;

pub async fn complete(
    provider: &dyn ModelProvider,
    request_name: String,
    system: String,
    user: String,
    cancellation: CancellationToken,
) -> Result<String, ProductRunnerError> {
    for attempt in 1..=MAX_ATTEMPTS {
        if cancellation.is_cancelled() {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::Cancelled,
                "complete model role",
                "model role was cancelled",
            ));
        }
        match complete_once(
            provider,
            format!("{request_name}-attempt-{attempt}"),
            system.clone(),
            user.clone(),
            cancellation.clone(),
        )
        .await
        {
            Ok(text) => return Ok(text),
            Err(failure) if failure.retryable && attempt < MAX_ATTEMPTS => {
                tokio::time::sleep(retry_delay(attempt)).await;
            }
            Err(failure) => return Err(failure.error),
        }
    }
    Err(ProductRunnerError::new(
        ProductRunnerErrorKind::Provider,
        "complete model role",
        "recoverable provider attempts were exhausted",
    ))
}

async fn complete_once(
    provider: &dyn ModelProvider,
    request_name: String,
    system: String,
    user: String,
    cancellation: CancellationToken,
) -> Result<String, AttemptFailure> {
    let profile = provider.profile();
    let limits = ProtocolLimits::PRODUCTION;
    let requested = RequestedCapabilities::new(&[], &[Capability::Streaming], profile.limits())
        .map_err(|error| AttemptFailure::terminal(protocol_error(&error)))?;
    let negotiated = negotiate(profile, requested)
        .map_err(|error| AttemptFailure::terminal(protocol_error(&error)))?;
    let max_output = profile.limits().max_output_tokens().min(32_768);
    let request = ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(request_name)
            .map_err(|error| AttemptFailure::terminal(protocol_error(&error)))?,
        vec![
            message(Role::System, system, limits).map_err(AttemptFailure::terminal)?,
            message(Role::User, user, limits).map_err(AttemptFailure::terminal)?,
        ],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(max_output, Vec::new(), None, None, None)
                .map_err(|error| AttemptFailure::terminal(protocol_error(&error)))?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        limits,
    )
    .map_err(|error| AttemptFailure::terminal(protocol_error(&error)))?;
    let mut stream = provider
        .start(request, cancellation)
        .await
        .map_err(|error| AttemptFailure::provider(&error))?;
    let mut reducer = ResponseReducer::new(profile.provider().clone(), limits);
    while let Some(event) = stream.pull().await.map_err(|error| AttemptFailure::provider(&error))? {
        reducer.push(event).map_err(|error| AttemptFailure::terminal(protocol_error(&error)))?;
    }
    if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) {
        let detail = reducer.terminal().map_or_else(
            || "provider stream ended without a terminal outcome".to_owned(),
            |terminal| format!("provider terminal was {terminal:?}"),
        );
        let retryable = reducer.terminal().is_some_and(retryable_terminal);
        return Err(AttemptFailure {
            error: ProductRunnerError::new(
                ProductRunnerErrorKind::Provider,
                "complete model role",
                detail,
            ),
            retryable,
        });
    }
    let mut text = String::new();
    for item in reducer.completed_items() {
        match item {
            ReducedItem::Text { text: value, .. } => text.push_str(value.expose_for_wire()),
            ReducedItem::Refusal { .. } => {
                return Err(AttemptFailure::terminal(ProductRunnerError::new(
                    ProductRunnerErrorKind::Provider,
                    "complete model role",
                    "provider refused the role request",
                )));
            }
            _ => {}
        }
    }
    if text.trim().is_empty() {
        return Err(AttemptFailure::recoverable(ProductRunnerError::new(
            ProductRunnerErrorKind::Provider,
            "complete model role",
            "provider returned no usable text",
        )));
    }
    Ok(text)
}

struct AttemptFailure {
    error: ProductRunnerError,
    retryable: bool,
}

impl AttemptFailure {
    const fn terminal(error: ProductRunnerError) -> Self {
        Self { error, retryable: false }
    }

    const fn recoverable(error: ProductRunnerError) -> Self {
        Self { error, retryable: true }
    }

    fn provider(error: &peritus_provider_core::ProviderCoreError) -> Self {
        let retryable = matches!(
            error.kind(),
            ProviderCoreErrorKind::Connect
                | ProviderCoreErrorKind::Transport
                | ProviderCoreErrorKind::MalformedStream
        );
        Self { error: provider_error(error), retryable }
    }
}

const fn retryable_terminal(terminal: &TerminalOutcome) -> bool {
    matches!(
        terminal,
        TerminalOutcome::Failed(failure)
            if matches!(
                failure.retryability(),
                Retryability::SafeNewRequest | Retryability::CallerDecision
            )
    )
}

const fn retry_delay(attempt: u8) -> std::time::Duration {
    std::time::Duration::from_millis(250_u64.saturating_mul(attempt as u64))
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
