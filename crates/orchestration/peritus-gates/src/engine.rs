//! Thin effect shell around the pure reducer and atomic durability boundary.

mod publication;

use peritus_journal::{CommittedBatch, SqliteJournal, StoreId};
use peritus_tools_quality::{CheckDefinition, CleanQualitySnapshot, QualityTerminal};
use peritus_types::{GateExecutionId, GateId, RevisionTuple, RunId};

use crate::durability::GateCommitDisposition;
use crate::{
    GateAttemptResult, GateCommand, GateCommandKind, GateError, GateErrorKind, GatePlan,
    GateRecoveryAction, GateRejection, GateRunState, GateSlotPhase, RecoveryDisposition,
};

/// Inert exact input to the authorized C4 executor or its recovery adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateDispatch {
    run_id: RunId,
    revision: RevisionTuple,
    gate_id: GateId,
    attempt: crate::ActiveAttempt,
    definition: CheckDefinition,
    snapshot: CleanQualitySnapshot,
}

impl GateDispatch {
    fn from_state(
        plan: &GatePlan,
        state: &GateRunState,
        gate_id: GateId,
        snapshot: &CleanQualitySnapshot,
        allowed_phases: &[GateSlotPhase],
    ) -> Result<Self, GateError> {
        let mismatches = [
            plan.run_id() != state.run_id(),
            plan.digest() != state.plan_digest(),
            plan.revision() != state.revision(),
            snapshot.revision() != state.revision(),
            snapshot.binding_digest() != state.snapshot_digest(),
        ];
        if mismatches.into_iter().any(core::convert::identity) {
            return Err(crate::error::reject(
                GateRejection::BindingMismatch,
                "effect input differs from the run plan, revision, or clean snapshot",
            ));
        }
        let planned = plan.gate(gate_id).ok_or_else(|| {
            crate::error::reject(
                GateRejection::IdentityMismatch,
                "effect gate is absent from the exact plan",
            )
        })?;
        let slot = state.slot(gate_id).ok_or_else(|| {
            crate::error::reject(
                GateRejection::IdentityMismatch,
                "effect gate is absent from authoritative state",
            )
        })?;
        let attempt = slot.active_attempt().ok_or_else(|| {
            crate::error::reject(
                GateRejection::IllegalTransition,
                "effect gate has no exact active attempt",
            )
        })?;
        if !allowed_phases.contains(&slot.phase())
            || attempt.snapshot_digest() != state.snapshot_digest()
        {
            return Err(crate::error::reject(
                GateRejection::IllegalTransition,
                "effect is not legal in the current durable attempt phase",
            ));
        }
        Ok(Self {
            run_id: state.run_id(),
            revision: state.revision(),
            gate_id,
            attempt,
            definition: planned.quality_definition().clone(),
            snapshot: snapshot.clone(),
        })
    }

    /// Returns the gate run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the exact run revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the declared gate identity.
    #[must_use]
    pub const fn gate_id(&self) -> GateId {
        self.gate_id
    }
    /// Returns the complete active attempt binding.
    #[must_use]
    pub const fn attempt(&self) -> crate::ActiveAttempt {
        self.attempt
    }
    /// Borrows the exact authorized C4 check definition.
    #[must_use]
    pub const fn quality_definition(&self) -> &CheckDefinition {
        &self.definition
    }
    /// Borrows the revalidated immutable C1 snapshot token.
    #[must_use]
    pub const fn snapshot(&self) -> &CleanQualitySnapshot {
        &self.snapshot
    }
}

/// Validated terminal returned by the shell, still inert until committed as a command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchReceipt {
    execution_id: GateExecutionId,
    result: GateAttemptResult,
}

impl DispatchReceipt {
    /// Returns the exact dispatched execution.
    #[must_use]
    pub const fn execution_id(&self) -> GateExecutionId {
        self.execution_id
    }
    /// Borrows the strict normalized terminal.
    #[must_use]
    pub const fn result(&self) -> &GateAttemptResult {
        &self.result
    }
}

