//! Explicit, bounded connection qualification without application tools or workspace access.

use crate::{
    CancellationToken, ModelProvider, ProviderCanaryError, ProviderCoreError, ProviderRequirement,
    verify_live_provider,
};
use core::fmt;
use peritus_model_protocol::{
    BoundedText, CachePolicy, CanonicalJson, Capability, ContentBlock, GenerationConfig,
    JsonBounds, JsonSchema, Message, ModelRequest, ParallelToolPolicy, PersistencePolicy,
    ProtocolLimits, ReasoningPolicy, ReasoningReplay, ReducedItem, RequestId, RequestOptions,
    RequestedCapabilities, ResponseReducer, Role, SchemaDialect, StructuredOutput, TerminalOutcome,
    ToolChoice, ToolDefinition, ToolName, ToolResult, negotiate,
};
use std::time::Duration;

/// The exact connection stage being checked; catalog discovery is deliberately separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionStage {
    /// A streamed text response completed.
    Generation,
    /// One declared harmless tool call was returned with valid arguments.
    ToolCalling,
    /// The model consumed the tool result and completed its response.
    ToolResult,
}

impl ConnectionStage {
    /// Short label suitable for settings and error output.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Generation => "generation",
            Self::ToolCalling => "tool calling",
            Self::ToolResult => "tool-result round trip",
        }
    }
}

/// Live evidence for all three stages on this exact provider/model selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionReport {
    /// Completed stages, in request order. This report is not a durable readiness claim.
    pub completed: [ConnectionStage; 3],
}

/// A safe stage-specific failure preserving the underlying diagnostic.
#[derive(Debug)]
pub struct ConnectionError {
    /// Stage which did not qualify.
    pub stage: ConnectionStage,
    /// Safe original protocol, HTTP, or transport failure.
    pub source: ProviderCanaryError,
}

impl fmt::Display for ConnectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} test failed: {}", self.stage.label(), self.source)
    }
}
impl std::error::Error for ConnectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Runs at most three small model requests, after the caller obtains the user's test choice.
///
/// The sole tool is an in-memory fixture: it cannot access files, the network, or application
/// tools. A 45-second total deadline cancels the current request and retains the failing stage.
///
/// # Errors
/// Returns the first failed stage. Earlier passes never make an incomplete check successful.
pub async fn verify_provider_connection(
    provider: &dyn ModelProvider,
    cancellation: CancellationToken,
) -> Result<ConnectionReport, ConnectionError> {
    let mut stage = ConnectionStage::Generation;
    let result = tokio::time::timeout(Duration::from_secs(45), async {
        verify_live_provider(provider, ProviderRequirement::new(false, 1, false)?, cancellation.clone()).await?;
        stage = ConnectionStage::ToolCalling;
        let messages = vec![message(Role::User, "Call peritus_connection_check once with no arguments. After its result, reply with the exact token from that result.")?];
        let tool_request = request(provider, messages.clone(), true)?;
        let reducer = drive(provider, tool_request, cancellation.clone()).await?;
        let assistant = assistant(&reducer)?;
        let calls: Vec<_> = reducer.completed_items().iter().filter_map(|item| if let ReducedItem::ToolCall { call, .. } = item { Some(call) } else { None }).collect();
        if calls.len() != 1 || calls[0].name().as_str() != "peritus_connection_check" || calls[0].arguments().to_wire_string() != "{}" {
            return Err(invalid("provider did not return the declared connection-test tool with empty arguments"));
        }
        stage = ConnectionStage::ToolResult;
        let mut messages = messages;
        messages.push(Message::new(Role::Assistant, assistant, ProtocolLimits::PRODUCTION)?);
        let output = CanonicalJson::parse(r#"{"token":"peritus-connection-ok"}"#, JsonBounds::value(ProtocolLimits::PRODUCTION))?;
        let result = ToolResult::new(calls[0].id().clone(), output, false);
        messages.push(Message::new(Role::Tool, vec![ContentBlock::ToolResult(result)], ProtocolLimits::PRODUCTION)?);
        let reducer = drive(provider, request(provider, messages, false)?, cancellation.clone()).await?;
        let text = reducer.completed_items().iter().filter_map(|item| if let ReducedItem::Text { text, .. } = item { Some(text.expose_for_wire()) } else { None }).collect::<String>();
        if !matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })) || !text.contains("peritus-connection-ok") {
            return Err(invalid("provider did not complete a response using the connection-test tool result"));
        }
        Ok(ConnectionReport { completed: [ConnectionStage::Generation, ConnectionStage::ToolCalling, ConnectionStage::ToolResult] })
    }).await;
    result
        .unwrap_or_else(|_| {
            let _ = cancellation.cancel();
            Err(invalid("connection test exceeded its 45-second deadline"))
        })
        .map_err(|source| ConnectionError { stage, source })
}

