//! Executable D2 predicates and focused Verus proof roots.

mod executable;

pub use executable::{
    evidence_is_fresh, findings_are_conserved, no_implicit_success, quorum_is_complete,
    replay_equivalent, transition_is_legal,
};

#[doc = " Closed mathematical finding-disposition vocabulary used by the D2 transition proofs."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "Verus ghost vocabulary is intentionally erased from ordinary Rust execution"
)]
pub enum DispositionModel {
    #[doc = " Newly admitted and unresolved."]
    Open,
    #[doc = " Fixer reports a repair; reviewer confirmation is still required."]
    Fixed,
    #[doc = " Fixer disputes the finding; reviewer confirmation is still required."]
    Disputed,
    #[doc = " Fixer proposes a provenance-preserving replacement."]
    SupersessionProposed,
    #[doc = " External authority has been requested but not observed."]
    WaiverRequested,
    #[doc = " Independent reviewer confirmed resolution."]
    ResolutionConfirmed,
    #[doc = " Independent reviewer confirmed invalidation."]
    InvalidationConfirmed,
    #[doc = " Independent reviewer confirmed a provenance-preserving replacement."]
    Superseded,
    #[doc = " Existing external authority observation matched exactly."]
    Waived,
}
