//! Deterministic, runtime-neutral H1 resilience qualification for integrated Peritus systems.
//!
//! The library owns no daemon, executor, storage engine, or platform process. Implementations
//! provide fresh black-box subjects and direct observations. The runner derives every case result,
//! canonical evidence digest, and the final production verdict from those observations.

mod cancellation;
mod catalog;
mod config;
mod contract;
mod evidence;
mod evidence_failure;
mod evidence_observation;
mod evidence_tags;
mod failure;
mod fault;
mod identity;
mod invariant;
mod observation;
mod recovery_state;
mod report;
mod resource_observation;
mod runner;
mod scenario;
mod text;
mod unwind;

pub use cancellation::CancellationToken;
pub use catalog::{CatalogError, CatalogProfile, H1_PRODUCTION_SCENARIO_COUNT, ScenarioCatalog};
pub use config::{
    ConfigurationError, HARD_MAX_MILESTONES, HARD_MAX_RETRIES, HARD_MAX_SCENARIOS,
    QualificationConfig, ResourceLimits, RetryLimits,
};
pub use contract::{
    QualificationFuture, ResilienceSubject, ResilienceSubjectFactory, SubjectDescriptor,
};
pub use evidence_observation::{EvidenceAnchor, EvidenceKind, Milestone, MilestoneKind};
pub use failure::{
    ContractViolation, FailurePhase, PanicFailure, ResourceKind, ScenarioFailure, SubjectError,
    SubjectErrorCode, SuiteFailure,
};
pub use fault::{
    CommitBoundary, CorruptTarget, CrashTiming, DaemonLifecyclePhase, DependencyKind, DiskScope,
    FaultInjection, RebootPhase, RecoveryOutcome,
};
pub use identity::{EvidenceDigest, EvidenceId, ScenarioId, SubjectId, ValueError, ValueViolation};
pub use observation::{
    AcceptanceObservation, ArtifactHealth, CorruptionObservation, DisruptionObservation,
    JournalHealth, ObservationError, PreparationObservation, ProjectionHealth, RecoveryObservation,
    TerminalState,
};
pub use recovery_state::{RecoveredStateObservation, RecoveryAccounting};
pub use report::{
    CaseStatus, NotReadyReason, QualificationReport, QualificationSummary, QualificationVerdict,
    ScenarioReport,
};
pub use resource_observation::{
    CleanupObservation, OwnershipObservation, OwnershipResolution, ResourceUsage, RetryUsage,
};
pub use runner::QualificationRunner;
pub use scenario::ScenarioSpec;
pub use text::{MAX_QUALIFICATION_TEXT_BYTES, QualificationText, TextError};
