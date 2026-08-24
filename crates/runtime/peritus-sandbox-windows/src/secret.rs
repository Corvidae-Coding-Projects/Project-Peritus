//! Protected inherited handles for secret delivery.

use peritus_sandbox::{
    BrokeredHandleLabel, EnvironmentName, SandboxPath, SecretDelivery, SecretReference,
};
use peritus_types::Sha256Digest;

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

const MAX_SECRET_HANDLES: usize = 64;

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
        Ok(Self { handle, reference_digest, destination })
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
}

/// Canonicalizes protected handles and rejects destination/handle collisions.
///
/// # Errors
/// Returns a typed error for duplicates or excessive handles.
pub fn canonical_handles(
    mut handles: Vec<ProtectedSecretHandle>,
) -> Result<Vec<ProtectedSecretHandle>, WindowsError> {
    if handles.len() > MAX_SECRET_HANDLES {
        return Err(secret_error("secret handle count exceeds its bound"));
    }
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
