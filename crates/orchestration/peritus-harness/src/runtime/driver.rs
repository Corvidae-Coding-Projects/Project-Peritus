//! Commit-before-effect E1 runtime driver.

use core::fmt;

use peritus_artifact_store::ArtifactStore;
use peritus_journal::SqliteJournal;
use peritus_types::SnapshotId;
use peritus_workspace::WorkspaceGateway;

use crate::{
    aggregate::{HarnessCommand, HarnessCommandKind, HarnessState, decide},
    durability::{
        DirectiveClaim, HarnessReplay, commit_harness_settlement, commit_harness_transition,
    },
    materialization::{
        MaterializationError, MaterializationErrorKind, MaterializationFailure,
        MaterializationFailureCode, MaterializationPlan, MaterializationPlanId,
        MaterializationReceipt, MaterializationRecovery, PlannedFileOperation, execute_plan,
    },
};

use super::{
    CommittedPlan, MaterializationTiming, PlanCommitEvidence, PlanningOutcome,
    RuntimeAuthorizations, RuntimeError, RuntimeErrorKind, RuntimeOutcome, SettlementIds,
};

/// C0/C1 effect shell. It cannot execute a plan that has not passed through [`CommittedPlan`].
pub struct HarnessRuntime<'a> {
    journal: &'a mut SqliteJournal,
    component_store: &'a ArtifactStore,
    manifest_store: &'a ArtifactStore,
}

impl<'a> HarnessRuntime<'a> {
    /// Creates a runtime over explicit C0 journal, component, and C1-manifest artifact stores.
    #[must_use]
    pub const fn new(
        journal: &'a mut SqliteJournal,
        component_store: &'a ArtifactStore,
        manifest_store: &'a ArtifactStore,
    ) -> Self {
        Self { journal, component_store, manifest_store }
    }

    /// Checks idempotency, decides the plan command, and atomically commits before C1 is callable.
    ///
    /// # Errors
    /// Rejects non-plan commands, stale aggregate fences, or any C0 commit failure.
    pub fn commit_plan(
        &mut self,
        prior: &HarnessState,
        command: &HarnessCommand,
    ) -> Result<PlanningOutcome, RuntimeError> {
        let HarnessCommandKind::PlanMaterialization { plan } = command.kind() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidInput,
                "runtime expected a plan command",
            ));
        };
        if let Some(receipt) = retained_receipt(prior, plan) {
            return Ok(PlanningOutcome::AlreadyMaterialized(receipt.clone()));
        }
        let transition = decide(Some(prior), command).map_err(aggregate)?;
        let batch =
            commit_harness_transition(self.journal, command, &transition).map_err(durability)?;
        let (_, state) = transition.into_parts();
        if state.pending_plan(plan.id()).is_none() {
            return Err(RuntimeError::new(
                RuntimeErrorKind::Aggregate,
                "committed plan is absent from successor state",
            ));
        }
        Ok(PlanningOutcome::Committed(CommittedPlan {
            plan: plan.clone(),
            state,
            evidence: PlanCommitEvidence::Fresh(batch),
        }))
    }

    /// Recreates execution type state after restart from checked C0 replay evidence.
    ///
    /// # Errors
    /// Rejects replay from another store, an empty aggregate, or a plan no longer pending.
    pub fn recover_plan(
        &self,
        replay: &HarnessReplay,
        plan_id: MaterializationPlanId,
    ) -> Result<CommittedPlan, RuntimeError> {
        if replay.store_id() != self.journal.store_id() {
            return Err(RuntimeError::new(
                RuntimeErrorKind::Durability,
                "recovery evidence belongs to another C0 store",
            ));
        }
        let state = replay.rebuild().map_err(durability)?.ok_or_else(|| {
            RuntimeError::new(RuntimeErrorKind::Durability, "recovery aggregate is empty")
        })?;
        let plan = state
            .pending_plan(plan_id)
            .ok_or_else(|| {
                RuntimeError::new(RuntimeErrorKind::InvalidInput, "recovery plan is not pending")
            })?
            .plan()
            .clone();
        Ok(CommittedPlan {
            plan,
            state,
            evidence: PlanCommitEvidence::Recovered { store_id: replay.store_id() },
        })
    }

    /// Executes one exact claimed committed plan and atomically settles success or failure.
    ///
    /// # Errors
    /// Returns no receipt when the claim is wrong or when success/failure settlement is not durable.
    #[allow(
        clippy::too_many_arguments,
        reason = "authority, C1, claim, identity, and time inputs stay explicit"
    )]
    pub fn execute_claimed(
        &mut self,
        committed: CommittedPlan,
        claim: DirectiveClaim,
        gateway: &mut WorkspaceGateway,
        authorizations: RuntimeAuthorizations<'_>,
        snapshot_id: SnapshotId,
        settlement_ids: SettlementIds,
        timing: MaterializationTiming,
    ) -> Result<RuntimeOutcome, RuntimeError> {
        if claim.plan_id() != committed.plan.id() {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidInput,
                "claim names another plan",
            ));
        }
        let effect = execute_plan(
            &committed.plan,
            self.component_store,
            gateway,
            authorizations.patch,
            authorizations.candidate,
            authorizations.actions,
            snapshot_id,
            self.manifest_store,
            timing.started_at_millis,
            timing.completed_at_millis,
        );
        match effect {
            Ok(result) => {
                self.settle_success(committed, claim, settlement_ids, result.into_receipt())
            }
            Err(error) => self.settle_failure(committed, claim, settlement_ids, timing, &error),
        }
    }

    fn settle_success(
        &mut self,
        committed: CommittedPlan,
        claim: DirectiveClaim,
        ids: SettlementIds,
        receipt: MaterializationReceipt,
    ) -> Result<RuntimeOutcome, RuntimeError> {
        let command = settlement_command(
            &committed.state,
            ids,
            HarnessCommandKind::RecordMaterialization { receipt: receipt.clone() },
        )?;
        let transition = decide(Some(&committed.state), &command).map_err(aggregate)?;
        let settlement_batch =
            commit_harness_settlement(self.journal, &command, &transition, claim)
                .map_err(settlement)?;
        let (_, state) = transition.into_parts();
        Ok(RuntimeOutcome::Completed {
            receipt,
            state,
            planning: committed.evidence,
            settlement_batch,
        })
    }

    fn settle_failure(
        &mut self,
        committed: CommittedPlan,
        claim: DirectiveClaim,
        ids: SettlementIds,
        timing: MaterializationTiming,
        error: &MaterializationError,
    ) -> Result<RuntimeOutcome, RuntimeError> {
        let failure = MaterializationFailure::new(
            committed.plan.id(),
            committed.plan.digest(),
            failure_code(error),
            peritus_codec::sha256(error.detail().as_bytes()),
            timing.completed_at_millis,
            ids.event,
        );
        let command = settlement_command(
            &committed.state,
            ids,
            HarnessCommandKind::RecordMaterializationFailure { failure: failure.clone() },
        )?;
        let transition = decide(Some(&committed.state), &command).map_err(aggregate)?;
        let settlement_batch =
            commit_harness_settlement(self.journal, &command, &transition, claim)
                .map_err(settlement)?;
        let (_, state) = transition.into_parts();
        Ok(RuntimeOutcome::Failed {
            failure,
            state,
            planning: committed.evidence,
            settlement_batch,
        })
    }
}

