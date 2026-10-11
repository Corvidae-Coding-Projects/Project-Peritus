//! Deterministic non-production credential store for tests.

use core::fmt;
use std::{collections::BTreeMap, sync::Mutex};

use zeroize::Zeroize;

use peritus_sandbox::SecretReference;

use crate::{
    CredentialStore, RecoveryClass, SecretError, SecretErrorKind, SecretMaterial, SecretOperation,
    StoreProbe,
};

/// Injected memory-store outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryStoreOutcome {
    /// Return inserted exact bytes.
    Available,
    /// Store locked.
    Locked,
    /// Access denied.
    Denied,
    /// Adapter unavailable.
    Unavailable,
    /// Entry corrupt.
    Corrupt,
    /// I/O failure.
    Io,
}

/// Explicitly non-production deterministic store.
pub struct MemoryCredentialStore {
    entries: Mutex<BTreeMap<SecretReference, Vec<u8>>>,
    outcome: Mutex<MemoryStoreOutcome>,
}

impl fmt::Debug for MemoryCredentialStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MemoryCredentialStore")
            .field("entries", &"[REDACTED]")
            .field(
                "outcome",
                &*self.outcome.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
            )
            .finish()
    }
}

impl Drop for MemoryCredentialStore {
    fn drop(&mut self) {
        for bytes in
            self.entries.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner).values_mut()
        {
            bytes.zeroize();
        }
    }
}

impl MemoryCredentialStore {
    /// Creates an empty available test store.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
            outcome: Mutex::new(MemoryStoreOutcome::Available),
        }
    }
    /// Inserts exact bytes for a reference.
    pub fn insert(&self, reference: SecretReference, bytes: Vec<u8>) {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(reference, bytes);
    }
    /// Selects one deterministic store outcome.
    pub fn set_outcome(&self, outcome: MemoryStoreOutcome) {
        *self.outcome.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = outcome;
    }
}

impl Default for MemoryCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for MemoryCredentialStore {
    fn probe(&self) -> StoreProbe {
        StoreProbe::new(
            *self.outcome.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
                != MemoryStoreOutcome::Unavailable,
            "memory-test-only",
        )
    }

    fn lookup(&self, reference: SecretReference) -> Result<SecretMaterial, SecretError> {
        let outcome = *self.outcome.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if outcome != MemoryStoreOutcome::Available {
            return Err(outcome_error(outcome));
        }
        let bytes = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&reference)
            .cloned()
            .ok_or_else(|| {
                error(
                    SecretErrorKind::Missing,
                    RecoveryClass::Reacquire,
                    "memory test entry is missing",
                )
            })?;
        if peritus_codec::sha256(&bytes) != reference.version() {
            return Err(error(
                SecretErrorKind::StaleVersion,
                RecoveryClass::Reacquire,
                "memory test entry version differs",
            ));
        }
        SecretMaterial::new(bytes)
    }
}

const fn outcome_error(outcome: MemoryStoreOutcome) -> SecretError {
    match outcome {
        MemoryStoreOutcome::Available => error(
            SecretErrorKind::Missing,
            RecoveryClass::Reacquire,
            "memory test entry is missing",
        ),
        MemoryStoreOutcome::Locked => error(
            SecretErrorKind::Locked,
            RecoveryClass::UnlockStore,
            "memory test store is locked",
        ),
        MemoryStoreOutcome::Denied => error(
            SecretErrorKind::Denied,
            RecoveryClass::Reacquire,
            "memory test store denied access",
        ),
        MemoryStoreOutcome::Unavailable => error(
            SecretErrorKind::Unavailable,
            RecoveryClass::Retry,
            "memory test store is unavailable",
        ),
        MemoryStoreOutcome::Corrupt => error(
            SecretErrorKind::Corrupt,
            RecoveryClass::Reacquire,
            "memory test entry is corrupt",
        ),
        MemoryStoreOutcome::Io => {
            error(SecretErrorKind::Io, RecoveryClass::Retry, "memory test store I/O failed")
        }
    }
}

const fn error(
    kind: SecretErrorKind,
    recovery: RecoveryClass,
    detail: &'static str,
) -> SecretError {
    SecretError::new(kind, SecretOperation::Lookup, recovery, detail)
}
