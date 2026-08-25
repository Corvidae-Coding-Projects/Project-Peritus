//! Exact D3 activation and writer/gate/review/fixer observation transitions.

use crate::state::mutation::{self, CounterKind};
use crate::{
    ActivePhase, ChildAggregateKind, ChildObservation, DirectiveDeliveryState, DirectiveKind,
    FixerCompletion, GateObservationClass, Handoff, HandoffActivationObservation, HandoffKind,
    OrchestratorError, OrchestratorEventKind, OrchestratorPhase, OrchestratorState,
    ReviewObservationClass, TerminalCause,
};

use super::{acceptance::set_terminal, record_observation};
use crate::reducer::{binding_error, illegal};

pub(super) fn observe_activation(
    state: &mut OrchestratorState,
    activation: &HandoffActivationObservation,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let handoff = state.open_handoff().ok_or_else(|| illegal("no handoff awaits D3 activation"))?;
    let expected_phase = OrchestratorPhase::Active(handoff.destination_phase());
    let pending = state.pending_directive().ok_or_else(|| illegal("activation lacks directive"))?;
    if state.phase() != expected_phase
        || pending.delivery_state() == DirectiveDeliveryState::Ready
        || pending.task_id() != Some(handoff.task_id())
        || pending.work_id() != Some(handoff.work_id())
        || !activation_matches(state, handoff, activation)
    {
        return Err(binding_error("D3 activation differs from current handoff or directive"));
    }
    let active = match handoff.kind() {
        HandoffKind::Writer => ActivePhase::WriterActive,
        HandoffKind::Reviewer => ActivePhase::ReviewActive,
        HandoffKind::Fixer => ActivePhase::FixerActive,
    };
    let child = if handoff.kind() == HandoffKind::Reviewer {
        ChildAggregateKind::Review
    } else {
        ChildAggregateKind::Agent
    };
    match handoff.kind() {
        HandoffKind::Writer => mutation::increment_counter(state, CounterKind::WriterCycles)?,
        HandoffKind::Reviewer => mutation::increment_counter(state, CounterKind::ReviewCycles)?,
        HandoffKind::Fixer => mutation::increment_counter(state, CounterKind::FixerCycles)?,
    }
    mutation::increment_counter(state, CounterKind::RetainedObservations)?;
    mutation::push_activation(state, activation.clone());
    for kind in [ChildAggregateKind::Scheduler, ChildAggregateKind::Collaboration, child] {
        mutation::insert_active_child(state, kind);
    }
    mutation::set_pending_directive(state, None);
    mutation::set_phase(state, OrchestratorPhase::Active(active));
    Ok(OrchestratorEventKind::HandoffActivated { activation: activation.clone() })
}

pub(super) fn observe_writer(
    state: &mut OrchestratorState,
    observation: &crate::AgentChildObservation,
    candidate: Option<&crate::CandidateBinding>,
    quality_cycle: Option<&crate::QualityCycleBinding>,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let handoff = state.open_handoff().ok_or_else(|| illegal("writer has no open handoff"))?;
    if state.phase() != OrchestratorPhase::Active(ActivePhase::WriterActive)
        || handoff.kind() != HandoffKind::Writer
        || !agent_matches(state, handoff, observation)
    {
        return Err(binding_error("writer observation differs from active handoff"));
    }
    let completed = observation.is_completed();
    record_observation(state, ChildObservation::Agent(observation.clone()))?;
    mutation::remove_active_child(state, ChildAggregateKind::Agent);
    mutation::set_open_handoff(state, None);
    if completed {
        let candidate = candidate
            .ok_or_else(|| illegal("completed writer observation lacks its material candidate"))?;
        let quality_cycle = quality_cycle
            .ok_or_else(|| illegal("completed writer observation lacks its rebound child cycle"))?;
        candidate.validate(state.limits())?;
        quality_cycle.validate_for_candidate(candidate)?;
        let writer = state.ownership().writer().actor();
        let producer =
            candidate.producer_actors().binary_search(&writer).ok().is_some_and(|index| {
                Some(candidate.producer_ancestries()[index]) == observation.proposal_digest()
            });
        let prior_cycle = state.current_quality_cycle();
        let cycle_rebound = state.candidate_history().len() == 1
            && state.quality_cycle_history().len() == 1
            && quality_cycle.gate_run_id() == prior_cycle.gate_run_id()
            && quality_cycle.scheduler_run_id() == prior_cycle.scheduler_run_id()
            && quality_cycle.collaboration_run_id() == prior_cycle.collaboration_run_id()
            && quality_cycle.scheduler_id() == prior_cycle.scheduler_id()
            && quality_cycle.scheduler_binding_digest() == prior_cycle.scheduler_binding_digest()
            && quality_cycle.collaboration_id() == prior_cycle.collaboration_id()
            && quality_cycle.collaboration_binding_digest()
                == prior_cycle.collaboration_binding_digest();
        if candidate.revision() != state.current_candidate().revision()
            || candidate.reuses_material(state.current_candidate())
            || !producer
            || !cycle_rebound
        {
            return Err(binding_error(
                "writer candidate or rebound child cycle differs from exact D0/D3 provenance",
            ));
        }
        mutation::install_writer_candidate(state, candidate.clone(), quality_cycle.clone());
        mutation::set_phase(state, OrchestratorPhase::Active(ActivePhase::GatesPending));
    } else {
        if candidate.is_some() || quality_cycle.is_some() {
            return Err(illegal(
                "failed writer observation cannot install a candidate or child cycle",
            ));
        }
        set_terminal(state, TerminalCause::WriterFailed, observation.head().state_digest())?;
    }
    Ok(OrchestratorEventKind::WriterObserved {
        observation: observation.clone(),
        candidate: candidate.cloned(),
        quality_cycle: quality_cycle.cloned(),
    })
}

