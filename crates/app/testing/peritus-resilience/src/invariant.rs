//! Private H1 invariant evaluation.

use crate::{
    AcceptanceObservation, ArtifactHealth, CleanupObservation, ContractViolation, CorruptTarget,
    DependencyKind, DisruptionObservation, EvidenceKind, FaultInjection, JournalHealth,
    MilestoneKind, PreparationObservation, ProjectionHealth, QualificationConfig,
    RecoveryObservation, ResourceKind, ScenarioSpec, TerminalState,
};

pub fn evaluate(
    config: QualificationConfig,
    scenario: &ScenarioSpec,
    preparation: &PreparationObservation,
    disruption: &DisruptionObservation,
    recovery: &RecoveryObservation,
) -> Vec<ContractViolation> {
    let mut violations = Vec::new();
    identity(scenario, preparation, disruption, recovery, &mut violations);
    if preparation.terminal() == TerminalState::Accepted {
        violations.push(ContractViolation::BaselineAlreadyAccepted);
    }
    if !disruption.reached() {
        violations.push(ContractViolation::FaultNotReached);
    }
    if recovery.outcome() != scenario.expected_recovery() {
        violations.push(ContractViolation::UnexpectedRecovery {
            expected: scenario.expected_recovery(),
            observed: recovery.outcome(),
        });
    }
    acceptance(recovery.acceptance(), &mut violations);
    integrity(scenario.fault(), recovery, &mut violations);
    ownership(scenario.fault(), recovery, &mut violations);
    retries(config, scenario.fault(), recovery, &mut violations);
    resources(config, recovery, &mut violations);
    evidence(recovery, &mut violations);
    milestones(config, recovery, &mut violations);
    violations
}

pub fn evaluate_cleanup(
    config: QualificationConfig,
    cleanup: CleanupObservation,
) -> Vec<ContractViolation> {
    let mut violations = Vec::new();
    if !cleanup.resources_released() || cleanup.owned_work_remaining() != 0 {
        violations.push(ContractViolation::CleanupIncomplete);
    }
    let limit = config.resources().cleanup_steps();
    if cleanup.cleanup_steps() > limit {
        violations.push(ContractViolation::ResourceLimitExceeded {
            resource: ResourceKind::CleanupSteps,
            observed: u64::from(cleanup.cleanup_steps()),
            limit: u64::from(limit),
        });
    }
    violations
}

fn identity(
    scenario: &ScenarioSpec,
    preparation: &PreparationObservation,
    disruption: &DisruptionObservation,
    recovery: &RecoveryObservation,
    violations: &mut Vec<ContractViolation>,
) {
    for observed in [preparation.scenario_id(), disruption.scenario_id(), recovery.scenario_id()] {
        if observed != scenario.id() {
            violations.push(ContractViolation::ScenarioIdentityMismatch {
                expected: scenario.id().clone(),
                observed: observed.clone(),
            });
        }
    }
    if disruption.fault() != scenario.fault() {
        violations.push(ContractViolation::FaultIdentityMismatch {
            expected: scenario.fault(),
            observed: disruption.fault(),
        });
    }
}

fn acceptance(observation: AcceptanceObservation, violations: &mut Vec<ContractViolation>) {
    if observation.terminal() == TerminalState::Accepted {
        violations.push(ContractViolation::FalseSuccess);
    } else if observation.revision_bound() || observation.evidence_current() {
        violations.push(ContractViolation::ContradictoryAcceptanceEvidence);
    }
}

