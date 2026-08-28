//! Bounded immutable construction of configured C5 provider adapters.

use core::fmt;
use std::collections::BTreeMap;
use std::sync::Arc;

use peritus_provider_anthropic::{AnthropicClient, ClaudeRuntimeProvider};
use peritus_provider_compatible::CompatibleClient;
use peritus_provider_core::{
    Credential, CredentialReference, CredentialSource, ModelProvider, ProviderCoreError,
};
use peritus_provider_google::GoogleClient;
use peritus_provider_openai::{CodexRuntimeProvider, OpenAiProvider};
use peritus_types::ProviderProfileId;

use super::profiles::{ProviderDeclaration, ProviderProfileKey};

const MAX_PROVIDER_PROFILES: usize = 256;

/// Startup ceiling for the immutable provider profile inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderRegistryLimits {
    max_profiles: usize,
}

impl ProviderRegistryLimits {
    /// A conservative production default for a local daemon.
    pub const PRODUCTION: Self = Self { max_profiles: 64 };

    /// Creates a positive profile ceiling no wider than the compiled production maximum.
    ///
    /// # Errors
    ///
    /// Rejects zero and values above the compiled registry bound.
    pub const fn new(max_profiles: usize) -> Result<Self, ProviderRegistryError> {
        if max_profiles == 0 || max_profiles > MAX_PROVIDER_PROFILES {
            Err(ProviderRegistryError::new(
                ProviderRegistryErrorKind::InvalidLimit,
                "provider registry limit must be positive and within its compiled maximum",
            ))
        } else {
            Ok(Self { max_profiles })
        }
    }

    /// Returns the maximum configured profile revisions.
    #[must_use]
    pub const fn max_profiles(self) -> usize {
        self.max_profiles
    }
}

impl Default for ProviderRegistryLimits {
    fn default() -> Self {
        Self::PRODUCTION
    }
}

/// Concrete C5 adapter selected for one registry entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderAdapterKind {
    /// First-party `OpenAI` Responses HTTP adapter.
    OpenAi,
    /// First-party Anthropic Messages HTTP adapter.
    Anthropic,
    /// First-party Google stable-v1 HTTP adapter.
    Google,
    /// Explicit compatible HTTP adapter.
    Compatible,
    /// Official Codex executable adapter.
    CodexRuntime,
    /// Official Claude executable adapter.
    ClaudeRuntime,
}

struct ProviderRegistration {
    kind: ProviderAdapterKind,
    provider: Arc<dyn ModelProvider>,
}

/// Immutable exact-revision provider inventory shared by bounded effect workers.
pub struct ProviderRegistry {
    entries: BTreeMap<ProviderProfileKey, ProviderRegistration>,
}

impl ProviderRegistry {
    /// Constructs every explicitly declared provider exactly once.
    ///
    /// Direct HTTP routes require a credential broker. The broker remains behind C5's
    /// [`CredentialSource`] boundary and is invoked only immediately before request submission.
    /// Executable routes never receive it and continue to own their login state externally.
    ///
    /// # Errors
    ///
    /// Rejects an oversized inventory, duplicate `(profile ID, revision)` key, absent credential
    /// broker for a direct route, adapter construction failure, or profile drift between the
    /// declaration and constructed adapter.
    pub fn build(
        declarations: Vec<ProviderDeclaration>,
        limits: ProviderRegistryLimits,
        credential_broker: Option<Arc<dyn CredentialSource>>,
    ) -> Result<Self, ProviderRegistryError> {
        if declarations.len() > limits.max_profiles() {
            return Err(ProviderRegistryError::new(
                ProviderRegistryErrorKind::LimitExceeded,
                "configured provider profiles exceed the daemon registry ceiling",
            ));
        }
        let mut entries = BTreeMap::new();
        for declaration in declarations {
            let expected = declaration.key();
            if entries.contains_key(&expected) {
                return Err(ProviderRegistryError::new(
                    ProviderRegistryErrorKind::DuplicateProfile,
                    "provider profile ID and revision are configured more than once",
                ));
            }
            let (kind, provider) = instantiate(declaration, credential_broker.as_ref())?;
            if ProviderProfileKey::from_profile(provider.profile()) != expected {
                return Err(ProviderRegistryError::new(
                    ProviderRegistryErrorKind::ProfileDrift,
                    "constructed provider does not expose its declared immutable profile",
                ));
            }
            entries.insert(expected, ProviderRegistration { kind, provider });
        }
        Ok(Self { entries })
    }

    /// Returns the number of configured exact profile revisions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no provider profile was configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the registered keys in deterministic order.
    #[must_use]
    pub fn keys(&self) -> Vec<ProviderProfileKey> {
        self.entries.keys().copied().collect()
    }

