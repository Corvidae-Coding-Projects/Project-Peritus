//! Authorized owned process, PTY, output, cancellation, and recovery backplane.
//!
//! [`ExecutionGateway`] is the sole public operating-system execution effect boundary. It checks
//! exact committed B0/B1/B3/C0 authority, durably consumes one action/process pair, and transfers
//! a private one-use launch into an [`OwnedProcess`]. Commands are always structured argv; this
//! crate contains no command-line parser or shell-string launch path.

#![allow(
    clippy::redundant_pub_crate,
    reason = "explicit crate visibility documents internal cross-module contracts"
)]

mod authorization;
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
mod output;
mod plan;
mod plan_canonical;
mod platform;
mod quiescence;
mod recovery;
mod refinement;
mod registry_storage;
mod resource;
mod supervisor;
mod terminal;
mod verified;
mod working_directory;

pub use authorization::ExecutionAuthorizationRequest;
pub use cancellation::{CancellationReason, EscalationRecord, StopTrigger};
pub use command::CommandSpec;
pub use consumption::ProcessStore;
pub use control::ProcessControl;
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
pub use output::{OutputCompleteness, OutputStream, StreamAccounting};
pub use plan::{BackendResourceFidelity, BackendSelection, ExecutionIsolation, ExecutionPlan};
pub use platform::ProcessTreeIdentity;
pub use quiescence::{HolderQuiescenceObservation, QuiescenceBlocker};
pub use recovery::{
    ProbeObservation, ProcessProbe, RecoveryDisposition, RecoveryEntry, RecoveryReport,
};
pub use resource::{
    ProcessResourceDimension, ProcessResourceObservation, ProcessResourcePolicy, ResourceFidelity,
};
pub use supervisor::{OwnedProcess, WaitAndPublishError};
pub use terminal::{
    OsExitObservation, OutputArtifact, OutputSummary, ProcessInstant, TerminalDisposition,
    TerminalRecovery, TerminalResult,
};
pub use working_directory::{WorkingDirectory, WorkspaceAccess};
