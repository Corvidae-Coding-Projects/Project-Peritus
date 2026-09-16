//! Verified exact-revision acceptance policy for Peritus.
//!
//! Checked observations are evaluated against one immutable acceptance contract. The evaluator is
//! pure and reports a canonical set of unmet conditions; it never executes work or changes state.

use vstd::prelude::*;

verus! {

mod authority;
mod canonical;
mod decision;
mod error;
mod evaluator;
mod evidence;
mod finding;
mod gate;
mod model;
mod ordinal;
mod proofs;
mod revision;
mod review;

pub use authority::{
    ApprovalObservation, ApprovalOutcome, ApprovalSubject, WaiverObservation,
};
pub use decision::{
    AcceptanceDecision, InvalidWaiverReason, ObservationKind, ReviewerIndependenceFailure,
    UnmetCondition,
};
pub use error::{CanonicalEvidenceCollection, EvidenceError, EvidenceErrorKind};
pub use evaluator::evaluate_acceptance;
#[cfg(verus_only)]
pub use evaluator::{accepted_evaluation_contract, input_defined_acceptance};
pub use evidence::{AcceptanceEvidence, EvidenceObservation};
pub use finding::{FindingDisposition, FindingObservation};
pub use gate::{GateFailure, GateObservation, GateOutcome};
pub use ordinal::{GateAttemptOrdinal, ObservationOrdinalError, ReviewCycleOrdinal};
#[cfg(verus_only)]
pub use model::authority::{
    blocking_findings_resolved, current_finding_at, current_waiver_at, finding_ids_match,
    invalid_waiver_reported, invalid_waivers_reported, supplied_current_waivers_valid,
    supplied_waiver_valid, waiver_authorized, waiver_phase_complete,
};
#[cfg(verus_only)]
pub use model::approvals::{
    approval_expected, final_approval_complete, final_approval_failure,
    unexpected_approvals_complete, waiver_approval_expected,
};
#[cfg(verus_only)]
pub use model::{
    required_artifacts_complete, required_gates_complete, required_reviews_complete, revision_fresh,
};
#[cfg(verus_only)]
pub use revision::{same_identifier, same_identifier_from};
pub use review::{ReviewObservation, ReviewerIdentity};

} // verus!
