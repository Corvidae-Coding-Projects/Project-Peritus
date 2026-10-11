//! Opaque authenticated facts returned only by strict decision verification.

use peritus_policy::{ActorRole, AuthorityInstant, ValidityWindow};
use peritus_types::{
    ActorId, ApprovalRequestId, CommandId, Generation, RevisionNumber, Sha256Digest,
};

#[doc = " Opaque successful authentication against one exact supplied snapshot."]
#[doc = ""]
#[doc = " This move-only value is not a durable-current-registry witness or effect permit."]
#[doc = " Its fields are deliberately unavailable to callers:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_approval::{"]
#[doc = "     ApprovalChoice, ApprovalDecisionDigest, ApprovalKeyId, ApprovalRequestDigest,"]
#[doc = "     AuthenticatedApprovalObservation,"]
#[doc = " };"]
#[doc = " use peritus_policy::{ActorRole, AuthorityInstant, ValidityWindow};"]
#[doc = " use peritus_types::{ActorId, ApprovalRequestId, CommandId, Generation, RevisionNumber};"]
#[doc = ""]
#[doc = " fn forge("]
#[doc = "     request_id: ApprovalRequestId,"]
#[doc = "     request_digest: ApprovalRequestDigest,"]
#[doc = "     decision_digest: ApprovalDecisionDigest,"]
#[doc = "     command_id: CommandId,"]
#[doc = "     responder: ActorId,"]
#[doc = "     approver_role: ActorRole,"]
#[doc = "     choice: ApprovalChoice,"]
#[doc = "     key_id: ApprovalKeyId,"]
#[doc = "     credential_generation: Generation,"]
#[doc = "     registry_revision: RevisionNumber,"]
#[doc = "     credential_validity: ValidityWindow,"]
#[doc = "     decision_expires_at: AuthorityInstant,"]
#[doc = "     observed_at: AuthorityInstant,"]
#[doc = " ) -> AuthenticatedApprovalObservation {"]
#[doc = "     AuthenticatedApprovalObservation {"]
#[doc = "         request_id, request_digest, decision_digest, command_id, responder, approver_role,"]
#[doc = "         choice, key_id, credential_generation, registry_revision, credential_validity,"]
#[doc = "         decision_expires_at, observed_at,"]
#[doc = "     }"]
#[doc = " }"]
#[doc = " ```"]
#[doc = ""]
#[doc = " A verified observation is also move-only:"]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_approval::AuthenticatedApprovalObservation;"]
#[doc = ""]
#[doc = " fn duplicate(value: AuthenticatedApprovalObservation) {"]
#[doc = "     let _copy = value.clone();"]
#[doc = " }"]
#[doc = " ```"]
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
    pub(crate) registry_digest: Sha256Digest,
    pub(crate) credential_validity: ValidityWindow,
    pub(crate) decision_expires_at: AuthorityInstant,
    pub(crate) observed_at: AuthorityInstant,
}
impl AuthenticatedApprovalObservation {
    #[doc = " Returns the exact request identity."]
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId {
        self.request_id
    }
    #[doc = " Returns the request digest authenticated by the signature."]
    #[must_use]
    pub const fn request_digest(&self) -> crate::ApprovalRequestDigest {
        self.request_digest
    }
    #[doc = " Returns the exact authenticated decision digest."]
    #[must_use]
    pub const fn decision_digest(&self) -> crate::ApprovalDecisionDigest {
        self.decision_digest
    }
    #[doc = " Returns the unique signed command identity."]
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[doc = " Returns the authenticated responder actor."]
    #[must_use]
    pub const fn responder(&self) -> ActorId {
        self.responder
    }
    #[doc = " Returns the exact signed approval role."]
    #[must_use]
    pub const fn approver_role(&self) -> ActorRole {
        self.approver_role
    }
    #[doc = " Returns the exact authenticated choice."]
    #[must_use]
    pub const fn choice(&self) -> crate::ApprovalChoice {
        self.choice
    }
    #[doc = " Returns the authenticated approval key ID."]
    #[must_use]
    pub const fn key_id(&self) -> super::ApprovalKeyId {
        self.key_id
    }
    #[doc = " Returns the credential generation used for authentication."]
    #[must_use]
    pub const fn credential_generation(&self) -> Generation {
        self.credential_generation
    }
    #[doc = " Returns the exact non-authoritative supplied snapshot revision."]
    #[must_use]
    pub const fn registry_revision(&self) -> RevisionNumber {
        self.registry_revision
    }
    #[doc = " Returns the digest of the exact supplied registry snapshot used for authentication."]
    #[must_use]
    pub const fn registry_digest(&self) -> Sha256Digest {
        self.registry_digest
    }
    #[doc = " Returns the authenticated credential validity interval."]
    #[must_use]
    pub const fn credential_validity(&self) -> ValidityWindow {
        self.credential_validity
    }
    #[doc = " Returns the accepted monotonic authority time."]
    #[must_use]
    pub const fn observed_at(&self) -> AuthorityInstant {
        self.observed_at
    }
    #[doc = " Returns the signed decision\'s exclusive expiry."]
    #[must_use]
    pub const fn decision_expires_at(&self) -> AuthorityInstant {
        self.decision_expires_at
    }
}
