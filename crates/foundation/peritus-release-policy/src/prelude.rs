//! Common imports for callers assembling and evaluating H4 evidence.

pub use crate::{
    AcceptanceCriterion, Architecture, CandidateId, ConstructionError, ConstructionErrorKind,
    DecisionDigest, Diagnostic, EvidenceBinding, EvidenceObservation, EvidenceRequirement,
    EvidenceSourceKind, FindingDisposition, FindingId, FindingObservation, FindingSeverity,
    GitCommitId, GitObjectFormat, OperatingSystem, PlatformIdentity, PlatformMatrix, PrincipalId,
    ProfileIdentity, QualificationObservation, QualificationSlice, QualificationVerdict,
    ReleaseCandidate, ReleaseDecision, ReleaseEvidence, ReleaseVerdict, ReleaseVersion, ReviewId,
    ReviewObservation, ReviewOutcome, SchemaIdentity, ToolchainIdentity, WaiverObservation,
    evaluate_release,
};