    /// Resolves only the exact profile identity and revision requested by authoritative work.
    ///
    /// No fallback to a newer revision or provider-family default is performed.
    #[must_use]
    pub fn provider(
        &self,
        profile_id: ProviderProfileId,
        revision: u64,
    ) -> Option<Arc<dyn ModelProvider>> {
        let key = ProviderProfileKey::from_parts(profile_id, revision)?;
        self.entries.get(&key).map(|registration| Arc::clone(&registration.provider))
    }

    /// Resolves the sole configured revision for one product-selected profile identity.
    ///
    /// Returns `None` when the profile is absent or its configuration contains multiple
    /// simultaneously active revisions.
    #[must_use]
    pub fn current_provider(
        &self,
        profile_id: ProviderProfileId,
    ) -> Option<Arc<dyn ModelProvider>> {
        let mut matches = self.entries.iter().filter(|(key, _)| key.profile_id() == profile_id);
        let (_, registration) = matches.next()?;
        if matches.next().is_some() { None } else { Some(Arc::clone(&registration.provider)) }
    }

    /// Returns the concrete adapter kind for an exact registered revision.
    #[must_use]
    pub fn adapter_kind(
        &self,
        profile_id: ProviderProfileId,
        revision: u64,
    ) -> Option<ProviderAdapterKind> {
        let key = ProviderProfileKey::from_parts(profile_id, revision)?;
        self.entries.get(&key).map(|registration| registration.kind)
    }
}

impl fmt::Debug for ProviderRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("ProviderRegistry").field("profiles", &self.entries.len()).finish()
    }
}

fn instantiate(
    declaration: ProviderDeclaration,
    credential_broker: Option<&Arc<dyn CredentialSource>>,
) -> Result<(ProviderAdapterKind, Arc<dyn ModelProvider>), ProviderRegistryError> {
    let direct_broker = || {
        credential_broker.cloned().ok_or_else(|| {
            ProviderRegistryError::new(
                ProviderRegistryErrorKind::CredentialBrokerMissing,
                "a direct provider route requires the configured credential broker",
            )
        })
    };
    match declaration {
        ProviderDeclaration::OpenAi { config, profile } => {
            let provider = OpenAiProvider::new(config, profile, direct_broker()?)?;
            Ok((ProviderAdapterKind::OpenAi, Arc::new(provider)))
        }
        ProviderDeclaration::Anthropic(config) => {
            let credentials = Box::new(SharedCredentialSource(direct_broker()?));
            let provider = AnthropicClient::new(config, credentials)?;
            Ok((ProviderAdapterKind::Anthropic, Arc::new(provider)))
        }
        ProviderDeclaration::Google(config) => {
            let credentials = Box::new(SharedCredentialSource(direct_broker()?));
            let provider = GoogleClient::new(config, credentials)?;
            Ok((ProviderAdapterKind::Google, Arc::new(provider)))
        }
        ProviderDeclaration::Compatible { config, profile } => {
            let provider = CompatibleClient::new(config, profile, direct_broker()?)?;
            Ok((ProviderAdapterKind::Compatible, Arc::new(provider)))
        }
        ProviderDeclaration::CodexRuntime(config) => {
            Ok((ProviderAdapterKind::CodexRuntime, Arc::new(CodexRuntimeProvider::new(config))))
        }
        ProviderDeclaration::ClaudeRuntime(config) => {
            Ok((ProviderAdapterKind::ClaudeRuntime, Arc::new(ClaudeRuntimeProvider::new(config))))
        }
    }
}

struct SharedCredentialSource(Arc<dyn CredentialSource>);

impl CredentialSource for SharedCredentialSource {
    fn resolve(&self, reference: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        self.0.resolve(reference)
    }
}

/// Stable category for a provider-registry construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRegistryErrorKind {
    /// A configured registry ceiling is invalid.
    InvalidLimit,
    /// The configured inventory exceeds its ceiling.
    LimitExceeded,
    /// The same exact profile key was configured twice.
    DuplicateProfile,
    /// A direct route has no credential broker.
    CredentialBrokerMissing,
    /// C5 rejected adapter construction.
    ProviderConfiguration,
    /// An instantiated adapter exposed a different profile key.
    ProfileDrift,
}

/// Redaction-safe provider-registry construction failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRegistryError {
    kind: ProviderRegistryErrorKind,
    detail: &'static str,
    source: Option<ProviderCoreError>,
}

impl ProviderRegistryError {
    const fn new(kind: ProviderRegistryErrorKind, detail: &'static str) -> Self {
        Self { kind, detail, source: None }
    }

    /// Returns the stable error category.
    #[must_use]
    pub const fn kind(&self) -> ProviderRegistryErrorKind {
        self.kind
    }
}

impl From<ProviderCoreError> for ProviderRegistryError {
    fn from(source: ProviderCoreError) -> Self {
        Self {
            kind: ProviderRegistryErrorKind::ProviderConfiguration,
            detail: "C5 rejected configured provider construction",
            source: Some(source),
        }
    }
}

impl fmt::Display for ProviderRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.detail)
    }
}

impl std::error::Error for ProviderRegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|source| source as &(dyn std::error::Error + 'static))
    }
}
