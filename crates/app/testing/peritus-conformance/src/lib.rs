//! Runtime-neutral, deterministic conformance-suite contracts and execution.
//! Cases receive fresh subjects in stable identifier order and retain typed setup, assertion,
//! panic, and teardown failures. Report text is bounded, and only a nonempty passed suite proves
//! conformance. Pending-run cancellation drops owned work in place, so subjects must be RAII-safe.

mod agent;
mod catalog;
mod contracts;
mod descriptor;
mod failure;
mod identity;
mod journal;
mod outcome;
mod process;
mod provider;
mod replay;
mod report;
mod runner;
mod sandbox;
mod text;
mod tool;
mod unwind;
mod workspace;

pub use agent::*;
pub use catalog::{plugin_suite, protocol_suite};
pub use contracts::{
    BoxedCase, ConformanceCase, ConformanceFuture, ConformanceSuite, StaticSuite, SubjectFactory,
};
pub use descriptor::{CaseDescriptor, SubjectDescriptor, SuiteDescriptor};
pub use failure::{
    AssertionFailure, CaseFailure, DuplicateCaseIdFailure, FailureKind, FailurePhase, PanicFailure,
    PanicMessage, SubjectFailure, SuiteFailure, TeardownFailure,
};
pub use identity::*;
pub use journal::{
    JournalAppendDisposition, JournalAppendFixture, JournalAppendObservation,
    JournalConformanceError, JournalConformanceSubject, JournalSnapshot, journal_suite,
};
pub use outcome::{CaseResult, Observation, ObservationValue};
pub use process::{
    ProcessAuthorizationDrift, ProcessConformanceError, ProcessConformanceFixture,
    ProcessConformanceObservation, ProcessConformanceSubject, ProcessDisposition,
    ProcessEffectObservation, ProcessEnvironmentBinding, ProcessInvocationObservation,
    ProcessIoMode, ProcessOutputObservation, ProcessOutputStream, ProcessOwnershipObservation,
    ProcessRecoveryDisposition, ProcessRecoveryProbe, ProcessScenario,
    ProcessStreamOffsetObservation, ProcessTrigger, process_suite,
};
pub use provider::{
    ProviderAttemptObservation, ProviderAttemptOutcome, ProviderCancellationObservation,
    ProviderCapability, ProviderCapabilityObservation, ProviderConformanceError,
    ProviderConformanceFixture, ProviderConformanceObservation, ProviderConformanceSubject,
    ProviderEventKind, ProviderEventObservation, ProviderFailureKind, ProviderFailureObservation,
    ProviderIsolationObservation, ProviderRedactionObservation, ProviderRetryObservation,
    ProviderScenario, ProviderStreamObservation, ProviderTerminal, ProviderUsageObservation,
    ProviderUsageSnapshot, provider_suite,
};
pub use replay::{
    ReplayConformanceError, ReplayConformanceSubject, ReplayObservation, replay_suite,
};
pub use report::{CaseReport, CaseStatus, SuiteReport, SuiteStatus, SuiteSummary};
pub use runner::ConformanceRunner;
pub use sandbox::{
    SandboxConformanceError, SandboxConformanceFixture, SandboxConformanceObservation,
    SandboxConformanceSubject, SandboxDecision, SandboxDomain, SandboxFeature,
    SandboxLifecyclePhase, SandboxPreparationFixture, SandboxPreparationObservation,
    SandboxScenario, sandbox_suite,
};
pub use text::{ReportText, ReportTextError};
pub use tool::{
    ToolAuthorizationDrift, ToolConformanceError, ToolConformanceFixture,
    ToolConformanceObservation, ToolConformanceSubject, ToolDescriptorObservation, ToolDisposition,
    ToolEffectObservation, ToolReplayMode, ToolReplayObservation, ToolResultObservation,
    ToolScenario, tool_suite,
};
pub use workspace::{
    WorkspaceConformanceError, WorkspaceConformanceSubject, WorkspaceMutationDisposition,
    WorkspaceMutationObservation, WorkspacePatchFixture, WorkspaceReconciliationDisposition,
    WorkspaceSnapshot, workspace_suite,
};