fn request(
    provider: &dyn ModelProvider,
    messages: Vec<Message>,
    call_tool: bool,
) -> Result<ModelRequest, ProviderCanaryError> {
    let profile = provider.profile();
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(
            &[Capability::ToolCalls],
            &[
                Capability::Streaming,
                Capability::UsageDetail,
                Capability::ReasoningReplay,
                Capability::ReasoningControls,
            ],
            profile.limits(),
        )?,
    )?;
    let name = ToolName::new("peritus_connection_check".to_owned())?;
    let schema = JsonSchema::parse(
        r#"{"type":"object","properties":{},"additionalProperties":false}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(limits),
    )?;
    let tool = ToolDefinition::new(name.clone(), None, schema, false);
    let options = RequestOptions::new(
        StructuredOutput::Text,
        ReasoningPolicy::Disabled,
        GenerationConfig::new(
            512.min(profile.limits().max_output_tokens()),
            Vec::new(),
            None,
            None,
            None,
        )?,
        CachePolicy::Disabled,
        PersistencePolicy::LOCAL_FIRST,
        None,
        Vec::new(),
    );
    Ok(ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(
            if call_tool { "peritus-connection-tool" } else { "peritus-connection-result" }
                .to_owned(),
        )?,
        messages,
        vec![tool],
        if call_tool { ToolChoice::Specific(name) } else { ToolChoice::None },
        ParallelToolPolicy::Disabled,
        options,
        limits,
    )?)
}

async fn drive(
    provider: &dyn ModelProvider,
    request: ModelRequest,
    cancellation: CancellationToken,
) -> Result<ResponseReducer, ProviderCanaryError> {
    let mut reducer =
        ResponseReducer::new(provider.profile().provider().clone(), ProtocolLimits::PRODUCTION);
    let mut stream = provider.start(request, cancellation).await?;
    while let Some(event) = stream.pull().await? {
        reducer.push(event)?;
        if let Some(terminal) = reducer.terminal() {
            match terminal {
                TerminalOutcome::Failed(failure) => {
                    return Err(ProviderCanaryError::Failure(Box::new(failure.clone())));
                }
                TerminalOutcome::Succeeded { .. } | TerminalOutcome::RequiresAction { .. } => {
                    return Ok(reducer);
                }
                _ => return Err(invalid("connection test ended without a usable completion")),
            }
        }
    }
    Err(invalid("connection test stream ended without a terminal response"))
}

fn assistant(reducer: &ResponseReducer) -> Result<Vec<ContentBlock>, ProviderCanaryError> {
    reducer
        .completed_items()
        .iter()
        .filter(|item| !matches!(item, ReducedItem::Reasoning { replay, .. } if replay.is_empty()))
        .map(|item| match item {
            ReducedItem::ToolCall { call, .. } => Ok(ContentBlock::ToolCall(call.clone())),
            ReducedItem::Text { text, .. } => Ok(ContentBlock::Text(text.clone())),
            ReducedItem::Reasoning { summary, replay, .. } => Ok(ContentBlock::Reasoning(
                ReasoningReplay::new(summary.clone(), replay.clone(), ProtocolLimits::PRODUCTION)?,
            )),
            _ => Err(invalid("connection-test tool response contained an unsupported output item")),
        })
        .collect()
}

fn message(role: Role, text: &str) -> Result<Message, ProviderCanaryError> {
    Ok(Message::new(
        role,
        vec![ContentBlock::Text(BoundedText::new(text.to_owned(), ProtocolLimits::PRODUCTION)?)],
        ProtocolLimits::PRODUCTION,
    )?)
}

fn invalid(detail: &'static str) -> ProviderCanaryError {
    ProviderCoreError::invalid_request("provider_connection_test", detail).into()
}
