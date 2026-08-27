//! Stable crate-root exports for process consumers and sandbox backends.

pub use crate::authorization::ExecutionAuthorizationRequest;
pub use crate::caller_binding::{ExecutionCallerBinding, ExecutionCallerTarget};
pub use crate::cancellation::{CancellationReason, EscalationRecord, StopTrigger};
pub use crate::command::CommandSpec;
pub use crate::consumption::ProcessStore;
pub use crate::control::{ProcessControl, ProcessSignal};
pub use crate::environment::{
    EnvironmentPlan, EnvironmentSource, EnvironmentValueSource, EnvironmentVariable,
};
pub use crate::error::{ErrorCode, ProcessError, ProcessOperation, RecoveryClass};
pub use crate::events::{ProcessCursor, ProcessEvent, ProcessEventKind};
pub use crate::gateway::ExecutionGateway;
pub use crate::identity::ExecutionIdentity;
pub use crate::intent::{EXECUTION_INTENT_MEDIA_TYPE, ExecutionIntentPayload};
pub use crate::io_policy::{
    DeadlinePolicy, GracefulAction, IoMode, OutputOverflowAction, OutputPolicy, StdinPolicy,
    TerminalCapabilities, TerminalSize,
};
pub use crate::lifecycle::{LifecyclePhase, LifecycleState};
pub use crate::native::{
    AuthorizedPreparationContext, NativeLaunchDescription, NativePlatform, NativePoll,
    NativeProcessProbe, NativeProtectedHandle, NativeSandboxBackend, NativeSandboxSession,
    native_activation_record, native_ready_record, native_target_exec_failed_record,
    native_target_started_record,
};
#[cfg(unix)]
pub use crate::native::{NATIVE_PTY_SLAVE_ENV, NativePtyAttachment};
#[cfg(windows)]
pub use crate::native::{
    NATIVE_WINDOWS_CONTROL_HANDLE_ENV, NATIVE_WINDOWS_STATUS_HANDLE_ENV,
    NativeWindowsHelperAttachment, NativeWindowsHelperChannels,
};
pub use crate::output::{OutputCompleteness, OutputStream, StreamAccounting};
pub use crate::plan::{
    BackendResourceFidelity, BackendSelection, ExecutionIsolation, ExecutionPlan,
};
pub use crate::platform::ProcessTreeIdentity;
pub use crate::result_api::{
    HolderQuiescenceObservation, OsExitObservation, OutputArtifact, OutputSummary,
    ProbeObservation, ProcessInstant, ProcessProbe, ProcessResourceDimension,
    ProcessResourceObservation, ProcessResourcePolicy, QuiescenceBlocker, RecoveryDisposition,
    RecoveryEntry, RecoveryReport, ResourceFidelity, TerminalDisposition, TerminalRecovery,
    TerminalResult,
};
pub use crate::supervisor::{OwnedProcess, WaitAndPublishError};
pub use crate::verified::{
    NativePreparationFacts, native_effect_count_valid, native_preparation_complete,
    native_release_complete,
};
pub use crate::working_directory::{WorkingDirectory, WorkspaceAccess};
