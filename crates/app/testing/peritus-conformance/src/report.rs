//! Deterministic aggregate reports produced by the conformance runner.

use crate::{
    CaseDescriptor, CaseFailure, FailureKind, Observation, SubjectDescriptor, SuiteDescriptor,
    SuiteFailure, TeardownFailure,
};

/// Terminal status of one conformance case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaseStatus {
    /// The case body and teardown completed without failure.
    Passed,
    /// The case body ran, and the body or teardown failed.
    Failed,
    /// Subject setup failed, so the case body did not run.
    NotExecuted,
}

/// Deterministic report for one case and its fresh subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseReport {
    descriptor: CaseDescriptor,
    status: CaseStatus,
    observations: Vec<Observation>,
    primary_failure: Option<CaseFailure>,
    teardown_failure: Option<TeardownFailure>,
}

impl CaseReport {
    pub(crate) const fn new(
        descriptor: CaseDescriptor,
        executed: bool,
        observations: Vec<Observation>,
        primary_failure: Option<CaseFailure>,
        teardown_failure: Option<TeardownFailure>,
    ) -> Self {
        let status = if !executed {
            CaseStatus::NotExecuted
        } else if primary_failure.is_some() || teardown_failure.is_some() {
            CaseStatus::Failed
        } else {
            CaseStatus::Passed
        };
        Self { descriptor, status, observations, primary_failure, teardown_failure }
    }

    /// Returns immutable case metadata.
    #[must_use]
    pub const fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    /// Returns the derived terminal status.
    #[must_use]
    pub const fn status(&self) -> CaseStatus {
        self.status
    }

    /// Returns case observations in their explicit case-defined order.
    #[must_use]
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }

    /// Returns a setup, assertion, or execution-panic failure when present.
    #[must_use]
    pub const fn primary_failure(&self) -> Option<&CaseFailure> {
        self.primary_failure.as_ref()
    }

    /// Returns teardown failure independently of the primary result.
    #[must_use]
    pub const fn teardown_failure(&self) -> Option<&TeardownFailure> {
        self.teardown_failure.as_ref()
    }

    /// Returns whether this case has at least one failure in `kind`.
    ///
    /// A contract assertion followed by teardown failure returns `true` for both categories.
    #[must_use]
    pub fn has_failure_kind(&self, kind: FailureKind) -> bool {
        self.primary_failure.as_ref().is_some_and(|failure| failure.kind() == kind)
            || self.teardown_failure.as_ref().is_some_and(|failure| failure.kind() == kind)
    }
}

/// Terminal aggregate status of a suite run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuiteStatus {
    /// The suite definition was valid but contained no cases.
    Empty,
    /// Every case and teardown passed. This is the only conformant status.
    Passed,
    /// At least one case was failed or not executed.
    Failed,
    /// The suite, subject, or case definition was invalid and no case ran.
    Invalid,
}

/// Deterministic aggregate counts for a suite report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SuiteSummary {
    total: usize,
    passed: usize,
    failed: usize,
    not_executed: usize,
    contract_violation_cases: usize,
    infrastructure_failure_cases: usize,
}

impl SuiteSummary {
    fn from_cases(cases: &[CaseReport]) -> Self {
        let mut summary = Self {
            total: cases.len(),
            passed: 0,
            failed: 0,
            not_executed: 0,
            contract_violation_cases: 0,
            infrastructure_failure_cases: 0,
        };
        for case in cases {
            match case.status() {
                CaseStatus::Passed => summary.passed += 1,
                CaseStatus::Failed => summary.failed += 1,
                CaseStatus::NotExecuted => summary.not_executed += 1,
            }
            if case.has_failure_kind(FailureKind::ContractViolation) {
                summary.contract_violation_cases += 1;
            }
            if case.has_failure_kind(FailureKind::Infrastructure) {
                summary.infrastructure_failure_cases += 1;
            }
        }
        summary
    }

    /// Returns the total number of reported cases.
    #[must_use]
    pub const fn total(self) -> usize {
        self.total
    }

    /// Returns the number of passing cases.
    #[must_use]
    pub const fn passed(self) -> usize {
        self.passed
    }

    /// Returns the number of cases that ran and failed.
    #[must_use]
    pub const fn failed(self) -> usize {
        self.failed
    }

    /// Returns the number of cases whose subject setup failed.
    #[must_use]
    pub const fn not_executed(self) -> usize {
        self.not_executed
    }

    /// Returns cases containing at least one contract violation.
    ///
    /// This is a case count, not a failure-occurrence count. One case can also be counted as an
    /// infrastructure failure when teardown fails after its assertion.
    #[must_use]
    pub const fn contract_violation_cases(self) -> usize {
        self.contract_violation_cases
    }

    /// Returns cases containing at least one infrastructure failure.
    ///
    /// This is a case count, not a failure-occurrence count. One case can also be counted as a
    /// contract violation when teardown fails after its assertion.
    #[must_use]
    pub const fn infrastructure_failure_cases(self) -> usize {
        self.infrastructure_failure_cases
    }
}

/// Complete deterministic output of one suite run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuiteReport {
    suite: Option<SuiteDescriptor>,
    subject: Option<SubjectDescriptor>,
    status: SuiteStatus,
    cases: Vec<CaseReport>,
    failure: Option<SuiteFailure>,
    summary: SuiteSummary,
}

impl SuiteReport {
    pub(crate) fn complete(
        suite: SuiteDescriptor,
        subject: SubjectDescriptor,
        cases: Vec<CaseReport>,
    ) -> Self {
        let summary = SuiteSummary::from_cases(&cases);
        let status = if cases.is_empty() {
            SuiteStatus::Empty
        } else if cases.iter().all(|case| case.status() == CaseStatus::Passed) {
            SuiteStatus::Passed
        } else {
            SuiteStatus::Failed
        };
        Self { suite: Some(suite), subject: Some(subject), status, cases, failure: None, summary }
    }

    pub(crate) fn invalid(
        suite: Option<SuiteDescriptor>,
        subject: Option<SubjectDescriptor>,
        failure: SuiteFailure,
    ) -> Self {
        Self {
            suite,
            subject,
            status: SuiteStatus::Invalid,
            cases: Vec::new(),
            failure: Some(failure),
            summary: SuiteSummary::from_cases(&[]),
        }
    }

    /// Returns suite metadata when its callback completed without panicking.
    #[must_use]
    pub const fn suite(&self) -> Option<&SuiteDescriptor> {
        self.suite.as_ref()
    }

    /// Returns subject metadata when its callback completed without panicking.
    #[must_use]
    pub const fn subject(&self) -> Option<&SubjectDescriptor> {
        self.subject.as_ref()
    }

    /// Returns the aggregate suite status.
    #[must_use]
    pub const fn status(&self) -> SuiteStatus {
        self.status
    }

    /// Returns whether this nonempty suite proved conformance.
    #[must_use]
    pub const fn is_conformant(&self) -> bool {
        matches!(self.status, SuiteStatus::Passed)
    }

    /// Returns case reports in deterministic case-ID order.
    #[must_use]
    pub fn cases(&self) -> &[CaseReport] {
        &self.cases
    }

    /// Returns the definition failure for an invalid suite.
    #[must_use]
    pub const fn failure(&self) -> Option<&SuiteFailure> {
        self.failure.as_ref()
    }

    /// Returns deterministic aggregate counts.
    #[must_use]
    pub const fn summary(&self) -> SuiteSummary {
        self.summary
    }
}
