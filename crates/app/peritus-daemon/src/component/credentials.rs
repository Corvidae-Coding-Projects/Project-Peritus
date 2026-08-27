//! C3-backed lazy credential resolution for direct C5 adapters.

use peritus_provider_core::{Credential, CredentialReference, CredentialSource, ProviderCoreError};
use peritus_sandbox::SecretReference;
use peritus_secrets::{CredentialStore, PlatformCredentialStore};
use peritus_types::{ResourceId, Sha256Digest};

const PREFIX: &str = "peritus-secret-v1:";

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
        let reference = parse_reference(reference.as_str())?;
        let material = self
            .store
            .lookup(reference)
            .map_err(|_| credential_error("platform credential lookup failed"))?;
        material.expose(|bytes| Credential::new(bytes.to_vec()))
    }
}

fn parse_reference(value: &str) -> Result<SecretReference, ProviderCoreError> {
    let body = value
        .strip_prefix(PREFIX)
        .ok_or_else(|| credential_error("credential reference has an unsupported scheme"))?;
    let (resource, version) = body
        .split_once(':')
        .ok_or_else(|| credential_error("credential reference is malformed"))?;
    if version.contains(':') {
        return Err(credential_error("credential reference is malformed"));
    }
    let resource = ResourceId::new(decode::<16>(resource)?)
        .map_err(|_| credential_error("credential resource identity is zero"))?;
    let version = Sha256Digest::new(decode::<32>(version)?);
    Ok(SecretReference::new(resource, version))
}

fn decode<const N: usize>(value: &str) -> Result<[u8; N], ProviderCoreError> {
    if value.len() != N * 2 {
        return Err(credential_error("credential reference hex length is invalid"));
    }
    let mut bytes = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high =
            hex(pair[0]).ok_or_else(|| credential_error("credential reference is not hex"))?;
        let low =
            hex(pair[1]).ok_or_else(|| credential_error("credential reference is not hex"))?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

const fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

const fn credential_error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::credential(detail)
}
