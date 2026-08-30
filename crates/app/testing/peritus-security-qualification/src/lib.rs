//! H0 fresh-subject security qualification and canonical evidence packaging.
//!
//! Native effects and external review remain explicit trust boundaries. This crate never turns an
//! adapter error, unsupported control, timeout, cancellation, or missing review into success.

mod assets;
mod catalog;
mod controller;
mod digest;
mod error;
mod evidence;
mod interchange;
mod limits;
mod manifest;
mod native;
mod observation;
mod operator;
mod policy_bridge;
mod report;
mod runner;
mod shard;

pub use assets::{BundledSecurityAsset, bundled_security_assets};
pub use catalog::{H0_PRODUCTION_PROBE_COUNT, ProbeId, ProbeSpec, ProbeTarget};
pub use controller::{ControllerStatus, run_from_env as run_h0_controller};
pub use digest::{digest_bytes, hex_digest};
pub use error::{QualificationError, QualificationErrorCode, QualificationRecovery};
pub use evidence::{
    EvidenceEntry, EvidenceSet, EvidenceValue, MAX_CASE_EVIDENCE_BYTES, MAX_CASE_EVIDENCE_ENTRIES,
    NativeExecutionReceipt, SafeEvidenceCode,
};
pub use interchange::{
    decode_candidate as parse_candidate_json, decode_review as parse_review_json,
    encode_candidate as candidate_json, encode_final_report as final_report_json,
};
pub use limits::{CancellationToken, QualificationLimits, ResourceUsage};
pub use manifest::EvidenceManifest;
pub use native::{HostFingerprint, NativeProbeFactory};
pub use observation::{
    CaseFailure, CaseOutcome, CaseReport, CleanupObservation, ProbeObservation, ProbeOutcome,
    QualificationRun,
};
pub use operator::{
    H0AggregateStatus, H0OperatorStatus, run_aggregate_from_env as run_h0_aggregate_operator,
    run_from_env as run_h0_operator,
};
pub use peritus_security_policy::{
    AcceptanceCriterion, FindingLifecycle, FindingObservation, FindingSeverity,
    IndependentSecurityReview, IntegratedCandidate, ReviewCompletion, ReviewScope,
    ReviewerIdentity, SecurityRequirement,
};
pub use report::{NotReadyReason, QualificationReport, ReadinessEvidence, ReadinessVerdict};
pub use runner::{FreshSubjectFactory, ProbeRequest, QualificationRunner, QualificationSubject};
pub use shard::{QualificationPlatform, QualificationShard};
