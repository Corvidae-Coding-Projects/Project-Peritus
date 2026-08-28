//! C3-backed lazy credential resolution for direct C5 adapters.

use peritus_provider_core::{Credential, CredentialReference, CredentialSource, ProviderCoreError};
use peritus_secrets::{CredentialStore, PlatformCredentialStore, parse_credential_reference};

/// Platform credential-store adapter understood by the daemon's opaque C5 references.
#[derive(Debug)]
pub struct PlatformCredentialSource {
    store: PlatformCredentialStore,
}

impl PlatformCredentialSource {
    /// Opens the platform credential namespace without reading any secret material.
    ///
    /// # Errors
    ///
    /// Rejects an invalid service namespace.
    pub fn new(service: &str) -> Result<Self, ProviderCoreError> {
        let store = PlatformCredentialStore::new(service.to_owned())
            .map_err(|_| credential_error("platform credential namespace is invalid"))?;
        Ok(Self { store })
    }

    /// Reports whether the current platform credential adapter is available.
    #[must_use]
    pub fn available(&self) -> bool {
        self.store.probe().available()
    }
}

impl CredentialSource for PlatformCredentialSource {
    fn resolve(&self, reference: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        let reference = parse_credential_reference(reference.as_str())
            .map_err(|_| credential_error("credential reference is malformed"))?;
        let material = self
            .store
            .lookup(reference)
            .map_err(|_| credential_error("platform credential lookup failed"))?;
        material.expose(|bytes| Credential::new(bytes.to_vec()))
    }
}

const fn credential_error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::credential(detail)
}
