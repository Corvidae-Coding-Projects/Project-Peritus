//! Authorized process backplane; [`ExecutionGateway`] consumes authority into an [`OwnedProcess`].

#![allow(clippy::redundant_pub_crate, reason = "documents internal contracts")]
mod authorization;
mod caller_binding;
mod cancellation;
mod command;
mod consumption;
mod control;
mod environment;
mod error;
mod events;
mod gateway;
mod identity;
mod intent;
mod io_policy;
mod lifecycle;
mod native;
mod output;
mod plan;
mod plan_canonical;
mod platform;
mod quiescence;
mod recovery;
mod refinement;
mod registry_storage;
mod resource;
mod result_api;
mod supervisor;
mod terminal;
mod verified;
mod working_directory;

pub use authorization::ExecutionAuthorizationRequest;
pub use caller_binding::{ExecutionCallerBinding, ExecutionCallerTarget};
pub use cancellation::{CancellationReason, EscalationRecord, StopTrigger};
pub use command::CommandSpec;
pub use consumption::ProcessStore;
pub use control::{ProcessControl, ProcessSignal};
pub use environment::{
    EnvironmentPlan, EnvironmentSource, EnvironmentValueSource, EnvironmentVariable,
};
pub use error::{ErrorCode, ProcessError, ProcessOperation, RecoveryClass};
pub use events::{ProcessCursor, ProcessEvent, ProcessEventKind};
pub use gateway::ExecutionGateway;
pub use identity::ExecutionIdentity;
pub use intent::{EXECUTION_INTENT_MEDIA_TYPE, ExecutionIntentPayload};
pub use io_policy::{
    DeadlinePolicy, GracefulAction, IoMode, OutputOverflowAction, OutputPolicy, StdinPolicy,
    TerminalCapabilities, TerminalSize,
};
pub use lifecycle::{LifecyclePhase, LifecycleState};
pub use native::{
    AuthorizedPreparationContext, NativeLaunchDescription, NativePlatform, NativePoll,
    NativeProcessProbe, NativeProtectedHandle, NativeSandboxBackend, NativeSandboxSession,
    native_activation_record, native_ready_record, native_target_exec_failed_record,
    native_target_started_record,
};
#[cfg(unix)]
pub use native::{NATIVE_PTY_SLAVE_ENV, NativePtyAttachment};
#[cfg(windows)]
pub use native::{
    NATIVE_WINDOWS_CONTROL_HANDLE_ENV, NATIVE_WINDOWS_STATUS_HANDLE_ENV,
    NativeWindowsHelperAttachment, NativeWindowsHelperChannels,
};
pub use output::{OutputCompleteness, OutputStream, StreamAccounting};
pub use plan::{BackendResourceFidelity, BackendSelection, ExecutionIsolation, ExecutionPlan};
pub use platform::ProcessTreeIdentity;
pub use result_api::{
    HolderQuiescenceObservation, OsExitObservation, OutputArtifact, OutputSummary,
    ProbeObservation, ProcessInstant, ProcessProbe, ProcessResourceDimension,
    ProcessResourceObservation, ProcessResourcePolicy, QuiescenceBlocker, RecoveryDisposition,
    RecoveryEntry, RecoveryReport, ResourceFidelity, TerminalDisposition, TerminalRecovery,
    TerminalResult,
};
pub use supervisor::{OwnedProcess, WaitAndPublishError};
pub use verified::{
    NativePreparationFacts, native_effect_count_valid, native_preparation_complete,
    native_release_complete,
};
pub use working_directory::{WorkingDirectory, WorkspaceAccess};
