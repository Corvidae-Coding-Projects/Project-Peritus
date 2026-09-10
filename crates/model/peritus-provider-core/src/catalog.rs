//! Provider-owned model discovery; no bundled model names or implicit fallback catalog.

mod http;
mod parse;
mod runtime;

use peritus_model_protocol::{CapabilityProvenance, ModelName, ProviderProfile};
use peritus_types::ProviderProfileId;
use sha2::{Digest, Sha256};

use crate::{ProviderCoreError, ProviderCoreErrorKind};

pub use http::{CatalogDialect, discover_http_models};
use parse::parse_runtime_models;
pub use runtime::{AccountCatalog, discover_account_models};

/// Maximum models in one complete discovery result.
pub const MAX_CATALOG_MODELS: usize = 4096;

/// Provider-advertised model metadata; `None` means unknown, not unsupported or verified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredModel {
    /// Exact provider model identifier.
    pub id: ModelName,
    /// Human-facing provider label.
    pub label: String,
    /// Advertised wire protocol, when supplied by a reviewed metadata source.
    pub dialect: Option<peritus_model_protocol::WireDialect>,
    /// Advertised tool-calling support, when the catalog actually supplies it.
    pub tools: Option<bool>,
    /// Advertised input-token ceiling.
    pub input_tokens: Option<u64>,
    /// Advertised output-token ceiling.
    pub output_tokens: Option<u64>,
}

impl DiscoveredModel {
    /// Validates a provider identifier and display label without interpreting them as commands.
    ///
    /// # Errors
    /// Rejects malformed identifiers or empty, oversized, control-bearing labels.
    pub fn new(id: String, label: String) -> Result<Self, ProviderCoreError> {
        let id =
            ModelName::new(id).map_err(|_| unavailable("catalog model identifier is invalid"))?;
        if label.is_empty() || label.len() > 512 || label.chars().any(char::is_control) {
            return Err(unavailable("catalog model label is invalid"));
        }
        Ok(Self { id, label, dialect: None, tools: None, input_tokens: None, output_tokens: None })
    }
}

/// Derives a new immutable profile identity for a selected model, preserving the explicit route.
///
/// Previous model-specific probe/discovery provenance is not transferred to another model.
/// Listing alone is not a capability probe; capabilities remain the configured route contract.
///
/// # Errors
/// Returns a checked profile error instead of mutating the original adapter.
pub fn selected_profile(
    profile: &ProviderProfile,
    model: ModelName,
) -> Result<ProviderProfile, ProviderCoreError> {
    if profile.model() == &model {
        return Ok(profile.clone());
    }
    let mut hash = Sha256::new();
    hash.update(b"peritus-selected-model-v1\0");
    hash.update(profile.profile_id().as_bytes());
    hash.update(profile.revision().to_le_bytes());
    hash.update(model.as_str().as_bytes());
    let digest = hash.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    let id = ProviderProfileId::new(bytes)
        .map_err(|_| unavailable("selected model identity is invalid"))?;
    ProviderProfile::new(
        id,
        1,
        profile.provider().clone(),
        model,
        profile.dialect(),
        profile.capabilities(),
        CapabilityProvenance::Profiled,
        profile.limits(),
        profile.output_limit_enforcement(),
        profile.state_mode(),
        profile.resume_kind(),
        profile.cancellation_kind(),
    )
    .map_err(|_| unavailable("selected model contradicts the configured transport profile"))
}

/// Creates a safe catalog diagnostic without copying provider bodies, headers, or credentials.
#[must_use]
pub const fn unavailable(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::new(ProviderCoreErrorKind::Configuration, "model_catalog", detail)
}
