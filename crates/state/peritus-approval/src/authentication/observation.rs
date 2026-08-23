//! Opaque authenticated facts returned only by strict decision verification.

use peritus_policy::{ActorRole, AuthorityInstant, ValidityWindow};
use peritus_types::{ActorId, ApprovalRequestId, CommandId, Generation, RevisionNumber};
use vstd::prelude::*;

verus! {

/// Opaque successful authentication against one exact supplied snapshot.
///
/// This move-only value is not a durable-current-registry witness or effect permit.
/// Its fields are deliberately unavailable to callers:
///
/// ```compile_fail
/// use peritus_approval::{
///     ApprovalChoice, ApprovalDecisionDigest, ApprovalKeyId, ApprovalRequestDigest,
///     AuthenticatedApprovalObservation,
/// };
/// use peritus_policy::{ActorRole, AuthorityInstant, ValidityWindow};
/// use peritus_types::{ActorId, ApprovalRequestId, CommandId, Generation, RevisionNumber};
///
/// fn forge(
///     request_id: ApprovalRequestId,
///     request_digest: ApprovalRequestDigest,
///     decision_digest: ApprovalDecisionDigest,
///     command_id: CommandId,
///     responder: ActorId,
///     approver_role: ActorRole,
///     choice: ApprovalChoice,
///     key_id: ApprovalKeyId,
///     credential_generation: Generation,
///     registry_revision: RevisionNumber,
///     credential_validity: ValidityWindow,
///     decision_expires_at: AuthorityInstant,
///     observed_at: AuthorityInstant,
/// ) -> AuthenticatedApprovalObservation {
///     AuthenticatedApprovalObservation {
///         request_id, request_digest, decision_digest, command_id, responder, approver_role,
///         choice, key_id, credential_generation, registry_revision, credential_validity,
///         decision_expires_at, observed_at,
///     }
/// }
/// ```
///
/// A verified observation is also move-only:
///
/// ```compile_fail
/// use peritus_approval::AuthenticatedApprovalObservation;
///
/// fn duplicate(value: AuthenticatedApprovalObservation) {
///     let _copy = value.clone();
/// }
/// ```
#[derive(Debug, Eq, PartialEq)]
pub struct AuthenticatedApprovalObservation {
    pub(crate) request_id: ApprovalRequestId,
    pub(crate) request_digest: crate::ApprovalRequestDigest,
    pub(crate) decision_digest: crate::ApprovalDecisionDigest,
    pub(crate) command_id: CommandId,
    pub(crate) responder: ActorId,
    pub(crate) approver_role: ActorRole,
    pub(crate) choice: crate::ApprovalChoice,
    pub(crate) key_id: super::ApprovalKeyId,
    pub(crate) credential_generation: Generation,
    pub(crate) registry_revision: RevisionNumber,
    pub(crate) credential_validity: ValidityWindow,
    pub(crate) decision_expires_at: AuthorityInstant,
    pub(crate) observed_at: AuthorityInstant,
}

impl AuthenticatedApprovalObservation {
    /// Returns the signed decision digest used by formal reducer contracts.
    pub closed spec fn spec_decision_digest(&self) -> crate::ApprovalDecisionDigest {
        self.decision_digest
    }

    /// Returns the signed semantic choice used by formal reducer contracts.
    pub closed spec fn spec_choice(&self) -> crate::ApprovalChoice { self.choice }

    pub(crate) proof fn prove_specs(&self)
        ensures
            self.spec_decision_digest() == self.decision_digest,
            self.spec_choice() == self.choice,
    {
    }

    /// Returns the exact request identity.
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId { self.request_id }

    /// Returns the request digest authenticated by the signature.
    #[must_use]
    pub const fn request_digest(&self) -> crate::ApprovalRequestDigest { self.request_digest }

    /// Returns the exact authenticated decision digest.
    #[must_use]
    pub const fn decision_digest(&self) -> crate::ApprovalDecisionDigest { self.decision_digest }

    /// Returns the unique signed command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId { self.command_id }

    /// Returns the authenticated responder actor.
    #[must_use]
    pub const fn responder(&self) -> ActorId { self.responder }

    /// Returns the exact signed approval role.
    #[must_use]
    pub const fn approver_role(&self) -> ActorRole { self.approver_role }

    /// Returns the exact authenticated choice.
    #[must_use]
    pub const fn choice(&self) -> crate::ApprovalChoice { self.choice }

    /// Returns the authenticated approval key ID.
    #[must_use]
    pub const fn key_id(&self) -> super::ApprovalKeyId { self.key_id }

    /// Returns the credential generation used for authentication.
    #[must_use]
    pub const fn credential_generation(&self) -> Generation { self.credential_generation }

    /// Returns the exact non-authoritative supplied snapshot revision.
    #[must_use]
    pub const fn registry_revision(&self) -> RevisionNumber { self.registry_revision }

    /// Returns the authenticated credential validity interval.
    #[must_use]
    pub const fn credential_validity(&self) -> ValidityWindow { self.credential_validity }

    /// Returns the accepted monotonic authority time.
    #[must_use]
    pub const fn observed_at(&self) -> AuthorityInstant { self.observed_at }

    /// Returns the signed decision's exclusive expiry.
    #[must_use]
    pub const fn decision_expires_at(&self) -> AuthorityInstant { self.decision_expires_at }
}

} // verus!
