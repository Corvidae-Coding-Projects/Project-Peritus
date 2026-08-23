//! Runtime-neutral, deterministic conformance-suite contracts and execution.
//!
//! Cases are sorted by validated identifier, receive fresh subjects, and retain setup, assertion,
//! panic, and teardown failures without defining any production protocol. Report-bearing text is
//! bounded by [`ReportText`]. Empty catalog suites are runnable scaffolding; only
//! [`SuiteStatus::Passed`] proves conformance.
//!
//! Rust unwinding is caught at runner-controlled boundaries. Aborting panics, process termination,
//! out-of-memory failure, stack overflow, undefined behavior, and double panics require supervised
//! subprocess isolation and cannot be represented reliably by an in-process report.
//! Dropping a pending runner is cancellation: in-flight futures and subjects are dropped in place,
//! teardown is not newly invoked or awaited, and no report is produced. Subjects must be RAII-safe.

mod catalog;
mod contracts;
mod descriptor;
mod failure;
mod identity;
mod journal;
mod outcome;
mod replay;
mod report;
mod runner;
mod text;
mod unwind;

pub use catalog::{plugin_suite, protocol_suite, provider_suite, sandbox_suite, tool_suite};
pub use contracts::{
    BoxedCase, ConformanceCase, ConformanceFuture, ConformanceSuite, StaticSuite, SubjectFactory,
};
pub use descriptor::{CaseDescriptor, SubjectDescriptor, SuiteDescriptor};
pub use failure::{
    AssertionFailure, CaseFailure, DuplicateCaseIdFailure, FailureKind, FailurePhase, PanicFailure,
    PanicMessage, SubjectFailure, SuiteFailure, TeardownFailure,
};
pub use identity::{
    CaseId, FailureCode, FailureCodeError, IdentifierError, ObservationId, SuiteId,
};
pub use journal::{
    JournalAppendDisposition, JournalAppendFixture, JournalAppendObservation,
    JournalConformanceError, JournalConformanceSubject, JournalSnapshot, journal_suite,
};
pub use outcome::{CaseResult, Observation, ObservationValue};
pub use replay::{
    ReplayConformanceError, ReplayConformanceSubject, ReplayObservation, replay_suite,
};
pub use report::{CaseReport, CaseStatus, SuiteReport, SuiteStatus, SuiteSummary};
pub use runner::ConformanceRunner;
pub use text::{ReportText, ReportTextError};
