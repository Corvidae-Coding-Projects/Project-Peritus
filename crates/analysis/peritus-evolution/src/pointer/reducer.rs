//! Pure production-pointer decision, event application, and replay.

use crate::{
    ActivationKind, EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    PendingActivation, PointerCommand, PointerCommandKind, PointerEvent, PointerEventKind,
    PointerPhase, PointerTransition, ProductionHarnessState,
};
use peritus_types::Sha256Digest;

use super::state::activation_record;

/// Decides one pointer command without effects.
///
/// # Errors
/// Rejects stale fences, illegal pending transitions, baseline drift, policy drift, unknown
/// rollback targets, mismatched authority, and generation overflow.
pub fn decide_pointer(
    prior: Option<&ProductionHarnessState>,
    command: &PointerCommand,
) -> Result<PointerTransition, EvolutionError> {
    validate_fence(prior, command)?;
    let sequence = command.expected_sequence().checked_add(1).ok_or_else(transition)?;
    let mut state = apply_kind(
        prior,
        command.project_id(),
        sequence,
        command.event_id(),
        command.policy_digest(),
        command.kind(),
    )?;
    state.refresh_digest();
    let event = PointerEvent::from_replay_parts(
        command.event_id(),
        command.command_id(),
        command.project_id(),
        sequence,
        command.expected_head(),
        command.expected_generation(),
        state.generation(),
        command.prior_state_digest(),
        command.policy_digest(),
        command.digest(),
        state.state_digest(),
        PointerEventKind::Accepted(command.kind().clone()),
    );
    Ok(PointerTransition::new(event, state))
}

/// Applies one persisted pointer event to its exact predecessor.
///
/// # Errors
/// Rejects gaps, baseline/generation drift, illegal semantics, or successor disagreement.
pub fn apply_pointer_event(
    prior: Option<&ProductionHarnessState>,
    event: &PointerEvent,
) -> Result<ProductionHarnessState, EvolutionError> {
    let expected_sequence = prior.map_or(1, |state| state.sequence().saturating_add(1));
    let expected_head = prior.map(ProductionHarnessState::last_event);
    let expected_generation = prior.map_or(0, ProductionHarnessState::generation);
    let expected_digest =
        prior.map_or(Sha256Digest::new([0; 32]), ProductionHarnessState::state_digest);
    if event.sequence() != expected_sequence
        || event.previous_event() != expected_head
        || event.prior_generation() != expected_generation
        || event.prior_state_digest() != expected_digest
        || prior.is_some_and(|state| {
            state.project_id() != event.project_id()
                || state.policy().digest() != event.policy_digest()
        })
    {
        return Err(stale());
    }
    let PointerEventKind::Accepted(kind) = event.kind();
    let mut state = apply_kind(
        prior,
        event.project_id(),
        event.sequence(),
        event.id(),
        event.policy_digest(),
        kind,
    )?;
    state.refresh_digest();
    if state.generation() != event.successor_generation()
        || state.state_digest() != event.successor_state_digest()
    {
        return Err(corrupt("pointer event successor differs from pure replay"));
    }
    Ok(state)
}

/// Folds one nonempty contiguous pointer history.
///
/// # Errors
/// Returns the first invalid event or rejects an empty history.
pub fn replay_pointer(events: &[PointerEvent]) -> Result<ProductionHarnessState, EvolutionError> {
    let mut state = None;
    for event in events {
        state = Some(apply_pointer_event(state.as_ref(), event)?);
    }
    state.ok_or_else(|| corrupt("pointer replay history is empty"))
}

