//! Exact original-receipt resolution; this never applies a current-state transition.

use super::{
    CONTROL_SCHEMA, ControlError, ControlOperation, ControlReceipt, OperationId, decode, encode,
    sha256,
};

impl ControlReceipt {
    /// Returns the operation identity.
    #[must_use]
    pub const fn operation(&self) -> OperationId {
        self.operation
    }
    /// Returns the originally accepted successor revision, including on later replay.
    #[must_use]
    pub const fn accepted_revision(&self) -> u64 {
        self.accepted_revision
    }
    /// Returns the exact actor-bound operation digest accepted by the journal.
    #[must_use]
    pub const fn payload_digest(&self) -> peritus_types::Sha256Digest {
        peritus_types::Sha256Digest::new(self.payload_digest)
    }
    /// Serializes a receipt for publication beside its aggregate transition.
    ///
    /// # Errors
    /// Rejects encoding/bound failures.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ControlError> {
        encode(self)
    }
    /// Resolves a stored receipt against the entire exact request, including actor and revision.
    ///
    /// # Errors
    /// Rejects malformed/unsupported receipts and same-key different-payload replay.
    pub fn resolve(bytes: &[u8], operation: &ControlOperation) -> Result<Self, ControlError> {
        let value: Self = decode(bytes)?;
        if value.schema != CONTROL_SCHEMA {
            return Err(ControlError::UnsupportedSchema);
        }
        if value.operation != operation.id()
            || value.conversation != operation.conversation()
            || value.payload_digest != sha256(&operation.canonical_bytes()?).into_bytes()
        {
            return Err(ControlError::IdempotencyConflict);
        }
        if operation.expected_revision.checked_add(1) != Some(value.accepted_revision) {
            return Err(ControlError::InvalidInput);
        }
        Ok(value)
    }
}
