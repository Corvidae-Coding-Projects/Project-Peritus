//! Provider role requirements and user-authorized route selection.

use core::fmt;

use peritus_model_protocol::{
    BoundedText, CachePolicy, Capability, ContentBlock, GenerationConfig, Message, ModelEvent,
    ModelRequest, ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderProfile,
    ReasoningEffort, ReasoningPolicy, RequestId, RequestOptions, RequestedCapabilities, Role,
    StructuredOutput, SummaryPolicy, ToolChoice, WireDialect, negotiate,
};

use crate::{CancellationToken, ModelProvider, ProviderCoreError, ProviderTerminal};

/// Provider transport and credential ownership family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRoute {
    /// Direct first-party provider API using an explicitly configured credential source.
    FirstPartyApi,
    /// Reviewed OpenAI-compatible API using an explicitly configured credential source.
    CompatibleApi,
    /// Official account-owning executable acting only as a model router.
    AccountRuntime,
}

impl ProviderRoute {
    /// Derives the route from the immutable provider wire dialect.
    #[must_use]
    pub const fn from_dialect(dialect: WireDialect) -> Self {
        match dialect {
            WireDialect::CompatibleResponses | WireDialect::CompatibleChatCompletions => {
                Self::CompatibleApi
            }
            WireDialect::OpenAiCodexRuntime | WireDialect::AnthropicClaudeRuntime => {
                Self::AccountRuntime
            }
            WireDialect::OpenAiResponses
            | WireDialect::AnthropicMessages
            | WireDialect::GeminiInteractionsV1
            | WireDialect::GeminiGenerateContentV1 => Self::FirstPartyApi,
        }
    }
}

/// Strongest current route-readiness observation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProviderAvailability {
    /// Configuration exists but no current credential observation has completed.
    Unchecked,
    /// The configured API credential source resolved a nonempty bounded credential.
    CredentialPresent,
    /// A real minimal provider request completed through this exact route.
    LiveCanary,
    /// Credential inspection or a live canary proved the route unavailable.
    Unavailable,
}

impl ProviderAvailability {
    /// Whether the route has current evidence sufficient to begin ordinary work.
    #[must_use]
    pub const fn is_available(self) -> bool {
        matches!(self, Self::CredentialPresent | Self::LiveCanary)
    }
}

/// Role-level capability envelope checked before a model turn is spent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderRequirement {
    image_input: bool,
    minimum_context_tokens: u64,
    tool_protocol: bool,
}

impl ProviderRequirement {
    /// Creates a role requirement. Text generation is always required.
    ///
    /// # Errors
    ///
    /// Rejects a zero context requirement.
    pub const fn new(
        image_input: bool,
        minimum_context_tokens: u64,
        tool_protocol: bool,
    ) -> Result<Self, ProviderCoreError> {
        if minimum_context_tokens == 0 {
            return Err(ProviderCoreError::unsupported_capability(
                "minimum context requirement must be nonzero",
            ));
        }
        Ok(Self { image_input, minimum_context_tokens, tool_protocol })
    }

    /// Whether image input is required for this role invocation.
    #[must_use]
    pub const fn image_input(self) -> bool {
        self.image_input
    }

    /// Minimum usable provider input-token limit.
    #[must_use]
    pub const fn minimum_context_tokens(self) -> u64 {
        self.minimum_context_tokens
    }

    /// Whether the Peritus application tool-call protocol is required.
    #[must_use]
    pub const fn tool_protocol(self) -> bool {
        self.tool_protocol
    }
}

/// Successfully checked provider route selected before invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderQualification {
    route: ProviderRoute,
    availability: ProviderAvailability,
    image_input: bool,
    maximum_context_tokens: u64,
    tool_protocol: bool,
}

/// Failed real provider canary with a stable terminal classification.
#[derive(Debug)]
pub enum ProviderCanaryError {
    /// The bounded canary request could not be represented by the model protocol.
    Protocol(peritus_model_protocol::ProtocolError),
    /// Request construction, provider setup, or stream transport failed.
    Core(ProviderCoreError),
    /// The provider emitted a normalized terminal failure.
    Terminal(ProviderTerminal),
}

