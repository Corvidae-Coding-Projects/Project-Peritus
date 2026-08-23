//! Restart reconciliation and explicit backup restoration.

use crate::{
    MigrationError, MigrationErrorCode, MigrationOperationId, RecoveryAction, RecoveryOperation,
    RecoveryReport, RecoveryState, backup, catalog, recovery,
};

use super::{MigrationEngine, recovery_required, verify_integrity};

impl MigrationEngine {
    /// Reconciles all incomplete durable operations without automatically applying or restoring.
    ///
    /// # Errors
    ///
    /// Returns registry, `SQLite`, or recovery-corruption errors.
    pub fn reconcile(&self) -> Result<RecoveryReport, MigrationError> {
        let mut actions = Vec::new();
        for operation in catalog::pending_operations(&self.connection)? {
            match operation.state() {
                RecoveryState::Planned => actions.push(if operation.backup_required() {
                    RecoveryAction::ResumeBackup(operation.id())
                } else {
                    RecoveryAction::ResumeApply(operation.id())
                }),
                RecoveryState::BackupReady => {
                    actions.push(RecoveryAction::ResumeApply(operation.id()));
                }
                RecoveryState::Applying => {
                    let current = catalog::current_version(&self.connection, self.registry)?;
                    if current == operation.target_version().get() {
                        catalog::update_state(
                            &self.connection,
                            operation.id(),
                            RecoveryState::Applied,
                            None,
                            None,
                        )?;
                        actions.push(RecoveryAction::ReconciledApplied(operation.id()));
                    } else if current == operation.from_version() {
                        let state = if operation.backup_required() {
                            RecoveryState::RestoreRequired
                        } else {
                            RecoveryState::Failed
                        };
                        catalog::update_state(
                            &self.connection,
                            operation.id(),
                            state,
                            None,
                            Some(MigrationErrorCode::Indeterminate.as_str()),
                        )?;
                        actions.push(if operation.backup_required() {
                            RecoveryAction::RestoreBackup(operation.id())
                        } else {
                            RecoveryAction::RetryApply(operation.id())
                        });
                    } else {
                        return Err(recovery::corrupt(
                            "atomic migration left an impossible intermediate version",
                        ));
                    }
                }
                RecoveryState::RestoreRequired => {
                    actions.push(RecoveryAction::RestoreBackup(operation.id()));
                }
                RecoveryState::Failed => {
                    actions.push(RecoveryAction::RetryApply(operation.id()));
                }
                RecoveryState::Applied | RecoveryState::Restored => {}
            }
        }
        Ok(RecoveryReport::new(actions))
    }

    /// Explicitly restores a verified pre-migration backup; no reverse SQL is run.
    ///
    /// # Errors
    ///
    /// Returns missing recovery state, backup digest, restore, or integrity errors.
    pub fn restore_backup(
        &mut self,
        operation: MigrationOperationId,
    ) -> Result<RecoveryOperation, MigrationError> {
        let stored = catalog::load_operation(&self.connection, operation)?
            .ok_or_else(|| recovery::corrupt("recovery operation is missing"))?;
        if !stored.backup_required()
            || !matches!(
                stored.state(),
                RecoveryState::BackupReady
                    | RecoveryState::Applying
                    | RecoveryState::Applied
                    | RecoveryState::RestoreRequired
            )
        {
            return Err(recovery_required("operation has no restorable pre-migration backup"));
        }
        let digest = stored.backup_digest().ok_or_else(|| {
            recovery::corrupt("backup-required operation has no durable backup digest")
        })?;
        let path = recovery::backup_path(&self.backup_directory, operation, stored.from_version());
        backup::verify(&path, digest)?;
        backup::restore(&mut self.connection, &path)?;
        verify_integrity(&self.connection)?;
        let current = catalog::current_version(&self.connection, self.registry)?;
        if current != stored.from_version() {
            return Err(recovery::corrupt("restored backup has the wrong migration version"));
        }
        catalog::update_state(
            &self.connection,
            operation,
            RecoveryState::Restored,
            Some(digest),
            None,
        )?;
        catalog::load_operation(&self.connection, operation)?
            .ok_or_else(|| recovery::corrupt("restored operation record is missing"))
    }
}
