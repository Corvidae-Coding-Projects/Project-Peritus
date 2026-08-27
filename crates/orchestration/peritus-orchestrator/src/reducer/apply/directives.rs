//! Commit-before-effect directive publication and acknowledgement.

use peritus_types::EventId;

use crate::directive::{DirectivePayloadBinding, directive_payload_digest};
use crate::state::mutation::{self, CounterKind};
use crate::{
    ActivePhase, ChildAggregateKind, ChildObservation, DirectiveDestination, DirectiveKind,
    KernelAcceptanceOutcome, OrchestratorError, OrchestratorEventKind, OrchestratorPhase,
    OrchestratorState, PendingDirective,
};

use crate::reducer::{binding_error, illegal};

pub(super) fn publish(
    state: &mut OrchestratorState,
    event_id: EventId,
    directive: &PendingDirective,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    directive.validate()?;
    if matches!(state.phase(), OrchestratorPhase::Paused(_))
        && directive.kind() != DirectiveKind::PauseChildren
    {
        return Err(illegal("paused orchestration may commit only child-pause directives"));
    }
    if let Some(existing) = state.pending_directive() {
        if existing != directive {
            return Err(binding_error("another exact directive is already pending"));
        }
        mutation::pending_directive_mut(state)
            .ok_or_else(|| illegal("pending directive disappeared"))?
            .mark_published()?;
    } else {
        if directive.source_event() != event_id || !matches_phase(state, directive) {
            return Err(binding_error("new directive differs from its phase, handoff, or source"));
        }
        let mut published = directive.clone();
        published.mark_published()?;
        mutation::increment_counter(state, CounterKind::ChildDirectives)?;
        mutation::set_pending_directive(state, Some(published));
    }
    Ok(OrchestratorEventKind::DirectivePublished { directive: directive.clone() })
}

pub(super) fn acknowledge(
    state: &mut OrchestratorState,
    id: crate::DirectiveId,
) -> Result<OrchestratorEventKind, OrchestratorError> {
    let directive = mutation::pending_directive_mut(state)
        .ok_or_else(|| illegal("no directive is pending acknowledgement"))?;
    if directive.id() != id {
        return Err(binding_error("acknowledgement names another directive"));
    }
    directive.acknowledge()?;
    let kind = directive.kind();
    let destination = directive.destination();
    if state.phase() == OrchestratorPhase::Active(ActivePhase::GatesPending) {
        mutation::increment_counter(state, CounterKind::GateCycles)?;
        mutation::insert_active_child(state, ChildAggregateKind::Gates);
        mutation::set_phase(state, OrchestratorPhase::Active(ActivePhase::GatesActive));
    } else if state.phase() == OrchestratorPhase::Active(ActivePhase::KernelAcceptancePending) {
        if matches!(
            kind,
            DirectiveKind::BeginKernelAcceptance | DirectiveKind::EvaluateKernelAcceptance
        ) {
            mutation::insert_active_child(state, ChildAggregateKind::Kernel);
        }
    } else if matches!(state.phase(), OrchestratorPhase::Paused(_))
        && kind == DirectiveKind::PauseChildren
    {
        let child = child_for_destination(destination)
            .ok_or_else(|| binding_error("pause directive destination is not a child"))?;
        if !state.active_children().contains(&child) || state.paused_children().contains(&child) {
            return Err(binding_error("pause acknowledgement names unowned or settled child"));
        }
        mutation::insert_paused_child(state, child);
        mutation::set_pending_directive(state, None);
    } else if matches!(state.phase(), OrchestratorPhase::Active(_))
        && state.paused_reconciliation().is_some()
        && kind == DirectiveKind::ResumeChildren
    {
        let child = child_for_destination(destination)
            .ok_or_else(|| binding_error("resume directive destination is not a child"))?;
        if !state.active_children().contains(&child) || !mutation::remove_paused_child(state, child)
        {
            return Err(binding_error("resume acknowledgement names unowned or active child"));
        }
        mutation::set_pending_directive(state, None);
        if state.paused_children().is_empty() {
            mutation::set_paused_reconciliation(state, None);
        }
    }
    Ok(OrchestratorEventKind::DirectiveAcknowledged { directive_id: id })
}