fn validate_fence(
    prior: Option<&ProductionHarnessState>,
    command: &PointerCommand,
) -> Result<(), EvolutionError> {
    match prior {
        None if command.expected_sequence() == 0
            && command.expected_head().is_none()
            && command.expected_generation() == 0
            && command.prior_state_digest() == Sha256Digest::new([0; 32])
            && matches!(command.kind(), PointerCommandKind::InitializeProductionHarness { .. }) =>
        {
            Ok(())
        }
        Some(state)
            if command.expected_sequence() == state.sequence()
                && command.expected_head() == Some(state.last_event())
                && command.expected_generation() == state.generation()
                && command.prior_state_digest() == state.state_digest()
                && command.project_id() == state.project_id()
                && command.policy_digest() == state.policy().digest()
                && !matches!(
                    command.kind(),
                    PointerCommandKind::InitializeProductionHarness { .. }
                ) =>
        {
            Ok(())
        }
        _ => Err(stale()),
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one exhaustive closed pointer command table keeps transition semantics auditable"
)]
fn apply_kind(
    prior: Option<&ProductionHarnessState>,
    project_id: peritus_types::ProjectId,
    sequence: u64,
    event_id: peritus_types::EventId,
    policy_digest: Sha256Digest,
    kind: &PointerCommandKind,
) -> Result<ProductionHarnessState, EvolutionError> {
    if let PointerCommandKind::InitializeProductionHarness {
        initial,
        policy,
        limits,
        evidence_artifact,
        evidence_digest,
    } = kind
    {
        if prior.is_some()
            || sequence != 1
            || policy.digest() != policy_digest
            || policy.production_revision() != initial.harness_revision()
        {
            return Err(binding("initial pointer and protected policy binding differ"));
        }
        let record = activation_record(
            ActivationKind::Initialization,
            1,
            None,
            *initial,
            None,
            initial.digest(),
            None,
            *evidence_artifact,
            *evidence_digest,
            None,
        );
        return Ok(ProductionHarnessState {
            project_id,
            current: *initial,
            policy: policy.clone(),
            limits: *limits,
            generation: 1,
            sequence,
            last_event: event_id,
            state_digest: Sha256Digest::new([0; 32]),
            phase: PointerPhase::Active,
            history: vec![record],
            pending: None,
        });
    }
    let mut state = prior.cloned().ok_or_else(transition)?;
    state.sequence = sequence;
    state.last_event = event_id;
    match kind {
        PointerCommandKind::InitializeProductionHarness { .. } => return Err(transition()),
        PointerCommandKind::PreparePromotion(proposal) => {
            require_active(&state)?;
            if proposal.project_id() != state.project_id
                || proposal.current() != state.current
                || proposal.policy_digest() != state.policy.digest()
            {
                return Err(binding("promotion proposal lost the current pointer or policy fence"));
            }
            state.pending = Some(PendingActivation::Promotion(proposal.clone()));
            state.phase = PointerPhase::PromotionPending;
        }
        PointerCommandKind::ActivatePromotion {
            promotion_id,
            campaign_terminal_digest,
            authorization,
        } => {
            if state.phase != PointerPhase::PromotionPending {
                return Err(transition());
            }
            let PendingActivation::Promotion(proposal) =
                state.pending.as_ref().ok_or_else(transition)?
            else {
                return Err(transition());
            };
            if proposal.id() != *promotion_id
                || proposal.current() != state.current
                || authorization.action_digest() != proposal.digest()
            {
                return Err(binding(
                    "promotion activation differs from prepared action or authority",
                ));
            }
            let predecessor = state.current;
            let successor = proposal.candidate();
            let generation = state.generation.checked_add(1).ok_or_else(transition)?;
            let record = activation_record(
                ActivationKind::Promotion,
                generation,
                Some(predecessor),
                successor,
                Some(proposal.campaign_id()),
                proposal.digest(),
                Some(*authorization),
                proposal.evidence_bundle_artifact(),
                *campaign_terminal_digest,
                None,
            );
            state.current = successor;
            state.generation = generation;
            append_history(&mut state, record);
            state.pending = None;
            state.phase = PointerPhase::Active;
        }
        PointerCommandKind::PrepareRollback(proposal) => {
            require_active(&state)?;
            if proposal.project_id() != state.project_id
                || proposal.current() != state.current
                || proposal.policy_digest() != state.policy.digest()
                || !state.history.iter().any(|value| {
                    value.id() == proposal.target_activation()
                        && value.successor() == proposal.target()
                })
            {
                return Err(binding("rollback proposal lost its retained target or current fence"));
            }
            state.pending = Some(PendingActivation::Rollback(proposal.clone()));
            state.phase = PointerPhase::RollbackPending;
        }
        PointerCommandKind::ActivateRollback { rollback_id, authorization } => {
            if state.phase != PointerPhase::RollbackPending {
                return Err(transition());
            }
            let PendingActivation::Rollback(proposal) =
                state.pending.as_ref().ok_or_else(transition)?
            else {
                return Err(transition());
            };
            if proposal.id() != *rollback_id
                || proposal.current() != state.current
                || authorization.action_digest() != proposal.digest()
                || !state.history.iter().any(|value| {
                    value.id() == proposal.target_activation()
                        && value.successor() == proposal.target()
                })
            {
                return Err(binding(
                    "rollback activation differs from prepared action or authority",
                ));
            }
            let predecessor = state.current;
            let successor = proposal.target();
            let generation = state.generation.checked_add(1).ok_or_else(transition)?;
            let record = activation_record(
                ActivationKind::Rollback,
                generation,
                Some(predecessor),
                successor,
                None,
                proposal.digest(),
                Some(*authorization),
                proposal.evidence_bundle_artifact(),
                proposal.compatibility_evidence_digest(),
                Some(proposal.rollback_of()),
            );
            state.current = successor;
            state.generation = generation;
            append_history(&mut state, record);
            state.pending = None;
            state.phase = PointerPhase::Active;
        }
        PointerCommandKind::CancelPending { .. } => {
            if state.phase == PointerPhase::Active || state.pending.is_none() {
                return Err(transition());
            }
            state.pending = None;
            state.phase = PointerPhase::Active;
        }
    }
    Ok(state)
}

fn append_history(state: &mut ProductionHarnessState, record: crate::ActivationRecord) {
    let limit = usize::from(state.limits.activation_history());
    if state.history.len() == limit {
        state.history.remove(0);
    }
    state.history.push(record);
}

fn require_active(state: &ProductionHarnessState) -> Result<(), EvolutionError> {
    if state.phase == PointerPhase::Active && state.pending.is_none() {
        Ok(())
    } else {
        Err(transition())
    }
}

const fn transition() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::IllegalTransition,
        EvolutionOperation::TransitionPointer,
        EvolutionRecovery::CorrectInput,
        "pointer command is illegal in the current phase",
    )
}
const fn stale() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::StaleState,
        EvolutionOperation::TransitionPointer,
        EvolutionRecovery::RefreshState,
        "pointer command or event fence is stale",
    )
}
const fn binding(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::TransitionPointer,
        EvolutionRecovery::CorrectInput,
        detail,
    )
}
const fn corrupt(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Corruption,
        EvolutionOperation::TransitionPointer,
        EvolutionRecovery::Quarantine,
        detail,
    )
}
