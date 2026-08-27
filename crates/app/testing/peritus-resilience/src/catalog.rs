//! Canonical H1 and diagnostic custom catalogs.

use std::error::Error;
use std::fmt;

use crate::config::HARD_MAX_SCENARIOS;
use crate::{
    CommitBoundary, CorruptTarget, CrashTiming, DaemonLifecyclePhase, DependencyKind, DiskScope,
    FaultInjection, QualificationText, RebootPhase, RecoveryOutcome, ScenarioId, ScenarioSpec,
};

/// Number of cases in the immutable H1 production catalog.
pub const H1_PRODUCTION_SCENARIO_COUNT: usize = 43;

/// Whether a catalog can contribute to a production H1 verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogProfile {
    /// Complete immutable H1 release qualification.
    H1Production,
    /// Caller-defined diagnostic subset or extension.
    Custom,
}

/// Invalid scenario catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogError {
    /// A custom catalog contained no scenarios.
    Empty,
    /// The catalog exceeded the hard allocation bound.
    TooMany {
        /// Number of supplied scenarios.
        actual: usize,
        /// Hard scenario-count ceiling.
        maximum: usize,
    },
    /// Two scenarios used one stable identifier.
    DuplicateId(ScenarioId),
    /// A built-in definition violated its own identifier or text constraints.
    InvalidBuiltInDefinition,
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => {
                formatter.write_str("resilience catalog must contain at least one scenario")
            }
            Self::TooMany { actual, maximum } => {
                write!(formatter, "resilience catalog has {actual} scenarios; maximum is {maximum}")
            }
            Self::DuplicateId(id) => {
                write!(formatter, "duplicate resilience scenario identifier: {id}")
            }
            Self::InvalidBuiltInDefinition => {
                formatter.write_str("the built-in H1 catalog violated a crate invariant")
            }
        }
    }
}

impl Error for CatalogError {}

/// Validated scenarios held in stable bytewise identifier order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioCatalog {
    profile: CatalogProfile,
    scenarios: Vec<ScenarioSpec>,
}

impl ScenarioCatalog {
    /// Builds the complete immutable production H1 catalog.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError`] if an internal scenario identifier, title, coverage count,
    /// uniqueness rule, or catalog bound violates the built-in invariants.
    pub fn h1_production() -> Result<Self, CatalogError> {
        let mut scenarios = Vec::with_capacity(H1_PRODUCTION_SCENARIO_COUNT);
        add_commit_crashes(&mut scenarios)?;
        for target in [
            CorruptTarget::Journal,
            CorruptTarget::Blob,
            CorruptTarget::Snapshot,
            CorruptTarget::Projection,
            CorruptTarget::AcceptanceEvidence,
            CorruptTarget::HarnessPromotion,
        ] {
            let expected = match target {
                CorruptTarget::Journal => RecoveryOutcome::FailedClosed,
                CorruptTarget::Projection => RecoveryOutcome::RebuiltProjection,
                CorruptTarget::Blob
                | CorruptTarget::Snapshot
                | CorruptTarget::AcceptanceEvidence
                | CorruptTarget::HarnessPromotion => RecoveryOutcome::QuarantinedCorruption,
            };
            add(
                &mut scenarios,
                format!("h1.corruption.{}", target.code()),
                format!("detect and contain {} corruption", target.code()),
                FaultInjection::Corruption(target),
                expected,
            )?;
        }
        for scope in [DiskScope::JournalAppend, DiskScope::BlobFinalize, DiskScope::SnapshotCommit]
        {
            let expected = if scope == DiskScope::BlobFinalize {
                RecoveryOutcome::DiscardedUnreferenced
            } else {
                RecoveryOutcome::RolledBackUncommitted
            };
            add(
                &mut scenarios,
                format!("h1.disk-full.{}", scope.code()),
                format!("disk exhaustion at {}", scope.code()),
                FaultInjection::DiskExhaustion(scope),
                expected,
            )?;
        }
        for dependency in [DependencyKind::Provider, DependencyKind::Tool, DependencyKind::Worker] {
            add(
                &mut scenarios,
                format!("h1.death.{}", dependency.code()),
                format!("{} death during owned work", dependency.code()),
                FaultInjection::DependencyDeath(dependency),
                RecoveryOutcome::ReconciledOwnedWork,
            )?;
            add(
                &mut scenarios,
                format!("h1.retry-exhaustion.{}", dependency.code()),
                format!("{} retry exhaustion remains non-success", dependency.code()),
                FaultInjection::RetryExhaustion(dependency),
                RecoveryOutcome::RetryBudgetExhausted,
            )?;
        }
        for phase in DaemonLifecyclePhase::ALL {
            add(
                &mut scenarios,
                format!("h1.daemon-kill.{}", phase.code()),
                format!("daemon kill during {}", phase.code()),
                FaultInjection::DaemonKill(phase),
                RecoveryOutcome::ReconciledOwnedWork,
            )?;
        }
        for phase in [
            RebootPhase::OutstandingEffect,
            RebootPhase::DurableBeforeAck,
            RebootPhase::StartupReconciliation,
        ] {
            add(
                &mut scenarios,
                format!("h1.reboot.{}", phase.code()),
                format!("host reboot during {}", phase.code()),
                FaultInjection::HostReboot(phase),
                RecoveryOutcome::ReconciledOwnedWork,
            )?;
        }
        if scenarios.len() != H1_PRODUCTION_SCENARIO_COUNT {
            return Err(CatalogError::InvalidBuiltInDefinition);
        }
        validate(CatalogProfile::H1Production, scenarios)
    }