pub(super) fn observe_gates(
    state: &mut OrchestratorState,
    observation: &crate::GateChildObservation,
    review_handoff: Option<&Handoff>,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    if state.phase() != OrchestratorPhase::Active(ActivePhase::GatesActive)
        || observation.run_id() != state.binding().run_id()
        || observation.gate_run_id() != state.current_quality_cycle().gate_run_id()
        || observation.revision() != state.current_quality_cycle().revision()
        || observation.plan_digest() != state.current_quality_cycle().gate_plan_digest()
        || observation.snapshot_digest() != state.current_candidate().quality_snapshot_digest()
    {
        return Err(binding_error("gate observation differs from exact current child cycle"));
    }
    record_observation(state, ChildObservation::Gates(observation.clone()))?;
    mutation::remove_active_child(state, ChildAggregateKind::Gates);
    mutation::set_pending_directive(state, None);
    match observation.class() {
        GateObservationClass::Passed => {
            let handoff =
                review_handoff.ok_or_else(|| illegal("gate pass requires review handoff"))?;
            admit_handoff(state, handoff, HandoffKind::Reviewer)?;
            mutation::set_phase(state, OrchestratorPhase::Active(ActivePhase::ReviewPending));
        }
        GateObservationClass::CandidateFailed => {
            set_terminal(state, TerminalCause::GateCandidateFailed, observation.evidence_digest())?;
        }
        GateObservationClass::InfrastructureFailed | GateObservationClass::Cancelled => {
            set_terminal(
                state,
                TerminalCause::GateInfrastructureFailed,
                observation.evidence_digest(),
            )?;
        }
        GateObservationClass::Indeterminate => {
            set_terminal(state, TerminalCause::ChildAmbiguous, observation.evidence_digest())?;
        }
    }
    Ok(OrchestratorEventKind::GatesObserved {
        observation: observation.clone(),
        review_handoff: review_handoff.cloned(),
    })
}