fn matches_phase(state: &OrchestratorState, directive: &PendingDirective) -> bool {
    if directive.revision() != state.current_candidate().revision() {
        return false;
    }
    if matches!(state.phase(), OrchestratorPhase::Active(_))
        && let Some(reconciliation) = state.paused_reconciliation()
    {
        return directive.kind() == DirectiveKind::ResumeChildren
            && child_for_destination(directive.destination()).is_some_and(|child| {
                state.active_children().contains(&child) && state.paused_children().contains(&child)
            })
            && payload_matches(directive, DirectivePayloadBinding::Reconciliation(reconciliation));
    }
    match state.phase() {
        OrchestratorPhase::Active(ActivePhase::WriterPending) => {
            matches_handoff(state, directive, DirectiveKind::StartWriter)
        }
        OrchestratorPhase::Active(ActivePhase::GatesPending) => {
            directive.kind() == DirectiveKind::StartGates
                && directive.destination() == DirectiveDestination::Gates
                && directive.task_id().is_none()
                && payload_matches(
                    directive,
                    DirectivePayloadBinding::QualityCycle(state.current_quality_cycle()),
                )
        }
        OrchestratorPhase::Active(ActivePhase::ReviewPending) => {
            matches_handoff(state, directive, DirectiveKind::StartReview)
        }
        OrchestratorPhase::Active(ActivePhase::FixerPending) => {
            matches_handoff(state, directive, DirectiveKind::StartFixer)
        }
        OrchestratorPhase::Active(ActivePhase::EvaluatingAcceptance) => {
            if d3_active(state) {
                finalize_children(state, directive)
            } else {
                directive.kind() == DirectiveKind::EvaluateAcceptance
                    && directive.destination() == DirectiveDestination::QualityEvaluator
                    && directive.task_id().is_none()
            }
        }
        OrchestratorPhase::Active(ActivePhase::RevisionAdvancing) => {
            d3_active(state) && finalize_children(state, directive)
        }
        OrchestratorPhase::Active(ActivePhase::KernelAcceptancePending) => {
            kernel_matches(state, directive)
        }
        OrchestratorPhase::Paused(_) => {
            directive.kind() == DirectiveKind::PauseChildren
                && child_for_destination(directive.destination()).is_some_and(|child| {
                    state.active_children().contains(&child)
                        && !state.paused_children().contains(&child)
                })
                && state.paused_reconciliation().is_some_and(|reconciliation| {
                    payload_matches(
                        directive,
                        DirectivePayloadBinding::Reconciliation(reconciliation),
                    )
                })
        }
        OrchestratorPhase::Cancelling => {
            directive.kind() == DirectiveKind::CancelChildren
                && state.cancellation_cause().is_some_and(|cause| {
                    payload_matches(directive, DirectivePayloadBinding::Cancellation(cause))
                })
                && child_for_destination(directive.destination())
                    .is_some_and(|child| state.active_children().contains(&child))
        }
        OrchestratorPhase::Active(
            ActivePhase::WriterActive
            | ActivePhase::GatesActive
            | ActivePhase::ReviewActive
            | ActivePhase::FixerActive,
        )
        | OrchestratorPhase::Terminal => false,
    }
}

fn matches_handoff(
    state: &OrchestratorState,
    directive: &PendingDirective,
    kind: DirectiveKind,
) -> bool {
    state.open_handoff().is_some_and(|handoff| {
        directive.kind() == kind
            && directive.destination() == DirectiveDestination::Collaboration
            && directive.task_id() == Some(handoff.task_id())
            && directive.work_id() == Some(handoff.work_id())
            && payload_matches(directive, DirectivePayloadBinding::Handoff(handoff))
    })
}

fn kernel_matches(state: &OrchestratorState, directive: &PendingDirective) -> bool {
    let Some(certificate) = state.acceptance_certificate() else {
        return false;
    };
    let plan = certificate.kernel_plan();
    let begun = state.children().iter().any(|child| {
        matches!(
            child,
            ChildObservation::KernelAcceptance(observation)
                if observation.outcome() == KernelAcceptanceOutcome::Begun
                    && observation.event_id() == plan.begin_event_id()
                    && observation.command_id() == plan.begin_command_id()
                    && observation.run_id() == state.binding().run_id()
                    && observation.revision() == state.current_candidate().revision()
        )
    });
    let (event_id, payload) = match directive.kind() {
        DirectiveKind::BeginKernelAcceptance if !begun => {
            (plan.begin_event_id(), certificate.begin_payload_digest())
        }
        DirectiveKind::EvaluateKernelAcceptance if begun => {
            (plan.evaluate_event_id(), certificate.evaluate_payload_digest())
        }
        _ => return false,
    };
    directive.destination() == DirectiveDestination::Kernel
        && directive.id().as_bytes() == event_id.as_bytes()
        && directive.payload_digest() == payload
        && directive.task_id().is_none()
}

fn d3_active(state: &OrchestratorState) -> bool {
    state.active_children().contains(&ChildAggregateKind::Scheduler)
        || state.active_children().contains(&ChildAggregateKind::Collaboration)
}

fn finalize_children(state: &OrchestratorState, directive: &PendingDirective) -> bool {
    directive.kind() == DirectiveKind::FinalizeChildren
        && directive.destination() == DirectiveDestination::Collaboration
        && directive.task_id().is_none()
        && payload_matches(
            directive,
            DirectivePayloadBinding::QualityCycle(state.current_quality_cycle()),
        )
}

fn payload_matches(directive: &PendingDirective, binding: DirectivePayloadBinding<'_>) -> bool {
    directive_payload_digest(directive.kind(), directive.destination(), binding)
        .is_ok_and(|digest| digest == directive.payload_digest())
}

pub(super) const fn child_for_destination(
    destination: DirectiveDestination,
) -> Option<ChildAggregateKind> {
    match destination {
        DirectiveDestination::Scheduler => Some(ChildAggregateKind::Scheduler),
        DirectiveDestination::Collaboration => Some(ChildAggregateKind::Collaboration),
        DirectiveDestination::Agent => Some(ChildAggregateKind::Agent),
        DirectiveDestination::Gates => Some(ChildAggregateKind::Gates),
        DirectiveDestination::Review => Some(ChildAggregateKind::Review),
        DirectiveDestination::Kernel => Some(ChildAggregateKind::Kernel),
        DirectiveDestination::QualityEvaluator => None,
    }
}
