//! Windows 11 24H2 and Server 2025 native sandbox preparation.
//!
//! This crate translates an already checked C2 sandbox plan into deterministic Windows native
//! controls and a digest-bound helper manifest. Authorization, durable consumption, process
//! creation, standard-stream ownership, cancellation, and terminal publication remain owned by
//! `peritus-process`.

#![allow(
    clippy::redundant_pub_crate,
    reason = "explicit crate visibility documents internal native-boundary ownership"
)]

mod channels;
mod config;
mod conformance;
mod descriptor;
mod diagnostic;
mod error;
mod filesystem;
mod helper_process;
mod identity;
mod manifest;
#[cfg(target_os = "windows")]
mod native;
mod network;
mod network_filter;
mod observation;
mod preparation;
mod probe;
mod process;
mod recovery;
mod refinement;
mod release;
mod resource;
mod runner;
mod secret;
mod session;
mod verified;

pub use config::WindowsBackendConfig;
pub use conformance::ConformanceFacts;
pub use descriptor::{BACKEND_NAME, BACKEND_VERSION, WindowsBackendDescriptor, WindowsIdentity};
pub use diagnostic::{enforcement_name, resource_name};
pub use error::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};
pub use filesystem::{
    AclAccess, AclEntry, AclPlan, AclTransaction, PathEvidence, PathPolicy, ResolvedWindowsPath,
    WindowsPath, compile_acl_plan,
};
pub use helper_process::helper_main;
pub use manifest::{EnvironmentEntry, HelperManifest};
pub use network::{NetworkIsolation, ProxyRoute, managed_wfp_policy_digest};
pub use observation::{
    ObservationBinding, ObservationStatus, WindowsCapability, WindowsObservation, WindowsPhase,
    resource_domain,
};
pub use preparation::WindowsBackend;
pub use probe::{
    MINIMUM_WINDOWS_BUILD, ProbeEvidence, ProbeRequest, WindowsProbe, production_resource_levels,
};
pub use process::{
    AppContainerProfile, DesktopPolicy, InheritedHandlePolicy, JobPlan, ProcessPolicy,
    TerminalMapping, TokenProfile,
};
pub use recovery::{
    RecoveryClassification, RecoveryProbe, RuntimeIdentity, WindowsRecoveryRecord, classify,
};
pub use release::{CleanupState, ReleaseProgress, ReleaseReport};
pub use resource::{EnforcementLevel, ResourceControl, ResourceControlPlan};
pub use runner::{
    HelperExit, ReservedHelperExit, WindowsActivation, WindowsLaunchDescription, activate_manifest,
    execute_manifest,
};
pub use secret::{
    ProtectedSecretHandle, SecretHandleDestination, canonical_handles, secret_reference_digest,
};
pub use session::WindowsSession;
