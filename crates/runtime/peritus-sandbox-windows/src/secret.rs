//! Protected inherited handles for secret delivery.

use peritus_sandbox::{
    BrokeredHandleLabel, EnvironmentName, SandboxPath, SecretDelivery, SecretReference,
};
use peritus_types::Sha256Digest;

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

/// Nonsensitive destination bound to one protected inherited handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretHandleDestination {
    /// Helper reads UTF-8 bytes and installs the exact environment name.
    Environment(EnvironmentName),
    /// Helper writes bytes to the exact private file destination.
    File(SandboxPath),
    /// Target inherits the handle under an opaque checked label.
    Brokered(BrokeredHandleLabel),
}

impl From<&SecretDelivery> for SecretHandleDestination {
    fn from(value: &SecretDelivery) -> Self {
        match value {
            SecretDelivery::Environment(name) => Self::Environment(name.clone()),
            SecretDelivery::File(path) => Self::File(path.clone()),
            SecretDelivery::BrokeredHandle(label) => Self::Brokered(label.clone()),
        }
    }
}

/// One already-created protected handle and exact lease/reference binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedSecretHandle {
    handle: u64,
    reference_digest: Sha256Digest,
    destination: SecretHandleDestination,
    payload_len: Option<u64>,
    payload_digest: Option<Sha256Digest>,
}

impl ProtectedSecretHandle {
    /// Creates a protected handle descriptor without secret material.
    ///
    /// # Errors
    /// Rejects a null handle or zero reference digest.
    pub fn new(
        handle: u64,
        reference_digest: Sha256Digest,
        destination: SecretHandleDestination,
    ) -> Result<Self, WindowsError> {
        if handle == 0 || reference_digest == Sha256Digest::new([0; 32]) {
            return Err(secret_error("secret handle identity is incomplete"));
        }
        Ok(Self {
            handle,
            reference_digest,
            destination,
            payload_len: None,
            payload_digest: None,
        })
    }

    /// Creates a protected handle descriptor with its exact finite payload length.
    ///
    /// # Errors
    /// Rejects a null handle, zero reference digest, or empty payload.
    pub fn new_bound(
        handle: u64,
        reference_digest: Sha256Digest,
        destination: SecretHandleDestination,
        payload_len: u64,
    ) -> Result<Self, WindowsError> {
        let mut value = Self::new(handle, reference_digest, destination)?;
        if payload_len == 0 {
            return Err(secret_error("secret payload length is empty"));
        }
        value.payload_len = Some(payload_len);
        Ok(value)
    }

    /// Creates a protected handle descriptor with exact finite payload length and digest.
    pub fn new_digest_bound(
        handle: u64,
        reference_digest: Sha256Digest,
        destination: SecretHandleDestination,
        payload_len: u64,
        payload_digest: Sha256Digest,
    ) -> Result<Self, WindowsError> {
        let mut value = Self::new_bound(handle, reference_digest, destination, payload_len)?;
        value.payload_digest = Some(payload_digest);
        Ok(value)
    }

    /// Returns the protected native handle value.
    #[must_use]
    pub const fn handle(&self) -> u64 {
        self.handle
    }

    /// Returns the exact nonsensitive secret-reference digest.
    #[must_use]
    pub const fn reference_digest(&self) -> Sha256Digest {
        self.reference_digest
    }

    /// Returns the declared destination.
    #[must_use]
    pub const fn destination(&self) -> &SecretHandleDestination {
        &self.destination
    }

    /// Returns the exact finite payload length when the manifest schema carries it.
    #[must_use]
    pub const fn payload_len(&self) -> Option<u64> {
        self.payload_len
    }

    /// Returns the exact finite payload digest when carried by the manifest schema.
    #[must_use]
    pub const fn payload_digest(&self) -> Option<Sha256Digest> {
        self.payload_digest
    }
}

/// Canonicalizes protected handles and rejects destination/handle collisions.
///
/// # Errors
/// Returns a typed error for duplicates or excessive handles.
pub fn canonical_handles(
    mut handles: Vec<ProtectedSecretHandle>,
) -> Result<Vec<ProtectedSecretHandle>, WindowsError> {
    handles.sort_by_key(ProtectedSecretHandle::handle);
    if handles.windows(2).any(|pair| pair[0].handle == pair[1].handle) {
        return Err(secret_error("secret handles contain a duplicate native handle"));
    }
    for (index, handle) in handles.iter().enumerate() {
        if handles[..index].iter().any(|prior| prior.destination == handle.destination) {
            return Err(secret_error("secret handles contain a duplicate destination"));
        }
    }
    Ok(handles)
}

/// Returns the nonsensitive canonical identity expected in a protected handle descriptor.
#[must_use]
pub fn secret_reference_digest(reference: SecretReference) -> Sha256Digest {
    let mut bytes = Vec::from(b"PERITUS-WINDOWS-SECRET-REFERENCE-V1\0".as_slice());
    bytes.extend_from_slice(reference.resource_id().as_bytes());
    bytes.extend_from_slice(reference.version().as_bytes());
    peritus_codec::sha256(&bytes)
}

fn secret_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Secret,
        WindowsOperation::Validate,
        WindowsRecovery::Reauthorize,
        detail,
    )
}
