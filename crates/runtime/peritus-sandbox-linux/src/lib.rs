//! Fail-closed Linux native sandbox preparation and lifecycle support.
//!
//! This crate does not authorize execution. [`peritus_process::NativeSandboxBackend::prepare`]
//! accepts only a C2 checked plan and its exact admission, creates the exact prepared cgroup leaf,
//! and returns an otherwise inert [`LinuxPreparedSession`]. It does not spawn or activate a target;
//! those effects remain solely behind the process-owned C2 native backend seam.

mod backend;
mod canonical;
mod cgroup;
mod configuration;
mod conformance;
mod descriptor;
mod error;
mod exec_status;
mod filesystem;
mod manifest;
mod network;
mod observation;
mod preparation;
mod preparation_validation;
mod probe;
mod process;
mod proxy_preparation;
mod recovery;
mod refinement;
mod resource;
mod runner;
mod secret;
mod session;
mod verified;

#[cfg(target_os = "linux")]
mod native;

pub use backend::{BACKEND_NAME, BACKEND_VERSION, MINIMUM_KERNEL, MINIMUM_LANDLOCK_ABI};
#[cfg(target_os = "linux")]
pub use backend::{helper_exit_code, run_linux_helper};
pub use cgroup::{CgroupHandle, CgroupPlan, CgroupSupport, CleanupOutcome};
pub use configuration::LinuxBackendConfig;
pub use conformance::ConformanceFacts;
pub use descriptor::{LinuxBackendDescriptor, LinuxIdentity};
pub use error::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery};
pub use exec_status::EXEC_STATUS_LABEL;
pub use filesystem::{LandlockAccess, LandlockRule, MountAction, MountPlan, MountPolicy};
pub use manifest::{
    ActivationRecord, EnvironmentEntry, HelperManifest, InheritedHandle, ProtectedPayloadBinding,
    TargetCommand,
};
pub use network::{NetworkIsolation, PROXY_LISTENER_LABEL, PROXY_TOKEN_LABEL, ProxyRoute};
pub use observation::{
    EnforcementLevel, LinuxObservation, NativeCapability, NativePhase, ObservationOutcome,
    ResourceEnforcement,
};
pub use preparation::LinuxBackend;
pub use probe::{
    Architecture, BubblewrapProbe, KernelVersion, LinuxProbe, NamespaceSupport, ProbeRequest,
};
pub use recovery::{RecoveryClassification, RuntimeRecord};
pub use refinement::RefinementFacts;
pub use resource::ResourcePlan;
pub use runner::{LinuxLaunchDescription, ProtectedInput};
pub use secret::{LinuxProtectedPayload, canonical_handles, canonical_payloads};
pub use session::LinuxPreparedSession;