pub(super) fn observe_review(
    state: &mut OrchestratorState,
    observation: &crate::ReviewChildObservation,
    fixer_handoff: Option<&Handoff>,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let handoff = state.open_handoff().ok_or_else(|| illegal("review has no active handoff"))?;
    if state.phase() != OrchestratorPhase::Active(ActivePhase::ReviewActive)
        || handoff.kind() != HandoffKind::Reviewer
        || observation.run_id() != state.binding().run_id()
        || observation.revision() != state.current_candidate().revision()
        || observation.binding_digest() != state.current_quality_cycle().review_binding_digest()
    {
        return Err(binding_error("review observation differs from current handoff or D2 binding"));
    }
    record_observation(state, ChildObservation::Review(observation.clone()))?;
    mutation::set_open_handoff(state, None);
    mutation::set_pending_directive(state, None);
    match observation.class() {
        ReviewObservationClass::NeedsFix => {
            let fixer =
                fixer_handoff.ok_or_else(|| illegal("needs-fix review requires handoff"))?;
            if fixer.blocking_findings() != observation.unconserved_findings() {
                return Err(binding_error("fixer handoff does not conserve exact review findings"));
            }
            admit_handoff(state, fixer, HandoffKind::Fixer)?;
            mutation::set_phase(state, OrchestratorPhase::Active(ActivePhase::FixerPending));
        }
        ReviewObservationClass::Completed => {
            if fixer_handoff.is_some() {
                return Err(illegal("completed review cannot create a fixer handoff"));
            }
            mutation::remove_active_child(state, ChildAggregateKind::Review);
            mutation::set_phase(
                state,
                OrchestratorPhase::Active(ActivePhase::EvaluatingAcceptance),
            );
        }
        ReviewObservationClass::NeedsHuman => {
            mutation::remove_active_child(state, ChildAggregateKind::Review);
            set_terminal(
                state,
                TerminalCause::ReviewNeedsHuman,
                observation.head().state_digest(),
            )?;
        }
        ReviewObservationClass::Failed | ReviewObservationClass::Cancelled => {
            mutation::remove_active_child(state, ChildAggregateKind::Review);
            set_terminal(state, TerminalCause::ReviewFailed, observation.head().state_digest())?;
        }
    }
    Ok(OrchestratorEventKind::ReviewObserved {
        observation: observation.clone(),
        fixer_handoff: fixer_handoff.cloned(),
    })
}

pub(super) fn observe_fixer(
    state: &mut OrchestratorState,
    completion: &FixerCompletion,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let handoff = state.open_handoff().ok_or_else(|| illegal("fixer has no active handoff"))?;
    if state.phase() != OrchestratorPhase::Active(ActivePhase::FixerActive)
        || !agent_matches(state, handoff, completion.observation())
    {
        return Err(binding_error("fixer result differs from active handoff"));
    }
    completion.validate(handoff)?;
    record_observation(state, ChildObservation::Agent(completion.observation().clone()))?;
    if let Some(review) = completion.review_observation() {
        if review.run_id() != state.binding().run_id()
            || review.revision() != state.current_candidate().revision()
            || review.binding_digest() != state.current_quality_cycle().review_binding_digest()
        {
            return Err(binding_error("durable D2 fixer response differs from current review"));
        }
        record_observation(state, ChildObservation::ReviewFixer(review.clone()))?;
    }
    mutation::remove_active_child(state, ChildAggregateKind::Agent);
    mutation::set_open_handoff(state, None);
    if let Some(candidate) = completion.proposed_candidate() {
        candidate.validate(state.limits())?;
        if !crate::state::revision_successor(
            state.current_candidate().revision(),
            candidate.revision(),
        ) || !candidate.producer_actors().contains(&state.ownership().fixer().actor())
        {
            return Err(binding_error("fixer proposal is stale or lacks fixer provenance"));
        }
        mutation::set_proposed_candidate(state, Some(candidate.clone()));
        mutation::set_phase(state, OrchestratorPhase::Active(ActivePhase::RevisionAdvancing));
    } else {
        set_terminal(
            state,
            TerminalCause::FixerFailed,
            completion.observation().head().state_digest(),
        )?;
    }
    Ok(OrchestratorEventKind::FixerObserved { completion: completion.clone() })
}

