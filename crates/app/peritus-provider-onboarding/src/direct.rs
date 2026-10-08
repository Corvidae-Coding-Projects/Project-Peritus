//! Direct provider profiles backed by the operating-system credential store.

use core::fmt;

use peritus_product_state::{
    CompatibleProtocol, DirectProviderProfile, ProviderKind, ProviderModelFacts,
    ProviderRouteIdentity,
};
use peritus_secrets::{
    PlatformCredentialStore, SecretMaterial, format_credential_reference,
    parse_credential_reference,
};
use peritus_types::ResourceId;
use sha2::{Digest as _, Sha256};

use crate::{OnboardingError, ProviderEffectStore};

/// Sensitive provider material that zeroizes its allocation on drop.
pub struct DirectCredential(SecretMaterial);

impl DirectCredential {
    /// Takes ownership of bounded credential bytes.
    ///
    /// # Errors
    ///
    /// Rejects empty or excessively large material without formatting it.
    pub fn new(bytes: Vec<u8>) -> Result<Self, OnboardingError> {
        Ok(Self(SecretMaterial::new(bytes)?))
    }
}

impl fmt::Debug for DirectCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DirectCredential([REDACTED])")
    }
}

/// Non-secret direct provider choices collected before credential publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectProviderDraft {
    kind: ProviderKind,
    endpoint: Option<String>,
    catalog_endpoint: Option<String>,
    model: String,
    model_facts: Option<ProviderModelFacts>,
    compatible_protocol: Option<CompatibleProtocol>,
    credential_header: Option<String>,
}

/// Credential publication owned between durable profile preparation and OS-store publication.
///
/// The profile may be committed to a new immutable product generation before this value is
/// consumed. Its durable effect receipt then distinguishes an uncommitted preparation from a
/// committed publication attempt.
pub struct PreparedDirectProvider {
    profile: DirectProviderProfile,
}

impl fmt::Debug for PreparedDirectProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedDirectProvider")
            .field("profile", &self.profile)
            .finish()
    }
}

impl PreparedDirectProvider {
    /// Borrows the durable non-secret profile to commit before credential publication.
    #[must_use]
    pub const fn profile(&self) -> &DirectProviderProfile {
        &self.profile
    }

    /// Publishes the credential after its exact profile has been durably committed.
    ///
    /// # Errors
    ///
    /// Returns an effect-journal, credential-store, or publication-identity failure. The durable
    /// committed receipt remains available when publication cannot be acknowledged.
    pub fn publish(
        self,
        effects: &ProviderEffectStore,
    ) -> Result<DirectProviderProfile, OnboardingError> {
        let Self { profile } = self;
        effects.publish_credential(profile.credential_reference())?;
        Ok(profile)
    }
}

impl DirectProviderDraft {
    /// Binds an exact compatible model-catalog endpoint for discovery and durable configuration.
    #[must_use]
    pub fn with_catalog_endpoint(mut self, endpoint: String) -> Self {
        self.catalog_endpoint = Some(endpoint);
        self
    }

    /// Selects an exact discovered or explicitly entered model before credential publication.
    #[must_use]
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// Binds the versioned capacity and feature facts for the exact selected model.
    #[must_use]
    pub fn with_model_facts(mut self, facts: ProviderModelFacts) -> Self {
        self.model_facts = Some(facts);
        self
    }

    /// Binds a discovered or explicitly selected protocol for a mixed-protocol hosted service.
    #[must_use]
    pub const fn with_protocol(mut self, protocol: CompatibleProtocol) -> Self {
        self.compatible_protocol = Some(protocol);
        self
    }

    /// Queries provider model metadata using the captured credential, without storing it or
    /// submitting an inference request.
    ///
    /// # Errors
    /// Returns a bounded metadata/authentication failure; never returns a fallback list.
    pub async fn discover_models(
        &self,
        credential: &DirectCredential,
        cancellation: &peritus_provider_core::CancellationToken,
    ) -> Result<Vec<peritus_provider_core::catalog::DiscoveredModel>, OnboardingError> {
        crate::models::direct(
            self.kind,
            self.endpoint.as_deref(),
            self.catalog_endpoint.as_deref(),
            self.credential_header.as_deref(),
            &credential.0,
            cancellation,
        )
        .await
    }
    /// Creates one direct provider draft.
    #[must_use]
    pub const fn new(
        kind: ProviderKind,
        endpoint: Option<String>,
        model: String,
        compatible_protocol: Option<CompatibleProtocol>,
        credential_header: Option<String>,
    ) -> Self {
        Self {
            kind,
            endpoint,
            catalog_endpoint: None,
            model,
            model_facts: None,
            compatible_protocol,
            credential_header,
        }
    }

