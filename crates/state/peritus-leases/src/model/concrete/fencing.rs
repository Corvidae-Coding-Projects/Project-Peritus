//! Executable fencing and reconciliation refinement predicates.

use vstd::prelude::*;

verus! {

/// Exact fieldwise identity for untrusted reconciliation correlation echoes.
pub(crate) open spec fn concrete_correlation_matches(
    left: crate::ReconciliationCorrelation,
    right: crate::ReconciliationCorrelation,
) -> bool {
    super::concrete_scope_matches(left.scope, right.scope)
        && left.fenced_generation.spec_value() == right.fenced_generation.spec_value()
        && super::concrete_holder_matches(left.prior_holder, right.prior_holder)
}

/// Whether a typed executable transition record is one of the complete fencing paths.
pub(crate) open spec fn concrete_fencing_kind(kind: crate::LeaseTransitionKind) -> bool {
    matches!(
        kind,
        crate::LeaseTransitionKind::ReleasedAvailable
            | crate::LeaseTransitionKind::ReleasedReconciling
            | crate::LeaseTransitionKind::Expired
            | crate::LeaseTransitionKind::HolderLost
            | crate::LeaseTransitionKind::ClockDiscontinuity
            | crate::LeaseTransitionKind::Revoked
            | crate::LeaseTransitionKind::Retired(_)
    )
}

/// Concrete fencing relation that invalidates the old active representation before takeover.
pub(crate) open spec fn concrete_fence_edge(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
) -> bool {
    super::concrete_record_matches(before, after, record)
        && super::concrete_refines_reachability_step(before, after)
        && concrete_fencing_kind(record.kind)
        && matches!(before.state, crate::state::LeaseState::Active(_))
        && !matches!(after.state, crate::state::LeaseState::Active(_))
}

/// Exact cause, kind, correlation, and command selected by a successful fencing reducer.
pub(crate) open spec fn concrete_fence_decision(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
    command_id: peritus_types::CommandId,
    normal_kind: crate::LeaseTransitionKind,
    cause: Option<crate::FenceCause>,
) -> bool {
    concrete_fence_edge(before, after, record)
        && record.command_id == command_id
        && match (before.state, after.state) {
            (crate::state::LeaseState::Active(_), crate::state::LeaseState::Available) => {
                before.version.spec_value() + 1 < (u64::MAX - 1) as int
                    && before.generation.spec_value() < u64::MAX as int
                    && after.generation.spec_value() == before.generation.spec_value() + 1
                    && cause.is_none()
                    && record.kind == normal_kind
            }
            (
                crate::state::LeaseState::Active(active),
                crate::state::LeaseState::Reconciling(reconciling),
            ) => {
                    before.version.spec_value() + 1 < (u64::MAX - 1) as int
                    && before.generation.spec_value() < u64::MAX as int
                    && after.generation.spec_value() == before.generation.spec_value() + 1
                    && cause == Some(reconciling.cause)
                    && record.kind == normal_kind
                    && reconciling.correlation.spec_scope() == before.scope
                    && reconciling.correlation.spec_fenced_generation() == before.generation
                    && reconciling.correlation.spec_prior_holder() == active.holder
            }
            (
                crate::state::LeaseState::Active(_),
                crate::state::LeaseState::Retired(reason),
            ) => {
                after.generation.spec_value() == before.generation.spec_value()
                    && match reason {
                        crate::RetirementReason::VersionExhausted => {
                            before.version.spec_value() + 1 >= (u64::MAX - 1) as int
                        }
                        crate::RetirementReason::GenerationExhausted => {
                            before.version.spec_value() + 1 < (u64::MAX - 1) as int
                                && before.generation.spec_value() == u64::MAX as int
                        }
                    }
                    && match record.kind {
                        crate::LeaseTransitionKind::Retired(actual) => actual == reason,
                        _ => false,
                    }
            }
            _ => false,
        }
}

/// A successful fence carries the exact accepted authority-time observation.
pub(crate) open spec fn concrete_fence_time_observed(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    observed_at: peritus_policy::AuthorityInstant,
    cause: Option<crate::FenceCause>,
) -> bool {
    if cause == Some(crate::FenceCause::ClockDiscontinuity) {
        super::concrete_discontinuity_time(before, after, observed_at)
    } else {
        super::concrete_time_observed(before, after, observed_at)
    }
}

/// Input-side authority-time condition required by the private fencing constructor.
pub(crate) open spec fn concrete_fence_time_state(
    before: &crate::LeaseAggregate,
    authority_time: peritus_policy::AuthorityTimeState,
    observed_at: peritus_policy::AuthorityInstant,
    cause: Option<crate::FenceCause>,
) -> bool {
    if cause == Some(crate::FenceCause::ClockDiscontinuity) {
        if observed_at.spec_epoch() == before.authority_time.spec_epoch() {
            authority_time == before.authority_time
                && observed_at.spec_tick_millis()
                    < before.authority_time.spec_greatest_tick_millis()
        } else {
            authority_time.spec_epoch() == observed_at.spec_epoch()
                && authority_time.spec_greatest_tick_millis()
                    == observed_at.spec_tick_millis()
        }
    } else {
        authority_time.spec_epoch() == before.authority_time.spec_epoch()
            && authority_time.spec_epoch() == observed_at.spec_epoch()
            && authority_time.spec_greatest_tick_millis()
                == observed_at.spec_tick_millis()
            && authority_time.spec_greatest_tick_millis()
                >= before.authority_time.spec_greatest_tick_millis()
    }
}

/// Exact reconciliation disposition decision and its abstract-machine refinement.
pub(crate) open spec fn concrete_reconcile_edge(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
    command: crate::ReconcileLease,
) -> bool {
    super::concrete_record_matches(before, after, record)
        && super::concrete_refines_reachability_step(before, after)
        && record.command_id == command.command_id
        && record.binding.matches_reconcile(command)
        && reconciliation_time_matches(before, after, command)
        && match (before.state, after.state) {
            (
                crate::state::LeaseState::Reconciling(previous),
                crate::state::LeaseState::Available,
            ) => {
                after.version.spec_value() < (u64::MAX - 1) as int
                    && concrete_correlation_matches(
                    command.observation.correlation,
                    previous.correlation,
                )
                    && matches!(
                        command.observation.disposition,
                        crate::ReconciliationDisposition::SafeToAcquire { .. }
                    )
                    && matches!(record.kind, crate::LeaseTransitionKind::ReconciledAvailable)
            }
            (
                crate::state::LeaseState::Reconciling(previous),
                crate::state::LeaseState::Quarantined(quarantined),
            ) => {
                after.version.spec_value() < (u64::MAX - 1) as int
                    && concrete_correlation_matches(
                    command.observation.correlation,
                    previous.correlation,
                )
                    && command.observation.disposition == quarantined.disposition
                    && quarantined.correlation == previous.correlation
                    && quarantined.cause == previous.cause
                    && matches!(
                        command.observation.disposition,
                        crate::ReconciliationDisposition::Dirty { .. }
                            | crate::ReconciliationDisposition::Indeterminate { .. }
                    )
                    && matches!(record.kind, crate::LeaseTransitionKind::ReconciledQuarantined)
            }
            (
                crate::state::LeaseState::Reconciling(previous),
                crate::state::LeaseState::Retired(crate::RetirementReason::VersionExhausted),
            ) => {
                after.version.spec_value() >= (u64::MAX - 1) as int
                    && concrete_correlation_matches(
                    command.observation.correlation,
                    previous.correlation,
                )
                    && matches!(
                        record.kind,
                        crate::LeaseTransitionKind::Retired(
                            crate::RetirementReason::VersionExhausted,
                        )
                    )
            }
            _ => false,
        }
}

/// Establishes the opaque public reconciliation contract from the exact internal edge.
pub(crate) proof fn establish_reconcile_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::ReconcileLease,
)
    requires concrete_reconcile_edge(before, &transition.next, transition.record, command),
    ensures concrete_reconcile_transition(before, transition, command),
{
}

