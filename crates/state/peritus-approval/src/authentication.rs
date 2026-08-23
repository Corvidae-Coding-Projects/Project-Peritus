//! Bounded credential snapshots and the sole strict Ed25519 verifier surface.

use ed25519_dalek::{Signature, VerifyingKey};
use peritus_policy::{ActorRole, AuthorityInstant, IndependenceRequirement, ValidityWindow};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use vstd::prelude::*;

mod credential;
mod observation;
pub use credential::{
    ApproverCredential, CredentialRegistrySnapshot, CredentialStatus,
    MAX_CREDENTIAL_APPROVAL_ROLES, MAX_CREDENTIAL_REGISTRY_ENTRIES,
};
pub use observation::AuthenticatedApprovalObservation;

verus! {

/// Exact unparsed 32-byte Ed25519 public key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ApprovalPublicKey([u8; 32]);

impl ApprovalPublicKey {
    /// Stores exact public key bytes without claiming curve validity.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self { Self(bytes) }

    /// Borrows the exact bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] { &self.0 }

    /// Validates the exact encoded length before storing public-key bytes.
    ///
    /// # Errors
    ///
    /// Returns `InvalidCryptoLength` unless `value` contains exactly 32 bytes.
    pub const fn from_slice(value: &[u8]) -> Result<Self, crate::ApprovalError> {
        if value.len() != 32 {
            return Err(crate::ApprovalError::InvalidCryptoLength);
        }
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(value);
        Ok(Self(bytes))
    }
}

/// Exact unparsed 64-byte Ed25519 signature.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ApprovalSignature([u8; 64]);

impl ApprovalSignature {
    /// Stores exact signature bytes without claiming validity.
    #[must_use]
    pub const fn new(bytes: [u8; 64]) -> Self { Self(bytes) }

    /// Borrows the exact bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 64] { &self.0 }

    /// Validates the exact encoded length before storing signature bytes.
    ///
    /// # Errors
    ///
    /// Returns `InvalidCryptoLength` unless `value` contains exactly 64 bytes.
    pub const fn from_slice(value: &[u8]) -> Result<Self, crate::ApprovalError> {
        if value.len() != 64 {
            return Err(crate::ApprovalError::InvalidCryptoLength);
        }
        let mut bytes = [0_u8; 64];
        bytes.copy_from_slice(value);
        Ok(Self(bytes))
    }
}

/// Domain-separated public-key identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApprovalKeyId(Sha256Digest);

impl ApprovalKeyId {
    /// Returns the exact key-identifier digest bytes used by registry specifications.
    pub closed spec fn spec_bytes(&self) -> [u8; 32] { self.0.spec_bytes() }

    /// Returns the exact identifier digest.
    #[must_use]
    pub const fn sha256(self) -> (digest: Sha256Digest)
        ensures digest.spec_bytes() == self.spec_bytes(),
    { self.0 }
}

} // verus!

impl ApprovalKeyId {
    /// Computes the algorithm-tagged domain-separated key identifier.
    ///
    /// # Errors
    ///
    /// Returns a preimage bound failure without producing an identifier.
    pub fn compute(public_key: ApprovalPublicKey) -> Result<Self, crate::ApprovalError> {
        let mut encoder = crate::digest::CanonicalEncoder::record(
            b"approval-key-id",
            crate::MAX_APPROVAL_KEY_ID_PREIMAGE_BYTES,
        )?;
        encoder.field(1, b"ed25519-strict-v1")?;
        encoder.field(2, public_key.as_bytes())?;
        let mut hasher = Sha256::new();
        hasher.update(encoder.finish());
        Ok(Self(Sha256Digest::new(hasher.finalize().into())))
    }
}