fn settlement_command(
    state: &HarnessState,
    ids: SettlementIds,
    kind: HarnessCommandKind,
) -> Result<HarnessCommand, RuntimeError> {
    HarnessCommand::new(
        ids.command,
        ids.event,
        state.harness_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        kind,
    )
    .map_err(aggregate)
}

fn retained_receipt<'a>(
    state: &'a HarnessState,
    plan: &MaterializationPlan,
) -> Option<&'a MaterializationReceipt> {
    let receipt_id = plan.prior_receipt()?;
    let receipt = state.receipt(receipt_id)?;
    if [receipt.revision_digest() != plan.revision_digest(), receipt.after() != plan.target()]
        .into_iter()
        .any(core::convert::identity)
    {
        return None;
    }
    let installs = plan
        .operations()
        .iter()
        .filter_map(|operation| match operation {
            PlannedFileOperation::Install { path, artifact_digest, byte_length, mode, .. } => {
                Some((path, artifact_digest, byte_length, mode))
            }
            PlannedFileOperation::Delete { .. } => None,
        })
        .collect::<Vec<_>>();
    if installs.len() != plan.operations().len() || installs.len() != receipt.files().len() {
        return None;
    }
    installs
        .iter()
        .zip(receipt.files())
        .all(|(planned, observed)| {
            planned.0 == observed.path()
                && planned.1 == &observed.digest()
                && planned.2 == &observed.byte_length()
                && planned.3 == &observed.mode()
        })
        .then_some(receipt)
}

const fn failure_code(error: &MaterializationError) -> MaterializationFailureCode {
    match (error.kind(), error.recovery()) {
        (MaterializationErrorKind::Artifact, MaterializationRecovery::Quarantine) => {
            MaterializationFailureCode::ArtifactMismatch
        }
        (MaterializationErrorKind::Artifact, _) => MaterializationFailureCode::ArtifactUnavailable,
        (MaterializationErrorKind::StaleWorkspace, _) => MaterializationFailureCode::StaleWorkspace,
        (MaterializationErrorKind::Workspace, MaterializationRecovery::Reauthorize) => {
            MaterializationFailureCode::AuthorizationRejected
        }
        (
            MaterializationErrorKind::Workspace,
            MaterializationRecovery::Reconcile | MaterializationRecovery::Quarantine,
        ) => MaterializationFailureCode::Indeterminate,
        (MaterializationErrorKind::Workspace, _) => MaterializationFailureCode::CandidateRejected,
        (MaterializationErrorKind::Patch, _) => MaterializationFailureCode::PatchRejected,
        (MaterializationErrorKind::Receipt, _) => MaterializationFailureCode::ManifestFinalization,
        _ => MaterializationFailureCode::Conflict,
    }
}

fn aggregate(error: impl fmt::Display) -> RuntimeError {
    RuntimeError::new(RuntimeErrorKind::Aggregate, error.to_string())
}
fn durability(error: impl fmt::Display) -> RuntimeError {
    RuntimeError::new(RuntimeErrorKind::Durability, error.to_string())
}
fn settlement(error: impl fmt::Display) -> RuntimeError {
    RuntimeError::new(RuntimeErrorKind::Settlement, error.to_string())
}
