//! Verified H4 production release policy for Peritus.
//!
//! The crate consumes already-produced observations and reduces them to a deterministic,
//! fail-closed [`ReleaseDecision`]. It performs no I/O, signs nothing, changes no repository or
//! release state, and deliberately exposes no publication capability.
//!
//! All twenty-five production criteria are closed and stable. Evidence is projected into that
//! fixed catalog before evaluation, so input permutations produce the same assessment order,
//! verdict, and [`DecisionDigest`].

use vstd::prelude::*;

verus! {

mod candidate;
mod catalog;
mod decision;
mod error;
mod evaluator;
mod evidence;
mod identity;
mod model;
pub mod prelude;
mod qualification;
mod review;

pub use candidate::{
    Architecture, GitCommitId, GitObjectFormat, OperatingSystem, PlatformIdentity, PlatformMatrix,
    ProfileIdentity, ReleaseCandidate, ReleaseVersion, SchemaIdentity, ToolchainIdentity,
};
pub use catalog::{
    AcceptanceCriterion, CriterionDefinition, EvidenceRequirement, EvidenceSourceKind,
    PRODUCTION_CRITERIA, REQUIRED_EVIDENCE,
};
pub use decision::{
    CriterionAssessment, DecisionDigest, Diagnostic, EvidenceAssessment, FindingAssessment,
    QualificationAssessment, ReleaseDecision, ReleaseVerdict, ReviewAssessment,
};
pub use error::{ConstructionError, ConstructionErrorKind};
pub use evaluator::evaluate_release;
pub use evidence::{EvidenceBinding, EvidenceObservation, ReleaseEvidence};
pub use identity::{CandidateId, FindingId, PrincipalId, ReviewId};
pub use qualification::{
    QualificationObservation, QualificationSlice, QualificationVerdict,
};
pub use review::{
    FindingDisposition, FindingObservation, FindingSeverity, ReviewObservation, ReviewOutcome,
    WaiverObservation,
};

} // verus!
