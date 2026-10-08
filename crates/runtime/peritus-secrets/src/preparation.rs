//! Authorized-input preparation of exact secret delivery sessions.

use core::fmt;
use std::{path::PathBuf, sync::Arc};

use peritus_sandbox::{SecretDelivery, SecretRequirement};
use peritus_types::{EnvironmentId, ProcessId, Sha256Digest};

use crate::{
    CredentialStore, RecoveryClass, SecretDeliveryContext, SecretDeliverySession, SecretError,
    SecretErrorKind, SecretLease, SecretOperation,
};

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
    /// Rejects a relative private-file staging root.
    pub fn new(
        store: Arc<dyn CredentialStore>,
        leases: Vec<SecretLease>,
        now_epoch_millis: u64,
        staging_root: PathBuf,
    ) -> Result<Self, SecretError> {
        if !staging_root.is_absolute() {
            return Err(preparation_error(
                SecretErrorKind::InvalidInput,
                RecoveryClass::CorrectRequest,
                "secret preparation staging root is invalid",
            ));
        }
        Ok(Self { store, leases, now_epoch_millis, staging_root })
    }

    /// Returns the nonsensitive number of supplied exact leases.
    #[must_use]
    pub fn lease_count(&self) -> usize {
        self.leases.len()
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
        self.prepare_cancellable(
            owner,
            environment,
            sandbox_digest,
            execution_digest,
            requirements,
            || true,
            |_, _| {},
        )
    }

    /// Resolves and stages exact checked requirements while retaining owner cancellation.
    ///
    /// Cancellation is checked before every credential lookup and after every completed delivery.
    /// The observer receives monotonic completed and total item counts without secret identity or
    /// material. Any partial session is explicitly released before cancellation is returned.
    ///
    /// # Errors
    /// Returns a typed store, lease, delivery, cancellation, or cleanup failure.
    #[allow(
        clippy::too_many_arguments,
        reason = "exact authority bindings and owner callbacks remain explicit"
    )]
    pub fn prepare_cancellable(
        mut self,
        owner: ProcessId,
        environment: EnvironmentId,
        sandbox_digest: Sha256Digest,
        execution_digest: Sha256Digest,
        requirements: &[SecretRequirement],
        mut should_continue: impl FnMut() -> bool,
        mut observe_completed: impl FnMut(usize, usize),
    ) -> Result<SecretDeliverySession, SecretError> {
        if requirements.len() != self.leases.len() {
            return Err(preparation_error(
                SecretErrorKind::Revoked,
                RecoveryClass::Reacquire,
                "secret requirements and supplied leases differ",
            ));
        }
        if !should_continue() {
            return Err(preparation_cancelled());
        }
        preflight_bindings(
            &self.leases,
            requirements,
            owner,
            environment,
            sandbox_digest,
            execution_digest,
        )?;
        preflight_staging_paths(&self.staging_root, &self.leases)?;
        observe_completed(0, requirements.len());
        let context = SecretDeliveryContext::new(
            owner,
            environment,
            sandbox_digest,
            execution_digest,
            self.now_epoch_millis,
        );
        let mut session = SecretDeliverySession::new();
        for (index, requirement) in requirements.iter().enumerate() {
            if !should_continue() {
                return release_after_failure(session, preparation_cancelled());
            }
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
            observe_completed(index.saturating_add(1), requirements.len());
            if !should_continue() {
                return release_after_failure(session, preparation_cancelled());
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

fn preflight_bindings(
    leases: &[SecretLease],
    requirements: &[SecretRequirement],
    owner: ProcessId,
    environment: EnvironmentId,
    sandbox_digest: Sha256Digest,
    execution_digest: Sha256Digest,
) -> Result<(), SecretError> {
    let mut matched = vec![false; leases.len()];
    for requirement in requirements {
        let position = leases.iter().enumerate().position(|(index, lease)| {
            !matched[index]
                && lease.owner() == owner
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
        matched[position] = true;
    }
    if matched.iter().all(|value| *value) {
        Ok(())
    } else {
        Err(preparation_error(
            SecretErrorKind::Revoked,
            RecoveryClass::Reacquire,
            "secret preparation retained a surplus lease",
        ))
    }
}

fn preflight_staging_paths(
    root: &std::path::Path,
    leases: &[SecretLease],
) -> Result<(), SecretError> {
    if !leases.iter().any(|lease| matches!(lease.delivery(), SecretDelivery::File(_))) {
        return Ok(());
    }
    std::fs::create_dir_all(root)
        .map_err(|_| preparation_delivery_error("secret staging root cannot be created"))?;
    let metadata = std::fs::symlink_metadata(root)
        .map_err(|_| preparation_delivery_error("secret staging root cannot be inspected"))?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || std::fs::canonicalize(root).ok().as_deref() != Some(root)
    {
        return Err(preparation_delivery_error(
            "secret staging root is not an exact private directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(preparation_delivery_error(
                "secret staging root permits writes by another account",
            ));
        }
    }
    for lease in leases {
        if !matches!(lease.delivery(), SecretDelivery::File(_)) {
            continue;
        }
        let path = crate::delivery::staging_path(root, lease.id());
        match std::fs::symlink_metadata(path) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(preparation_delivery_error(
                    "selected private secret staging path already exists",
                ));
            }
            Err(_) => {
                return Err(preparation_delivery_error(
                    "selected private secret staging path cannot be inspected",
                ));
            }
        }
    }
    Ok(())
}

const fn preparation_delivery_error(detail: &'static str) -> SecretError {
    SecretError::new(
        SecretErrorKind::Delivery,
        SecretOperation::Deliver,
        RecoveryClass::RevokeAndClean,
        detail,
    )
}

const fn preparation_cancelled() -> SecretError {
    SecretError::new(
        SecretErrorKind::Cancelled,
        SecretOperation::Deliver,
        RecoveryClass::RevokeAndClean,
        "secret preparation was cancelled by its owner",
    )
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
