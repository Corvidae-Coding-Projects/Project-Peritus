//! Nonsensitive versioned secret-delivery recovery records.

use peritus_sandbox::SecretReference;
use peritus_types::{ProcessId, Sha256Digest};

use crate::{RecoveryClass, SecretError, SecretErrorKind, SecretLeaseId, SecretOperation};

/// Reopened secret-resource state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SecretRecoveryState {
    /// Exact lease/delivery resources remain live and owned.
    LiveOwned,
    /// No material or delivery resource remains.
    AbsentClean,
    /// An observed resource belongs to another identity.
    Mismatched,
    /// Exact state cannot be established.
    Indeterminate,
}

/// Version-one recovery identity containing references but never values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretRecoveryRecord {
    owner: ProcessId,
    lease_id: SecretLeaseId,
    reference: SecretReference,
    execution_digest: Sha256Digest,
    delivery_live: bool,
    released: bool,
    checksum: Sha256Digest,
}

impl SecretRecoveryRecord {
    /// Creates and checksums one nonsensitive recovery record.
    #[must_use]
    pub fn new(
        owner: ProcessId,
        lease_id: SecretLeaseId,
        reference: SecretReference,
        execution_digest: Sha256Digest,
        delivery_live: bool,
        released: bool,
    ) -> Self {
        let checksum =
            checksum(owner, lease_id, reference, execution_digest, delivery_live, released);
        Self { owner, lease_id, reference, execution_digest, delivery_live, released, checksum }
    }

    /// Validates and classifies exact observed identity and material absence.
    ///
    /// # Errors
    /// Rejects corrupt records.
    pub fn classify(
        &self,
        observed_owner: ProcessId,
        observed_lease: Option<SecretLeaseId>,
        resource_present: Option<bool>,
    ) -> Result<SecretRecoveryState, SecretError> {
        if self.checksum
            != checksum(
                self.owner,
                self.lease_id,
                self.reference,
                self.execution_digest,
                self.delivery_live,
                self.released,
            )
        {
            return Err(recovery_error("secret recovery checksum differs"));
        }
        if observed_owner != self.owner
            || observed_lease.is_some_and(|lease| lease != self.lease_id)
        {
            return Ok(SecretRecoveryState::Mismatched);
        }
        Ok(match resource_present {
            Some(false) if self.released => SecretRecoveryState::AbsentClean,
            Some(true) if self.delivery_live && !self.released => SecretRecoveryState::LiveOwned,
            Some(_) => SecretRecoveryState::Mismatched,
            None => SecretRecoveryState::Indeterminate,
        })
    }
}

fn checksum(
    owner: ProcessId,
    lease_id: SecretLeaseId,
    reference: SecretReference,
    execution: Sha256Digest,
    live: bool,
    released: bool,
) -> Sha256Digest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PERITUS_SECRET_RECOVERY\0\x01");
    bytes.extend_from_slice(owner.as_bytes());
    bytes.extend_from_slice(lease_id.as_bytes());
    bytes.extend_from_slice(reference.resource_id().as_bytes());
    bytes.extend_from_slice(reference.version().as_bytes());
    bytes.extend_from_slice(execution.as_bytes());
    bytes.extend_from_slice(&[u8::from(live), u8::from(released)]);
    peritus_codec::sha256(&bytes)
}

const fn recovery_error(detail: &'static str) -> SecretError {
    SecretError::new(
        SecretErrorKind::Recovery,
        SecretOperation::Recover,
        RecoveryClass::Reconcile,
        detail,
    )
}