fn integrity(
    fault: FaultInjection,
    recovery: &RecoveryObservation,
    violations: &mut Vec<ContractViolation>,
) {
    match fault {
        FaultInjection::CommitCrash { .. } => {
            if !matches!(
                recovery.journal(),
                JournalHealth::Verified | JournalHealth::RecoveredAndVerified
            ) {
                violations.push(ContractViolation::CrashJournalDivergence);
            }
            require_verified_objects(recovery, violations);
        }
        FaultInjection::Corruption(target) => {
            if recovery.corruption().detected() != Some(target) {
                violations.push(ContractViolation::CorruptionNotDetected {
                    expected: target,
                    observed: recovery.corruption().detected(),
                });
            }
            match target {
                CorruptTarget::Journal => {
                    if recovery.journal() != JournalHealth::HashDivergenceDetected {
                        violations.push(ContractViolation::CorruptionNotDetected {
                            expected: target,
                            observed: recovery.corruption().detected(),
                        });
                    }
                    if recovery.corruption().mutation_admitted() {
                        violations.push(ContractViolation::MutationAdmittedWithCorruption);
                    }
                }
                CorruptTarget::Projection => {
                    if recovery.projection() != ProjectionHealth::RebuiltAndVerified {
                        violations.push(ContractViolation::ProjectionNotRebuilt);
                    }
                    require_healthy_journal(recovery, violations);
                    require_verified_objects(recovery, violations);
                }
                CorruptTarget::Blob
                | CorruptTarget::Snapshot
                | CorruptTarget::AcceptanceEvidence
                | CorruptTarget::HarnessPromotion => {
                    if recovery.artifacts() != ArtifactHealth::DivergenceDetected {
                        violations.push(ContractViolation::ReferencedObjectUnverified);
                    }
                    if recovery.corruption().mutation_admitted() {
                        violations.push(ContractViolation::MutationAdmittedWithCorruption);
                    }
                    require_healthy_journal(recovery, violations);
                }
            }
        }
        FaultInjection::DiskExhaustion(_) => {
            require_healthy_journal(recovery, violations);
            require_verified_objects(recovery, violations);
            if recovery.temporary_objects() != 0 {
                violations.push(ContractViolation::TemporaryObjectLeak {
                    count: recovery.temporary_objects(),
                });
            }
        }
        FaultInjection::DependencyDeath(_)
        | FaultInjection::RetryExhaustion(_)
        | FaultInjection::DaemonKill(_)
        | FaultInjection::HostReboot(_) => {
            require_healthy_journal(recovery, violations);
            require_verified_objects(recovery, violations);
        }
    }
    if !matches!(fault, FaultInjection::Corruption(_))
        && let Some(observed) = recovery.corruption().detected()
    {
        violations.push(ContractViolation::UnexpectedCorruption { observed });
    }
}

fn require_healthy_journal(
    recovery: &RecoveryObservation,
    violations: &mut Vec<ContractViolation>,
) {
    if !matches!(recovery.journal(), JournalHealth::Verified | JournalHealth::RecoveredAndVerified)
    {
        violations.push(ContractViolation::CrashJournalDivergence);
    }
}

fn require_verified_objects(
    recovery: &RecoveryObservation,
    violations: &mut Vec<ContractViolation>,
) {
    if recovery.artifacts() != ArtifactHealth::Verified {
        violations.push(ContractViolation::ReferencedObjectUnverified);
    }
    if !matches!(
        recovery.projection(),
        ProjectionHealth::Verified | ProjectionHealth::RebuiltAndVerified
    ) {
        violations.push(ContractViolation::ProjectionNotRebuilt);
    }
}

fn ownership(
    fault: FaultInjection,
    recovery: &RecoveryObservation,
    violations: &mut Vec<ContractViolation>,
) {
    let ownership = recovery.ownership();
    if !ownership.scan_completed() {
        violations.push(ContractViolation::OwnershipScanMissing);
    }
    let accounted = ownership
        .resumed()
        .checked_add(ownership.failed())
        .and_then(|value| value.checked_add(ownership.indeterminate()))
        .and_then(|value| value.checked_add(ownership.unaccounted()));
    if accounted != Some(ownership.discovered()) {
        violations.push(ContractViolation::OwnershipAccountingInvalid);
    }
    if ownership.unaccounted() != 0 {
        violations.push(ContractViolation::UnaccountedWork { count: ownership.unaccounted() });
    }
    if ownership.orphans_remaining() != 0 {
        violations.push(ContractViolation::OrphanedWork { count: ownership.orphans_remaining() });
    }
    if matches!(
        fault,
        FaultInjection::DependencyDeath(_)
            | FaultInjection::DaemonKill(_)
            | FaultInjection::HostReboot(_)
    ) && ownership.discovered() == 0
    {
        violations.push(ContractViolation::NoOwnedWorkExercised);
    }
}

