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
        mut self,
        owner: ProcessId,
        environment: EnvironmentId,
        sandbox_digest: Sha256Digest,
        execution_digest: Sha256Digest,
        requirements: &[SecretRequirement],
    ) -> Result<SecretDeliverySession, SecretError> {
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
        let mut session = SecretDeliverySession::new();
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
                return release_after_failure(
                    session,
                    preparation_error(
                        SecretErrorKind::Revoked,
                        RecoveryClass::Reacquire,
                        "no exact live lease matches a secret requirement",
                    ),
                );
            };
            let lease = self.leases.remove(position);
            let material = match self.store.lookup(requirement.reference()) {
                Ok(material) => material,
                Err(error) => return release_after_failure(session, error),
            };
            if let Err(error) = session.deliver(lease, material, context, &self.staging_root) {
                return release_after_failure(session, error);
            }
        }
        if !self.leases.is_empty() {
            return release_after_failure(
                session,
                preparation_error(
                    SecretErrorKind::Revoked,
                    RecoveryClass::Reacquire,
                    "secret preparation retained a surplus lease",
                ),
            );
        }
        Ok(session)
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