/// Checked recovery observation, still inert until committed as a command.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RecoveryReceipt {
    execution_id: GateExecutionId,
    disposition: RecoveryDisposition,
}

impl RecoveryReceipt {
    /// Constructs an observation from a trusted recovery adapter.
    #[must_use]
    pub const fn new(execution_id: GateExecutionId, disposition: RecoveryDisposition) -> Self {
        Self { execution_id, disposition }
    }
    /// Returns the reconciled execution.
    #[must_use]
    pub const fn execution_id(self) -> GateExecutionId {
        self.execution_id
    }
    /// Returns the closed recovery classification.
    #[must_use]
    pub const fn disposition(self) -> RecoveryDisposition {
        self.disposition
    }
}

/// Only authorized C4 composition may implement quality execution.
pub trait GateExecutor {
    /// Executes one already-durably-dispatched attempt and returns a strict quality terminal.
    ///
    /// # Errors
    /// Returns an explicit executor/recovery failure; absence never implies success.
    fn execute(&mut self, dispatch: &GateDispatch) -> Result<QualityTerminal, GateError>;
}

/// Recovery port for a dispatched attempt whose ownership/terminality is uncertain.
pub trait GateRecovery {
    /// Reconciles the prior effect without creating a fresh action.
    ///
    /// # Errors
    /// Returns an explicit recovery failure and leaves the state recovery-pending.
    fn reconcile(
        &mut self,
        dispatch: &GateDispatch,
        last_result: Option<&GateAttemptResult>,
    ) -> Result<RecoveryReceipt, GateError>;
}

/// Stateful shell that advances authority only after atomic C0 commit observations.
#[derive(Debug)]
pub struct GateEngine {
    store_id: StoreId,
    plan: GatePlan,
    state: GateRunState,
    effect_permit: Option<GateExecutionId>,
}

impl GateEngine {
    /// Starts and atomically persists a new run bound to this journal's store identity.
    ///
    /// # Errors
    /// Rejects invalid genesis or any codec/C0 failure. No engine is returned before persistence.
    pub fn start(
        journal: &mut SqliteJournal,
        plan: GatePlan,
        command: &GateCommand,
    ) -> Result<(Self, CommittedBatch), GateError> {
        let store_id = journal.store_id();
        let transition = crate::start(&plan, command)?;
        let observation =
            crate::durability::commit_gate_transition_observed(journal, command, &transition)?;
        let committed = observation.into_batch();
        Ok((
            Self { store_id, plan, state: transition.into_state(), effect_permit: None },
            committed,
        ))
    }

    /// Resumes only from an event-replayed, checkpoint-matched, store-bound aggregate.
    ///
    /// # Errors
    /// Rejects a missing aggregate or any replay/checkpoint mismatch.
    pub fn resume(plan: GatePlan, replay: &crate::GateReplay) -> Result<Self, GateError> {
        let state = replay.rebuild(&plan)?.ok_or_else(|| {
            GateError::new(
                GateErrorKind::Journal,
                GateRecoveryAction::CorrectInput,
                "cannot resume a gate run that has no durable aggregate",
            )
        })?;
        Ok(Self { store_id: replay.store_id(), plan, state, effect_permit: None })
    }

    /// Reduces and atomically commits one command, then advances in-memory authority.
    ///
    /// # Errors
    /// Leaves current in-memory state unchanged on store, reducer, codec, or C0 failure.
    pub fn commit(
        &mut self,
        journal: &mut SqliteJournal,
        command: &GateCommand,
    ) -> Result<CommittedBatch, GateError> {
        self.ensure_store(journal)?;
        if self.effect_permit.is_some()
            && !matches!(command.kind(), GateCommandKind::BeginCancellation)
        {
            return Err(crate::error::reject(
                GateRejection::IllegalTransition,
                "durably dispatched attempt must cross the effect boundary before another command",
            ));
        }
        let transition = crate::decide(&self.plan, &self.state, command)?;
        let observation =
            crate::durability::commit_gate_transition_observed(journal, command, &transition)?;
        let disposition = observation.disposition();
        let committed = observation.into_batch();
        self.effect_permit = match (disposition, command.kind()) {
            (
                GateCommitDisposition::Committed,
                GateCommandKind::MarkDispatched { execution_id, .. },
            ) => Some(*execution_id),
            _ => None,
        };
        self.state = transition.into_state();
        Ok(committed)
    }

