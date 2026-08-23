//! Preflight integrity and capacity observations.

use crate::MigrationPlan;

/// Filesystem capacity observed for migration planning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpaceObservation {
    database_size: u64,
    database_available: u64,
    backup_available: u64,
}

impl SpaceObservation {
    pub(crate) const fn new(
        database_bytes: u64,
        database_available_bytes: u64,
        backup_available_bytes: u64,
    ) -> Self {
        Self {
            database_size: database_bytes,
            database_available: database_available_bytes,
            backup_available: backup_available_bytes,
        }
    }

    /// Returns logical `SQLite` database bytes from page count and page size.
    #[must_use]
    pub const fn database_bytes(self) -> u64 {
        self.database_size
    }
    /// Returns bytes available on the database filesystem.
    #[must_use]
    pub const fn database_available_bytes(self) -> u64 {
        self.database_available
    }
    /// Returns bytes available on the backup filesystem.
    #[must_use]
    pub const fn backup_available_bytes(self) -> u64 {
        self.backup_available
    }
    /// Returns the conservative capacity used for deterministic planning.
    #[must_use]
    pub const fn planning_available_bytes(self) -> u64 {
        if self.database_available < self.backup_available {
            self.database_available
        } else {
            self.backup_available
        }
    }
}

/// Successful preflight report containing the only plan accepted by apply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreflightReport {
    plan: MigrationPlan,
    space: SpaceObservation,
}

impl PreflightReport {
    pub(crate) const fn new(plan: MigrationPlan, space: SpaceObservation) -> Self {
        Self { plan, space }
    }

    /// Returns the checked migration plan.
    #[must_use]
    pub const fn plan(&self) -> &MigrationPlan {
        &self.plan
    }
    /// Returns exact capacity observations.
    #[must_use]
    pub const fn space(&self) -> SpaceObservation {
        self.space
    }

    /// Consumes the report and returns its checked plan.
    #[must_use]
    pub fn into_plan(self) -> MigrationPlan {
        self.plan
    }
}
