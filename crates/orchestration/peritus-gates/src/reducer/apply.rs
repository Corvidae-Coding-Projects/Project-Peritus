//! Closed command application separated from replay/fence orchestration.

use crate::error::{GateError, GateRejection, reject};
use crate::state::mutation;
use crate::{
    GateCommandKind, GateEventKind, GateOutcomeKind, GatePlan, GateResumePhase, GateRunPhase,
    GateRunState, GateSlotPhase, RecoveryDisposition, RetryPermission,
};

use super::{
    dependencies_satisfied, finalize, illegal, require_active, require_attempt, slot, slot_mut,
    unknown_gate,
};

pub fn apply(
    plan: &GatePlan,
    state: &mut GateRunState,
    event_id: peritus_types::EventId,
    command: &GateCommandKind,
) -> Result<GateEventKind, GateError> {
    match command {
        GateCommandKind::StartRun { .. } => Err(illegal("run already started")),
        GateCommandKind::PrepareAttempt { gate_id, attempt } => {
            prepare_attempt(plan, state, *gate_id, *attempt)
        }
        GateCommandKind::MarkDispatched { gate_id, execution_id } => {
            require_active(state)?;
            let slot = slot_mut(state, *gate_id)?;
            require_attempt(slot, GateSlotPhase::Prepared, *execution_id)?;
            mutation::dispatch(slot);
            Ok(GateEventKind::AttemptDispatched { gate_id: *gate_id, execution_id: *execution_id })
        }
        GateCommandKind::ObserveResult { gate_id, execution_id, result } => {
            let phase = state.phase();
            if !matches!(phase, GateRunPhase::Active | GateRunPhase::Cancelling) {
                return Err(illegal("result cannot be observed after terminal aggregation"));
            }
            let maximum = state.maximum_attempts();
            let slot = slot_mut(state, *gate_id)?;
            require_attempt(slot, GateSlotPhase::Dispatched, *execution_id)?;
            if result.gate_id() != *gate_id {
                return Err(reject(
                    GateRejection::IdentityMismatch,
                    "normalized result belongs to another gate",
                ));
            }
            let next = result_phase(phase, slot.attempts(), maximum, result);
            mutation::observe(slot, result.clone(), event_id, next);
            Ok(GateEventKind::ResultObserved {
                gate_id: *gate_id,
                execution_id: *execution_id,
                result: result.clone(),
            })
        }
        GateCommandKind::ClassifyRecovery { gate_id, execution_id, disposition } => {
            let run_phase = state.phase();
            if !matches!(run_phase, GateRunPhase::Active | GateRunPhase::Cancelling) {
                return Err(illegal("recovery cannot be classified while the gate run is paused"));
            }
            let maximum = state.maximum_attempts();
            let slot = slot_mut(state, *gate_id)?;
            if !matches!(slot.phase(), GateSlotPhase::Dispatched | GateSlotPhase::RecoveryPending)
                || slot.active_attempt().is_none_or(|active| active.execution_id() != *execution_id)
            {
                return Err(reject(
                    GateRejection::IdentityMismatch,
                    "recovery phase or active execution identity differs",
                ));
            }
            let phase = recovery_phase(run_phase, slot.attempts(), maximum, *disposition);
            mutation::recover(slot, phase);
            Ok(GateEventKind::RecoveryClassified {
                gate_id: *gate_id,
                execution_id: *execution_id,
                disposition: *disposition,
            })
        }
        GateCommandKind::PublishEvidence { gate_id, execution_id, receipt } => {
            publish_evidence(plan, state, *gate_id, *execution_id, receipt)
        }
        GateCommandKind::BeginCancellation => cancel(state),
        GateCommandKind::PauseRun | GateCommandKind::ResumeRun => apply_lifecycle(state, command),
        GateCommandKind::FinalizeRun => {
            finalize(state)?;
            Ok(GateEventKind::RunFinalized)
        }
    }
}

fn prepare_attempt(
    plan: &GatePlan,
    state: &mut GateRunState,
    gate_id: peritus_types::GateId,
    attempt: crate::ActiveAttempt,
) -> Result<GateEventKind, GateError> {
    require_active(state)?;
    let slot = slot(state, gate_id)?;
    if !matches!(slot.phase(), GateSlotPhase::Pending | GateSlotPhase::RetryPending)
        || attempt.snapshot_digest() != state.snapshot_digest()
        || attempt.ordinal().get() != slot.attempts().saturating_add(1)
        || attempt.ordinal().get() > state.maximum_attempts()
        || state.used_executions().contains(&attempt.execution_id())
        || state.used_actions().contains(&attempt.action_id())
    {
        return Err(reject(
            GateRejection::IllegalRetry,
            "attempt phase, snapshot, ordinal, limit, execution, or action identity is invalid",
        ));
    }
    if !dependencies_satisfied(plan, state, gate_id) {
        return Err(reject(
            GateRejection::DependencyUnsatisfied,
            "attempt cannot begin before all declared dependencies pass",
        ));
    }
    let index = mutation::slot_index(state, gate_id).ok_or_else(unknown_gate)?;
    mutation::prepare(state, index, attempt);
    Ok(GateEventKind::AttemptPrepared { gate_id, attempt })
}

