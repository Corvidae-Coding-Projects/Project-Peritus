//! Checked platform-neutral sandbox contracts and executable reference semantics.
//!
//! Values in this crate are inert. They neither authorize nor launch an operating-system process.
//! The process-owned C2 gateway consumes a [`CheckedSandboxPlan`] and [`BackendAdmission`] before
//! handing an opaque authorized launch to a native backend.

mod admission;
mod backend;
mod binding;
mod cancellation;
mod canonical;
mod contract;
mod environment;
mod error;
mod feature;
mod filesystem;
mod lifecycle;
mod network;
mod observation;
mod plan;
mod process_policy;
mod reference;
mod refinement;
mod requirements;
mod resource;
mod secret;
mod terminal;
mod verified;

pub use admission::{AdmissionProfile, BackendAdmission, admit_backend};
pub use backend::{
    BackendDescriptor, BackendKind, BackendName, BackendVersion, PathSemantics, ResourceFidelity,
    SandboxPreparation,
};
pub use binding::SandboxBinding;
pub use cancellation::{CancellationAcceptance, CancellationReason, CancellationState};
pub use contract::{IsolationRequirement, SandboxContract, SandboxOperationClass};
pub use environment::{EnvironmentContract, EnvironmentMode, EnvironmentName};
pub use error::{RecoveryClass, SandboxError, SandboxErrorKind, SandboxOperation};
pub use feature::{FeatureSet, SandboxFeature};
pub use filesystem::{
    FileDecision, FileOperation, FileOperationSet, FilesystemContract, FilesystemRule, PathScope,
    RuleEffect, SandboxPath,
};
pub use lifecycle::SandboxPhase;
pub use network::{
    DnsName, HostMatcher, NetworkContract, NetworkDecision, NetworkHost, NetworkRule,
    NetworkTarget, PortRange, Transport,
};
pub use observation::{
    CapabilityDomain, EnforcementObservation, ObservationDisposition, ObservationKind,
    TeardownCompleteness, teardown_completeness,
};
pub use plan::{CheckedSandboxPlan, compile_sandbox};
pub use process_policy::{
    DescendantPolicy, ProcessContract, ProcessRequirements, SignalPolicy, TreeContainment,
};
pub use reference::{
    ProbeDecision, ReferenceBackend, ReferenceFault, ReferenceFaultPlan, ReferenceProbe,
    ReferenceSession, RequestedProcessSignal, ResourceDecision, TerminationKind,
};
pub use requirements::{EnvironmentRequirements, FileRequirement, SandboxRequirements};
pub use resource::{ResourceLimits, ResourceUsage, SandboxResourceKind};
pub use secret::{
    BrokeredHandleLabel, SecretContract, SecretDelivery, SecretGrant, SecretReference,
    SecretRequirement,
};
pub use terminal::{
    InputPermission, RequestedTerminalOperation, ResizePermission, TerminalContract,
    TerminalLimits, TerminalMode, TerminalModes, TerminalRequirements, TerminalSignalPermission,
    TerminalSize,
};
