//! Verified exact-revision acceptance policy for Peritus.
//!
//! Checked observations are evaluated against one immutable acceptance contract. The evaluator is
//! pure and reports a canonical set of unmet conditions; it never executes work or changes state.

use vstd::prelude::*;

verus! {

mod authority;
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
pub use evidence::{AcceptanceEvidence, EvidenceObservation};
pub use finding::{FindingDisposition, FindingObservation};
pub use gate::{GateFailure, GateObservation, GateOutcome};
pub use ordinal::{GateAttemptOrdinal, ObservationOrdinalError, ReviewCycleOrdinal};
pub use review::{ReviewObservation, ReviewerIdentity};

} // verus!
