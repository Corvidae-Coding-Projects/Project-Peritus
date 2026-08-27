//! Typed provider declarations accepted by the daemon composition root.

use std::path::PathBuf;

use peritus_model_protocol::ProviderProfile;
use peritus_provider_anthropic::{AnthropicConfig, ClaudeExecutable, ClaudeRuntimeConfig};
use peritus_provider_compatible::{CompatibleConfig, CompatibleProfile};
use peritus_provider_core::{ProcessLimits, ProviderCoreError};
use peritus_provider_google::GoogleConfig;
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, OpenAiConfig};
use peritus_types::ProviderProfileId;

/// Exact immutable lookup key for one provider profile revision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderProfileKey {
    profile_id: ProviderProfileId,
    revision: u64,
}

impl ProviderProfileKey {
    /// Builds the exact key carried by an already validated C5 profile.
    #[must_use]
    pub const fn from_profile(profile: &ProviderProfile) -> Self {
        Self { profile_id: profile.profile_id(), revision: profile.revision() }
    }

    pub(super) const fn from_parts(profile_id: ProviderProfileId, revision: u64) -> Option<Self> {
        if revision == 0 { None } else { Some(Self { profile_id, revision }) }
    }

    /// Returns the stable profile identity.
    #[must_use]
    pub const fn profile_id(self) -> ProviderProfileId {
        self.profile_id
    }

    /// Returns the nonzero immutable profile revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
}

/// Explicit executable selection for an account-backed official adapter.
///
/// Discovery is performed only when this variant is present in validated configuration. It never
/// discovers or enables a provider profile by itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OfficialExecutableSelection {
    /// Find the official executable on the daemon's startup `PATH` and pin its canonical path.
    Discover,
    /// Pin this explicitly configured path after canonical-file and executable checks.
    Pinned(PathBuf),
}

impl OfficialExecutableSelection {
    fn codex(self) -> Result<CodexExecutable, ProviderCoreError> {
        match self {
            Self::Discover => CodexExecutable::discover(),
            Self::Pinned(path) => CodexExecutable::pin(path),
        }
    }

    fn claude(self) -> Result<ClaudeExecutable, ProviderCoreError> {
        match self {
            Self::Discover => ClaudeExecutable::discover(),
            Self::Pinned(path) => ClaudeExecutable::pin(path),
        }
    }
}

/// One configured C5 adapter declaration.
///
/// Direct declarations carry C5 configuration values whose credentials are opaque
/// [`peritus_provider_core::CredentialReference`] values. Account-backed declarations contain no
/// credential reference: the unmodified official executable owns login state and acts only as the
/// constrained transport selected here.
pub enum ProviderDeclaration {
    /// First-party `OpenAI` Responses HTTP adapter.
    OpenAi {
        /// Checked endpoint, routing, limits, and opaque credential reference.
        config: OpenAiConfig,
        /// Exact immutable C5 capability profile.
        profile: ProviderProfile,
    },
    /// First-party Anthropic Messages HTTP adapter.
    Anthropic(AnthropicConfig),
    /// First-party Google stable-v1 HTTP adapter.
    Google(GoogleConfig),
    /// Explicitly reviewed Responses or Chat Completions compatible adapter.
    Compatible {
        /// Checked exact endpoint, header mappings, limits, and opaque credential reference.
        config: CompatibleConfig,
        /// Separately validated compatible wire contract and immutable profile.
        profile: CompatibleProfile,
    },
    /// Account-backed `OpenAI` route through the official Codex executable.
    CodexRuntime(CodexRuntimeConfig),
    /// Account-backed Anthropic route through the official Claude executable.
    ClaudeRuntime(ClaudeRuntimeConfig),
}

impl ProviderDeclaration {
    /// Declares a first-party `OpenAI` Responses adapter.
    #[must_use]
    pub const fn openai(config: OpenAiConfig, profile: ProviderProfile) -> Self {
        Self::OpenAi { config, profile }
    }

    /// Declares a first-party Anthropic Messages adapter.
    #[must_use]
    pub const fn anthropic(config: AnthropicConfig) -> Self {
        Self::Anthropic(config)
    }

    /// Declares a first-party Google stable-v1 adapter.
    #[must_use]
    pub const fn google(config: GoogleConfig) -> Self {
        Self::Google(config)
    }

    /// Declares one explicitly reviewed compatible adapter.
    #[must_use]
    pub const fn compatible(config: CompatibleConfig, profile: CompatibleProfile) -> Self {
        Self::Compatible { config, profile }
    }

    /// Pins and declares an official Codex executable adapter.
    ///
    /// # Errors
    ///
    /// Rejects an unavailable executable or a profile that drifts from the constrained Codex
    /// runtime contract.
    pub fn codex_runtime(
        profile: ProviderProfile,
        executable: OfficialExecutableSelection,
        limits: ProcessLimits,
    ) -> Result<Self, ProviderCoreError> {
        let executable = executable.codex()?;
        CodexRuntimeConfig::new(executable, profile, limits).map(Self::CodexRuntime)
    }

    /// Pins and declares an official Claude executable adapter.
    ///
    /// # Errors
    ///
    /// Rejects an unavailable executable or a profile that drifts from the constrained Claude
    /// runtime contract.
    pub fn claude_runtime(
        profile: ProviderProfile,
        executable: OfficialExecutableSelection,
        limits: ProcessLimits,
    ) -> Result<Self, ProviderCoreError> {
        let executable = executable.claude()?;
        ClaudeRuntimeConfig::new(executable, profile, limits).map(Self::ClaudeRuntime)
    }

    /// Returns the exact profile that the instantiated adapter must expose.
    #[must_use]
    pub const fn profile(&self) -> &ProviderProfile {
        match self {
            Self::OpenAi { profile, .. } => profile,
            Self::Anthropic(config) => config.profile(),
            Self::Google(config) => config.profile(),
            Self::Compatible { profile, .. } => profile.provider_profile(),
            Self::CodexRuntime(config) => config.profile(),
            Self::ClaudeRuntime(config) => config.profile(),
        }
    }

    /// Returns the declaration's immutable lookup key.
    #[must_use]
    pub const fn key(&self) -> ProviderProfileKey {
        ProviderProfileKey::from_profile(self.profile())
    }
}
