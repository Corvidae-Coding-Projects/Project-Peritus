//! Executable/refinement predicates for G0 owner invariants.

use peritus_app_protocol::DaemonReadiness;
use vstd::prelude::*;

use crate::StartupPhase;

verus! {

/// Closed formal model of the A3 readiness vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadinessModel {
    /// Startup or recovery is incomplete.
    Starting,
    /// Mutation and diagnostics are available.
    ReadyReadWrite,
    /// Only diagnostic requests are available.
    ReadyReadOnly,
    /// Existing work is draining and diagnostics remain available.
    Draining,
    /// No application surface is available.
    Unavailable,
}

/// Closed formal model of the G0 startup phase vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupPhaseModel {
    /// Validate configuration and roots.
    Validate,
    /// Acquire exclusive ownership.
    Lock,
    /// Reconcile and apply migrations.
    Migrate,
    /// Open and verify C0.
    Journal,
    /// Recover artifact state.
    Artifacts,
    /// Recover evidence state.
    Evidence,
    /// Rebuild or validate projections.
    Projections,
    /// Allocate the authority epoch.
    AuthorityEpoch,
    /// Recover domain aggregates.
    DomainRecovery,
    /// Reconcile external effects.
    EffectRecovery,
    /// Recover application sessions and commands.
    AppRecovery,
    /// Resume durable outbox delivery.
    Outbox,
    /// Publish the authenticated local endpoint.
    Ipc,
    /// Publish final readiness.
    Ready,
}

/// Mathematical mutation-admission rule.
pub open spec fn mutation_admitted(readiness: ReadinessModel) -> bool {
    readiness == ReadinessModel::ReadyReadWrite
}

/// Mathematical diagnostic-admission rule.
pub open spec fn diagnostic_admitted(readiness: ReadinessModel) -> bool {
    readiness == ReadinessModel::ReadyReadWrite
        || readiness == ReadinessModel::ReadyReadOnly
        || readiness == ReadinessModel::Draining
}

/// Mathematical exact-successor rule for the closed startup sequence.
pub open spec fn startup_transition(current: StartupPhaseModel, next: StartupPhaseModel) -> bool {
    matches!((current, next),
        (StartupPhaseModel::Validate, StartupPhaseModel::Lock)
        | (StartupPhaseModel::Lock, StartupPhaseModel::Migrate)
        | (StartupPhaseModel::Migrate, StartupPhaseModel::Journal)
        | (StartupPhaseModel::Journal, StartupPhaseModel::Artifacts)
        | (StartupPhaseModel::Artifacts, StartupPhaseModel::Evidence)
        | (StartupPhaseModel::Evidence, StartupPhaseModel::Projections)
        | (StartupPhaseModel::Projections, StartupPhaseModel::AuthorityEpoch)
        | (StartupPhaseModel::AuthorityEpoch, StartupPhaseModel::DomainRecovery)
        | (StartupPhaseModel::DomainRecovery, StartupPhaseModel::EffectRecovery)
        | (StartupPhaseModel::EffectRecovery, StartupPhaseModel::AppRecovery)
        | (StartupPhaseModel::AppRecovery, StartupPhaseModel::Outbox)
        | (StartupPhaseModel::Outbox, StartupPhaseModel::Ipc)
        | (StartupPhaseModel::Ipc, StartupPhaseModel::Ready)
    )
}

/// Executable mutation-admission predicate refining `mutation_admitted`.
#[must_use]
pub const fn mutation_admitted_model_exec(readiness: ReadinessModel) -> (result: bool)
    ensures result == mutation_admitted(readiness)
{
    matches!(readiness, ReadinessModel::ReadyReadWrite)
}

/// Executable diagnostic-admission predicate refining `diagnostic_admitted`.
#[must_use]
pub const fn diagnostic_admitted_model_exec(readiness: ReadinessModel) -> (result: bool)
    ensures result == diagnostic_admitted(readiness)
{
    matches!(
        readiness,
        ReadinessModel::ReadyReadWrite | ReadinessModel::ReadyReadOnly | ReadinessModel::Draining
    )
}

/// Executable startup-order predicate refining `startup_transition`.
#[must_use]
pub const fn startup_transition_model_exec(
    current: StartupPhaseModel,
    next: StartupPhaseModel,
) -> (result: bool)
    ensures result == startup_transition(current, next)
{
    matches!((current, next),
        (StartupPhaseModel::Validate, StartupPhaseModel::Lock)
        | (StartupPhaseModel::Lock, StartupPhaseModel::Migrate)
        | (StartupPhaseModel::Migrate, StartupPhaseModel::Journal)
        | (StartupPhaseModel::Journal, StartupPhaseModel::Artifacts)
        | (StartupPhaseModel::Artifacts, StartupPhaseModel::Evidence)
        | (StartupPhaseModel::Evidence, StartupPhaseModel::Projections)
        | (StartupPhaseModel::Projections, StartupPhaseModel::AuthorityEpoch)
        | (StartupPhaseModel::AuthorityEpoch, StartupPhaseModel::DomainRecovery)
        | (StartupPhaseModel::DomainRecovery, StartupPhaseModel::EffectRecovery)
        | (StartupPhaseModel::EffectRecovery, StartupPhaseModel::AppRecovery)
        | (StartupPhaseModel::AppRecovery, StartupPhaseModel::Outbox)
        | (StartupPhaseModel::Outbox, StartupPhaseModel::Ipc)
        | (StartupPhaseModel::Ipc, StartupPhaseModel::Ready)
    )
}

} // verus!

