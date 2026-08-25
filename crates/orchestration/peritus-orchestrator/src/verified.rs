//! Executable E0 invariants and focused Verus proof roots.

use vstd::prelude::*;

use crate::{
    ChildObservation, KernelAcceptanceOutcome, OrchestratorPhase, OrchestratorState,
    OrchestratorTerminalKind, OrchestratorTransition, TerminalCause,
};

/// Returns whether the service, mutating agents, and reviewers remain pairwise separated.
#[must_use]
pub fn roles_are_separated(state: &OrchestratorState) -> bool {
    let ownership = state.ownership();
    let service = ownership.service_actor();
    let writer = ownership.writer().actor();
    let fixer = ownership.fixer().actor();
    service != writer
        && service != fixer
        && writer != fixer
        && !ownership.reviewers().is_empty()
        && ownership.reviewers().windows(2).all(|pair| pair[0].actor() < pair[1].actor())
        && ownership.reviewers().iter().all(|reviewer| {
            reviewer.actor() != service && reviewer.actor() != writer && reviewer.actor() != fixer
        })
}

/// Returns whether candidate and quality-cycle history advance together without identity reuse.
#[must_use]
pub fn candidate_cycles_are_fresh(state: &OrchestratorState) -> bool {
    let candidates = state.candidate_history();
    let cycles = state.quality_cycle_history();
    candidates.len() == cycles.len()
        && candidates
            .iter()
            .zip(cycles)
            .all(|(candidate, cycle)| candidate.revision() == cycle.revision())
        && candidates.windows(2).all(|pair| {
            crate::state::revision_successor(pair[0].revision(), pair[1].revision())
                && !pair[0].materially_equal(&pair[1])
        })
        && candidates.iter().enumerate().all(|(index, candidate)| {
            candidates[..index].iter().all(|prior| {
                !prior.reuses_material(candidate) && prior.digest() != candidate.digest()
            })
        })
        && cycles.iter().enumerate().all(|(index, cycle)| {
            cycles[..index].iter().all(|prior| {
                prior.gate_run_id() != cycle.gate_run_id()
                    && prior.scheduler_run_id() != cycle.scheduler_run_id()
                    && prior.collaboration_run_id() != cycle.collaboration_run_id()
                    && prior.scheduler_id() != cycle.scheduler_id()
                    && prior.collaboration_id() != cycle.collaboration_id()
                    && prior.digest() != cycle.digest()
            })
        })
}

/// Returns whether every retained counter is within its independent immutable ceiling.
#[must_use]
pub fn counters_are_bounded(state: &OrchestratorState) -> bool {
    let counters = state.counters();
    let binding = state.binding();
    let limits = binding.limits();
    [
        counters.revisions() > 0,
        counters.revisions() <= limits.revisions(),
        counters.writer_cycles() <= limits.writer_cycles(),
        counters.fixer_cycles() <= limits.fixer_cycles(),
        counters.gate_cycles() <= binding.effective_gate_cycles(),
        counters.review_cycles() <= binding.effective_review_cycles(),
        counters.handoffs() <= limits.handoffs(),
        counters.child_directives() <= limits.child_directives(),
        counters.retained_observations() <= limits.retained_observations(),
        counters.cancellation_reconciliations() <= limits.cancellation_reconciliations(),
    ]
    .into_iter()
    .all(|bounded| bounded)
}

