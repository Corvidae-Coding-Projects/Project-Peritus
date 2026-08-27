//! Strict immutable C5 provider route declarations.

use std::{path::PathBuf, time::Duration};

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind, StateMode, WireDialect,
};
use peritus_provider_anthropic::{AnthropicBeta, AnthropicConfig};
use peritus_provider_compatible::{CompatibleAuth, CompatibleConfig, CompatibleProfile};
use peritus_provider_core::{
    CredentialReference, Endpoint, FramingLimits, HeaderName, HttpLimits, ProcessLimits,
    RetryPolicy,
};
use peritus_provider_google::GoogleConfig;
use peritus_provider_openai::OpenAiConfig;
use serde::Deserialize;
use serde::Deserializer;

use super::decode_identifier;
use crate::{
    DaemonError, DaemonErrorCode, DaemonRecovery, OfficialExecutableSelection, ProviderDeclaration,
};

const MAX_PROVIDERS: usize = 256;
const MAX_CAPABILITIES: usize = 17;
const PROVIDER_ROUTE_KINDS: &[&str] = &[
    "open-ai",
    "anthropic",
    "google-interactions",
    "google-generate-content",
    "compatible-responses",
    "compatible-chat-completions",
    "codex-runtime",
    "claude-runtime",
];

/// Closed provider adapter families configurable in G0.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRouteKind {
    /// First-party OpenAI Responses HTTP route.
    OpenAi,
    /// First-party Anthropic Messages HTTP route.
    Anthropic,
    /// Stable-v1 Google Interactions HTTP route.
    GoogleInteractions,
    /// Stable-v1 Google Generate Content HTTP route.
    GoogleGenerateContent,
    /// Explicit compatible Responses endpoint.
    CompatibleResponses,
    /// Explicit compatible Chat Completions endpoint.
    CompatibleChatCompletions,
    /// Official credential-owning Codex executable route.
    CodexRuntime,
    /// Official credential-owning Claude executable route.
    ClaudeRuntime,
}

impl<'de> Deserialize<'de> for ProviderRouteKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        match name.as_str() {
            "open-ai" => Ok(Self::OpenAi),
            "anthropic" => Ok(Self::Anthropic),
            "google-interactions" => Ok(Self::GoogleInteractions),
            "google-generate-content" => Ok(Self::GoogleGenerateContent),
            "compatible-responses" => Ok(Self::CompatibleResponses),
            "compatible-chat-completions" => Ok(Self::CompatibleChatCompletions),
            "codex-runtime" => Ok(Self::CodexRuntime),
            "claude-runtime" => Ok(Self::ClaudeRuntime),
            _ => Err(serde::de::Error::unknown_variant(&name, PROVIDER_ROUTE_KINDS)),
        }
    }
}

/// Immutable model identity, capabilities, and resource ceilings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderProfileDeclaration {
    profile_id: String,
    revision: u64,
    model: String,
    provider_name: Option<String>,
    capabilities: Vec<String>,
    max_input_tokens: u64,
    max_output_tokens: u64,
    max_tools: u32,
    max_parallel_tool_calls: u32,
    max_inline_media_bytes: u64,
}

/// One exact configured C5 transport route.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderRoute {
    kind: ProviderRouteKind,
    profile: ProviderProfileDeclaration,
    endpoint: Option<String>,
    credential_reference: Option<String>,
    credential_header: Option<String>,
    executable: Option<PathBuf>,
}

