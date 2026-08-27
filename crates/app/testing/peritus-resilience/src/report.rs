//! Deterministic case reports and final production verdict.

use crate::evidence::digest_report;
use crate::{
    CatalogProfile, CleanupObservation, DisruptionObservation, EvidenceDigest,
    PreparationObservation, QualificationConfig, RecoveryObservation, ScenarioFailure,
    ScenarioSpec, SubjectDescriptor, SuiteFailure,
};

/// Terminal status of one fresh-subject scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaseStatus {
    /// Setup failed, so the scenario body did not execute.
    NotExecuted,
    /// Execution or cleanup failed, or a contract invariant was violated.
    Failed,
    /// Execution, every invariant, and cleanup passed.
    Passed,
}

/// Deterministic report for one scenario and one fresh subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioReport {
    scenario: ScenarioSpec,
    status: CaseStatus,
    preparation: Option<PreparationObservation>,
    disruption: Option<DisruptionObservation>,
    recovery: Option<RecoveryObservation>,
    cleanup: Option<CleanupObservation>,
    failures: Vec<ScenarioFailure>,
}

impl ScenarioReport {
    pub(super) const fn new(
        scenario: ScenarioSpec,
        subject_created: bool,
        preparation: Option<PreparationObservation>,
        disruption: Option<DisruptionObservation>,
        recovery: Option<RecoveryObservation>,
        cleanup: Option<CleanupObservation>,
        failures: Vec<ScenarioFailure>,
    ) -> Self {
        let status = if !subject_created {
            CaseStatus::NotExecuted
        } else if failures.is_empty() {
            CaseStatus::Passed
        } else {
            CaseStatus::Failed
        };
        Self { scenario, status, preparation, disruption, recovery, cleanup, failures }
    }

    /// Returns the exact scenario definition.
    #[must_use]
    pub const fn scenario(&self) -> &ScenarioSpec {
        &self.scenario
    }
    /// Returns the derived terminal case status.
    #[must_use]
    pub const fn status(&self) -> CaseStatus {
        self.status
    }
    /// Returns preparation facts when that stage completed.
    #[must_use]
    pub const fn preparation(&self) -> Option<&PreparationObservation> {
        self.preparation.as_ref()
    }
    /// Returns injection facts when that stage completed.
    #[must_use]
    pub const fn disruption(&self) -> Option<&DisruptionObservation> {
        self.disruption.as_ref()
    }
    /// Returns recovery facts when that stage completed.
    #[must_use]
    pub const fn recovery(&self) -> Option<&RecoveryObservation> {
        self.recovery.as_ref()
    }
    /// Returns cleanup facts when cleanup completed.
    #[must_use]
    pub const fn cleanup(&self) -> Option<CleanupObservation> {
        self.cleanup
    }
    /// Returns failures in deterministic discovery order.
    #[must_use]
    pub fn failures(&self) -> &[ScenarioFailure] {
        &self.failures
    }
}

/// Aggregate case counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QualificationSummary {
    total: usize,
    passed: usize,
    failed: usize,
    not_executed: usize,
}

impl QualificationSummary {
    fn from_cases(cases: &[ScenarioReport]) -> Self {
        let mut summary = Self { total: cases.len(), passed: 0, failed: 0, not_executed: 0 };
        for case in cases {
            match case.status() {
                CaseStatus::Passed => summary.passed += 1,
                CaseStatus::Failed => summary.failed += 1,
                CaseStatus::NotExecuted => summary.not_executed += 1,
            }
        }
        summary
    }

    /// Returns total reported scenarios.
    #[must_use]
    pub const fn total(self) -> usize {
        self.total
    }
    /// Returns passing scenarios.
    #[must_use]
    pub const fn passed(self) -> usize {
        self.passed
    }
    /// Returns executed failing scenarios.
    #[must_use]
    pub const fn failed(self) -> usize {
        self.failed
    }
    /// Returns scenarios not executed because setup failed.
    #[must_use]
    pub const fn not_executed(self) -> usize {
        self.not_executed
    }
}

