//! Authorized-input preparation of exact secret delivery sessions.

use core::fmt;
use std::{path::PathBuf, sync::Arc};

use peritus_sandbox::SecretRequirement;
use peritus_types::{EnvironmentId, ProcessId, Sha256Digest};

use crate::{
    CredentialStore, RecoveryClass, SecretDeliveryContext, SecretDeliverySession, SecretError,
    SecretErrorKind, SecretLease, SecretOperation,
};

const MAX_PREPARED_SECRETS: usize = 128;

/// Inert exact leases and store access consumed only during authorized native preparation.
///
/// This value does not read a store or materialize a destination when constructed. A platform
/// backend moves it into its opaque post-consumption callback and calls [`Self::prepare`] with the
/// current checked bindings. The returned session owns material, artifacts, leases, and cleanup.
pub struct SecretPreparation {
    store: Arc<dyn CredentialStore>,
    leases: Vec<SecretLease>,
    now_epoch_millis: u64,
    staging_root: PathBuf,
}

impl SecretPreparation {
    /// Creates an inert preparation request.
    ///
    /// # Errors
    ///
    /// Rejects excessive leases or a relative private-file staging root.
    pub fn new(
        store: Arc<dyn CredentialStore>,
        leases: Vec<SecretLease>,
        now_epoch_millis: u64,
        staging_root: PathBuf,
    ) -> Result<Self, SecretError> {
        if leases.len() > MAX_PREPARED_SECRETS || !staging_root.is_absolute() {
            return Err(preparation_error(
                SecretErrorKind::InvalidInput,
                RecoveryClass::CorrectRequest,
                "secret preparation lease count or staging root is invalid",
            ));
        }
        Ok(Self { store, leases, now_epoch_millis, staging_root })
    }

    /// Returns the number of inert exact leases.
    #[must_use]
    pub const fn lease_count(&self) -> usize {
        self.leases.len()
    }

    /// Checks exact lease bindings without consuming uses or accessing credential material.
    ///
    /// # Errors
    /// Rejects missing, duplicated, expired, exhausted, or mismatched leases.
    pub fn preflight(
        &self,
        owner: ProcessId,
        environment: EnvironmentId,
        sandbox_digest: Sha256Digest,
        execution_digest: Sha256Digest,
        requirements: &[SecretRequirement],
    ) -> Result<(), SecretError> {
        let mut matched = std::collections::BTreeSet::new();
        for requirement in requirements {
            let position = self
                .leases
                .iter()
                .position(|lease| {
                    lease.owner() == owner
                        && lease.environment() == environment
                        && lease.sandbox_digest() == sandbox_digest
                        && lease.execution_digest() == execution_digest
                        && lease.reference() == requirement.reference()
                        && lease.delivery() == requirement.delivery()
                        && lease.state() == crate::SecretLeaseState::Active
                        && lease.remaining_uses() > 0
                        && self.now_epoch_millis < lease.expires_epoch_millis()
                })
                .ok_or_else(|| {
                    preparation_error(
                        SecretErrorKind::Revoked,
                        RecoveryClass::Reacquire,
                        "no exact live lease matches a secret requirement",
                    )
                })?;
            if !matched.insert(position) {
                return Err(preparation_error(
                    SecretErrorKind::Revoked,
                    RecoveryClass::Reacquire,
                    "secret requirement duplicates one exact lease",
                ));
            }
        }
        if matched.len() != self.leases.len() {
            return Err(preparation_error(
                SecretErrorKind::Revoked,
                RecoveryClass::Reacquire,
                "secret requirements and supplied leases differ",
            ));
        }
        Ok(())
    }

    /// Resolves and stages exactly the checked requirements under current execution bindings.
    ///
    /// Store lookup and delivery begin only when the platform backend invokes this method from its
    /// authorized preparation callback. Missing, duplicate, surplus, or drifted leases fail closed;
    /// any partially prepared session is released before the failure is returned.
    ///
    /// # Errors
    ///
    /// Returns a typed store, lease, delivery, or cleanup failure.
    pub fn prepare(
        self,
        owner: ProcessId,
        environment: EnvironmentId,
        sandbox_digest: Sha256Digest,
        execution_digest: Sha256Digest,
        requirements: &[SecretRequirement],
    ) -> Result<SecretDeliverySession, SecretError> {
        let mut session = SecretDeliverySession::new();
        match self.prepare_into(
            owner,
            environment,
            sandbox_digest,
            execution_digest,
            requirements,
            &mut session,
        ) {
            Ok(()) => Ok(session),
            Err(error) => release_after_failure(session, error),
        }
    }

    /// Stages into a caller-owned session, retaining partial artifacts on every failure.
    ///
    /// # Errors
    /// Returns the original typed cause; the caller retains cleanup ownership.
    #[allow(clippy::too_many_arguments, reason = "exact authority bindings and retained owner")]
    pub fn prepare_into(
        mut self,
        owner: ProcessId,
        environment: EnvironmentId,
        sandbox_digest: Sha256Digest,
        execution_digest: Sha256Digest,
        requirements: &[SecretRequirement],
        session: &mut SecretDeliverySession,
    ) -> Result<(), SecretError> {
        self.preflight(owner, environment, sandbox_digest, execution_digest, requirements)?;
        if requirements.len() != self.leases.len() {
            return Err(preparation_error(
                SecretErrorKind::Revoked,
                RecoveryClass::Reacquire,
                "secret requirements and supplied leases differ",
            ));
        }
        let context = SecretDeliveryContext::new(
            owner,
            environment,
            sandbox_digest,
            execution_digest,
            self.now_epoch_millis,
        );
        for requirement in requirements {
            let position = self.leases.iter().position(|lease| {
                lease.owner() == owner
                    && lease.environment() == environment
                    && lease.sandbox_digest() == sandbox_digest
                    && lease.execution_digest() == execution_digest
                    && lease.reference() == requirement.reference()
                    && lease.delivery() == requirement.delivery()
            });
            let Some(position) = position else {
                return Err(preparation_error(
                    SecretErrorKind::Revoked,
                    RecoveryClass::Reacquire,
                    "no exact live lease matches a secret requirement",
                ));
            };
            let lease = self.leases.remove(position);
            let material = self.store.lookup(requirement.reference())?;
            session.deliver(lease, material, context, &self.staging_root)?;
        }
        if !self.leases.is_empty() {
            return Err(preparation_error(
                SecretErrorKind::Revoked,
                RecoveryClass::Reacquire,
                "secret preparation retained a surplus lease",
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for SecretPreparation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretPreparation")
            .field("store", &"[OPAQUE]")
            .field("leases", &self.leases)
            .field("now_epoch_millis", &self.now_epoch_millis)
            .field("staging_root", &self.staging_root)
            .finish()
    }
}

fn release_after_failure(
    mut session: SecretDeliverySession,
    original: SecretError,
) -> Result<SecretDeliverySession, SecretError> {
    match session.release() {
        Ok(()) => Err(original),
        Err(cleanup) => Err(cleanup),
    }
}

const fn preparation_error(
    kind: SecretErrorKind,
    recovery: RecoveryClass,
    detail: &'static str,
) -> SecretError {
    SecretError::new(kind, SecretOperation::Deliver, recovery, detail)
}
