//! Verified H0 security-readiness policy for an exact integrated candidate.
//!
//! The evaluator is deterministic and effect-free. It consumes canonical observations and
//! returns a fail-closed [`SecurityDecision`]. A ready decision is evidence, not release authority.

use vstd::prelude::*;

verus! {

mod binding;
mod catalog;
mod decision;
mod error;
mod evaluator;
mod evidence;
mod model;
mod observation;
mod proofs;
mod review;

pub use binding::IntegratedCandidate;
pub use catalog::{AcceptanceCriterion, EvidenceArtifactKind, InventoryKind, SecurityRequirement};
pub use decision::{ObservationClass, SecurityDecision, SecurityVerdict, UnmetSecurityCondition};
pub use error::{EvidenceCollection, EvidenceError, EvidenceErrorKind};
pub use evaluator::evaluate_security_readiness;
pub use evidence::SecurityEvidence;
pub use observation::{
    ArtifactObservation, CriterionObservation, InventoryObservation, RequirementObservation,
    SecurityControlOutcome,
};
pub use review::{
    FindingLifecycle, FindingObservation, FindingSeverity, IndependentSecurityReview,
    ReviewCompletion, ReviewScope, ReviewerIdentity,
};

} // verus!
