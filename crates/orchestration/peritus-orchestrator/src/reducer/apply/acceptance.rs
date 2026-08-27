//! Candidate advancement, B2/B0 acceptance, pause, cancellation, and settlement.

use peritus_types::Sha256Digest;

use crate::state::mutation::{self, CounterKind};
use crate::{
    ActivePhase, ChildAggregateKind, ChildObservation, DirectiveDeliveryState, DirectiveKind,
    KernelAcceptanceOutcome, OrchestratorError, OrchestratorEventKind, OrchestratorPhase,
    OrchestratorState, OrchestratorTerminal, TerminalCause,
};

use super::{directives::child_for_destination, record_observation};
use crate::reducer::{binding_error, illegal};

pub(super) fn advance_candidate(
    state: &mut OrchestratorState,
    quality_cycle: &crate::QualityCycleBinding,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    if state.phase() != OrchestratorPhase::Active(ActivePhase::RevisionAdvancing) {
        return Err(illegal("candidate can advance only after checked fixer completion"));
    }
    let candidate = state
        .proposed_candidate()
        .cloned()
        .ok_or_else(|| illegal("revision advancement lacks a checked proposal"))?;
    if state
        .candidate_history()
        .iter()
        .any(|prior| prior.digest() == candidate.digest() || prior.reuses_material(&candidate))
    {
        return Err(binding_error("candidate advancement reuses historical material identity"));
    }
    quality_cycle.validate_for_candidate(&candidate)?;
    if quality_cycle.revision() != candidate.revision()
        || state.quality_cycle_history().iter().any(|prior| {
            prior.gate_run_id() == quality_cycle.gate_run_id()
                || prior.scheduler_run_id() == quality_cycle.scheduler_run_id()
                || prior.collaboration_run_id() == quality_cycle.collaboration_run_id()
                || prior.scheduler_id() == quality_cycle.scheduler_id()
                || prior.collaboration_id() == quality_cycle.collaboration_id()
                || prior.digest() == quality_cycle.digest()
        })
        || state.active_children().contains(&ChildAggregateKind::Scheduler)
        || state.active_children().contains(&ChildAggregateKind::Collaboration)
    {
        return Err(binding_error("new candidate cycle is stale, reused, or old D3 is active"));
    }
    mutation::increment_counter(state, CounterKind::Revisions)?;
    mutation::advance_candidate(state, candidate.clone(), quality_cycle.clone());
    mutation::set_phase(state, OrchestratorPhase::Active(ActivePhase::GatesPending));
    Ok(OrchestratorEventKind::CandidateAdvanced { candidate, quality_cycle: quality_cycle.clone() })
}

pub(super) fn record_certificate(
    state: &mut OrchestratorState,
    certificate: &crate::AcceptanceCertificate,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let directive = state
        .pending_directive()
        .ok_or_else(|| illegal("acceptance certificate lacks evaluator directive"))?;
    let last_gate = state.children().iter().rev().find_map(|child| match child {
        ChildObservation::Gates(value) => Some(value),
        _ => None,
    });
    let last_review = state.children().iter().rev().find_map(|child| match child {
        ChildObservation::Review(value) => Some(value),
        _ => None,
    });
    certificate.validate()?;
    let exact = state.phase() == OrchestratorPhase::Active(ActivePhase::EvaluatingAcceptance)
        && state.active_children().is_empty()
        && directive.kind() == DirectiveKind::EvaluateAcceptance
        && directive.delivery_state() == DirectiveDeliveryState::Acknowledged
        && directive.payload_digest() == certificate.evaluation_request_digest()
        && certificate.orchestrator_binding_digest() == state.binding().digest()
        && certificate.contract_id() == state.binding().contract_id()
        && certificate.contract_digest() == state.binding().contract_digest()
        && certificate.maximum_gate_attempts() == state.binding().contract_gate_cycles()
        && certificate.maximum_review_cycles() == state.binding().contract_review_cycles()
        && certificate.revision() == state.current_candidate().revision()
        && certificate.candidate_binding_digest() == state.current_candidate().digest()
        && last_gate
            .is_some_and(|gate| certificate.gate_state_digest() == gate.head().state_digest())
        && last_review.is_some_and(|review| {
            certificate.review_state_digest() == review.head().state_digest()
        });
    if !exact {
        return Err(binding_error("acceptance certificate differs from current B2/D1/D2 truth"));
    }
    mutation::set_pending_directive(state, None);
    mutation::set_certificate(state, Some(certificate.clone()));
    mutation::set_phase(state, OrchestratorPhase::Active(ActivePhase::KernelAcceptancePending));
    Ok(OrchestratorEventKind::AcceptanceCertificateRecorded { certificate: certificate.clone() })
}

