//! Deterministic migration planning.

use peritus_types::Sha256Digest;

use crate::{
    ApplicationCompatibility, BackupPolicy, MigrationDescriptor, MigrationError,
    MigrationErrorCode, MigrationRegistry, MigrationVersion, RecoveryClass,
    verified::{backup_required, checked_required_space, versions_are_compatible},
};

/// One selected immutable migration step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MigrationStep(MigrationDescriptor);

impl MigrationStep {
    /// Returns the immutable descriptor.
    #[must_use]
    pub const fn descriptor(self) -> MigrationDescriptor {
        self.0
    }
}

/// Checked deterministic plan bound to registry, versions, and capacity observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationPlan {
    current: u64,
    target: MigrationVersion,
    registry_digest: Sha256Digest,
    steps: Vec<MigrationStep>,
    backup_required: bool,
    required_space_bytes: u64,
    observed_available_bytes: u64,
}

impl MigrationPlan {
    /// Plans from explicit trusted observations without performing I/O.
    ///
    /// # Errors
    ///
    /// Returns registry, direction, compatibility, arithmetic, or insufficient-space errors.
    pub fn from_observation(
        registry: MigrationRegistry,
        current: u64,
        target: MigrationVersion,
        compatibility: ApplicationCompatibility,
        database_bytes: u64,
        available_bytes: u64,
        reserve_bytes: u64,
    ) -> Result<Self, MigrationError> {
        Self::build(
            registry,
            current,
            target,
            compatibility,
            database_bytes,
            available_bytes,
            reserve_bytes,
        )
    }

    pub(crate) fn build(
        registry: MigrationRegistry,
        current: u64,
        target: MigrationVersion,
        compatibility: ApplicationCompatibility,
        database_bytes: u64,
        available_bytes: u64,
        reserve_bytes: u64,
    ) -> Result<Self, MigrationError> {
        registry.validate()?;
        if target.get() < current {
            return Err(MigrationError::message(
                MigrationErrorCode::ForwardOnly,
                RecoveryClass::CorrectRequest,
                "plan migration",
                "reverse migrations are not supported",
            ));
        }
        if target > registry.latest()? {
            return Err(MigrationError::message(
                MigrationErrorCode::UnsupportedVersion,
                RecoveryClass::CorrectRequest,
                "plan migration",
                "target exceeds the compiled migration registry",
            ));
        }
        if !versions_are_compatible(
            current,
            target.get(),
            compatibility.minimum(),
            compatibility.maximum().get(),
        ) {
            return Err(MigrationError::message(
                MigrationErrorCode::IncompatibleApplication,
                RecoveryClass::CorrectRequest,
                "plan migration",
                "running application does not support current-to-target range",
            ));
        }
        let mut steps = Vec::new();
        let mut backup = false;
        let mut scratch = 0_u64;
        for descriptor in registry.descriptors() {
            if descriptor.version().get() > current && descriptor.version() <= target {
                backup =
                    backup_required(backup, descriptor.backup_policy() == BackupPolicy::Required);
                scratch = scratch.checked_add(descriptor.scratch_bytes()).ok_or_else(overflow)?;
                steps.push(MigrationStep(*descriptor));
            }
        }
        let required_space_bytes =
            checked_required_space(database_bytes, scratch, reserve_bytes, backup)
                .ok_or_else(overflow)?;
        if required_space_bytes > available_bytes {
            return Err(MigrationError::space(required_space_bytes, available_bytes));
        }
        Ok(Self {
            current,
            target,
            registry_digest: registry.digest()?,
            steps,
            backup_required: backup,
            required_space_bytes,
            observed_available_bytes: available_bytes,
        })
    }

    /// Returns observed current version, where zero means legacy/unversioned.
    #[must_use]
    pub const fn current_version(&self) -> u64 {
        self.current
    }
    /// Returns target version.
    #[must_use]
    pub const fn target_version(&self) -> MigrationVersion {
        self.target
    }
    /// Returns digest of the exact registry used to plan.
    #[must_use]
    pub const fn registry_digest(&self) -> Sha256Digest {
        self.registry_digest
    }
    /// Returns selected steps in exact execution order.
    #[must_use]
    pub fn steps(&self) -> &[MigrationStep] {
        &self.steps
    }
    /// Returns whether a consistent backup is mandatory.
    #[must_use]
    pub const fn backup_required(&self) -> bool {
        self.backup_required
    }
    /// Returns checked required capacity.
    #[must_use]
    pub const fn required_space_bytes(&self) -> u64 {
        self.required_space_bytes
    }
    /// Returns the preflight capacity observation.
    #[must_use]
    pub const fn observed_available_bytes(&self) -> u64 {
        self.observed_available_bytes
    }
}

const fn overflow() -> MigrationError {
    MigrationError::message(
        MigrationErrorCode::InvalidRegistry,
        RecoveryClass::Terminal,
        "plan migration capacity",
        "migration capacity arithmetic overflowed",
    )
}