/// Primary deterministic reason production readiness was withheld.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotReadyReason {
    /// Caller supplied a diagnostic custom catalog.
    CustomCatalog,
    /// Suite definition failed before trustworthy execution.
    SuiteFailure,
    /// At least one production scenario failed or was not executed.
    ScenarioFailure,
}

/// Final H1 release verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QualificationVerdict {
    /// Complete production catalog passed with cleanup and canonical evidence.
    Ready,
    /// Production readiness was withheld for the typed reason.
    NotReadyForProduction(NotReadyReason),
}

/// Complete deterministic H1 report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualificationReport {
    config: QualificationConfig,
    profile: CatalogProfile,
    subject: Option<SubjectDescriptor>,
    cases: Vec<ScenarioReport>,
    suite_failure: Option<SuiteFailure>,
    summary: QualificationSummary,
    verdict: QualificationVerdict,
    evidence_digest: EvidenceDigest,
}

impl QualificationReport {
    pub(crate) fn complete(
        config: QualificationConfig,
        profile: CatalogProfile,
        subject: SubjectDescriptor,
        cases: Vec<ScenarioReport>,
    ) -> Self {
        let summary = QualificationSummary::from_cases(&cases);
        let verdict = if profile != CatalogProfile::H1Production {
            QualificationVerdict::NotReadyForProduction(NotReadyReason::CustomCatalog)
        } else if cases.is_empty() || cases.iter().any(|case| case.status() != CaseStatus::Passed) {
            QualificationVerdict::NotReadyForProduction(NotReadyReason::ScenarioFailure)
        } else {
            QualificationVerdict::Ready
        };
        let mut report = Self {
            config,
            profile,
            subject: Some(subject),
            cases,
            suite_failure: None,
            summary,
            verdict,
            evidence_digest: EvidenceDigest::from_bytes([0; 32]),
        };
        report.evidence_digest = digest_report(&report);
        report
    }

    pub(crate) fn invalid(
        config: QualificationConfig,
        profile: CatalogProfile,
        subject: Option<SubjectDescriptor>,
        failure: SuiteFailure,
    ) -> Self {
        let mut report = Self {
            config,
            profile,
            subject,
            cases: Vec::new(),
            suite_failure: Some(failure),
            summary: QualificationSummary::from_cases(&[]),
            verdict: QualificationVerdict::NotReadyForProduction(NotReadyReason::SuiteFailure),
            evidence_digest: EvidenceDigest::from_bytes([0; 32]),
        };
        report.evidence_digest = digest_report(&report);
        report
    }

    /// Returns the immutable invocation bounds.
    #[must_use]
    pub const fn config(&self) -> QualificationConfig {
        self.config
    }
    /// Returns the catalog profile.
    #[must_use]
    pub const fn profile(&self) -> CatalogProfile {
        self.profile
    }
    /// Returns qualified build identity when metadata inspection succeeded.
    #[must_use]
    pub const fn subject(&self) -> Option<&SubjectDescriptor> {
        self.subject.as_ref()
    }
    /// Returns case reports in stable scenario-ID order.
    #[must_use]
    pub fn cases(&self) -> &[ScenarioReport] {
        &self.cases
    }
    /// Returns the definition failure, if one prevented execution.
    #[must_use]
    pub const fn suite_failure(&self) -> Option<&SuiteFailure> {
        self.suite_failure.as_ref()
    }
    /// Returns aggregate counts.
    #[must_use]
    pub const fn summary(&self) -> QualificationSummary {
        self.summary
    }
    /// Returns the non-bypassable final verdict.
    #[must_use]
    pub const fn verdict(&self) -> QualificationVerdict {
        self.verdict
    }
    /// Returns whether full production H1 qualification succeeded.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self.verdict, QualificationVerdict::Ready)
    }
    /// Returns a canonical SHA-256 digest binding all report content except this field itself.
    #[must_use]
    pub const fn evidence_digest(&self) -> EvidenceDigest {
        self.evidence_digest
    }
}