pub(super) fn observe_kernel(
    state: &mut OrchestratorState,
    observation: crate::KernelAcceptanceObservation,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let certificate = state
        .acceptance_certificate()
        .cloned()
        .ok_or_else(|| illegal("B0 observation lacks an acceptance certificate"))?;
    let directive = state
        .pending_directive()
        .ok_or_else(|| illegal("B0 observation lacks its stage directive"))?;
    let plan = certificate.kernel_plan();
    let stage_exact = match observation.outcome() {
        KernelAcceptanceOutcome::Begun => {
            directive.kind() == DirectiveKind::BeginKernelAcceptance
                && observation.event_id() == plan.begin_event_id()
                && observation.command_id() == plan.begin_command_id()
                && observation.previous_event_id() == plan.expected_previous_kernel_event()
        }
        KernelAcceptanceOutcome::Accepted | KernelAcceptanceOutcome::NeedsChanges => {
            directive.kind() == DirectiveKind::EvaluateKernelAcceptance
                && observation.event_id() == plan.evaluate_event_id()
                && observation.command_id() == plan.evaluate_command_id()
                && observation.previous_event_id() == Some(plan.evaluate_previous_event_id())
        }
        KernelAcceptanceOutcome::Cancelled => false,
    };
    let exact = state.phase() == OrchestratorPhase::Active(ActivePhase::KernelAcceptancePending)
        && stage_exact
        && directive.delivery_state() == DirectiveDeliveryState::Acknowledged
        && observation.run_id() == state.binding().run_id()
        && observation.revision() == state.current_candidate().revision();
    if !exact {
        return Err(binding_error("B0 observation differs from certificate plan or directive"));
    }
    record_observation(state, ChildObservation::KernelAcceptance(observation))?;
    mutation::set_pending_directive(state, None);
    match observation.outcome() {
        KernelAcceptanceOutcome::Begun => {}
        KernelAcceptanceOutcome::Accepted => {
            mutation::remove_active_child(state, ChildAggregateKind::Kernel);
            set_terminal(state, TerminalCause::KernelAccepted, certificate.digest())?;
        }
        KernelAcceptanceOutcome::NeedsChanges => {
            mutation::remove_active_child(state, ChildAggregateKind::Kernel);
            set_terminal(state, TerminalCause::KernelNeedsChanges, certificate.digest())?;
        }
        KernelAcceptanceOutcome::Cancelled => {
            return Err(illegal("B0 cancellation is accepted only during settlement"));
        }
    }
    Ok(OrchestratorEventKind::KernelAcceptanceObserved { observation })
}

pub(super) fn pause(
    state: &mut OrchestratorState,
    reconciliation: &crate::ResumeReconciliation,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let OrchestratorPhase::Active(active) = state.phase() else {
        return Err(illegal("only active orchestration can pause"));
    };
    reconciliation.validate_for_state(state)?;
    let phase = OrchestratorPhase::Paused(active);
    mutation::clear_paused_children(state);
    mutation::set_paused_reconciliation(state, Some(reconciliation.clone()));
    mutation::set_phase(state, phase);
    Ok(OrchestratorEventKind::Paused { phase, reconciliation: reconciliation.clone() })
}

