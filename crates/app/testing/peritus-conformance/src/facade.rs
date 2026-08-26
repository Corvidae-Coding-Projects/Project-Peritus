//! Stable crate-root facade kept separate from the composition root.

pub use crate::agent::*;
pub use crate::catalog::{plugin_suite, protocol_suite};
pub use crate::collaboration::{
    CollaborationConformanceError, CollaborationConformanceFixture,
    CollaborationConformanceObservation, CollaborationConformanceSubject, CollaborationScenario,
    CollaborationTerminal, collaboration_suite,
};
pub use crate::contracts::{
    BoxedCase, ConformanceCase, ConformanceFuture, ConformanceSuite, StaticSuite, SubjectFactory,
};
pub use crate::descriptor::{CaseDescriptor, SubjectDescriptor, SuiteDescriptor};
pub use crate::failure::{
    AssertionFailure, CaseFailure, DuplicateCaseIdFailure, FailureKind, FailurePhase, PanicFailure,
    PanicMessage, SubjectFailure, SuiteFailure, TeardownFailure,
};
pub use crate::gate::{
    GateConformanceError, GateConformanceFixture, GateConformanceObservation,
    GateConformanceSubject, GateScenario, GateTerminal, gate_suite,
};
pub use crate::harness_materialization::{
    HarnessConformanceError, HarnessConformanceFixture, HarnessConformanceObservation,
    HarnessConformanceSubject, HarnessScenario, HarnessTerminal, harness_suite,
};
pub use crate::identity::*;
pub use crate::journal::{
    JournalAppendDisposition, JournalAppendFixture, JournalAppendObservation,
    JournalConformanceError, JournalConformanceSubject, JournalSnapshot, journal_suite,
};
pub use crate::orchestrator::{
    OrchestratorConformanceError, OrchestratorConformanceFixture,
    OrchestratorConformanceObservation, OrchestratorConformanceSubject, OrchestratorScenario,
    OrchestratorTerminal, orchestrator_suite,
};
pub use crate::outcome::{CaseResult, Observation, ObservationValue};
pub use crate::process::{
    ProcessAuthorizationDrift, ProcessConformanceError, ProcessConformanceFixture,
    ProcessConformanceObservation, ProcessConformanceSubject, ProcessDisposition,
    ProcessEffectObservation, ProcessEnvironmentBinding, ProcessInvocationObservation,
    ProcessIoMode, ProcessOutputObservation, ProcessOutputStream, ProcessOwnershipObservation,
    ProcessRecoveryDisposition, ProcessRecoveryProbe, ProcessScenario,
    ProcessStreamOffsetObservation, ProcessTrigger, process_suite,
};
pub use crate::provider::{
    ProviderAttemptObservation, ProviderAttemptOutcome, ProviderCancellationObservation,
    ProviderCapability, ProviderCapabilityObservation, ProviderConformanceError,
    ProviderConformanceFixture, ProviderConformanceObservation, ProviderConformanceSubject,
    ProviderEventKind, ProviderEventObservation, ProviderFailureKind, ProviderFailureObservation,
    ProviderIsolationObservation, ProviderRedactionObservation, ProviderRetryObservation,
    ProviderScenario, ProviderStreamObservation, ProviderTerminal, ProviderUsageObservation,
    ProviderUsageSnapshot, provider_suite,
};
pub use crate::replay::{
    ReplayConformanceError, ReplayConformanceSubject, ReplayObservation, replay_suite,
};
pub use crate::report::{CaseReport, CaseStatus, SuiteReport, SuiteStatus, SuiteSummary};
pub use crate::review::{
    ReviewConformanceError, ReviewConformanceFixture, ReviewConformanceObservation,
    ReviewConformanceSubject, ReviewScenario, ReviewTerminal, review_suite,
};
pub use crate::runner::ConformanceRunner;
pub use crate::sandbox::{
    SandboxConformanceError, SandboxConformanceFixture, SandboxConformanceObservation,
    SandboxConformanceSubject, SandboxDecision, SandboxDomain, SandboxFeature,
    SandboxLifecyclePhase, SandboxPreparationFixture, SandboxPreparationObservation,
    SandboxScenario, sandbox_suite,
};
pub use crate::scheduler::{
    SchedulerConformanceError, SchedulerConformanceFixture, SchedulerConformanceObservation,
    SchedulerConformanceSubject, SchedulerScenario, SchedulerTerminal, scheduler_suite,
};
pub use crate::text::{ReportText, ReportTextError};
pub use crate::tool::{
    ToolAuthorizationDrift, ToolConformanceError, ToolConformanceFixture,
    ToolConformanceObservation, ToolConformanceSubject, ToolDescriptorObservation, ToolDisposition,
    ToolEffectObservation, ToolReplayMode, ToolReplayObservation, ToolResultObservation,
    ToolScenario, tool_suite,
};
pub use crate::trace::{
    TraceConformanceError, TraceConformanceFixture, TraceConformanceObservation,
    TraceConformanceSubject, TraceScenario, trace_suite,
};
pub use crate::workspace::{
    WorkspaceConformanceError, WorkspaceConformanceSubject, WorkspaceMutationDisposition,
    WorkspaceMutationObservation, WorkspacePatchFixture, WorkspaceReconciliationDisposition,
    WorkspaceSnapshot, workspace_suite,
};
