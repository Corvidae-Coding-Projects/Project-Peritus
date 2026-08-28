//! Direct provider profiles backed by the operating-system credential store.

use core::fmt;

use peritus_product_state::{CompatibleProtocol, DirectProviderProfile, ProviderKind};
use peritus_secrets::{
    PlatformCredentialStore, SecretMaterial, format_credential_reference,
    parse_credential_reference,
};
use peritus_types::ResourceId;

use crate::OnboardingError;

const SERVICE: &str = "org.corvidae-coding.peritus.providers";

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
    model: String,
    compatible_protocol: Option<CompatibleProtocol>,
    credential_header: Option<String>,
}

impl DirectProviderDraft {
    /// Creates one direct provider draft.
    #[must_use]
    pub const fn new(
        kind: ProviderKind,
        endpoint: Option<String>,
        model: String,
        compatible_protocol: Option<CompatibleProtocol>,
        credential_header: Option<String>,
    ) -> Self {
        Self { kind, endpoint, model, compatible_protocol, credential_header }
    }

    /// Publishes credential material and returns only durable non-secret profile data.
    ///
    /// # Errors
    ///
    /// Returns a random-source, credential-store, or direct-profile validation failure.
    pub fn store(
        self,
        credential: &DirectCredential,
    ) -> Result<DirectProviderProfile, OnboardingError> {
        let resource_id = random_resource_id()?;
        let store = PlatformCredentialStore::new(SERVICE.to_owned())?;
        let reference = store.store(resource_id, &credential.0)?;
        let profile = DirectProviderProfile::new(
            self.kind,
            format_credential_reference(reference),
            self.endpoint,
            self.model,
            self.compatible_protocol,
            self.credential_header,
        );
        match profile {
            Ok(profile) => Ok(profile),
            Err(error) => {
                let _ignored = store.remove(resource_id);
                Err(OnboardingError::from(error))
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
    PlatformCredentialStore::new(SERVICE.to_owned())?.remove(reference.resource_id())?;
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
