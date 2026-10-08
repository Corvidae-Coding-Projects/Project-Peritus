//! Direct provider profiles backed by the operating-system credential store.

use core::fmt;

use peritus_product_state::{
    CompatibleProtocol, DirectProviderProfile, ProviderKind, ProviderRouteIdentity,
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
    compatible_protocol: Option<CompatibleProtocol>,
    credential_header: Option<String>,
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
            compatible_protocol,
            credential_header,
        }
    }

    /// Publishes credential material and returns only durable non-secret profile data.
    ///
    /// # Errors
    ///
    /// Returns a random-source, credential-store, or direct-profile validation failure.
    pub fn store(
        self,
        credential: &DirectCredential,
        effects: &ProviderEffectStore,
    ) -> Result<DirectProviderProfile, OnboardingError> {
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
        let route_identity = ProviderRouteIdentity::new(*resource_id.as_bytes())?;
        let expected_reference = credential.0.expose(|bytes| {
            let digest = Sha256::digest(bytes);
            format!(
                "peritus-secret-v1:{}:{}",
                hex(resource_id.as_bytes()),
                hex(&digest),
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
        )?;
        effects.begin_credential(&expected_reference)?;
        let store = PlatformCredentialStore::providers();
        match store.store(resource_id, &credential.0) {
            Ok(reference) if format_credential_reference(reference) == expected_reference => {
                if let Err(error) = effects.credential_published(&expected_reference) {
                    return Err(reconcile_publication_failure(
                        &store,
                        effects,
                        resource_id,
                        expected_reference,
                        error,
                    ));
                }
                Ok(profile)
            }
            Ok(_) => Err(reconcile_publication_failure(
                &store,
                effects,
                resource_id,
                expected_reference.clone(),
                OnboardingError::CredentialPublication {
                    credential_reference: expected_reference,
                    detail: "credential store returned a different content identity",
                },
            )),
            Err(error) => Err(reconcile_publication_failure(
                &store,
                effects,
                resource_id,
                expected_reference,
                OnboardingError::Secret(error),
            )),
        }
    }
}

fn reconcile_publication_failure(
    store: &PlatformCredentialStore,
    effects: &ProviderEffectStore,
    resource_id: ResourceId,
    credential_reference: String,
    publication: OnboardingError,
) -> OnboardingError {
    match store.remove(resource_id) {
        Ok(()) => effects.credential_settled(&credential_reference).err().unwrap_or(publication),
        Err(cleanup) => {
            let _retained = effects.credential_cleanup_required(&credential_reference);
            OnboardingError::CredentialReconciliation {
                credential_reference,
                publication: publication.to_string(),
                cleanup,
            }
        }
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