impl ProviderRoute {
    /// Builds the exact C5 declaration selected by validated configuration.
    ///
    /// # Errors
    ///
    /// Returns a stable configuration error for profile, endpoint, credential-reference, or
    /// executable drift.
    pub fn declaration(&self) -> Result<ProviderDeclaration, DaemonError> {
        let profile = self.profile.build(self.kind)?;
        let declaration = match self.kind {
            ProviderRouteKind::OpenAi => ProviderDeclaration::openai(
                OpenAiConfig::new(self.credential()?).map_err(provider_error)?,
                profile,
            ),
            ProviderRouteKind::Anthropic => ProviderDeclaration::anthropic(
                AnthropicConfig::new(
                    Endpoint::new(self.required_endpoint()?.to_owned()).map_err(provider_error)?,
                    self.credential()?,
                    profile,
                    Vec::<AnthropicBeta>::new(),
                    HttpLimits::PRODUCTION,
                    FramingLimits::PRODUCTION,
                    retry_policy()?,
                )
                .map_err(provider_error)?,
            ),
            ProviderRouteKind::GoogleInteractions | ProviderRouteKind::GoogleGenerateContent => {
                ProviderDeclaration::google(
                    GoogleConfig::new(
                        Endpoint::new(self.required_endpoint()?.to_owned())
                            .map_err(provider_error)?,
                        self.credential()?,
                        profile,
                        HttpLimits::PRODUCTION,
                        FramingLimits::PRODUCTION,
                        retry_policy()?,
                    )
                    .map_err(provider_error)?,
                )
            }
            ProviderRouteKind::CompatibleResponses
            | ProviderRouteKind::CompatibleChatCompletions => {
                let auth = match &self.credential_header {
                    None => CompatibleAuth::bearer(self.credential()?),
                    Some(name) => CompatibleAuth::raw_header(
                        self.credential()?,
                        HeaderName::new(name.clone()).map_err(provider_error)?,
                    ),
                }
                .map_err(provider_error)?;
                let config = CompatibleConfig::new(
                    Endpoint::new(self.required_endpoint()?.to_owned()).map_err(provider_error)?,
                    auth,
                )
                .map_err(provider_error)?;
                let profile = if self.kind == ProviderRouteKind::CompatibleResponses {
                    CompatibleProfile::responses(profile)
                } else {
                    CompatibleProfile::chat_completions(profile)
                }
                .map_err(provider_error)?;
                ProviderDeclaration::compatible(config, profile)
            }
            ProviderRouteKind::CodexRuntime => ProviderDeclaration::codex_runtime(
                profile,
                self.executable_selection(),
                ProcessLimits::PRODUCTION,
            )
            .map_err(provider_error)?,
            ProviderRouteKind::ClaudeRuntime => ProviderDeclaration::claude_runtime(
                profile,
                self.executable_selection(),
                ProcessLimits::PRODUCTION,
            )
            .map_err(provider_error)?,
        };
        Ok(declaration)
    }

    fn required_endpoint(&self) -> Result<&str, DaemonError> {
        self.endpoint.as_deref().ok_or_else(|| invalid("provider route requires an endpoint"))
    }

    fn credential(&self) -> Result<CredentialReference, DaemonError> {
        let value = self.credential_reference.clone().ok_or_else(|| {
            invalid("direct provider route requires an opaque credential reference")
        })?;
        CredentialReference::new(value).map_err(provider_error)
    }

    fn executable_selection(&self) -> OfficialExecutableSelection {
        self.executable
            .clone()
            .map_or(OfficialExecutableSelection::Discover, OfficialExecutableSelection::Pinned)
    }

    pub(crate) const fn requires_credential_broker(&self) -> bool {
        !matches!(self.kind, ProviderRouteKind::CodexRuntime | ProviderRouteKind::ClaudeRuntime)
    }
}

impl ProviderProfileDeclaration {
    fn build(&self, kind: ProviderRouteKind) -> Result<ProviderProfile, DaemonError> {
        let profile_id = peritus_types::ProviderProfileId::new(decode_identifier(
            &self.profile_id,
            "provider profile identity",
        )?)
        .map_err(|_| invalid("provider profile identity must be nonzero"))?;
        let (provider, dialect, output_limit) = route_profile(kind, self.provider_name.as_deref())?;
        let mut capabilities =
            self.capabilities.iter().map(|name| capability(name)).collect::<Result<Vec<_>, _>>()?;
        capabilities.sort_unstable();
        if capabilities.len() > MAX_CAPABILITIES
            || capabilities.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(invalid("provider profile capabilities are duplicate or oversized"));
        }
        let matrix = CapabilityMatrix::new(&capabilities, &[]).map_err(protocol_error)?;
        let limits = ModelLimits::new(
            self.max_input_tokens,
            self.max_output_tokens,
            self.max_tools,
            self.max_parallel_tool_calls,
            self.max_inline_media_bytes,
        )
        .map_err(protocol_error)?;
        ProviderProfile::new(
            profile_id,
            self.revision,
            ProviderName::new(provider).map_err(protocol_error)?,
            ModelName::new(self.model.clone()).map_err(protocol_error)?,
            dialect,
            matrix,
            CapabilityProvenance::Profiled,
            limits,
            output_limit,
            StateMode::StatelessReplay,
            ResumeKind::Unsupported,
            CancellationKind::BestEffortLocalAbort,
        )
        .map_err(protocol_error)
    }
}

