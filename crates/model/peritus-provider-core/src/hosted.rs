//! Reviewed hosted API identities and operation routes; never a fallback model catalog.
//!
//! Contract sources and fixture provenance live in `docs/provider-contracts.md`.

use peritus_model_protocol::WireDialect;

use crate::ProviderCoreError;

mod catalog;

pub use catalog::discover_hosted_models;

/// A named service with an explicitly reviewed HTTP contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostedService {
    /// `OpenCode`'s pay-as-you-go model gateway.
    OpenCodeZen,
    /// `OpenCode`'s subscription model gateway.
    OpenCodeGo,
    /// `OpenRouter`'s normalized Chat Completions API.
    OpenRouter,
    /// Groq's OpenAI-compatible Chat Completions API.
    Groq,
    /// Together AI's Chat Completions API.
    Together,
    /// Fireworks AI's inference API.
    Fireworks,
    /// `DeepSeek`'s Chat Completions API.
    DeepSeek,
}

/// An exact documented inference route and its separately reviewed wire family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostedRoute {
    /// Complete operation URL (a base URL only for the native Google adapter).
    pub endpoint: &'static str,
    /// Protocol the selected model actually documents.
    pub dialect: WireDialect,
}

impl HostedService {
    /// Every reviewed hosted service in stable UI order.
    pub const ALL: [Self; 7] = [
        Self::OpenCodeZen,
        Self::OpenCodeGo,
        Self::OpenRouter,
        Self::Groq,
        Self::Together,
        Self::Fireworks,
        Self::DeepSeek,
    ];

    /// Parses only an explicit service identifier; URLs never imply compatibility.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|service| service.name() == name)
    }

    /// Stable configuration identifier.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenCodeZen => "opencode-zen",
            Self::OpenCodeGo => "opencode-go",
            Self::OpenRouter => "openrouter",
            Self::Groq => "groq",
            Self::Together => "together",
            Self::Fireworks => "fireworks",
            Self::DeepSeek => "deepseek",
        }
    }

    /// Metadata-only discovery URL; success here does not establish inference availability.
    #[must_use]
    pub const fn models_endpoint(self) -> &'static str {
        match self {
            Self::OpenCodeZen => "https://opencode.ai/zen/v1/models",
            Self::OpenCodeGo => "https://opencode.ai/zen/go/v1/models",
            Self::OpenRouter => "https://openrouter.ai/api/v1/models",
            Self::Groq => "https://api.groq.com/openai/v1/models",
            Self::Together => "https://api.together.ai/v1/models",
            Self::Fireworks => "https://api.fireworks.ai/v1/accounts/fireworks/models?pageSize=200",
            Self::DeepSeek => "https://api.deepseek.com/models",
        }
    }

    /// Resolves a discovered or explicitly selected wire family to an approved operation URL.
    ///
    /// # Errors
    /// Rejects protocols the service does not document. Catalog URLs never control credentials.
    pub fn route(self, dialect: WireDialect) -> Result<HostedRoute, ProviderCoreError> {
        use WireDialect::{
            AnthropicMessages, CompatibleChatCompletions, CompatibleResponses,
            GeminiGenerateContentV1,
        };
        let route_dialect =
            if dialect == WireDialect::OpenAiResponses { CompatibleResponses } else { dialect };
        let endpoint = match (self, route_dialect) {
            (Self::OpenCodeZen, CompatibleResponses) => "https://opencode.ai/zen/v1/responses",
            (Self::OpenCodeZen, CompatibleChatCompletions) => {
                "https://opencode.ai/zen/v1/chat/completions"
            }
            (Self::OpenCodeZen, AnthropicMessages) => "https://opencode.ai/zen/v1/messages",
            (Self::OpenCodeZen, GeminiGenerateContentV1) => "https://opencode.ai/zen/",
            (Self::OpenCodeGo, CompatibleResponses) => "https://opencode.ai/zen/go/v1/responses",
            (Self::OpenCodeGo, CompatibleChatCompletions) => {
                "https://opencode.ai/zen/go/v1/chat/completions"
            }
            (Self::OpenCodeGo, AnthropicMessages) => "https://opencode.ai/zen/go/v1/messages",
            (Self::OpenCodeGo, GeminiGenerateContentV1) => "https://opencode.ai/zen/go/",
            (Self::OpenRouter, CompatibleChatCompletions) => {
                "https://openrouter.ai/api/v1/chat/completions"
            }
            (Self::Groq, CompatibleChatCompletions) => {
                "https://api.groq.com/openai/v1/chat/completions"
            }
            (Self::Together, CompatibleChatCompletions) => {
                "https://api.together.ai/v1/chat/completions"
            }
            (Self::Fireworks, CompatibleChatCompletions) => {
                "https://api.fireworks.ai/inference/v1/chat/completions"
            }
            (Self::DeepSeek, CompatibleChatCompletions) => {
                "https://api.deepseek.com/chat/completions"
            }
            _ => {
                return Err(ProviderCoreError::unsupported_capability(
                    "selected protocol is not supported by this hosted service",
                ));
            }
        };
        Ok(HostedRoute { endpoint, dialect })
    }

    /// Services whose models can use different wire families.
    #[must_use]
    pub const fn mixed_protocols(self) -> bool {
        matches!(self, Self::OpenCodeZen | Self::OpenCodeGo)
    }
}