/// Returns whether a terminal state is quiescent and acceptance came from the planned B0 event.
#[must_use]
pub fn terminal_is_truthful(state: &OrchestratorState) -> bool {
    let Some(terminal) = state.terminal().copied() else {
        return state.phase() != OrchestratorPhase::Terminal;
    };
    if state.phase() != OrchestratorPhase::Terminal
        || terminal.revision() != state.current_candidate().revision()
        || !state.active_children().is_empty()
        || state.pending_directive().is_some()
    {
        return false;
    }
    if terminal.kind() != OrchestratorTerminalKind::Accepted {
        return true;
    }
    let Some(certificate) = state.acceptance_certificate() else {
        return false;
    };
    let plan = certificate.kernel_plan();
    let begun = state.children().iter().any(|child| {
        matches!(
            child,
            ChildObservation::KernelAcceptance(observation)
                if observation.outcome() == KernelAcceptanceOutcome::Begun
                    && observation.run_id() == state.binding().run_id()
                    && observation.revision() == certificate.revision()
                    && observation.command_id() == plan.begin_command_id()
                    && observation.event_id() == plan.begin_event_id()
                    && observation.previous_event_id() == plan.expected_previous_kernel_event()
        )
    });
    let accepted = state.children().iter().any(|child| {
        matches!(
            child,
            ChildObservation::KernelAcceptance(observation)
                if observation.outcome() == KernelAcceptanceOutcome::Accepted
                    && observation.run_id() == state.binding().run_id()
                    && observation.revision() == certificate.revision()
                    && observation.command_id() == plan.evaluate_command_id()
                    && observation.event_id() == plan.evaluate_event_id()
                    && observation.previous_event_id() == Some(plan.evaluate_previous_event_id())
        )
    });
    terminal.cause() == TerminalCause::KernelAccepted
        && terminal.cause_digest() == certificate.digest()
        && begun
        && accepted
}

/// Returns whether cancellation/settlement state can never itself imply acceptance.
#[must_use]
pub fn cancellation_dominates(state: &OrchestratorState) -> bool {
    if state.phase() == OrchestratorPhase::Cancelling {
        state.terminal().is_none()
            && state
                .pending_terminal()
                .is_none_or(|terminal| terminal.kind() != OrchestratorTerminalKind::Accepted)
    } else {
        state.terminal().is_none_or(|terminal| {
            terminal.kind() != OrchestratorTerminalKind::Cancelled
                || terminal.cause() == TerminalCause::CancellationReconciled
        })
    }
}

/// Returns exact complete-state replay equivalence.
#[must_use]
pub fn replay_equivalent(expected: &OrchestratorState, observed: &OrchestratorState) -> bool {
    expected == observed && expected.state_digest() == observed.state_digest()
}

/// Returns whether one accepted transition carries every sequence and digest fence exactly once.
#[must_use]
pub fn transition_is_legal(prior: &OrchestratorState, transition: &OrchestratorTransition) -> bool {
    let event = transition.event();
    let successor = transition.state();
    prior.phase() != OrchestratorPhase::Terminal
        && event.run_id() == prior.binding().run_id()
        && event.previous_event() == Some(prior.last_event_id())
        && event.prior_state_digest() == prior.state_digest()
        && event.sequence().get() == prior.sequence().get().saturating_add(1)
        && successor.sequence() == event.sequence()
        && successor.last_event_id() == event.id()
        && successor.state_digest() == event.successor_state_digest()
        && successor.used_commands().last() == Some(&event.command_id())
}