pub(super) fn validate(routes: &[ProviderRoute]) -> Result<(), DaemonError> {
    if routes.len() > MAX_PROVIDERS {
        return Err(invalid("provider route inventory exceeds its production bound"));
    }
    let mut keys = std::collections::BTreeSet::new();
    for route in routes {
        let profile = route.profile.build(route.kind)?;
        if !keys.insert((profile.profile_id(), profile.revision())) {
            return Err(invalid("provider profile identity and revision are configured twice"));
        }
        let direct = !matches!(
            route.kind,
            ProviderRouteKind::CodexRuntime | ProviderRouteKind::ClaudeRuntime
        );
        if direct != route.credential_reference.is_some()
            || !direct && (route.endpoint.is_some() || route.credential_header.is_some())
            || direct && route.executable.is_some()
            || matches!(route.kind, ProviderRouteKind::OpenAi) && route.endpoint.is_some()
            || matches!(
                route.kind,
                ProviderRouteKind::Anthropic
                    | ProviderRouteKind::GoogleInteractions
                    | ProviderRouteKind::GoogleGenerateContent
                    | ProviderRouteKind::CompatibleResponses
                    | ProviderRouteKind::CompatibleChatCompletions
            ) && route.endpoint.is_none()
            || !matches!(
                route.kind,
                ProviderRouteKind::CompatibleResponses
                    | ProviderRouteKind::CompatibleChatCompletions
            ) && route.credential_header.is_some()
        {
            return Err(invalid("provider route fields do not match the selected adapter kind"));
        }
    }
    Ok(())
}

fn route_profile(
    kind: ProviderRouteKind,
    configured: Option<&str>,
) -> Result<(String, WireDialect, OutputLimitEnforcement), DaemonError> {
    let (default, dialect, output) = match kind {
        ProviderRouteKind::OpenAi => {
            ("openai", WireDialect::OpenAiResponses, OutputLimitEnforcement::ProviderEnforced)
        }
        ProviderRouteKind::Anthropic => {
            ("anthropic", WireDialect::AnthropicMessages, OutputLimitEnforcement::ProviderEnforced)
        }
        ProviderRouteKind::GoogleInteractions => {
            ("google", WireDialect::GeminiInteractionsV1, OutputLimitEnforcement::ProviderEnforced)
        }
        ProviderRouteKind::GoogleGenerateContent => (
            "google",
            WireDialect::GeminiGenerateContentV1,
            OutputLimitEnforcement::ProviderEnforced,
        ),
        ProviderRouteKind::CompatibleResponses => (
            "compatible",
            WireDialect::CompatibleResponses,
            OutputLimitEnforcement::ProviderEnforced,
        ),
        ProviderRouteKind::CompatibleChatCompletions => (
            "compatible",
            WireDialect::CompatibleChatCompletions,
            OutputLimitEnforcement::ProviderEnforced,
        ),
        ProviderRouteKind::CodexRuntime => {
            ("openai", WireDialect::OpenAiCodexRuntime, OutputLimitEnforcement::Advisory)
        }
        ProviderRouteKind::ClaudeRuntime => {
            ("anthropic", WireDialect::AnthropicClaudeRuntime, OutputLimitEnforcement::Advisory)
        }
    };
    if configured.is_some_and(|value| value != default)
        && !matches!(
            kind,
            ProviderRouteKind::CompatibleResponses | ProviderRouteKind::CompatibleChatCompletions
        )
    {
        return Err(invalid("provider name contradicts the selected adapter kind"));
    }
    Ok((configured.unwrap_or(default).to_owned(), dialect, output))
}

fn capability(value: &str) -> Result<Capability, DaemonError> {
    match value {
        "streaming" => Ok(Capability::Streaming),
        "tool-calls" => Ok(Capability::ToolCalls),
        "parallel-tool-calls" => Ok(Capability::ParallelToolCalls),
        "strict-structured-output" => Ok(Capability::StrictStructuredOutput),
        "prompt-caching" => Ok(Capability::PromptCaching),
        "image-input" => Ok(Capability::ImageInput),
        "audio-input" => Ok(Capability::AudioInput),
        "document-input" => Ok(Capability::DocumentInput),
        "reasoning-controls" => Ok(Capability::ReasoningControls),
        "reasoning-summaries" => Ok(Capability::ReasoningSummaries),
        "resumable-response" => Ok(Capability::ResumableResponse),
        "confirmed-cancellation" => Ok(Capability::ConfirmedCancellation),
        "usage-detail" => Ok(Capability::UsageDetail),
        "rate-limit-detail" => Ok(Capability::RateLimitDetail),
        "stored-state" => Ok(Capability::StoredState),
        "provider-extensions" => Ok(Capability::ProviderExtensions),
        "sampling-controls" => Ok(Capability::SamplingControls),
        _ => Err(invalid("provider profile contains an unknown capability name")),
    }
}

fn retry_policy() -> Result<RetryPolicy, DaemonError> {
    RetryPolicy::new(
        3,
        [
            Duration::from_millis(100),
            Duration::from_secs(2),
            Duration::from_secs(2),
            Duration::from_secs(10),
        ],
        64 * 1024 * 1024,
    )
    .map_err(provider_error)
}

fn provider_error(error: peritus_provider_core::ProviderCoreError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "construct provider route",
        error.to_string(),
        error,
    )
}

fn protocol_error(error: peritus_model_protocol::ProtocolError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "construct provider profile",
        error.to_string(),
        error,
    )
}

fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "validate daemon provider inventory",
        detail,
    )
}
