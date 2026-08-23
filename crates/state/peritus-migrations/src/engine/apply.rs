//! Exclusive migration transaction application and commit classification.

use rusqlite::TransactionBehavior;

use crate::{
    MigrationError, MigrationErrorCode, MigrationFailpoint, MigrationHooks, MigrationOperationId,
    MigrationPlan, RecoveryClass, catalog,
};

use super::{MigrationEngine, fail};

pub(super) enum ApplyTransactionError {
    BeforeCommit(MigrationError),
    CommitIndeterminate(MigrationError),
}

impl MigrationEngine {
    pub(super) fn apply_transaction<H: MigrationHooks>(
        &mut self,
        plan: &MigrationPlan,
        operation: MigrationOperationId,
        hooks: &mut H,
    ) -> Result<(), ApplyTransactionError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Exclusive)
            .map_err(|error| {
                ApplyTransactionError::BeforeCommit(MigrationError::sqlite(
                    "begin exclusive migration transaction",
                    error,
                ))
            })?;
        for step in plan.steps() {
            let descriptor = step.descriptor();
            fail(hooks, MigrationFailpoint::BeforeStep(descriptor.version()))
                .map_err(ApplyTransactionError::BeforeCommit)?;
            transaction
                .execute_batch(descriptor.sql())
                .map_err(MigrationError::migration_sql)
                .map_err(ApplyTransactionError::BeforeCommit)?;
            catalog::record_step(&transaction, operation, descriptor)
                .map_err(ApplyTransactionError::BeforeCommit)?;
        }
        fail(hooks, MigrationFailpoint::BeforeCommit)
            .map_err(ApplyTransactionError::BeforeCommit)?;
        transaction.commit().map_err(|error| {
            ApplyTransactionError::CommitIndeterminate(MigrationError::indeterminate_commit(error))
        })?;
        if hooks.should_fail(MigrationFailpoint::AfterCommit) {
            return Err(ApplyTransactionError::CommitIndeterminate(MigrationError::message(
                MigrationErrorCode::Indeterminate,
                RecoveryClass::Reconcile,
                "acknowledge migration commit",
                "commit succeeded but acknowledgement was intentionally interrupted",
            )));
        }
        Ok(())
    }
}
