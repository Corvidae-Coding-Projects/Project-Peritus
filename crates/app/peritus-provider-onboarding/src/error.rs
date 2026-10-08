//! Redaction-safe onboarding failures.

/// Provider discovery, status, or interactive-login failure.
#[derive(Debug, thiserror::Error)]
pub enum OnboardingError {
    /// The caller cancelled provider discovery or login.
    #[error("provider setup was cancelled")]
    Cancelled,
    /// Durable provider-effect ownership could not be published or reconciled.
    #[error("provider effect journal failed during {operation}: {detail}")]
    EffectJournal {
        /// Stable journal operation.
        operation: &'static str,
        /// Redaction-safe filesystem or record detail.
        detail: String,
    },
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
    /// Installer completion and owned-effect cleanup both produced relevant outcomes.
    #[error(
        "could not reconcile {provider} installation during {stage}: {primary}; cleanup: {cleanup}"
    )]
    InstallationReconciliation {
        /// User-facing provider label.
        provider: &'static str,
        /// Exact installation stage whose effect required reconciliation.
        stage: &'static str,
        /// Redaction-safe primary operation outcome.
        primary: String,
        /// Redaction-safe cleanup outcome.
        cleanup: String,
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
    /// Credential publication returned an identity inconsistent with its validated profile.
    #[error("credential publication did not match validated reference {credential_reference}: {detail}")]
    CredentialPublication {
        /// Exact opaque non-secret credential reference reserved by the profile.
        credential_reference: String,
        /// Stable non-secret mismatch detail.
        detail: &'static str,
    },
    /// Credential publication failed and exact-resource cleanup also failed.
    #[error(
        "credential publication requires reconciliation for {credential_reference}: {publication}; cleanup: {cleanup}"
    )]
    CredentialReconciliation {
        /// Exact opaque non-secret credential reference requiring reconciliation.
        credential_reference: String,
        /// Redaction-safe publication failure.
        publication: String,
        /// Exact credential-store cleanup failure.
        cleanup: peritus_secrets::SecretError,
    },
    /// Credential-store publication, lookup, or removal failed safely.
    #[error("operating-system credential store failed: {0}")]
    Secret(#[from] peritus_secrets::SecretError),
    /// Non-secret direct-provider settings are invalid.
    #[error("direct provider settings are invalid: {0}")]
    ProductState(#[from] peritus_product_state::ProductStateError),
}