    /// Creates a bounded custom diagnostic catalog.
    ///
    /// A custom catalog is always `NotReadyForProduction` even when every case passes.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::Empty`] for an empty catalog, [`CatalogError::TooMany`] when the
    /// hard scenario bound is exceeded, or [`CatalogError::DuplicateId`] when identifiers are not
    /// unique.
    pub fn custom(scenarios: Vec<ScenarioSpec>) -> Result<Self, CatalogError> {
        validate(CatalogProfile::Custom, scenarios)
    }

    /// Returns the catalog profile.
    #[must_use]
    pub const fn profile(&self) -> CatalogProfile {
        self.profile
    }

    /// Returns scenarios in stable bytewise identifier order.
    #[must_use]
    pub fn scenarios(&self) -> &[ScenarioSpec] {
        &self.scenarios
    }
}

fn add_commit_crashes(scenarios: &mut Vec<ScenarioSpec>) -> Result<(), CatalogError> {
    for boundary in CommitBoundary::ALL {
        add(
            scenarios,
            format!("h1.crash.{}.before", boundary.code()),
            format!("crash before durable {} commit", boundary.code()),
            FaultInjection::CommitCrash { boundary, timing: CrashTiming::BeforeDurableCommit },
            RecoveryOutcome::RolledBackUncommitted,
        )?;
        add(
            scenarios,
            format!("h1.crash.{}.after-before-ack", boundary.code()),
            format!("crash after durable {} commit before acknowledgement", boundary.code()),
            FaultInjection::CommitCrash {
                boundary,
                timing: CrashTiming::AfterDurableCommitBeforeAck,
            },
            RecoveryOutcome::ReplayedCommitted,
        )?;
    }
    Ok(())
}

fn add(
    scenarios: &mut Vec<ScenarioSpec>,
    id: String,
    title: String,
    fault: FaultInjection,
    expected: RecoveryOutcome,
) -> Result<(), CatalogError> {
    let id = ScenarioId::new(id).map_err(|_| CatalogError::InvalidBuiltInDefinition)?;
    let title =
        QualificationText::new(title).map_err(|_| CatalogError::InvalidBuiltInDefinition)?;
    scenarios.push(ScenarioSpec::new(id, title, fault, expected));
    Ok(())
}

fn validate(
    profile: CatalogProfile,
    mut scenarios: Vec<ScenarioSpec>,
) -> Result<ScenarioCatalog, CatalogError> {
    if scenarios.is_empty() {
        return Err(CatalogError::Empty);
    }
    if scenarios.len() > usize::from(HARD_MAX_SCENARIOS) {
        return Err(CatalogError::TooMany {
            actual: scenarios.len(),
            maximum: usize::from(HARD_MAX_SCENARIOS),
        });
    }
    scenarios.sort_by(|left, right| left.id().cmp(right.id()));
    if let Some(pair) = scenarios.windows(2).find(|pair| pair[0].id() == pair[1].id()) {
        return Err(CatalogError::DuplicateId(pair[0].id().clone()));
    }
    Ok(ScenarioCatalog { profile, scenarios })
}