pub(super) fn observe_infrastructure(
    state: &mut OrchestratorState,
    scheduler: &crate::SchedulerChildObservation,
    collaboration: &crate::CollaborationChildObservation,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let directive = state
        .pending_directive()
        .ok_or_else(|| illegal("D3 quiescence lacks finalize-children directive"))?;
    let cycle = state.current_quality_cycle();
    if !matches!(
        state.phase(),
        OrchestratorPhase::Active(
            ActivePhase::EvaluatingAcceptance | ActivePhase::RevisionAdvancing
        )
    ) || directive.kind() != DirectiveKind::FinalizeChildren
        || directive.delivery_state() != DirectiveDeliveryState::Acknowledged
        || scheduler.run_id() != cycle.scheduler_run_id()
        || collaboration.run_id() != cycle.collaboration_run_id()
        || scheduler.revision() != cycle.revision()
        || collaboration.revision() != cycle.revision()
        || scheduler.head().terminal().is_none()
        || collaboration.head().terminal().is_none()
    {
        return Err(binding_error("D3 quiescence differs from current exact child cycle"));
    }
    let scheduler_terminal = scheduler.head().terminal();
    let collaboration_terminal = collaboration.head().terminal();
    record_observation(state, ChildObservation::Scheduler(scheduler.clone()))?;
    record_observation(state, ChildObservation::Collaboration(collaboration.clone()))?;
    if !mutation::remove_active_child(state, ChildAggregateKind::Scheduler)
        || !mutation::remove_active_child(state, ChildAggregateKind::Collaboration)
    {
        return Err(binding_error("D3 quiescence names children not owned by E0"));
    }
    mutation::set_pending_directive(state, None);
    if scheduler_terminal != Some(crate::ChildTerminalClass::Completed)
        || collaboration_terminal != Some(crate::ChildTerminalClass::Completed)
    {
        let ambiguous = [scheduler_terminal, collaboration_terminal]
            .iter()
            .any(|terminal| matches!(terminal, Some(crate::ChildTerminalClass::Indeterminate)));
        let cause = if ambiguous {
            TerminalCause::ChildAmbiguous
        } else {
            TerminalCause::CoordinationFailed
        };
        let digest = if scheduler_terminal == Some(crate::ChildTerminalClass::Completed) {
            collaboration.head().state_digest()
        } else {
            scheduler.head().state_digest()
        };
        set_terminal(state, cause, digest)?;
    }
    Ok(OrchestratorEventKind::RoleInfrastructureObserved {
        scheduler: scheduler.clone(),
        collaboration: collaboration.clone(),
    })
}

fn admit_handoff(
    state: &mut OrchestratorState,
    handoff: &Handoff,
    expected: HandoffKind,
) -> Result<(), OrchestratorError> {
    handoff.validate(state.limits())?;
    let role_exact = match expected {
        HandoffKind::Writer => false,
        HandoffKind::Reviewer => {
            handoff.source_actor() == state.ownership().service_actor()
                && state.ownership().reviewer(handoff.destination_actor()).is_some()
        }
        HandoffKind::Fixer => {
            state.ownership().reviewer(handoff.source_actor()).is_some()
                && handoff.destination_actor() == state.ownership().fixer().actor()
        }
    };
    if handoff.kind() != expected
        || !role_exact
        || !handoff.candidate().materially_equal(state.current_candidate())
        || state.handoffs().iter().any(|prior| prior.id() == handoff.id())
    {
        return Err(binding_error("handoff role, candidate, actor, or identity is invalid"));
    }
    mutation::increment_counter(state, CounterKind::Handoffs)?;
    mutation::push_handoff(state, handoff.clone());
    mutation::set_open_handoff(state, Some(handoff.clone()));
    Ok(())
}

fn activation_matches(
    state: &OrchestratorState,
    handoff: &Handoff,
    activation: &HandoffActivationObservation,
) -> bool {
    let cycle = state.current_quality_cycle();
    [
        activation.handoff_id() == handoff.id(),
        activation.task_id() == handoff.task_id(),
        activation.work_id() == handoff.work_id(),
        activation.owner() == handoff.destination_actor(),
        handoff.destination_role().harness_role() == Some(activation.role()),
        activation.scheduler_run_id() == cycle.scheduler_run_id(),
        activation.collaboration_run_id() == cycle.collaboration_run_id(),
        activation.revision() == cycle.revision(),
        !state.activations().iter().any(|prior| prior.handoff_id() == activation.handoff_id()),
    ]
    .into_iter()
    .all(|exact| exact)
}

fn agent_matches(
    state: &OrchestratorState,
    handoff: &Handoff,
    observation: &crate::AgentChildObservation,
) -> bool {
    [
        observation.handoff_id() == handoff.id(),
        observation.task_id() == handoff.task_id(),
        observation.work_id() == handoff.work_id(),
        observation.run_id() == state.binding().run_id(),
        observation.actor() == handoff.destination_actor(),
        observation.attempt_id() == state.binding().attempt_id(),
        observation.revision() == state.current_candidate().revision(),
        Some(observation.turn_id()) == handoff.turn_id(),
        handoff.destination_role().harness_role() == Some(observation.role()),
        state.activations().iter().any(|activation| activation.handoff_id() == handoff.id()),
    ]
    .into_iter()
    .all(|exact| exact)
}