    /// Executes one attempt only after `MarkDispatched` is already durable.
    ///
    /// # Errors
    /// Rejects stale snapshot/state/phase or invalid quality terminal identity.
    pub fn execute(
        &mut self,
        gate_id: GateId,
        snapshot: &CleanQualitySnapshot,
        executor: &mut impl GateExecutor,
    ) -> Result<DispatchReceipt, GateError> {
        let dispatch = GateDispatch::from_state(
            &self.plan,
            &self.state,
            gate_id,
            snapshot,
            &[GateSlotPhase::Dispatched],
        )?;
        if self.effect_permit != Some(dispatch.attempt.execution_id()) {
            return Err(crate::error::reject(
                GateRejection::IllegalTransition,
                "no live post-commit effect permit exists; reconcile instead of redispatching",
            ));
        }
        self.effect_permit = None;
        let terminal = executor.execute(&dispatch)?;
        let result = GateAttemptResult::from_quality(gate_id, &terminal)?;
        Ok(DispatchReceipt { execution_id: dispatch.attempt.execution_id(), result })
    }

    /// Reconciles a recovery-pending attempt without permitting a new dispatch.
    ///
    /// # Errors
    /// Rejects stale snapshot/state/phase, mismatched receipt, or adapter failure.
    pub fn recover(
        &self,
        gate_id: GateId,
        snapshot: &CleanQualitySnapshot,
        recovery: &mut impl GateRecovery,
    ) -> Result<RecoveryReceipt, GateError> {
        let dispatch = GateDispatch::from_state(
            &self.plan,
            &self.state,
            gate_id,
            snapshot,
            &[GateSlotPhase::Dispatched, GateSlotPhase::RecoveryPending],
        )?;
        if self.effect_permit.is_some() {
            return Err(crate::error::reject(
                GateRejection::IllegalTransition,
                "live dispatch permit must be consumed before recovery",
            ));
        }
        let result = self.state.slot(gate_id).and_then(crate::GateSlot::last_result);
        let receipt = recovery.reconcile(&dispatch, result)?;
        if receipt.execution_id != dispatch.attempt.execution_id() {
            return Err(crate::error::reject(
                GateRejection::IdentityMismatch,
                "recovery receipt belongs to another execution",
            ));
        }
        Ok(receipt)
    }

    /// Borrows the immutable run plan.
    #[must_use]
    pub const fn plan(&self) -> &GatePlan {
        &self.plan
    }
    /// Returns the durable journal store that owns this engine's authority.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }
    /// Borrows current authoritative state.
    #[must_use]
    pub const fn state(&self) -> &GateRunState {
        &self.state
    }

    fn ensure_store(&self, journal: &SqliteJournal) -> Result<(), GateError> {
        if journal.store_id() == self.store_id {
            Ok(())
        } else {
            Err(GateError::new(
                GateErrorKind::Journal,
                GateRecoveryAction::CorrectInput,
                "gate engine is bound to another durable journal store",
            ))
        }
    }
}

#[cfg(test)]
mod tests;

/// Builds the observation command kind corresponding to one executor receipt.
#[must_use]
pub fn observed_result_kind(gate_id: GateId, receipt: DispatchReceipt) -> GateCommandKind {
    GateCommandKind::ObserveResult {
        gate_id,
        execution_id: receipt.execution_id,
        result: receipt.result,
    }
}

/// Builds the recovery command kind corresponding to one adapter receipt.
#[must_use]
pub const fn recovery_kind(gate_id: GateId, receipt: RecoveryReceipt) -> GateCommandKind {
    GateCommandKind::ClassifyRecovery {
        gate_id,
        execution_id: receipt.execution_id,
        disposition: receipt.disposition,
    }
}