pub(super) fn resume(
    state: &mut OrchestratorState,
    reconciliation: &crate::ResumeReconciliation,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let OrchestratorPhase::Paused(active) = state.phase() else {
        return Err(illegal("only paused orchestration can resume"));
    };
    if state.paused_reconciliation() != Some(reconciliation) {
        return Err(binding_error("resume child heads differ from committed pause checkpoint"));
    }
    if state.paused_children() != state.active_children() {
        return Err(illegal("resume requires pause acknowledgement from every active child"));
    }
    let phase = OrchestratorPhase::Active(active);
    mutation::set_phase(state, phase);
    Ok(OrchestratorEventKind::Resumed { phase, reconciliation: reconciliation.clone() })
}

pub(super) fn cancel(
    state: &mut OrchestratorState,
    cause_digest: Sha256Digest,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    require_nonzero(cause_digest, "cancellation cause digest must be nonzero")?;
    if state.phase() == OrchestratorPhase::Cancelling {
        return Err(illegal("orchestration is already cancelling"));
    }
    if let Some(kind) = state.pending_directive().map(crate::PendingDirective::kind) {
        match kind {
            DirectiveKind::StartWriter | DirectiveKind::StartFixer => {
                for child in [
                    ChildAggregateKind::Scheduler,
                    ChildAggregateKind::Collaboration,
                    ChildAggregateKind::Agent,
                ] {
                    mutation::insert_active_child(state, child);
                }
            }
            DirectiveKind::StartReview => {
                for child in [
                    ChildAggregateKind::Scheduler,
                    ChildAggregateKind::Collaboration,
                    ChildAggregateKind::Review,
                ] {
                    mutation::insert_active_child(state, child);
                }
            }
            DirectiveKind::StartGates => {
                mutation::insert_active_child(state, ChildAggregateKind::Gates);
            }
            DirectiveKind::BeginKernelAcceptance | DirectiveKind::EvaluateKernelAcceptance => {
                mutation::insert_active_child(state, ChildAggregateKind::Kernel);
            }
            DirectiveKind::EvaluateAcceptance
            | DirectiveKind::FinalizeChildren
            | DirectiveKind::PauseChildren
            | DirectiveKind::ResumeChildren
            | DirectiveKind::CancelChildren => {}
        }
    }
    mutation::set_cancellation_cause(state, cause_digest);
    mutation::set_pending_directive(state, None);
    mutation::set_paused_reconciliation(state, None);
    mutation::clear_paused_children(state);
    mutation::set_phase(state, OrchestratorPhase::Cancelling);
    Ok(OrchestratorEventKind::CancellationRequested { cause_digest })
}

pub(super) fn reconcile(
    state: &mut OrchestratorState,
    observation: &ChildObservation,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let directive = state
        .pending_directive()
        .ok_or_else(|| illegal("cancellation reconciliation lacks a committed directive"))?;
    let terminal = match observation {
        ChildObservation::HandoffActivation(_) | ChildObservation::ReviewFixer(_) => false,
        ChildObservation::KernelAcceptance(kernel) => {
            kernel.outcome() != KernelAcceptanceOutcome::Begun
        }
        ChildObservation::CancellationClassification(classification) => {
            classification.revision() == state.current_candidate().revision()
                && classification.evidence_digest().as_bytes().iter().any(|byte| *byte != 0)
        }
        _ => observation.head().is_some_and(|head| head.terminal().is_some()),
    };
    let destination_exact =
        child_for_destination(directive.destination()) == Some(observation.aggregate());
    if state.phase() != OrchestratorPhase::Cancelling
        || directive.kind() != DirectiveKind::CancelChildren
        || directive.delivery_state() != DirectiveDeliveryState::Acknowledged
        || !destination_exact
        || !terminal
        || !mutation::remove_active_child(state, observation.aggregate())
    {
        return Err(binding_error("cancellation observation is nonterminal, stale, or unowned"));
    }
    mutation::increment_counter(state, CounterKind::CancellationReconciliations)?;
    record_observation(state, observation.clone())?;
    if let ChildObservation::CancellationClassification(classification) = observation
        && classification.kind() == crate::child::CancellationClassificationKind::Ambiguous
        && state.pending_terminal().is_none()
    {
        let terminal = OrchestratorTerminal::new(
            TerminalCause::ChildAmbiguous,
            classification.evidence_digest(),
            state.current_candidate().revision(),
        )?;
        mutation::set_pending_terminal(state, terminal);
    }
    mutation::set_pending_directive(state, None);
    Ok(OrchestratorEventKind::CancellationReconciled { observation: observation.clone() })
}

