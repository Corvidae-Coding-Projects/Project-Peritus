//! Native probe, cleanup, case, and complete campaign observations.

use std::collections::BTreeSet;

use peritus_security_policy::IntegratedCandidate;
use peritus_types::Sha256Digest;

use crate::{
    H0_PRODUCTION_PROBE_COUNT, NativeExecutionReceipt, ProbeId, ProbeSpec, QualificationError,
    QualificationLimits, ResourceUsage, error::protocol,
};

/// Semantic terminal result asserted by a native probe adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProbeOutcome {
    /// Every required security assertion was directly observed.
    Passed,
    /// At least one security assertion was contradicted.
    Failed,
    /// A required native control was unavailable.
    Unsupported,
}

/// One native observation before cleanup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeObservation {
    candidate: IntegratedCandidate,
    probe: ProbeId,
    outcome: ProbeOutcome,
    receipt: NativeExecutionReceipt,
}

impl ProbeObservation {
    /// Creates a direct native probe observation.
    #[must_use]
    pub const fn from_native_execution(
        candidate: IntegratedCandidate,
        probe: ProbeId,
        outcome: ProbeOutcome,
        receipt: NativeExecutionReceipt,
    ) -> Self {
        Self { candidate, probe, outcome, receipt }
    }

    /// Returns the exact observed candidate.
    #[must_use]
    pub const fn candidate(&self) -> IntegratedCandidate {
        self.candidate
    }

    /// Returns the exact probe identity.
    #[must_use]
    pub const fn probe(&self) -> ProbeId {
        self.probe
    }

    /// Returns the semantic outcome.
    #[must_use]
    pub const fn outcome(&self) -> ProbeOutcome {
        self.outcome
    }

    /// Borrows native execution and resource evidence.
    #[must_use]
    pub const fn receipt(&self) -> &NativeExecutionReceipt {
        &self.receipt
    }
}

/// Direct teardown accounting for one fresh subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupObservation {
    subject_id: String,
    remaining_processes: u32,
    remaining_paths: u32,
    remaining_mounts: u32,
    remaining_endpoints: u32,
    cleanup_digest: Sha256Digest,
}

impl CleanupObservation {
    /// Creates complete cleanup accounting for a named subject.
    ///
    /// # Errors
    ///
    /// Rejects malformed identities or an empty cleanup-evidence digest.
    pub fn new(
        subject_id: impl Into<String>,
        remaining_processes: u32,
        remaining_paths: u32,
        remaining_mounts: u32,
        remaining_endpoints: u32,
        cleanup_digest: Sha256Digest,
    ) -> Result<Self, QualificationError> {
        let subject_id = checked_subject_id(subject_id.into())?;
        if !crate::evidence::digest_is_present(cleanup_digest) {
            return Err(protocol("cleanup observation contains an empty evidence digest"));
        }
        Ok(Self {
            subject_id,
            remaining_processes,
            remaining_paths,
            remaining_mounts,
            remaining_endpoints,
            cleanup_digest,
        })
    }

    /// Borrows the exact fresh-subject identity.
    #[must_use]
    pub fn subject_id(&self) -> &str {
        &self.subject_id
    }

    /// Reports whether all owned external resources were removed.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.remaining_processes == 0
            && self.remaining_paths == 0
            && self.remaining_mounts == 0
            && self.remaining_endpoints == 0
    }

    /// Returns processes still owned by the subject.
    #[must_use]
    pub const fn remaining_processes(&self) -> u32 {
        self.remaining_processes
    }

    /// Returns filesystem paths still owned by the subject.
    #[must_use]
    pub const fn remaining_paths(&self) -> u32 {
        self.remaining_paths
    }

    /// Returns native mounts or sandbox bindings still owned by the subject.
    #[must_use]
    pub const fn remaining_mounts(&self) -> u32 {
        self.remaining_mounts
    }

    /// Returns listeners or network endpoints still owned by the subject.
    #[must_use]
    pub const fn remaining_endpoints(&self) -> u32 {
        self.remaining_endpoints
    }

    /// Returns the digest of direct cleanup evidence.
    #[must_use]
    pub const fn cleanup_digest(&self) -> Sha256Digest {
        self.cleanup_digest
    }
}

/// Stable failure retained for one case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaseFailure {
    /// Campaign cancellation prevented execution.
    Cancelled,
    /// Fresh-subject provisioning failed.
    Provision(QualificationError),
    /// Native subject code panicked at the named stage.
    AdapterPanicked(&'static str),
    /// Native execution returned a typed failure.
    NativeExecution(QualificationError),
    /// Adapter returned an observation for another candidate.
    CandidateMismatch,
    /// Adapter returned an observation for another probe.
    ProbeMismatch,
    /// A subject identity was reused across cases.
    SubjectReused,
    /// Direct resource accounting exceeded at least one configured bound.
    ResourceLimitExceeded {
        /// Exact observed usage.
        usage: ResourceUsage,
        /// Exact configured limits.
        limits: QualificationLimits,
    },
    /// A sandbox-dependent probe claimed pass without native sandbox evidence.
    NativeSandboxNotObserved,
    /// Cleanup returned a typed failure.
    Cleanup(QualificationError),
    /// Cleanup belonged to another fresh subject.
    CleanupSubjectMismatch,
    /// Cleanup left at least one owned resource behind.
    CleanupIncomplete,
    /// Required native facility was unavailable.
    Unsupported,
    /// One or more direct assertions failed.
    AssertionFailed,
}

