//! Forward-only ordered migrations, backups, and recovery for Peritus.
//!
//! Migration selection and resource planning are deterministic. `SQLite` integrity checks, online
//! backups, transactions, filesystem synchronization, and free-space observations remain narrow
//! host adapters. Rollback is always an explicit restore from a verified pre-migration backup;
//! reverse migrations are not supported.

mod backup;
mod catalog;
mod config;
mod descriptor;
mod engine;
mod error;
mod plan;
mod preflight;
mod recovery;
mod registry;
mod verified;

pub use config::{ApplicationCompatibility, MigrationConfig};
pub use descriptor::{BackupPolicy, MigrationDescriptor, MigrationVersion};
pub use engine::{
    AppliedMigration, MigrationEngine, MigrationFailpoint, MigrationHooks, NoMigrationHooks,
};
pub use error::{MigrationError, MigrationErrorCode, RecoveryClass};
pub use plan::{MigrationPlan, MigrationStep};
pub use preflight::{PreflightReport, SpaceObservation};
pub use recovery::{
    MigrationOperationId, RecoveryAction, RecoveryOperation, RecoveryReport, RecoveryState,
};
pub use registry::MigrationRegistry;
