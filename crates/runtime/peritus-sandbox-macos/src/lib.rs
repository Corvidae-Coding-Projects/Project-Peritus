//! macOS 15+ native sandbox preparation.
//!
//! This crate compiles a checked C2 sandbox plan into a deterministic, digest-bound Seatbelt
//! profile and helper manifest. It does not authorize or spawn a target. The process-owned C2
//! gateway remains responsible for authorization, durable consumption, literal process creation,
//! PTY or pipe ownership, cancellation, and terminal publication.

#![allow(
    clippy::redundant_pub_crate,
    reason = "explicit crate visibility documents cross-module internal boundaries"
)]

mod activation;
mod canonical;
#[cfg(test)]
mod conformance_tests;
mod descriptor;
mod environment;
mod error;
mod exec_status;
mod filesystem;
mod helper;
mod manifest;
mod network;
mod observation;
mod preparation;
mod probe;
mod process;
mod recovery;
mod refinement;
mod resource;
mod resource_monitor;
mod runner;
mod secret;
mod session;
#[cfg(test)]
mod test_support;
mod verified;

pub use activation::{ACTIVATION_RECORD_BYTES, ActivationRecord};
pub use descriptor::{BACKEND_NAME, BACKEND_VERSION, MacosDescriptor};
pub use environment::EnvironmentEntry;
pub use error::{MacosError, MacosErrorKind, MacosOperation, RecoveryAction};
pub use exec_status::EXEC_STATUS_LABEL;
pub use filesystem::{CompiledSeatbeltProfile, ProfileCompiler, ProfileDecision};
pub use helper::run_helper_process;
pub use manifest::{
    HelperManifest, MANIFEST_DESCRIPTOR, MANIFEST_DESCRIPTOR_NUMBER, ManifestHandle,
};
pub use network::{ProtectedProxyRoute, ProxyHandleDescriptor, ProxyRoute};
pub use observation::{MacosObservation, ObservationEvent, ObservationStatus};
pub use preparation::{MacosBackend, PreparationConfig, PreparedMacosSandbox};
pub use probe::{MacosHostProbe, ProbeEvidence, ProbeRequest, ResourceProbe, SystemProbe};
pub use process::{HelperLaunch, InheritedDescriptor, ProcessContainment, TerminalMapping};
pub use recovery::{CleanupProgress, MacosRecoveryRecord, RecoveryClassification, RuntimeIdentity};
pub use resource::{EnforcementLevel, ResourceControl, ResourceControlPlan};
#[cfg(target_os = "macos")]
pub use runner::{
    PreparedTargetCommand, activate_manifest_with_pty, execute_manifest_with_pty,
    execute_prepared_target, prepare_target_command,
};
pub use runner::{ReservedHelperExit, activate_manifest, execute_manifest};
pub use secret::{
    ProtectedSecretHandle, SecretHandleDescriptor, SecretHandleDestination,
    canonical_secret_handles, secret_binding_digest, secret_reference_digest,
};
pub use session::{MacosSession, ReleaseReport, SessionPhase, TerminationReason};