impl ProviderCanaryError {
    /// Stable provider terminal when execution reached one.
    #[must_use]
    pub const fn terminal(&self) -> Option<ProviderTerminal> {
        match self {
            Self::Protocol(_) | Self::Core(_) => None,
            Self::Terminal(terminal) => Some(*terminal),
        }
    }
}

impl fmt::Display for ProviderCanaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(error) => {
                write!(formatter, "provider canary request is invalid: {error}")
            }
            Self::Core(error) => write!(formatter, "provider canary setup failed: {error}"),
            Self::Terminal(terminal) => write!(
                formatter,
                "provider canary ended with {:?}; recovery {:?}",
                terminal.cause(),
                terminal.recovery(),
            ),
        }
    }
}

impl std::error::Error for ProviderCanaryError {}

impl From<ProviderCoreError> for ProviderCanaryError {
    fn from(error: ProviderCoreError) -> Self {
        Self::Core(error)
    }
}

impl From<peritus_model_protocol::ProtocolError> for ProviderCanaryError {
    fn from(error: peritus_model_protocol::ProtocolError) -> Self {
        Self::Protocol(error)
    }
}

impl ProviderQualification {
    /// Qualifies an immutable profile and current readiness observation.
    ///
    /// # Errors
    ///
    /// Rejects unavailable routes, image mismatch, missing tool calls, or insufficient context.
    pub const fn evaluate(
        profile: &ProviderProfile,
        availability: ProviderAvailability,
        requirement: ProviderRequirement,
    ) -> Result<Self, ProviderCoreError> {
        if !availability.is_available() {
            return Err(ProviderCoreError::unavailable(
                "provider route has no current credential or live-canary evidence",
            ));
        }
        if requirement.image_input() && !profile.capabilities().supports(Capability::ImageInput) {
            return Err(ProviderCoreError::unsupported_capability(
                "provider route does not support required image input",
            ));
        }
        if requirement.tool_protocol() && !profile.capabilities().supports(Capability::ToolCalls) {
            return Err(ProviderCoreError::unsupported_capability(
                "provider route does not support the application tool protocol",
            ));
        }
        if profile.limits().max_input_tokens() < requirement.minimum_context_tokens() {
            return Err(ProviderCoreError::unsupported_capability(
                "provider route context limit is below the role requirement",
            ));
        }
        Ok(Self {
            route: ProviderRoute::from_dialect(profile.dialect()),
            availability,
            image_input: profile.capabilities().supports(Capability::ImageInput),
            maximum_context_tokens: profile.limits().max_input_tokens(),
            tool_protocol: profile.capabilities().supports(Capability::ToolCalls),
        })
    }

    /// Selected provider route family.
    #[must_use]
    pub const fn route(self) -> ProviderRoute {
        self.route
    }

    /// Current readiness evidence used for selection.
    #[must_use]
    pub const fn availability(self) -> ProviderAvailability {
        self.availability
    }

    /// Whether this route supports image input.
    #[must_use]
    pub const fn image_input(self) -> bool {
        self.image_input
    }

    /// Maximum declared provider input-token limit.
    #[must_use]
    pub const fn maximum_context_tokens(self) -> u64 {
        self.maximum_context_tokens
    }

    /// Whether this route supports the Peritus tool-call protocol.
    #[must_use]
    pub const fn tool_protocol(self) -> bool {
        self.tool_protocol
    }
}

/// One primary or explicitly user-authorized fallback candidate.
#[derive(Clone, Copy)]
pub struct ProviderCandidate<'a> {
    provider: &'a dyn ModelProvider,
    authorized: bool,
}

impl<'a> ProviderCandidate<'a> {
    /// Binds a provider to the caller's explicit fallback authorization decision.
    #[must_use]
    pub const fn new(provider: &'a dyn ModelProvider, authorized: bool) -> Self {
        Self { provider, authorized }
    }
}

/// Selects the primary route or first capable, available, user-authorized fallback.
///
/// # Errors
///
/// Returns the primary qualification failure when no authorized candidate qualifies.
pub fn select_qualified_provider<'a>(
    primary: ProviderCandidate<'a>,
    fallbacks: &[ProviderCandidate<'a>],
    requirement: ProviderRequirement,
) -> Result<(&'a dyn ModelProvider, ProviderQualification), ProviderCoreError> {
    let primary_result = qualify_candidate(primary, requirement);
    if let Ok(qualified) = primary_result {
        return Ok((primary.provider, qualified));
    }
    for candidate in fallbacks.iter().copied().filter(|candidate| candidate.authorized) {
        if let Ok(qualified) = qualify_candidate(candidate, requirement) {
            return Ok((candidate.provider, qualified));
        }
    }
    Err(primary_result.expect_err("primary qualification was already observed to fail"))
}

