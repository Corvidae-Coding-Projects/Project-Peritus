//! H4 release-evidence collection, independent audit, and final qualification.
//!
//! Readiness is fail-closed and remains subordinate to the deterministic release policy supplied
//! through [`DeterministicReleasePolicy`]. [`VerifiedReleasePolicyAdapter`] provides the checked
//! bridge to `peritus-release-policy`. This crate never signs, tags, publishes, or creates evidence.

mod audit;
mod collection;
mod criteria;
mod error;
mod evidence;
mod identity;
mod manifest;
pub mod operator;
mod policy_adapter;
pub mod prelude;
mod report;
mod verified_policy;

pub use audit::{
    AuditDraft, AuditFinding, FinalAudit, FindingDisposition, FindingId, FindingSeverity,
};
pub use collection::{
    CleanupObservation, CollectionCase, CollectionFailure, CollectionOutcome, CollectionRequest,
    CollectionRun, FreshSubjectFactory, FreshSubjectRunner, QualificationSubject,
};
pub use criteria::{
    ACCEPTANCE_CRITERIA_COUNT, AcceptanceCriterion, CriterionEvidenceMap, CriterionMapping,
};
pub use error::{QualificationError, QualificationErrorCode};
pub use evidence::{
    EvidenceDisposition, EvidenceKind, EvidenceReference, EvidenceSignature, SignedEvidenceRecord,
    canonical_evidence_signature_envelope,
};
pub use identity::{ParticipantId, SubjectId};
pub use manifest::{
    EvidenceManifest, EvidenceManifestEntry, EvidenceManifestRole, MAX_MANIFEST_ENTRIES,
};
pub use policy_adapter::{
    DeterministicReleasePolicy, PolicyCriterionInput, PolicyDecision, PolicyFailure,
    ReleasePolicyInput,
};
pub use report::{
    AcReference, Blocker, QualificationInputs, QualificationReport, QualificationVerdict,
    RequiredInput,
};
pub use verified_policy::{VerifiedPolicyBinding, VerifiedReleasePolicyAdapter};