/// Public-contract wrapper for an accepted move-only reconciliation transition.
pub closed spec fn concrete_reconcile_transition(
    before: &crate::LeaseAggregate,
    transition: &crate::LeaseTransition,
    command: crate::ReconcileLease,
) -> bool {
    concrete_reconcile_edge(before, &transition.next, transition.record, command)
}

pub(crate) open spec fn reconciliation_time_matches(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    command: crate::ReconcileLease,
) -> bool {
    match before.state {
        crate::state::LeaseState::Reconciling(previous)
            if previous.cause == crate::FenceCause::ClockDiscontinuity
                && command.observed_at.spec_epoch() != before.authority_time.spec_epoch() => {
                    after.authority_time.spec_epoch() == command.observed_at.spec_epoch()
                        && after.authority_time.spec_greatest_tick_millis()
                            == command.observed_at.spec_tick_millis()
                }
        _ => super::concrete_time_observed(before, after, command.observed_at),
    }
}

/// Projects terminal retirement from a successful boundary fencing decision.
pub(crate) proof fn boundary_fence_is_terminal(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
    command_id: peritus_types::CommandId,
    normal_kind: crate::LeaseTransitionKind,
    cause: Option<crate::FenceCause>,
)
    requires
        concrete_fence_decision(before, after, record, command_id, normal_kind, cause),
        after.version.spec_value() >= (u64::MAX - 1) as int,
    ensures matches!(
        after.state,
        crate::state::LeaseState::Retired(crate::RetirementReason::VersionExhausted),
    ),
{
}

/// Projects rejection of a pre-fence claim from the concrete fencing edge.
pub(crate) proof fn fence_invalidates_claim(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
    previous_claim: crate::LeaseClaim,
)
    requires
        super::concrete_claim_is_current(before, previous_claim),
        concrete_fence_edge(before, after, record),
    ensures !super::concrete_claim_is_current(after, previous_claim),
{
}

/// Establishes one concrete kind as a fencing path.
pub(crate) proof fn establish_fencing_kind(kind: crate::LeaseTransitionKind)
    requires matches!(
        kind,
        crate::LeaseTransitionKind::ReleasedAvailable
            | crate::LeaseTransitionKind::ReleasedReconciling
            | crate::LeaseTransitionKind::Expired
            | crate::LeaseTransitionKind::HolderLost
            | crate::LeaseTransitionKind::ClockDiscontinuity
            | crate::LeaseTransitionKind::Revoked
            | crate::LeaseTransitionKind::Retired(_)
    ),
    ensures concrete_fencing_kind(kind),
{
}

/// Projects absence of active authority after exact reconciliation.
pub(crate) proof fn reconcile_is_not_active(
    before: &crate::LeaseAggregate,
    after: &crate::LeaseAggregate,
    record: crate::LeaseTransitionRecord,
    command: crate::ReconcileLease,
)
    requires concrete_reconcile_edge(before, after, record, command),
    ensures !matches!(after.state, crate::state::LeaseState::Active(_)),
{
}

} // verus!