/// Applies the verified mutation-admission predicate to the exact A3 readiness value.
#[must_use]
pub const fn mutation_admitted_exec(readiness: DaemonReadiness) -> bool {
    mutation_admitted_model_exec(readiness_model(readiness))
}

/// Applies the verified diagnostic-admission predicate to the exact A3 readiness value.
#[must_use]
pub const fn diagnostic_admitted_exec(readiness: DaemonReadiness) -> bool {
    diagnostic_admitted_model_exec(readiness_model(readiness))
}

/// Applies the verified startup relation to the exact runtime phases.
#[must_use]
pub const fn startup_transition_exec(current: StartupPhase, next: StartupPhase) -> bool {
    startup_transition_model_exec(startup_phase_model(current), startup_phase_model(next))
}

const fn readiness_model(readiness: DaemonReadiness) -> ReadinessModel {
    match readiness {
        DaemonReadiness::Starting => ReadinessModel::Starting,
        DaemonReadiness::ReadyReadWrite => ReadinessModel::ReadyReadWrite,
        DaemonReadiness::ReadyReadOnly => ReadinessModel::ReadyReadOnly,
        DaemonReadiness::Draining => ReadinessModel::Draining,
        DaemonReadiness::Unavailable => ReadinessModel::Unavailable,
    }
}

const fn startup_phase_model(phase: StartupPhase) -> StartupPhaseModel {
    match phase {
        StartupPhase::Validate => StartupPhaseModel::Validate,
        StartupPhase::Lock => StartupPhaseModel::Lock,
        StartupPhase::Migrate => StartupPhaseModel::Migrate,
        StartupPhase::Journal => StartupPhaseModel::Journal,
        StartupPhase::Artifacts => StartupPhaseModel::Artifacts,
        StartupPhase::Evidence => StartupPhaseModel::Evidence,
        StartupPhase::Projections => StartupPhaseModel::Projections,
        StartupPhase::AuthorityEpoch => StartupPhaseModel::AuthorityEpoch,
        StartupPhase::DomainRecovery => StartupPhaseModel::DomainRecovery,
        StartupPhase::EffectRecovery => StartupPhaseModel::EffectRecovery,
        StartupPhase::AppRecovery => StartupPhaseModel::AppRecovery,
        StartupPhase::Outbox => StartupPhaseModel::Outbox,
        StartupPhase::Ipc => StartupPhaseModel::Ipc,
        StartupPhase::Ready => StartupPhaseModel::Ready,
    }
}

#[cfg(test)]
mod tests {
    use peritus_app_protocol::DaemonReadiness;

    use super::{diagnostic_admitted_exec, mutation_admitted_exec, startup_transition_exec};
    use crate::StartupPhase;

    #[test]
    fn executable_readiness_predicates_match_the_closed_admission_matrix() {
        for (readiness, mutation, diagnostic) in [
            (DaemonReadiness::Starting, false, false),
            (DaemonReadiness::ReadyReadWrite, true, true),
            (DaemonReadiness::ReadyReadOnly, false, true),
            (DaemonReadiness::Draining, false, true),
            (DaemonReadiness::Unavailable, false, false),
        ] {
            assert_eq!(mutation_admitted_exec(readiness), mutation);
            assert_eq!(diagnostic_admitted_exec(readiness), diagnostic);
        }
    }

    #[test]
    fn executable_startup_relation_admits_only_exact_successors() {
        let phases = [
            StartupPhase::Validate,
            StartupPhase::Lock,
            StartupPhase::Migrate,
            StartupPhase::Journal,
            StartupPhase::Artifacts,
            StartupPhase::Evidence,
            StartupPhase::Projections,
            StartupPhase::AuthorityEpoch,
            StartupPhase::DomainRecovery,
            StartupPhase::EffectRecovery,
            StartupPhase::AppRecovery,
            StartupPhase::Outbox,
            StartupPhase::Ipc,
            StartupPhase::Ready,
        ];
        for (index, current) in phases.into_iter().enumerate() {
            for (candidate_index, candidate) in phases.into_iter().enumerate() {
                assert_eq!(
                    startup_transition_exec(current, candidate),
                    candidate_index == index.saturating_add(1) && index + 1 < phases.len(),
                );
            }
        }
    }
}