fn qualify_candidate(
    candidate: ProviderCandidate<'_>,
    requirement: ProviderRequirement,
) -> Result<ProviderQualification, ProviderCoreError> {
    ProviderQualification::evaluate(
        candidate.provider.profile(),
        candidate.provider.availability(),
        requirement,
    )
}

/// Sends one minimal real request and qualifies the exact route only after a usable completion.
///
/// This is intended after login or credential repair, before expensive work. It never grants
/// fallback authority and does not treat an empty or failed terminal as availability evidence.
///
/// # Errors
///
/// Returns a typed provider terminal or provider-core construction/transport error.
pub async fn verify_live_provider(
    provider: &dyn ModelProvider,
    requirement: ProviderRequirement,
    cancellation: CancellationToken,
) -> Result<ProviderQualification, ProviderCanaryError> {
    validate_capabilities(provider.profile(), requirement)?;
    let request = canary_request(provider.profile())?;
    let mut stream = provider.start(request, cancellation).await.map_err(|error| {
        ProviderCanaryError::Terminal(ProviderTerminal::from_core_error(&error))
    })?;
    let mut text_observed = false;
    loop {
        let Some(envelope) = stream.pull().await.map_err(|error| {
            ProviderCanaryError::Terminal(ProviderTerminal::from_core_error(&error))
        })?
        else {
            return Err(ProviderCanaryError::Terminal(ProviderTerminal::empty_response()));
        };
        match envelope.event() {
            ModelEvent::TextDelta { fragment, .. } => {
                text_observed |= fragment.expose().iter().any(|byte| !byte.is_ascii_whitespace());
            }
            ModelEvent::ResponseCompleted if text_observed => {
                return ProviderQualification::evaluate(
                    provider.profile(),
                    ProviderAvailability::LiveCanary,
                    requirement,
                )
                .map_err(ProviderCanaryError::Core);
            }
            ModelEvent::ResponseCompleted => {
                return Err(ProviderCanaryError::Terminal(ProviderTerminal::empty_response()));
            }
            ModelEvent::ResponseFailed(failure) => {
                return Err(ProviderCanaryError::Terminal(ProviderTerminal::from_model_failure(
                    failure,
                )));
            }
            ModelEvent::ResponseCancelled => {
                return Err(ProviderCanaryError::Terminal(ProviderTerminal::from_core_error(
                    &ProviderCoreError::cancelled("provider_canary"),
                )));
            }
            _ => {}
        }
    }
}

fn validate_capabilities(
    profile: &ProviderProfile,
    requirement: ProviderRequirement,
) -> Result<(), ProviderCoreError> {
    ProviderQualification::evaluate(profile, ProviderAvailability::CredentialPresent, requirement)
        .map(|_| ())
}

fn canary_request(profile: &ProviderProfile) -> Result<ModelRequest, ProviderCanaryError> {
    let optional = [Capability::ReasoningControls];
    let negotiated =
        negotiate(profile, RequestedCapabilities::new(&[], &optional, profile.limits())?)?;
    let limits = ProtocolLimits::PRODUCTION;
    let prompt = BoundedText::new(
        "Reply with one short word to confirm this provider route is usable.".to_owned(),
        limits,
    )?;
    let messages = vec![Message::new(Role::User, vec![ContentBlock::Text(prompt)], limits)?];
    let reasoning = if negotiated.includes(Capability::ReasoningControls) {
        ReasoningPolicy::Effort { effort: ReasoningEffort::Low, summary: SummaryPolicy::None }
    } else {
        ReasoningPolicy::Disabled
    };
    let options = RequestOptions::new(
        StructuredOutput::Text,
        reasoning,
        GenerationConfig::new(16, Vec::new(), None, None, None)?,
        CachePolicy::Disabled,
        PersistencePolicy::LOCAL_FIRST,
        None,
        Vec::new(),
    );
    Ok(ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("peritus-live-provider-canary".to_owned())?,
        messages,
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        options,
        limits,
    )?)
}