/// Derived terminal state of one production probe.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CaseOutcome {
    /// Setup did not produce a subject and execution never began.
    NotExecuted,
    /// A fresh subject executed but the case or cleanup failed.
    Failed,
    /// Direct execution, assertions, resource bounds, and cleanup all passed.
    Passed,
}

/// Complete fail-closed report for one catalog entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseReport {
    spec: ProbeSpec,
    subject_id: Option<String>,
    observation: Option<ProbeObservation>,
    cleanup: Option<CleanupObservation>,
    failures: Vec<CaseFailure>,
    outcome: CaseOutcome,
}

impl CaseReport {
    pub(crate) fn new(
        spec: ProbeSpec,
        subject_id: Option<String>,
        observation: Option<ProbeObservation>,
        cleanup: Option<CleanupObservation>,
        failures: Vec<CaseFailure>,
    ) -> Self {
        let outcome = if subject_id.is_none() {
            CaseOutcome::NotExecuted
        } else if failures.is_empty()
            && observation.as_ref().is_some_and(|value| value.outcome() == ProbeOutcome::Passed)
            && cleanup.as_ref().is_some_and(CleanupObservation::complete)
        {
            CaseOutcome::Passed
        } else {
            CaseOutcome::Failed
        };
        Self { spec, subject_id, observation, cleanup, failures, outcome }
    }

    /// Returns the immutable probe contract.
    #[must_use]
    pub const fn spec(&self) -> ProbeSpec {
        self.spec
    }

    /// Borrows the fresh-subject identity when provisioning succeeded.
    #[must_use]
    pub fn subject_id(&self) -> Option<&str> {
        self.subject_id.as_deref()
    }

    /// Borrows direct native observation when execution completed.
    #[must_use]
    pub const fn observation(&self) -> Option<&ProbeObservation> {
        self.observation.as_ref()
    }

    /// Borrows terminal cleanup accounting when cleanup completed.
    #[must_use]
    pub const fn cleanup(&self) -> Option<&CleanupObservation> {
        self.cleanup.as_ref()
    }

    /// Borrows failures in deterministic discovery order.
    #[must_use]
    pub fn failures(&self) -> &[CaseFailure] {
        &self.failures
    }

    /// Returns the derived terminal case state.
    #[must_use]
    pub const fn outcome(&self) -> CaseOutcome {
        self.outcome
    }

    /// Returns exact observed resource usage when native execution completed.
    #[must_use]
    pub fn resource_usage(&self) -> Option<ResourceUsage> {
        self.observation.as_ref().map(|value| value.receipt().usage())
    }

    /// Returns a direct evidence digest when native execution completed.
    #[must_use]
    pub fn evidence_digest(&self) -> Option<Sha256Digest> {
        self.observation.as_ref().map(|value| value.receipt().evidence().digest())
    }
}

/// Complete canonical fresh-subject H0 run before independent review reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualificationRun {
    candidate: IntegratedCandidate,
    limits: QualificationLimits,
    cases: Vec<CaseReport>,
}

impl QualificationRun {
    pub(crate) fn new(
        candidate: IntegratedCandidate,
        limits: QualificationLimits,
        cases: Vec<CaseReport>,
    ) -> Result<Self, QualificationError> {
        let catalog = ProbeSpec::h0_production();
        if cases.len() != H0_PRODUCTION_PROBE_COUNT
            || cases.iter().zip(catalog).any(|(case, spec)| case.spec() != *spec)
        {
            return Err(protocol("H0 run does not contain one canonical report per probe"));
        }
        let subjects = cases.iter().filter_map(CaseReport::subject_id).collect::<BTreeSet<_>>();
        let subject_count = cases.iter().filter(|case| case.subject_id().is_some()).count();
        if subjects.len() != subject_count {
            return Err(protocol("H0 qualification reused a fresh-subject identity"));
        }
        Ok(Self { candidate, limits, cases })
    }

    /// Returns the exact integrated candidate.
    #[must_use]
    pub const fn candidate(&self) -> IntegratedCandidate {
        self.candidate
    }

    /// Returns per-case limits used by the campaign.
    #[must_use]
    pub const fn limits(&self) -> QualificationLimits {
        self.limits
    }

    /// Borrows case reports in canonical probe order.
    #[must_use]
    pub fn cases(&self) -> &[CaseReport] {
        &self.cases
    }

    /// Reports whether every case reached direct pass with complete cleanup.
    #[must_use]
    pub fn all_passed(&self) -> bool {
        self.cases.iter().all(|case| case.outcome() == CaseOutcome::Passed)
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "the private runner validates subject identities through this constructor"
)]
pub(super) fn checked_subject_id(value: String) -> Result<String, QualificationError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(protocol("fresh-subject identity is not bounded canonical ASCII"));
    }
    Ok(value)
}
