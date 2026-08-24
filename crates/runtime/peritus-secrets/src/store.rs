//! Exact secret-store boundary and platform credential adapter.

#[cfg(any(test, feature = "test-memory-store"))]
mod memory;

use peritus_sandbox::SecretReference;
use zeroize::Zeroize;

use crate::{RecoveryClass, SecretError, SecretErrorKind, SecretMaterial, SecretOperation};

#[cfg(any(test, feature = "test-memory-store"))]
pub use memory::{MemoryCredentialStore, MemoryStoreOutcome};

/// Truthful credential-store capability probe.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StoreProbe {
    available: bool,
    adapter: &'static str,
}

impl StoreProbe {
    /// Creates one probe observation.
    #[must_use]
    pub const fn new(available: bool, adapter: &'static str) -> Self {
        Self { available, adapter }
    }
    /// Returns whether exact lookup can be attempted.
    #[must_use]
    pub const fn available(self) -> bool {
        self.available
    }
    /// Returns stable adapter identity.
    #[must_use]
    pub const fn adapter(self) -> &'static str {
        self.adapter
    }
}

/// Exact material lookup boundary.
pub trait CredentialStore: Send + Sync {
    /// Reports adapter availability without reading material.
    fn probe(&self) -> StoreProbe;
    /// Resolves the exact opaque reference and verifies its version digest.
    ///
    /// # Errors
    /// Returns stable missing/locked/denied/stale/unavailable/corrupt/I/O outcomes.
    fn lookup(&self, reference: SecretReference) -> Result<SecretMaterial, SecretError>;
}

/// Platform OS credential-store adapter backed by pinned `keyring` version one semantics.
#[derive(Clone, Debug)]
pub struct PlatformCredentialStore {
    service: String,
}

impl PlatformCredentialStore {
    /// Creates a portable service namespace.
    ///
    /// # Errors
    /// Rejects empty, oversized, non-ASCII, or control-bearing names.
    pub fn new(service: impl Into<String>) -> Result<Self, SecretError> {
        let service = service.into();
        if service.is_empty()
            || service.len() > 128
            || !service.is_ascii()
            || service.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(crate::error::invalid("credential service name is invalid"));
        }
        Ok(Self { service })
    }
}

impl CredentialStore for PlatformCredentialStore {
    fn probe(&self) -> StoreProbe {
        StoreProbe::new(keyring::Entry::store_status().is_ok(), adapter_name())
    }

    fn lookup(&self, reference: SecretReference) -> Result<SecretMaterial, SecretError> {
        if !self.probe().available() {
            return Err(unavailable("platform credential store is unavailable"));
        }
        let username = hex(reference.resource_id().as_bytes());
        let entry = keyring::Entry::new(&self.service, &username)
            .map_err(|_| unavailable("credential entry cannot be opened"))?;
        let bytes = entry.get_secret().map_err(map_store_error)?;
        if peritus_codec::sha256(&bytes) != reference.version() {
            return Err(SecretError::new(
                SecretErrorKind::StaleVersion,
                SecretOperation::Lookup,
                RecoveryClass::Reacquire,
                "credential entry version differs from the exact reference",
            ));
        }
        SecretMaterial::new(bytes)
    }
}

fn map_store_error(error: keyring::Error) -> SecretError {
    let (kind, recovery, detail) = match error {
        keyring::Error::NoEntry => (
            SecretErrorKind::Missing,
            RecoveryClass::Reacquire,
            "exact credential entry is missing",
        ),
        keyring::Error::NoStorageAccess(_) => (
            SecretErrorKind::Locked,
            RecoveryClass::UnlockStore,
            "platform credential store is locked or inaccessible",
        ),
        keyring::Error::BadEncoding(mut bytes) => {
            bytes.zeroize();
            (
                SecretErrorKind::Corrupt,
                RecoveryClass::Reacquire,
                "credential entry encoding is corrupt",
            )
        }
        keyring::Error::BadDataFormat(mut bytes, _) => {
            bytes.zeroize();
            (SecretErrorKind::Corrupt, RecoveryClass::Reacquire, "credential entry data is corrupt")
        }
        keyring::Error::BadStoreFormat(_) | keyring::Error::Ambiguous(_) => (
            SecretErrorKind::Corrupt,
            RecoveryClass::Reacquire,
            "credential store returned ambiguous or corrupt data",
        ),
        keyring::Error::NoDefaultStore | keyring::Error::NotSupportedByStore(_) => (
            SecretErrorKind::Unavailable,
            RecoveryClass::Retry,
            "platform credential store adapter is unavailable",
        ),
        keyring::Error::TooLong(_, _) | keyring::Error::Invalid(_, _) => (
            SecretErrorKind::InvalidInput,
            RecoveryClass::CorrectRequest,
            "credential lookup attributes are invalid for the platform store",
        ),
        keyring::Error::PlatformFailure(_) => (
            SecretErrorKind::Io,
            RecoveryClass::Retry,
            "platform credential store operation failed",
        ),
        _ => (
            SecretErrorKind::Unavailable,
            RecoveryClass::Retry,
            "platform credential store returned an unsupported failure",
        ),
    };
    SecretError::new(kind, SecretOperation::Lookup, recovery, detail)
}

#[cfg(target_os = "linux")]
const fn adapter_name() -> &'static str {
    "secret-service-zbus"
}
#[cfg(target_os = "macos")]
const fn adapter_name() -> &'static str {
    "apple-keychain"
}
#[cfg(target_os = "windows")]
const fn adapter_name() -> &'static str {
    "windows-credential-manager"
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

const fn unavailable(detail: &'static str) -> SecretError {
    SecretError::new(
        SecretErrorKind::Unavailable,
        SecretOperation::Probe,
        RecoveryClass::Retry,
        detail,
    )
}
