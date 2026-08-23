//! Immutable ordered migration registry.

use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

use crate::{
    BackupPolicy, MigrationDescriptor, MigrationError, MigrationErrorCode, MigrationVersion,
    RecoveryClass, verified::versions_are_contiguous,
};

const VERSION_ONE_SQL: &str = "PRAGMA user_version = 1;\n";
const VERSION_ONE_DIGEST: [u8; 32] = [
    0xef, 0x5d, 0x06, 0x65, 0x33, 0xdb, 0x1a, 0xfc, 0x26, 0x00, 0x90, 0x28, 0xc1, 0x86, 0xb2, 0x9c,
    0xfe, 0x53, 0xf4, 0xe6, 0x74, 0x4d, 0x2e, 0x79, 0xf5, 0x29, 0x5f, 0x98, 0x1b, 0xc7, 0x81, 0x3a,
];
const CURRENT_DESCRIPTORS: [MigrationDescriptor; 1] = [MigrationDescriptor::new(
    MigrationVersion::FIRST,
    "0.0.0",
    VERSION_ONE_SQL,
    Sha256Digest::new(VERSION_ONE_DIGEST),
    BackupPolicy::Required,
    64 * 1024,
)];

/// Immutable ordered migration registry.
#[derive(Clone, Copy, Debug)]
pub struct MigrationRegistry {
    descriptors: &'static [MigrationDescriptor],
}

impl MigrationRegistry {
    /// Returns the compiled production registry.
    #[must_use]
    pub const fn current() -> Self {
        Self { descriptors: &CURRENT_DESCRIPTORS }
    }

    /// Creates a registry from statically compiled descriptors.
    ///
    /// This constructor supports compatibility fixtures and future composed registries. Every use
    /// still recomputes exact source digests and validates ordering.
    #[must_use]
    pub const fn from_static(descriptors: &'static [MigrationDescriptor]) -> Self {
        Self { descriptors }
    }

    /// Returns descriptors in required application order.
    #[must_use]
    pub const fn descriptors(self) -> &'static [MigrationDescriptor] {
        self.descriptors
    }

    /// Validates nonempty contiguous ordering, exact digests, release text, and safe transaction
    /// ownership of migration SQL.
    ///
    /// # Errors
    ///
    /// Returns a stable registry or digest-drift error.
    pub fn validate(self) -> Result<(), MigrationError> {
        if self.descriptors.is_empty() {
            return Err(invalid_registry("migration registry must not be empty"));
        }
        let mut previous = 0_u64;
        for descriptor in self.descriptors {
            if !versions_are_contiguous(previous, descriptor.version().get()) {
                return Err(invalid_registry("migration versions are not contiguous from one"));
            }
            if descriptor.release().is_empty()
                || descriptor.release().len() > 128
                || !descriptor.release().bytes().all(|byte| byte.is_ascii_graphic())
            {
                return Err(invalid_registry("migration release text is invalid"));
            }
            if descriptor.sql().is_empty() || descriptor.sql().len() > 4 * 1024 * 1024 {
                return Err(invalid_registry("migration SQL source is empty or too large"));
            }
            reject_transaction_control(descriptor.sql())?;
            let actual = Sha256Digest::new(Sha256::digest(descriptor.sql().as_bytes()).into());
            if actual != descriptor.source_digest() {
                return Err(MigrationError::message(
                    MigrationErrorCode::DigestDrift,
                    RecoveryClass::Terminal,
                    "validate migration registry",
                    "migration source digest differs from the reviewed descriptor",
                ));
            }
            previous = descriptor.version().get();
        }
        Ok(())
    }

    /// Returns the latest compiled version after validation.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::validate`].
    pub fn latest(self) -> Result<MigrationVersion, MigrationError> {
        self.validate()?;
        self.descriptors
            .last()
            .map(|descriptor| descriptor.version())
            .ok_or_else(|| invalid_registry("migration registry must not be empty"))
    }

    /// Computes a digest binding the complete ordered registry.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`Self::validate`].
    pub fn digest(self) -> Result<Sha256Digest, MigrationError> {
        self.validate()?;
        let mut hasher = Sha256::new();
        hasher.update(b"peritus/migration-registry/v1\0");
        for descriptor in self.descriptors {
            hasher.update(descriptor.version().get().to_be_bytes());
            hasher.update(descriptor.source_digest().as_bytes());
            hasher.update([match descriptor.backup_policy() {
                BackupPolicy::NotRequired => 0,
                BackupPolicy::Required => 1,
            }]);
            hasher.update(descriptor.scratch_bytes().to_be_bytes());
            hasher.update((descriptor.release().len() as u64).to_be_bytes());
            hasher.update(descriptor.release().as_bytes());
        }
        Ok(Sha256Digest::new(hasher.finalize().into()))
    }
}

fn reject_transaction_control(sql: &str) -> Result<(), MigrationError> {
    let uppercase = sql.to_ascii_uppercase();
    for forbidden in ["BEGIN", "COMMIT", "ROLLBACK", "ATTACH", "DETACH", "VACUUM"] {
        if uppercase
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .any(|token| token == forbidden)
        {
            return Err(invalid_registry(
                "migration SQL must not control transactions, attachment, or vacuum",
            ));
        }
    }
    Ok(())
}

const fn invalid_registry(message: &'static str) -> MigrationError {
    MigrationError::message(
        MigrationErrorCode::InvalidRegistry,
        RecoveryClass::CorrectRequest,
        "validate migration registry",
        message,
    )
}
