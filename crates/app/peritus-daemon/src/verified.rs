//! Executable/refinement predicates for G0 owner invariants.

use peritus_app_protocol::DaemonReadiness;

use crate::StartupPhase;

#[doc = " Closed formal model of the A3 readiness vocabulary."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadinessModel {
    #[doc = " Startup or recovery is incomplete."]
    Starting,
    #[doc = " Mutation and diagnostics are available."]
    ReadyReadWrite,
    #[doc = " Only diagnostic requests are available."]
    ReadyReadOnly,
    #[doc = " Existing work is draining and diagnostics remain available."]
    Draining,
    #[doc = " No application surface is available."]
    Unavailable,
}
#[doc = " Closed formal model of the G0 startup phase vocabulary."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupPhaseModel {
    #[doc = " Validate configuration and roots."]
    Validate,
    #[doc = " Acquire exclusive ownership."]
    Lock,
    #[doc = " Reconcile and apply migrations."]
    Migrate,
    #[doc = " Open and verify C0."]
    Journal,
    #[doc = " Recover artifact state."]
    Artifacts,
    #[doc = " Recover evidence state."]
    Evidence,
    #[doc = " Rebuild or validate projections."]
    Projections,
    #[doc = " Allocate the authority epoch."]
    AuthorityEpoch,
    #[doc = " Recover domain aggregates."]
    DomainRecovery,
    #[doc = " Reconcile external effects."]
    EffectRecovery,
    #[doc = " Recover application sessions and commands."]
    AppRecovery,
    #[doc = " Resume durable outbox delivery."]
    Outbox,
    #[doc = " Publish the authenticated local endpoint."]
    Ipc,
    #[doc = " Publish final readiness."]
    Ready,
}
#[doc = " Executable mutation-admission predicate refining `mutation_admitted`."]
#[must_use]
pub const fn mutation_admitted_model_exec(readiness: ReadinessModel) -> bool {
    matches!(readiness, ReadinessModel::ReadyReadWrite)
}
#[doc = " Executable diagnostic-admission predicate refining `diagnostic_admitted`."]
#[must_use]
pub const fn diagnostic_admitted_model_exec(readiness: ReadinessModel) -> bool {
    matches!(
        readiness,
        ReadinessModel::ReadyReadWrite | ReadinessModel::ReadyReadOnly | ReadinessModel::Draining
    )
}
#[doc = " Executable startup-order predicate refining `startup_transition`."]
#[must_use]
pub const fn startup_transition_model_exec(
    current: StartupPhaseModel,
    next: StartupPhaseModel,
) -> bool {
    matches!(
        (current, next),
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