pub(super) fn finalize(
    state: &mut OrchestratorState,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    if state.phase() != OrchestratorPhase::Cancelling
        || !state.active_children().is_empty()
        || state.pending_directive().is_some()
    {
        return Err(illegal("settlement finalization requires reconciled quiescence"));
    }
    let terminal = if let Some(pending) = state.pending_terminal().copied() {
        mutation::set_terminal(state, pending);
        pending
    } else {
        let digest =
            state.cancellation_cause().ok_or_else(|| illegal("cancellation cause is absent"))?;
        let terminal = OrchestratorTerminal::new(
            TerminalCause::CancellationReconciled,
            digest,
            state.current_candidate().revision(),
        )?;
        mutation::set_terminal(state, terminal);
        terminal
    };
    Ok(OrchestratorEventKind::Finalized { terminal })
}

pub(super) fn explicit_terminal(
    state: &mut OrchestratorState,
    cause: TerminalCause,
    digest: Sha256Digest,
) -> Result<OrchestratorTerminal, OrchestratorError> {
    if state.phase() == OrchestratorPhase::Cancelling {
        return Err(illegal("explicit terminal cannot bypass settlement reconciliation"));
    }
    require_nonzero(digest, "terminal evidence digest must be nonzero")?;
    set_terminal(state, cause, digest)
}

pub(super) fn set_terminal(
    state: &mut OrchestratorState,
    cause: TerminalCause,
    digest: Sha256Digest,
) -> Result<OrchestratorTerminal, OrchestratorError> {
    let terminal = OrchestratorTerminal::new(cause, digest, state.current_candidate().revision())?;
    normalize_pending_ownership(state);
    mutation::set_pending_directive(state, None);
    if state.active_children().is_empty() {
        mutation::set_terminal(state, terminal);
    } else {
        mutation::set_cancellation_cause(state, digest);
        mutation::set_pending_terminal(state, terminal);
    }
    Ok(terminal)
}

fn normalize_pending_ownership(state: &mut OrchestratorState) {
    let Some(kind) = state.pending_directive().map(crate::PendingDirective::kind) else {
        return;
    };
    let children: &[ChildAggregateKind] = match kind {
        DirectiveKind::StartWriter | DirectiveKind::StartFixer => &[
            ChildAggregateKind::Scheduler,
            ChildAggregateKind::Collaboration,
            ChildAggregateKind::Agent,
        ],
        DirectiveKind::StartReview => &[
            ChildAggregateKind::Scheduler,
            ChildAggregateKind::Collaboration,
            ChildAggregateKind::Review,
        ],
        DirectiveKind::StartGates => &[ChildAggregateKind::Gates],
        DirectiveKind::BeginKernelAcceptance | DirectiveKind::EvaluateKernelAcceptance => {
            &[ChildAggregateKind::Kernel]
        }
        DirectiveKind::EvaluateAcceptance
        | DirectiveKind::FinalizeChildren
        | DirectiveKind::PauseChildren
        | DirectiveKind::ResumeChildren
        | DirectiveKind::CancelChildren => &[],
    };
    for child in children {
        mutation::insert_active_child(state, *child);
    }
}

fn require_nonzero(digest: Sha256Digest, detail: &'static str) -> Result<(), OrchestratorError> {
    if digest.as_bytes().iter().any(|byte| *byte != 0) {
        Ok(())
    } else {
        Err(binding_error(detail))
    }
}
