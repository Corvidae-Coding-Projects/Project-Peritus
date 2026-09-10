//! Redaction-safe onboarding failures.

/// Provider discovery, status, or interactive-login failure.
#[derive(Debug, thiserror::Error)]
pub enum OnboardingError {
    /// Model metadata discovery failed without substituting a bundled catalog.
    #[error(
        "model discovery unavailable; check authentication or explicitly enter a manual model ID"
    )]
    ModelCatalog,
    /// Safe model-discovery failure retaining the actual operation diagnostic.
    #[error("model discovery failed: {0}")]
    ModelDiscovery(peritus_provider_core::ProviderCoreError),
    /// The selected provider is not an official account-backed route.
    #[error("selected provider does not support official account login")]
    UnsupportedProvider,
    /// A checked official executable is not installed.
    #[error("{provider} executable is not installed or executable")]
    ExecutableUnavailable {
        /// User-facing provider label.
        provider: &'static str,
    },
    /// The user-approved official installer could not finish successfully.
    #[error("could not install {provider} during {stage}: {detail}")]
    Installation {
        /// User-facing provider label.
        provider: &'static str,
        /// Non-secret failed operation.
        stage: &'static str,
        /// Operating-system or process-exit detail.
        detail: String,
    },
    /// A bounded status process could not be started.
    #[error("could not inspect {provider} login status: {detail}")]
    StatusProcess {
        /// User-facing provider label.
        provider: &'static str,
        /// Redaction-safe operating-system detail.
        detail: String,
    },
    /// The official interactive login process could not be started.
    #[error("could not start {provider} login: {detail}")]
    LoginProcess {
        /// User-facing provider label.
        provider: &'static str,
        /// Redaction-safe operating-system detail.
        detail: String,
    },
    /// The official login process returned without establishing authentication.
    #[error("{provider} login did not complete; retry or choose another provider")]
    LoginIncomplete {
        /// User-facing provider label.
        provider: &'static str,
    },
    /// Operating-system random identity generation failed.
    #[error("could not generate a credential identity: {0}")]
    Random(String),
    /// Credential-store publication, lookup, or removal failed safely.
    #[error("operating-system credential store failed: {0}")]
    Secret(#[from] peritus_secrets::SecretError),
    /// Non-secret direct-provider settings are invalid.
    #[error("direct provider settings are invalid: {0}")]
    ProductState(#[from] peritus_product_state::ProductStateError),
}
