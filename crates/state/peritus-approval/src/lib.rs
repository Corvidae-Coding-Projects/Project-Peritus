//! Verified digest-bound human approval transitions for Peritus.
//!
//! Authentication observations and successful reducers remain logical, uncommitted facts. This
//! crate neither establishes credential-registry currentness nor constructs an effect permit.

mod amendment;
pub(crate) mod authentication;
mod codec;
mod decision;
mod digest;
mod failure;
mod grant;
#[cfg(verus_only)]
pub mod model;
#[cfg(verus_only)]
mod proofs;
mod render;
mod request;
pub(crate) mod state;

use vstd::prelude::*;

pub use codec::{
    decode_approval_request, decode_credential_registry, decode_signed_decision,
    encode_approval_request, encode_signed_decision,
};

verus! {

pub use amendment::{AmendmentIdentity, ApprovalAmendmentOutcome, ApprovedPolicyAmendment};
pub use authentication::{
    ApprovalKeyId, ApprovalPublicKey, ApprovalSignature, ApproverCredential,
    AuthenticatedApprovalObservation, CredentialRegistrySnapshot, CredentialStatus,
    MAX_CREDENTIAL_APPROVAL_ROLES, MAX_CREDENTIAL_REGISTRY_ENTRIES, verify_signed_decision,
};
pub use decision::{ApprovalChoice, ApprovalDecision, SignedApprovalDecision};
pub use digest::{ActionDigest, ApprovalDecisionDigest, ApprovalRequestDigest};
pub use digest::{MAX_APPROVAL_DECISION_PREIMAGE_BYTES, MAX_APPROVAL_KEY_ID_PREIMAGE_BYTES};
pub use digest::MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES;
pub use failure::{
    ApprovalError, ApprovalPhase, ApprovalTransitionFailure, ApprovalUseFailure,
    CanonicalCollection, CredentialDimension, RecoveryClass, ScopeDimension,
};
pub use grant::{ApprovedActionTransition, ApprovalUseOutcome, ConsumedApproval};
pub use render::{
    MAX_RENDERED_APPROVAL_BYTES, MAX_RENDERED_FIELD_BYTES, MAX_RENDERED_PARTICIPANTS,
    MAX_RENDERED_PERMISSIONS,
    RENDER_TRUNCATION_SUFFIX_BYTES, RenderedApproval, render_approval,
};
pub use request::{
    ApprovalRequest, ParticipantSet, MAX_APPROVAL_PERMISSIONS,
    MAX_APPROVAL_REQUEST_PREIMAGE_BYTES, MAX_INDEPENDENCE_REQUIREMENTS,
    MAX_PRODUCING_PARTICIPANTS, MAX_REVIEW_PARTICIPANTS, MAX_RISK_CLASSES,
};
pub use state::{
    ApprovalAggregate, ApprovalResolutionFacts, ApprovalTransition, ApprovalTransitionKind,
    ApprovalTransitionOutcome,
};

} // verus!