verus! {

/// Mathematical exact actor-role separation.
pub open spec fn exact_role_separation(
    service_writer_distinct: bool,
    service_fixer_distinct: bool,
    writer_fixer_distinct: bool,
    reviewers_distinct: bool,
) -> bool {
    service_writer_distinct && service_fixer_distinct
        && writer_fixer_distinct && reviewers_distinct
}

/// Mathematical material revision freshness.
pub open spec fn fresh_candidate_cycle(
    revision_advanced: bool,
    material_changed: bool,
    gate_run_fresh: bool,
    scheduler_run_fresh: bool,
    collaboration_run_fresh: bool,
) -> bool {
    revision_advanced && material_changed && gate_run_fresh
        && scheduler_run_fresh && collaboration_run_fresh
}

/// Mathematical independently bounded counter.
pub open spec fn bounded_counter(value: int, limit: int) -> bool {
    0 <= value && value <= limit
}

/// Mathematical single pending-directive ownership.
pub open spec fn unique_pending_directive(pending: int, owners: int) -> bool {
    0 <= pending && pending <= 1 && (pending == 0 || owners == 1)
}

/// Mathematical cancellation dominance over later acceptance.
pub open spec fn cancellation_dominance(
    cancelling: bool,
    accepted_late_result: bool,
    published_accepted: bool,
) -> bool {
    !cancelling || (!accepted_late_result && !published_accepted)
}

/// Mathematical truthful terminal quiescence.
pub open spec fn truthful_terminal(
    terminal: bool,
    active_children: int,
    pending_directives: int,
    exact_revision: bool,
) -> bool {
    !terminal
        || (active_children == 0 && pending_directives == 0 && exact_revision)
}

/// Mathematical B0-only acceptance authority.
pub open spec fn exact_acceptance_chain(
    accepted: bool,
    certificate_exact: bool,
    begin_observed: bool,
    evaluate_observed: bool,
    durable_b0_accepted: bool,
) -> bool {
    !accepted
        || (certificate_exact && begin_observed && evaluate_observed && durable_b0_accepted)
}

/// Mathematical exact replay equivalence.
pub open spec fn exact_replay(expected: int, observed: int, claimed: bool) -> bool {
    !claimed || expected == observed
}

/// Mathematical one-event reducer fence.
#[allow(clippy::too_many_arguments)]
pub open spec fn legal_reducer_step(
    open: bool,
    sequence: int,
    expected_sequence: int,
    predecessor_matches: bool,
    revision_matches: bool,
    digest_matches: bool,
    command_fresh: bool,
    event_count: int,
    successor_sequence: int,
) -> bool {
    open && sequence == expected_sequence && predecessor_matches && revision_matches
        && digest_matches && command_fresh && event_count == 1
        && successor_sequence == sequence + 1
}

/// Proves any reused D3 child run invalidates a claimed fresh candidate cycle.
pub proof fn reused_child_run_breaks_freshness(
    revision_advanced: bool,
    material_changed: bool,
    gate_run_fresh: bool,
    collaboration_run_fresh: bool,
)
    ensures !fresh_candidate_cycle(
        revision_advanced,
        material_changed,
        gate_run_fresh,
        false,
        collaboration_run_fresh,
    )
{
}

/// Proves one actor cannot occupy both writer and fixer assignments.
pub proof fn writer_fixer_alias_breaks_separation(
    service_writer_distinct: bool,
    service_fixer_distinct: bool,
    reviewers_distinct: bool,
)
    ensures !exact_role_separation(
        service_writer_distinct,
        service_fixer_distinct,
        false,
        reviewers_distinct,
    )
{
}

/// Proves a counter above its immutable limit is not bounded.
pub proof fn counter_over_limit_is_rejected(value: int, limit: int)
    requires value > limit
    ensures !bounded_counter(value, limit)
{
}

/// Proves two pending directives violate unique outbox ownership.
pub proof fn duplicate_pending_directive_is_rejected(owners: int)
    ensures !unique_pending_directive(2, owners)
{
}

/// Proves cancellation cannot publish a late accepted result.
pub proof fn cancelled_run_cannot_accept_late_success()
    ensures !cancellation_dominance(true, true, true)
{
}

/// Proves a terminal claim with a live child is untruthful.
pub proof fn live_child_blocks_terminal(children: int, pending: int, exact_revision: bool)
    requires children > 0
    ensures !truthful_terminal(true, children, pending, exact_revision)
{
}

/// Proves a local certificate without durable B0 truth cannot accept.
pub proof fn certificate_alone_cannot_accept(
    begin_observed: bool,
    evaluate_observed: bool,
)
    ensures !exact_acceptance_chain(true, true, begin_observed, evaluate_observed, false)
{
}

/// Proves evaluation cannot replace the planned durable begin observation.
pub proof fn evaluation_without_begin_cannot_accept(
    certificate_exact: bool,
    evaluate_observed: bool,
    durable_b0_accepted: bool,
)
    ensures !exact_acceptance_chain(
        true,
        certificate_exact,
        false,
        evaluate_observed,
        durable_b0_accepted,
    )
{
}

/// Proves a claimed replay cannot differ from the expected semantic state.
pub proof fn replay_claim_requires_equality(expected: int, observed: int)
    requires expected != observed
    ensures !exact_replay(expected, observed, true)
{
}

/// Proves a reducer step cannot claim two emitted events.
pub proof fn reducer_step_emits_exactly_one_event(
    open: bool,
    sequence: int,
    expected_sequence: int,
    predecessor_matches: bool,
    revision_matches: bool,
    digest_matches: bool,
    command_fresh: bool,
    successor_sequence: int,
)
    ensures !legal_reducer_step(
        open,
        sequence,
        expected_sequence,
        predecessor_matches,
        revision_matches,
        digest_matches,
        command_fresh,
        2,
        successor_sequence,
    )
{
}

} // verus!
