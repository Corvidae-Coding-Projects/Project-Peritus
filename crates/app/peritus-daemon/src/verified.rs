//! Executable/refinement predicates for G0 owner invariants.

use peritus_app_protocol::DaemonReadiness;
use vstd::prelude::*;

use crate::StartupPhase;

verus! {

/// Mathematical mutation-admission rule.
pub open spec fn mutation_admitted(readiness: DaemonReadiness) -> bool {
    readiness == DaemonReadiness::ReadyReadWrite
}

/// Mathematical diagnostic-admission rule.
pub open spec fn diagnostic_admitted(readiness: DaemonReadiness) -> bool {
    readiness == DaemonReadiness::ReadyReadWrite
        || readiness == DaemonReadiness::ReadyReadOnly
        || readiness == DaemonReadiness::Draining
}

/// Mathematical exact-successor rule for the closed startup sequence.
pub open spec fn startup_transition(current: StartupPhase, next: StartupPhase) -> bool {
    match current {
        StartupPhase::Validate => next == StartupPhase::Lock,
        StartupPhase::Lock => next == StartupPhase::Migrate,
        StartupPhase::Migrate => next == StartupPhase::Journal,
        StartupPhase::Journal => next == StartupPhase::Artifacts,
        StartupPhase::Artifacts => next == StartupPhase::Evidence,
        StartupPhase::Evidence => next == StartupPhase::Projections,
        StartupPhase::Projections => next == StartupPhase::AuthorityEpoch,
        StartupPhase::AuthorityEpoch => next == StartupPhase::DomainRecovery,
        StartupPhase::DomainRecovery => next == StartupPhase::EffectRecovery,
        StartupPhase::EffectRecovery => next == StartupPhase::AppRecovery,
        StartupPhase::AppRecovery => next == StartupPhase::Outbox,
        StartupPhase::Outbox => next == StartupPhase::Ipc,
        StartupPhase::Ipc => next == StartupPhase::Ready,
        StartupPhase::Ready => false,
    }
}

/// Runtime mutation-admission predicate corresponding to [`mutation_admitted`].
pub fn mutation_admitted_exec(readiness: DaemonReadiness) -> (result: bool)
    ensures result == mutation_admitted(readiness)
{
    readiness.mutation_ready()
}

/// Runtime diagnostic-admission predicate corresponding to [`diagnostic_admitted`].
pub fn diagnostic_admitted_exec(readiness: DaemonReadiness) -> (result: bool)
    ensures result == diagnostic_admitted(readiness)
{
    readiness.diagnostic_ready()
}

/// Runtime startup-order predicate corresponding to [`startup_transition`].
pub fn startup_transition_exec(current: StartupPhase, next: StartupPhase) -> (result: bool)
    ensures result == startup_transition(current, next)
{
    match current {
        StartupPhase::Validate => next == StartupPhase::Lock,
        StartupPhase::Lock => next == StartupPhase::Migrate,
        StartupPhase::Migrate => next == StartupPhase::Journal,
        StartupPhase::Journal => next == StartupPhase::Artifacts,
        StartupPhase::Artifacts => next == StartupPhase::Evidence,
        StartupPhase::Evidence => next == StartupPhase::Projections,
        StartupPhase::Projections => next == StartupPhase::AuthorityEpoch,
        StartupPhase::AuthorityEpoch => next == StartupPhase::DomainRecovery,
        StartupPhase::DomainRecovery => next == StartupPhase::EffectRecovery,
        StartupPhase::EffectRecovery => next == StartupPhase::AppRecovery,
        StartupPhase::AppRecovery => next == StartupPhase::Outbox,
        StartupPhase::Outbox => next == StartupPhase::Ipc,
        StartupPhase::Ipc => next == StartupPhase::Ready,
        StartupPhase::Ready => false,
    }
}

} // verus!

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
