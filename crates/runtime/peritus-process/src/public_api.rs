//! Stable crate-root exports for process consumers and sandbox backends.

pub use crate::authorization::ExecutionAuthorizationRequest;
pub use crate::caller_binding::{ExecutionCallerBinding, ExecutionCallerTarget};
pub use crate::cancellation::{CancellationReason, EscalationRecord, StopTrigger};
pub use crate::command::{
    CommandSpec, is_native_executable_reference, native_executable_reference,
    native_executable_reference_matches,
};
pub use crate::consumption::{
    ProcessStore, RetainedCompletionBinding, RetainedOwnerCompletionStatus,
    RetainedStreamSnapshot,
};
pub use crate::control::{ProcessControl, ProcessSignal};
pub use crate::environment::{
    EnvironmentPlan, EnvironmentSource, EnvironmentValueSource, EnvironmentVariable,
    native_environment_name_cmp, native_environment_names_equal,
};
pub use crate::error::{ControlRejection, ErrorCode, ProcessError, ProcessOperation, RecoveryClass};
pub use crate::events::{ProcessCursor, ProcessEvent, ProcessEventKind, ProcessEventLoss};
pub use crate::gateway::ExecutionGateway;
pub use crate::identity::ExecutionIdentity;
pub use crate::intent::{EXECUTION_INTENT_MEDIA_TYPE, ExecutionIntentPayload};
pub use crate::io_policy::{
    DeadlinePolicy, GracefulAction, IoMode, OutputOverflowAction, OutputPolicy, StdinPolicy,
    StopStrategy,
    TerminalCapabilities, TerminalSize,
};
pub use crate::lifecycle::{LifecyclePhase, LifecycleState};
pub use crate::native::{
    AuthorizedPreparationContext, NATIVE_OBSERVATION_PAGE_RECORDS, NativeLaunchDescription,
    NativeObservationPage, NativeObservationReceipt, NativeObservationTransport, NativePlatform,
    NativePoll, NativeProcessProbe, NativeProtectedHandle, NativeRecoveryPhase,
    NativeSandboxBackend, NativeSandboxSession, NativeSessionRecovery,
    NativeWindowsContainmentIdentity, NATIVE_MANIFEST_FRAME_BYTES, NATIVE_MANIFEST_STREAM_MARKER,
    native_activation_record, native_helper_quiesced_record,
    native_helper_worker_failed_record, native_ready_record,
    native_observation_prefix_digest, native_observation_producer_binding,
    native_target_adoption_record, native_target_exec_failed_record, native_target_started_record,
};
#[cfg(unix)]
pub use crate::native::{NATIVE_PTY_SLAVE_ENV, NativePtyAttachment};
#[cfg(windows)]
pub use crate::native::{
    NATIVE_WINDOWS_CONTROL_HANDLE_ENV, NATIVE_WINDOWS_JOB_HANDLE_ENV,
    NATIVE_WINDOWS_JOB_HANDLE_LABEL, NATIVE_WINDOWS_STATUS_HANDLE_ENV,
    NativeWindowsHelperAttachment, NativeWindowsHelperChannels,
    NativeWindowsOwnerInspection,
};
pub use crate::output::{OutputCompleteness, OutputStream, StreamAccounting};
pub use crate::plan::{
    BackendResourceFidelity, BackendSelection, ExecutionIsolation, ExecutionPlan,
};
pub use crate::platform::ProcessTreeIdentity;
pub use crate::platform::admission::validate_command_environment as validate_native_command_environment;
#[cfg(windows)]
pub use crate::platform::current_process_resident_memory_bytes;
pub use crate::result_api::{
    HolderQuiescenceObservation, OsExitObservation, OutputArtifact, OutputSummary,
    ProbeObservation, ProcessInstant, ProcessProbe, ProcessResourceDimension,
    ProcessResourceObservation, ProcessResourcePolicy, ProcessTreeQuiescence, QuiescenceBlocker, RecoveryDisposition,
    RecoveryEntry, RecoveryObservation, RecoveryReport, ResourceFidelity, TerminalDisposition, TerminalRecovery,
    TerminalResult,
};
pub use crate::retained_owner::{
    RetainedBackendFactoryRequest, RetainedOwnerBinding, RetainedOwnerNonce,
    RetainedOwnerObservation, RetainedOwnerRequest, RetainedOwnerReservation, RetainedProcessKey,
    RetainedProcessTransport, RetainedServiceOwner, RetainedStreamPage,
};
pub use crate::supervisor::{OwnedProcess, WaitAndPublishError};
pub use crate::verified::{
    NativePreparationFacts, native_effect_count_valid, native_preparation_complete,
    native_release_complete,
};
pub use crate::working_directory::{WorkingDirectory, WorkspaceAccess};
