//! Stable provider identities, menu order, and service names.
use serde::{Deserialize, Serialize};

/// Provider login routes selectable in the product.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    /// Subscription-backed `OpenAI` access through the official Codex executable.
    CodexAccount,
    /// Subscription-backed Anthropic access through the official Claude executable.
    ClaudeAccount,
    /// Direct `OpenAI` API access through an operating-system credential store.
    OpenAiApi,
    /// Direct Anthropic API access through an operating-system credential store.
    AnthropicApi,
    /// Direct Google Gemini API access through an operating-system credential store.
    GoogleGeminiApi,
    /// Explicit compatible HTTP endpoint with credential-store-backed authentication.
    CompatibleEndpoint,
    /// `OpenCode` Zen hosted model API.
    OpenCodeZen,
    /// `OpenCode` Go hosted model API.
    OpenCodeGo,
    /// `OpenRouter` hosted model API.
    OpenRouter,
    /// Groq hosted model API.
    Groq,
    /// Together AI hosted model API.
    TogetherAi,
    /// Fireworks AI hosted model API.
    FireworksAi,
    /// `DeepSeek` hosted model API.
    DeepSeek,
}

impl ProviderKind {
    /// Provider choices in stable menu order; existing numeric selections retain their meaning.
    pub const ALL: [Self; 13] = [
        Self::CodexAccount,
        Self::ClaudeAccount,
        Self::OpenAiApi,
        Self::AnthropicApi,
        Self::GoogleGeminiApi,
        Self::CompatibleEndpoint,
        Self::OpenCodeZen,
        Self::OpenCodeGo,
        Self::OpenRouter,
        Self::Groq,
        Self::TogetherAi,
        Self::FireworksAi,
        Self::DeepSeek,
    ];

    /// Exact identifier of a reviewed hosted API, when selected.
    #[must_use]
    pub const fn hosted_service(self) -> Option<&'static str> {
        match self {
            Self::OpenCodeZen => Some("opencode-zen"),
            Self::OpenCodeGo => Some("opencode-go"),
            Self::OpenRouter => Some("openrouter"),
            Self::Groq => Some("groq"),
            Self::TogetherAi => Some("together"),
            Self::FireworksAi => Some("fireworks"),
            Self::DeepSeek => Some("deepseek"),
            _ => None,
        }
    }

    /// Stable identity shared by saved configuration and the launched product context.
    #[must_use]
    pub const fn profile_identity(self) -> [u8; 16] {
        let index = match self {
            Self::CodexAccount => 1,
            Self::ClaudeAccount => 2,
            Self::OpenAiApi => 3,
            Self::AnthropicApi => 4,
            Self::GoogleGeminiApi => 5,
            Self::CompatibleEndpoint => 6,
            Self::OpenCodeZen => 7,
            Self::OpenCodeGo => 8,
            Self::OpenRouter => 9,
            Self::Groq => 10,
            Self::TogetherAi => 11,
            Self::FireworksAi => 12,
            Self::DeepSeek => 13,
        };
        let mut bytes = [0; 16];
        bytes[0] = 0xa0 + index;
        bytes[15] = index;
        bytes
    }

    /// Returns the user-facing provider label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CodexAccount => "OpenAI with ChatGPT account",
            Self::ClaudeAccount => "Anthropic with Claude account",
            Self::OpenAiApi => "OpenAI API",
            Self::AnthropicApi => "Anthropic API",
            Self::GoogleGeminiApi => "Google Gemini API",
            Self::CompatibleEndpoint => "Compatible endpoint",
            Self::OpenCodeZen => "OpenCode Zen",
            Self::OpenCodeGo => "OpenCode Go",
            Self::OpenRouter => "OpenRouter",
            Self::Groq => "Groq",
            Self::TogetherAi => "Together AI",
            Self::FireworksAi => "Fireworks AI",
            Self::DeepSeek => "DeepSeek",
        }
    }

    /// Returns whether the route delegates account ownership to an official executable.
    #[must_use]
    pub const fn is_account(self) -> bool {
        matches!(self, Self::CodexAccount | Self::ClaudeAccount)
    }

    /// Returns whether the route uses a credential stored by the operating system.
    #[must_use]
    pub const fn is_direct(self) -> bool {
        !self.is_account()
    }
}