fn publish_evidence(
    plan: &GatePlan,
    state: &mut GateRunState,
    gate_id: peritus_types::GateId,
    execution_id: peritus_types::GateExecutionId,
    receipt: &crate::GateEvidenceReceipt,
) -> Result<GateEventKind, GateError> {
    require_active(state)?;
    let planned = plan.gate(gate_id).ok_or_else(unknown_gate)?;
    let run_id = state.run_id();
    let revision = state.revision();
    let slot = slot(state, gate_id)?;
    require_attempt(slot, GateSlotPhase::EvidencePending, execution_id)?;
    let attempt = slot.active_attempt().ok_or_else(|| {
        reject(GateRejection::EvidenceInvalid, "evidence-pending gate has no active attempt")
    })?;
    let result = slot.last_result().ok_or_else(|| {
        reject(GateRejection::EvidenceInvalid, "evidence-pending gate has no passing result")
    })?;
    let result_event = slot.result_event().ok_or_else(|| {
        reject(GateRejection::EvidenceInvalid, "evidence-pending gate has no result event")
    })?;
    let expected_publication = crate::EvidencePublication::new(
        run_id,
        gate_id,
        attempt,
        revision,
        result_event,
        receipt.result_position(),
        result.tool_result_digest(),
        planned.required_evidence().to_vec(),
        result.artifacts().to_vec(),
    )?;
    let exact_requirements = receipt.evidence().len() == planned.required_evidence().len()
        && receipt
            .evidence()
            .iter()
            .zip(planned.required_evidence())
            .all(|(actual, required)| actual.requirement_id() == *required);
    if !result.passed()
        || receipt.publication() != &expected_publication
        || receipt.gate_id() != gate_id
        || receipt.execution_id() != execution_id
        || receipt.revision() != revision
        || !exact_requirements
    {
        return Err(reject(
            GateRejection::EvidenceInvalid,
            "evidence receipt is missing, stale, or bound to another passing attempt",
        ));
    }
    let slot = slot_mut(state, gate_id)?;
    mutation::publish(slot, receipt.clone());
    Ok(GateEventKind::EvidencePublished { gate_id, execution_id, receipt: receipt.clone() })
}

fn result_phase(
    run_phase: GateRunPhase,
    attempts: u16,
    maximum: u16,
    result: &crate::GateAttemptResult,
) -> GateSlotPhase {
    if run_phase == GateRunPhase::Cancelling {
        return GateSlotPhase::Cancelled;
    }
    match result.kind() {
        GateOutcomeKind::Passed => GateSlotPhase::EvidencePending,
        GateOutcomeKind::CandidateFailure => GateSlotPhase::Failed,
        _ if result.retry_permission() == RetryPermission::AfterRecovery => {
            GateSlotPhase::RecoveryPending
        }
        _ if attempts >= maximum || result.retry_permission() == RetryPermission::Never => {
            GateSlotPhase::Failed
        }
        _ => GateSlotPhase::RetryPending,
    }
}

fn recovery_phase(
    run_phase: GateRunPhase,
    attempts: u16,
    maximum: u16,
    disposition: RecoveryDisposition,
) -> GateSlotPhase {
    if run_phase == GateRunPhase::Cancelling {
        return match disposition {
            RecoveryDisposition::StillActive => GateSlotPhase::RecoveryPending,
            RecoveryDisposition::SafeToRetry | RecoveryDisposition::TerminalFailure => {
                GateSlotPhase::Cancelled
            }
        };
    }
    match disposition {
        RecoveryDisposition::SafeToRetry if attempts < maximum => GateSlotPhase::RetryPending,
        RecoveryDisposition::SafeToRetry | RecoveryDisposition::TerminalFailure => {
            GateSlotPhase::Failed
        }
        RecoveryDisposition::StillActive => GateSlotPhase::RecoveryPending,
    }
}

fn cancel(state: &mut GateRunState) -> Result<GateEventKind, GateError> {
    let begin = matches!(
        state.phase(),
        GateRunPhase::Active | GateRunPhase::Paused(GateResumePhase::Active)
    );
    mutation::set_phase(state, GateRunPhase::Cancelling);
    if begin {
        let phases =
            state.slots().iter().map(|slot| (slot.gate_id(), slot.phase())).collect::<Vec<_>>();
        for (gate_id, phase) in phases {
            if matches!(
                phase,
                GateSlotPhase::Pending
                    | GateSlotPhase::Prepared
                    | GateSlotPhase::RetryPending
                    | GateSlotPhase::EvidencePending
            ) {
                mutation::cancel(slot_mut(state, gate_id)?);
            }
        }
    }
    Ok(GateEventKind::CancellationStarted)
}

pub(super) fn apply_lifecycle(
    state: &mut GateRunState,
    command: &GateCommandKind,
) -> Result<GateEventKind, GateError> {
    match command {
        GateCommandKind::PauseRun => pause(state),
        GateCommandKind::ResumeRun => resume(state),
        _ => Err(illegal("plan-free gate transition admits only pause or resume")),
    }
}

fn pause(state: &mut GateRunState) -> Result<GateEventKind, GateError> {
    let resume_phase = match state.phase() {
        GateRunPhase::Active => GateResumePhase::Active,
        GateRunPhase::Cancelling => GateResumePhase::Cancelling,
        GateRunPhase::Paused(_) | GateRunPhase::Terminal => {
            return Err(illegal("only an unpaused nonterminal gate run can pause"));
        }
    };
    mutation::set_phase(state, GateRunPhase::Paused(resume_phase));
    Ok(GateEventKind::RunPaused { resume_phase })
}

fn resume(state: &mut GateRunState) -> Result<GateEventKind, GateError> {
    let GateRunPhase::Paused(resume_phase) = state.phase() else {
        return Err(illegal("only a paused gate run can resume"));
    };
    let phase = match resume_phase {
        GateResumePhase::Active => GateRunPhase::Active,
        GateResumePhase::Cancelling => GateRunPhase::Cancelling,
    };
    mutation::set_phase(state, phase);
    Ok(GateEventKind::RunResumed { resume_phase })
}