verus! {

pub open spec fn spec_contains_role(roles: Seq<ActorRole>, target: ActorRole) -> bool {
    exists |index: int| 0 <= index < roles.len()
        && #[trigger] roles[index].spec_rank() == target.spec_rank()
}

pub fn contains_role(roles: &[ActorRole], target: ActorRole) -> (result: bool)
    ensures result == spec_contains_role(roles@, target),
{
    let target_rank = crate::digest::role_tag(target);
    let mut index = 0;
    while index < roles.len()
        invariant
            0 <= index <= roles.len(),
            target_rank as int == target.spec_rank(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] roles@[prior].spec_rank() != target.spec_rank(),
        decreases roles.len() - index,
    {
        if crate::digest::role_tag(roles[index]) == target_rank {
            assert(spec_contains_role(roles@, target)) by {
                assert(exists |found: int| found == index
                    && 0 <= found < roles@.len()
                    && #[trigger] roles@[found].spec_rank() == target.spec_rank());
            }
            return true;
        }
        index += 1;
    }
    false
}

fn check_window(window: ValidityWindow, now: AuthorityInstant) -> Result<(), crate::ApprovalError> {
    if window.not_before().epoch() != now.epoch() || window.expires_at().epoch() != now.epoch() {
        return Err(crate::ApprovalError::ClockEpochMismatch);
    }
    if now.tick_millis() < window.not_before().tick_millis() {
        return Err(crate::ApprovalError::NotYetValid);
    }
    if now.tick_millis() >= window.expires_at().tick_millis() {
        return Err(crate::ApprovalError::Expired);
    }
    Ok(())
}

} // verus!

/// Strictly authenticates one complete decision against one supplied registry snapshot.
///
/// The returned observation binds the snapshot revision but deliberately does not claim that the
/// snapshot is the current durable registry.
///
/// # Errors
///
/// Returns the exact malformed, binding, credential, time, independence, or signature failure.
#[allow(
    clippy::too_many_lines,
    reason = "the single mandated verifier exclusion lexically owns every strict encoding and crypto check"
)]
pub fn verify_signed_decision(
    request: &crate::ApprovalRequest,
    signed: &crate::SignedApprovalDecision,
    registry: &CredentialRegistrySnapshot,
    observed_at: AuthorityInstant,
) -> Result<AuthenticatedApprovalObservation, crate::ApprovalError> {
    let order_l: [u8; 32] = [
        0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde,
        0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x10,
    ];
    let decision = signed.decision();
    if crate::ApprovalRequestDigest::compute(request)? != request.digest() {
        return Err(crate::ApprovalError::RequestDigestMismatch);
    }
    if crate::ApprovalDecisionDigest::compute(decision)? != decision.digest() {
        return Err(crate::ApprovalError::DecisionDigestMismatch);
    }
    if decision.request_id() != request.request_id() {
        return Err(crate::ApprovalError::BindingMismatch(crate::ScopeDimension::Request));
    }
    if decision.request_digest() != request.digest() {
        return Err(crate::ApprovalError::BindingMismatch(crate::ScopeDimension::RequestDigest));
    }
    if decision.registry_revision() != registry.revision() {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::RegistryRevision,
        ));
    }
    let credential =
        registry.credential(decision.key_id()).ok_or(crate::ApprovalError::CredentialMissing)?;
    if credential.status() != CredentialStatus::Enabled {
        return Err(crate::ApprovalError::CredentialMismatch(crate::CredentialDimension::Status));
    }
    if credential.key_id() != ApprovalKeyId::compute(credential.public_key())? {
        return Err(crate::ApprovalError::CredentialMismatch(crate::CredentialDimension::KeyId));
    }
    if credential.generation() != decision.credential_generation() {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::Generation,
        ));
    }
    if credential.actor() != decision.responder() {
        return Err(crate::ApprovalError::CredentialMismatch(crate::CredentialDimension::Actor));
    }
    if credential.principal_role() != ActorRole::HumanAuthority {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::PrincipalRole,
        ));
    }
    if !contains_role(request.requirement().approver_roles(), decision.approver_role())
        || !contains_role(credential.allowed_approval_roles(), decision.approver_role())
    {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::ApprovalRole,
        ));
    }
    if credential.environment() != request.scope().environment_id() {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::Environment,
        ));
    }
    if credential.workspace() != request.scope().revision().workspace_id() {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::Workspace,
        ));
    }
    if !credential.maximum_tier().at_least(request.requirement().minimum_tier()) {
        return Err(crate::ApprovalError::CredentialMismatch(
            crate::CredentialDimension::AuthorityTier,
        ));
    }
    let floor = request.authority_time();
    if floor.epoch() != observed_at.epoch() {
        return Err(crate::ApprovalError::ClockEpochMismatch);
    }
    if observed_at.tick_millis() < floor.greatest_tick_millis() {
        return Err(crate::ApprovalError::ClockRegression);
    }
    check_window(request.validity(), observed_at)?;
    check_window(request.scope().validity(), observed_at)?;
    check_window(request.requirement().validity(), observed_at)?;
    check_window(credential.validity(), observed_at)?;
    if decision.expires_at().epoch() != observed_at.epoch() {
        return Err(crate::ApprovalError::ClockEpochMismatch);
    }
    if observed_at.tick_millis() >= decision.expires_at().tick_millis() {
        return Err(crate::ApprovalError::Expired);
    }
    let requirements = request.requirement().independence().as_slice();
    let mut requirement_index = 0;
    while requirement_index < requirements.len() {
        let conflicted = match requirements[requirement_index] {
            IndependenceRequirement::NotRequester => decision.responder() == request.requester(),
            IndependenceRequirement::NotActionActor => {
                decision.responder() == request.scope().actor_id()
            }
            IndependenceRequirement::NoProducingAttemptParticipation => {
                request.producing_participants().contains(decision.responder())
            }
            IndependenceRequirement::NoReviewParticipation => {
                request.review_participants().contains(decision.responder())
            }
        };
        if conflicted {
            return Err(crate::ApprovalError::IndependenceViolation);
        }
        requirement_index += 1;
    }

    let public_bytes = credential.public_key();
    let Ok(verifying) = VerifyingKey::from_bytes(public_bytes.as_bytes()) else {
        return Err(crate::ApprovalError::InvalidCryptoEncoding);
    };
    let public_point = verifying.to_edwards();
    if public_point.compress().to_bytes() != *public_bytes.as_bytes()
        || public_point.is_small_order()
        || !public_point.is_torsion_free()
    {
        return Err(crate::ApprovalError::InvalidCryptoEncoding);
    }
    let signature_bytes = signed.signature();
    let mut r_bytes = [0_u8; 32];
    r_bytes.copy_from_slice(&signature_bytes.as_bytes()[..32]);
    let Ok(r_encoding) = VerifyingKey::from_bytes(&r_bytes) else {
        return Err(crate::ApprovalError::InvalidCryptoEncoding);
    };
    let r_point = r_encoding.to_edwards();
    if r_point.compress().to_bytes() != r_bytes
        || r_point.is_small_order()
        || !r_point.is_torsion_free()
    {
        return Err(crate::ApprovalError::InvalidCryptoEncoding);
    }
    let scalar = &signature_bytes.as_bytes()[32..];
    let mut canonical_scalar = false;
    let mut scalar_index = 32;
    while scalar_index > 0 {
        scalar_index -= 1;
        if scalar[scalar_index] < order_l[scalar_index] {
            canonical_scalar = true;
            break;
        }
        if scalar[scalar_index] > order_l[scalar_index] {
            break;
        }
    }
    if !canonical_scalar {
        return Err(crate::ApprovalError::InvalidCryptoEncoding);
    }
    let parsed = Signature::from_bytes(signature_bytes.as_bytes());
    let mut message_encoder = crate::digest::CanonicalEncoder::record(
        b"approval-signed-decision",
        crate::MAX_APPROVAL_KEY_ID_PREIMAGE_BYTES,
    )?;
    message_encoder.field(1, decision.digest().sha256().as_bytes())?;
    let message = message_encoder.finish();
    if verifying.verify_strict(&message, &parsed).is_err() {
        return Err(crate::ApprovalError::SignatureInvalid);
    }

    Ok(AuthenticatedApprovalObservation {
        request_id: decision.request_id(),
        request_digest: decision.request_digest(),
        decision_digest: decision.digest(),
        command_id: decision.command_id(),
        responder: decision.responder(),
        approver_role: decision.approver_role(),
        choice: decision.choice(),
        key_id: decision.key_id(),
        credential_generation: decision.credential_generation(),
        registry_revision: decision.registry_revision(),
        credential_validity: credential.validity(),
        decision_expires_at: decision.expires_at(),
        observed_at,
    })
}