    /// Prepares a durable profile and stages credential material under an effect-owned reference.

    /// The staged resource is not the route credential. It exists only so the exact route
    /// publication can be recovered after the product generation commits.
    ///
    /// # Errors
    ///
    /// Returns a random-source, effect-journal, or direct-profile validation failure.
    pub fn prepare(
        self,
        credential: DirectCredential,
        effects: &ProviderEffectStore,
    ) -> Result<PreparedDirectProvider, OnboardingError> {
        let model_facts = self.model_facts.clone().ok_or_else(|| {
            peritus_product_state::ProductStateError::InvalidPayload(
                "selected direct model capacity and feature facts are unresolved".to_owned(),
            )
        })?;
        let catalog_endpoint = match (self.kind, self.catalog_endpoint.as_deref()) {
            (ProviderKind::CompatibleEndpoint, configured) => {
                let inference = self.endpoint.as_deref().ok_or(OnboardingError::ModelCatalog)?;
                crate::models::compatible_catalog_endpoint(inference, configured)?
                    .map(|endpoint| endpoint.as_str().to_owned())
            }
            (_, None) => None,
            (_, Some(_)) => {
                return Err(OnboardingError::ModelDiscovery(
                    peritus_provider_core::catalog::unavailable(
                        "catalog endpoint is only valid for a compatible provider",
                    ),
                ));
            }
        };
        let resource_id = random_resource_id()?;
        let staged_resource_id = loop {
            let candidate = random_resource_id()?;
            if candidate != resource_id {
                break candidate;
            }
        };
        let route_identity = ProviderRouteIdentity::new(*resource_id.as_bytes())?;
        let (expected_reference, staged_reference) = credential.0.expose(|bytes| {
            let digest = Sha256::digest(bytes);
            (
                credential_reference(resource_id, &digest),
                credential_reference(staged_resource_id, &digest),
            )
        });
        let profile = DirectProviderProfile::new_with_route_identity_and_catalog_endpoint(
            self.kind,
            route_identity,
            expected_reference.clone(),
            self.endpoint,
            catalog_endpoint,
            self.model,
            self.compatible_protocol,
            self.credential_header,
        )?
        .with_model_facts(model_facts)?;
        effects.begin_credential(&expected_reference, &staged_reference)?;
        let staged = PlatformCredentialStore::providers().store(staged_resource_id, &credential.0)?;
        if format_credential_reference(staged) != staged_reference {
            return Err(OnboardingError::CredentialPublication {
                credential_reference: expected_reference,
                detail: "credential store returned a different staged content identity",
            });
        }
        effects.credential_staged(profile.credential_reference())?;
        Ok(PreparedDirectProvider { profile })
    }
}

/// Removes credential material belonging to one durable direct provider profile.
///
/// # Errors
///
/// Returns a malformed-reference or credential-store removal failure.
pub fn remove_direct_credential(profile: &DirectProviderProfile) -> Result<(), OnboardingError> {
    let reference = parse_credential_reference(profile.credential_reference())?;
    PlatformCredentialStore::providers().remove(reference.resource_id())?;
    Ok(())
}

fn random_resource_id() -> Result<ResourceId, OnboardingError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| OnboardingError::Random(error.to_string()))?;
    if bytes.iter().all(|byte| *byte == 0) {
        bytes[0] = 1;
    }
    ResourceId::new(bytes).map_err(|_| OnboardingError::Random("generated a zero identity".into()))
}

fn credential_reference(resource_id: ResourceId, digest: &[u8]) -> String {
    format!(
        "peritus-secret-v1:{}:{}",
        hex(resource_id.as_bytes()),
        hex(digest),
    )
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_debug_is_redacted() {
        let credential = DirectCredential::new(b"private-provider-key".to_vec()).expect("secret");
        let debug = format!("{credential:?}");
        assert!(!debug.contains("private-provider-key"));
        assert_eq!(debug, "DirectCredential([REDACTED])");
    }
}
