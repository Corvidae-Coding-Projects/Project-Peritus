//! Narrow, zeroizing credential resolution and HTTP projection.

use core::fmt;

use zeroize::Zeroizing;

use crate::{Header, HeaderName, HeaderValue, ProviderCoreError, ProviderCoreErrorKind};

const MAX_CREDENTIAL_REFERENCE_BYTES: usize = 512;
const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;

/// An opaque identifier understood only by a configured [`CredentialSource`].
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CredentialReference(String);

impl CredentialReference {
    /// Creates a checked opaque reference.
    ///
    /// # Errors
    ///
    /// Rejects empty, control-containing, or oversized input.
    pub fn new(value: String) -> Result<Self, ProviderCoreError> {
        if value.is_empty()
            || value.len() > MAX_CREDENTIAL_REFERENCE_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(credential_error(
                "credential reference is empty, contains controls, or exceeds its byte bound",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the opaque value for resolution by the configured source.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CredentialReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialReference([redacted])")
    }
}

/// A short-lived credential buffer that is zeroized on drop and redacted in diagnostics.
pub struct Credential(Zeroizing<Vec<u8>>);

impl Credential {
    /// Takes ownership of credential bytes.
    ///
    /// # Errors
    ///
    /// Rejects empty or oversized credentials.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ProviderCoreError> {
        if bytes.is_empty() || bytes.len() > MAX_CREDENTIAL_BYTES {
            return Err(credential_error("credential is empty or exceeds its byte bound"));
        }
        Ok(Self(Zeroizing::new(bytes)))
    }

    /// Consumes the credential into a sensitive HTTP header, optionally adding a fixed prefix.
    ///
    /// # Errors
    ///
    /// Rejects a prefix or credential that cannot form a safe HTTP header value.
    pub fn into_header(
        self,
        name: HeaderName,
        prefix: Option<&'static str>,
    ) -> Result<Header, ProviderCoreError> {
        let prefix = prefix.unwrap_or_default().as_bytes();
        let capacity = prefix.len().checked_add(self.0.len()).ok_or_else(|| {
            credential_error("credential header length overflowed its byte representation")
        })?;
        if capacity > MAX_CREDENTIAL_BYTES + 128 {
            return Err(credential_error("credential header exceeds its byte bound"));
        }
        let mut bytes = Vec::with_capacity(capacity);
        bytes.extend_from_slice(prefix);
        bytes.extend_from_slice(&self.0);
        Ok(Header::from_value(name, HeaderValue::sensitive(bytes)?))
    }

    /// Returns the credential byte length without exposing its contents.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the credential has no bytes.
    ///
    /// Checked credentials are never empty; this is provided for ordinary collection semantics.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Credential([redacted])")
    }
}

/// Immediate credential resolver used at request-construction time.
pub trait CredentialSource: Send + Sync {
    /// Resolves one opaque reference into a short-lived credential.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe credential acquisition failure.
    fn resolve(&self, reference: &CredentialReference) -> Result<Credential, ProviderCoreError>;
}

const fn credential_error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::new(ProviderCoreErrorKind::InvalidCredential, "credential", detail)
}
