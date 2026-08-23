//! Exact signed approval choices and payload bindings.

// Verus lowers documented payload variants to synthetic methods without carrying documentation.
// This module's public items are fully documented; this scopes the workaround to those artifacts.
#![allow(missing_docs)]

use peritus_policy::{ActorRole, AuthorityInstant};
use peritus_types::{
    ActorId, ApprovalRequestId, CommandId, Generation, RevisionNumber, Sha256Digest,
};
use vstd::prelude::*;

verus! {

/// One of the three exact human approval choices.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApprovalChoice {
    /// Deny this exact request.
    Deny,
    /// Authorize one logical use of this exact action.
    ApproveOnce,
    /// Authorize one exact previewed policy amendment.
    Amend(crate::AmendmentIdentity),
}

/// Exact digest-bound decision payload presented to strict authentication.
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalDecision {
    pub(crate) command_id: CommandId,
    pub(crate) responder: ActorId,
    pub(crate) approver_role: ActorRole,
    pub(crate) request_id: ApprovalRequestId,
    pub(crate) request_digest: crate::ApprovalRequestDigest,
    pub(crate) choice: ApprovalChoice,
    pub(crate) expires_at: AuthorityInstant,
    pub(crate) key_id: crate::ApprovalKeyId,
    pub(crate) credential_generation: Generation,
    pub(crate) registry_revision: RevisionNumber,
    pub(crate) digest: crate::ApprovalDecisionDigest,
}

impl ApprovalDecision {
    /// Returns the unique command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId { self.command_id }

    /// Returns the authenticated human responder.
    #[must_use]
    pub const fn responder(&self) -> ActorId { self.responder }

    /// Returns the separately asserted and signed approval role.
    #[must_use]
    pub const fn approver_role(&self) -> ActorRole { self.approver_role }

    /// Returns the exact request identity.
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId { self.request_id }

    /// Returns the exact request digest.
    #[must_use]
    pub const fn request_digest(&self) -> crate::ApprovalRequestDigest { self.request_digest }

    /// Returns the exact semantic choice.
    #[must_use]
    pub const fn choice(&self) -> ApprovalChoice { self.choice }

    /// Returns the decision's exclusive expiry instant.
    #[must_use]
    pub const fn expires_at(&self) -> AuthorityInstant { self.expires_at }

    /// Returns the signed approval key ID.
    #[must_use]
    pub const fn key_id(&self) -> crate::ApprovalKeyId { self.key_id }

    /// Returns the signed credential generation.
    #[must_use]
    pub const fn credential_generation(&self) -> Generation { self.credential_generation }

    /// Returns the signed supplied-registry revision.
    #[must_use]
    pub const fn registry_revision(&self) -> RevisionNumber { self.registry_revision }

    /// Returns the semantic decision digest.
    #[must_use]
    pub const fn digest(&self) -> crate::ApprovalDecisionDigest { self.digest }
}

/// Exact decision payload plus an unparsed 64-byte Ed25519 signature.
#[derive(Debug, Eq, PartialEq)]
pub struct SignedApprovalDecision {
    decision: ApprovalDecision,
    signature: crate::ApprovalSignature,
}

impl SignedApprovalDecision {
    /// Attaches exact signature bytes without claiming authentication.
    #[must_use]
    pub const fn new(decision: ApprovalDecision, signature: crate::ApprovalSignature) -> Self {
        Self { decision, signature }
    }

    /// Borrows the exact unprivileged decision.
    #[must_use]
    pub const fn decision(&self) -> &ApprovalDecision { &self.decision }

    /// Returns the exact unprivileged signature bytes.
    #[must_use]
    pub const fn signature(&self) -> crate::ApprovalSignature { self.signature }

    /// Consumes the signed input into its unprivileged parts.
    #[must_use]
    pub const fn into_parts(self) -> (ApprovalDecision, crate::ApprovalSignature) {
        (self.decision, self.signature)
    }
}

impl ApprovalDecision {
    #[allow(clippy::too_many_arguments, reason = "all signed authority fields remain explicit")]
    const fn new_with_digest(
        command_id: CommandId,
        responder: ActorId,
        approver_role: ActorRole,
        request_id: ApprovalRequestId,
        request_digest: crate::ApprovalRequestDigest,
        choice: ApprovalChoice,
        expires_at: AuthorityInstant,
        key_id: crate::ApprovalKeyId,
        credential_generation: Generation,
        registry_revision: RevisionNumber,
        digest: crate::ApprovalDecisionDigest,
    ) -> Self {
        Self {
            command_id,
            responder,
            approver_role,
            request_id,
            request_digest,
            choice,
            expires_at,
            key_id,
            credential_generation,
            registry_revision,
            digest,
        }
    }
}

} // verus!

impl ApprovalDecision {
    /// Constructs a decision and computes its complete canonical digest.
    ///
    /// # Errors
    ///
    /// Returns a digest-preimage failure rather than constructing a partial payload.
    #[allow(clippy::too_many_arguments, reason = "all signed authority fields remain explicit")]
    pub fn new(
        command_id: CommandId,
        responder: ActorId,
        approver_role: ActorRole,
        request_id: ApprovalRequestId,
        request_digest: crate::ApprovalRequestDigest,
        choice: ApprovalChoice,
        expires_at: AuthorityInstant,
        key_id: crate::ApprovalKeyId,
        credential_generation: Generation,
        registry_revision: RevisionNumber,
    ) -> Result<Self, crate::ApprovalError> {
        let placeholder = crate::ApprovalDecisionDigest::from_sha256(Sha256Digest::new([0; 32]));
        let mut decision = Self::new_with_digest(
            command_id,
            responder,
            approver_role,
            request_id,
            request_digest,
            choice,
            expires_at,
            key_id,
            credential_generation,
            registry_revision,
            placeholder,
        );
        decision.digest = crate::ApprovalDecisionDigest::compute(&decision)?;
        Ok(decision)
    }
}