fn retries(
    config: QualificationConfig,
    fault: FaultInjection,
    recovery: &RecoveryObservation,
    violations: &mut Vec<ContractViolation>,
) {
    let usage = recovery.retries();
    let limits = config.retries();
    retry_limit(DependencyKind::Provider, usage.provider(), limits.provider(), violations);
    retry_limit(DependencyKind::Tool, usage.tool(), limits.tool(), violations);
    retry_limit(DependencyKind::Worker, usage.worker(), limits.worker(), violations);
    if usage.reconciliation() > limits.reconciliation() {
        violations.push(ContractViolation::ResourceLimitExceeded {
            resource: ResourceKind::ReconciliationSteps,
            observed: u64::from(usage.reconciliation()),
            limit: u64::from(limits.reconciliation()),
        });
    }
    if let FaultInjection::RetryExhaustion(dependency) = fault {
        let (observed, limit) = match dependency {
            DependencyKind::Provider => (usage.provider(), limits.provider()),
            DependencyKind::Tool => (usage.tool(), limits.tool()),
            DependencyKind::Worker => (usage.worker(), limits.worker()),
        };
        if observed != limit {
            violations.push(ContractViolation::RetryExhaustionNotReached {
                dependency,
                observed,
                limit,
            });
        }
    }
}

fn retry_limit(
    dependency: DependencyKind,
    observed: u16,
    limit: u16,
    violations: &mut Vec<ContractViolation>,
) {
    if observed > limit {
        violations.push(ContractViolation::RetryLimitExceeded { dependency, observed, limit });
    }
}

fn resources(
    config: QualificationConfig,
    recovery: &RecoveryObservation,
    violations: &mut Vec<ContractViolation>,
) {
    let usage = recovery.resources();
    let limits = config.resources();
    resource_limit(
        ResourceKind::Events,
        u64::from(usage.events()),
        u64::from(limits.events()),
        violations,
    );
    resource_limit(
        ResourceKind::EvidenceBytes,
        u64::from(usage.evidence_bytes()),
        u64::from(limits.evidence_bytes()),
        violations,
    );
    resource_limit(
        ResourceKind::OwnedProcesses,
        u64::from(usage.peak_owned_processes()),
        u64::from(limits.owned_processes()),
        violations,
    );
    resource_limit(
        ResourceKind::CleanupSteps,
        u64::from(usage.cleanup_steps()),
        u64::from(limits.cleanup_steps()),
        violations,
    );
    resource_limit(
        ResourceKind::LogicalTicks,
        usage.logical_ticks(),
        limits.logical_ticks(),
        violations,
    );
}

fn resource_limit(
    resource: ResourceKind,
    observed: u64,
    limit: u64,
    violations: &mut Vec<ContractViolation>,
) {
    if observed > limit {
        violations.push(ContractViolation::ResourceLimitExceeded { resource, observed, limit });
    }
}

fn evidence(recovery: &RecoveryObservation, violations: &mut Vec<ContractViolation>) {
    for required in EvidenceKind::REQUIRED {
        let count = recovery.evidence().iter().filter(|anchor| anchor.kind() == required).count();
        if count == 0 {
            violations.push(ContractViolation::MissingEvidence(required));
        } else if count > 1 {
            violations.push(ContractViolation::DuplicateEvidence);
        }
    }
}

fn milestones(
    config: QualificationConfig,
    recovery: &RecoveryObservation,
    violations: &mut Vec<ContractViolation>,
) {
    let maximum = usize::from(config.max_milestones_per_scenario());
    if recovery.milestones().len() > maximum {
        violations.push(ContractViolation::ResourceLimitExceeded {
            resource: ResourceKind::Milestones,
            observed: recovery.milestones().len() as u64,
            limit: maximum as u64,
        });
    }
    let expected = [
        MilestoneKind::Prepared,
        MilestoneKind::FaultArmed,
        MilestoneKind::FaultObserved,
        MilestoneKind::RecoveryStarted,
        MilestoneKind::Reconciled,
        MilestoneKind::Inspected,
    ];
    let canonical = recovery.milestones().len() == expected.len()
        && recovery.milestones().iter().zip(expected).enumerate().all(
            |(index, (observed, kind))| {
                u16::try_from(index).is_ok_and(|sequence| {
                    observed.sequence() == sequence && observed.kind() == kind
                })
            },
        );
    if !canonical {
        violations.push(ContractViolation::NonCanonicalMilestones);
    }
}
