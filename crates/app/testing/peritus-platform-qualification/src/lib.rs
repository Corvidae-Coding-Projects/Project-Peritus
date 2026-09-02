//! H2 packaged-host qualification contracts for Linux, macOS, and Windows.
//!
//! This crate contains deterministic policy and evidence reduction only. Platform adapters own
//! installation and process effects through [`FreshSubjectFactory`] and [`QualificationSubject`].

mod assets;
mod digest;
mod equivalence;
mod error;
mod evidence;
mod layout;
mod lifecycle;
mod manifest;
mod native;
mod native_controller;
mod observation;
mod package_builder;
mod platform;
mod qualification_operator;
mod report_json;
mod runner;
mod sandbox;
mod scenario;
mod service;
mod transport;
mod verdict;

pub use assets::{BundledPackagingAsset, bundled_packaging_assets};
pub use digest::{ArtifactDigest, Sha256Digest, digest_bytes, digest_file};
pub use equivalence::{
    EquivalenceDifference, EquivalenceField, ProcessEquivalenceContract, ProcessObservation,
};
pub use error::{QualificationError, QualificationErrorCode, QualificationRecovery};
pub use evidence::{
    EvidenceEntry, EvidenceKind, EvidenceSet, EvidenceText, MAX_EVIDENCE_BYTES,
    MAX_EVIDENCE_ENTRIES,
};
pub use layout::{
    EntryKind, InstallPath, LayoutEntry, PathOwnership, PermissionContract, ReleaseLayout,
};
pub use lifecycle::{
    LifecycleAction, LifecyclePlan, LifecycleStep, PlanOwnership, RollbackDisposition,
};
pub use manifest::{
    ArtifactRole, ManifestArtifact, PackageManifest, PackageVersion, RelativePackagePath,
};
pub use native::{NativeControllerLimits, NativePlatformFactory};
pub use native_controller::run_from_env as run_h2_native_controller;
pub use observation::{
    CleanupObservation, ObservationOutcome, QualificationRun, ScenarioObservation,
};
pub use package_builder::run_from_env as run_package_builder;
pub use platform::{
    Architecture, NativePrerequisite, Platform, PlatformContract, PlatformDelta, PlatformVersion,
    QualificationTarget,
};
pub use qualification_operator::{H2OperatorStatus, run_from_env as run_h2_operator};
pub use runner::{FreshSubjectFactory, FreshSubjectRunner, QualificationSubject, ScenarioRequest};
pub use sandbox::{
    EnforcementClaim, NativeSandboxContract, SandboxCapability, SandboxExecutionResult,
    SandboxObservation,
};
pub use scenario::{ScenarioCategory, ScenarioId, ScenarioSpec};
pub use service::{RestartPolicy, ServiceContract, ServiceLogContract, SupervisorKind};
pub use transport::{EndpointAddress, EndpointExpectation, StoreIdentity};
pub use verdict::{NotReadyReason, QualificationReport, ReadinessVerdict, ReadyEvidence};
