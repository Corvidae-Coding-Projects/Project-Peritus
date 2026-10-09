//! Exact expiring one-owner secret delivery leases.

use peritus_sandbox::{SecretDelivery, SecretReference};
use peritus_types::{EnvironmentId, ProcessId, Sha256Digest};

use crate::{RecoveryClass, SecretError, SecretErrorKind, SecretOperation};

/// Caller-generated opaque lease identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SecretLeaseId([u8; 16]);

impl SecretLeaseId {
    /// Stores exact lease identity bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
    /// Returns exact bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Closed lease lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SecretLeaseState {
    /// Uses remain before the half-open expiry.
    Active,
    /// Every allowed use was consumed.
    Exhausted,
    /// Explicitly revoked.
    Revoked,
    /// Expiry was observed.
    Expired,
}

/// Exact secret reference, destination, owner, environment, and plan binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretLease {
    id: SecretLeaseId,
    owner: ProcessId,
    environment: EnvironmentId,
    sandbox_digest: Sha256Digest,
    execution_digest: Sha256Digest,
    reference: SecretReference,
    delivery: SecretDelivery,
    remaining_uses: u32,
    expires_epoch_millis: u64,
    state: SecretLeaseState,
}

impl SecretLease {
    /// Creates one active exact lease.
    ///
    /// # Errors
    /// Rejects zero use or expiry bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SecretLeaseId,
        owner: ProcessId,
        environment: EnvironmentId,
        sandbox_digest: Sha256Digest,
        execution_digest: Sha256Digest,
        reference: SecretReference,
        delivery: SecretDelivery,
        uses: u32,
        expires_epoch_millis: u64,
    ) -> Result<Self, SecretError> {
        if uses == 0 || expires_epoch_millis == 0 {
            return Err(crate::error::invalid("secret lease use or expiry bound is zero"));
        }
        Ok(Self {
            id,
            owner,
            environment,
            sandbox_digest,
            execution_digest,
            reference,
            delivery,
            remaining_uses: uses,
            expires_epoch_millis,
            state: SecretLeaseState::Active,
        })
    }

    /// Consumes one exact matching delivery use.
    ///
    /// # Errors
    /// Rejects drift, expiry, exhaustion, or revocation without granting delivery.
    #[allow(clippy::too_many_arguments)]
    pub fn consume(
        &mut self,
        owner: ProcessId,
        environment: EnvironmentId,
        sandbox_digest: Sha256Digest,
        execution_digest: Sha256Digest,
        reference: SecretReference,
        delivery: &SecretDelivery,
        now_epoch_millis: u64,
    ) -> Result<(), SecretError> {
        if self.state != SecretLeaseState::Active {
            return Err(revoked("secret lease is not active"));
        }
        if now_epoch_millis >= self.expires_epoch_millis {
            self.state = SecretLeaseState::Expired;
            return Err(revoked("secret lease expired"));
        }
        let exact = self.owner == owner
            && self.environment == environment
            && self.sandbox_digest == sandbox_digest
            && self.execution_digest == execution_digest
            && self.reference == reference
            && &self.delivery == delivery;
        if !crate::verified::secret_delivery_exact(
            self.reference == reference,
            &self.delivery == delivery,
            exact && self.remaining_uses > 0,
        ) || !exact
            || self.remaining_uses == 0
        {
            return Err(revoked("secret lease binding differs from requested delivery"));
        }
        self.remaining_uses -= 1;
        if self.remaining_uses == 0 {
            self.state = SecretLeaseState::Exhausted;
        }
        Ok(())
    }

    /// Revokes future use idempotently.
    pub fn revoke(&mut self) {
        if self.state == SecretLeaseState::Active {
            self.state = SecretLeaseState::Revoked;
        }
    }
    /// Returns lease identity.
    #[must_use]
    pub const fn id(&self) -> SecretLeaseId {
        self.id
    }
    /// Returns owner.
    #[must_use]
    pub const fn owner(&self) -> ProcessId {
        self.owner
    }
    /// Returns environment.
    #[must_use]
    pub const fn environment(&self) -> EnvironmentId {
        self.environment
    }
    /// Returns sandbox plan digest.
    #[must_use]
    pub const fn sandbox_digest(&self) -> Sha256Digest {
        self.sandbox_digest
    }
    /// Returns execution plan digest.
    #[must_use]
    pub const fn execution_digest(&self) -> Sha256Digest {
        self.execution_digest
    }
    /// Returns secret reference.
    #[must_use]
    pub const fn reference(&self) -> SecretReference {
        self.reference
    }
    /// Returns exact delivery.
    #[must_use]
    pub const fn delivery(&self) -> &SecretDelivery {
        &self.delivery
    }
    /// Returns remaining uses.
    #[must_use]
    pub const fn remaining_uses(&self) -> u32 {
        self.remaining_uses
    }
    /// Returns state.
    #[must_use]
    pub const fn state(&self) -> SecretLeaseState {
        self.state
    }
}

const fn revoked(detail: &'static str) -> SecretError {
    SecretError::new(
        SecretErrorKind::Revoked,
        SecretOperation::Lease,
        RecoveryClass::Reacquire,
        detail,
    )
}
