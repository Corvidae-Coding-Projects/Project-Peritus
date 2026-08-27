//! Canonical signed approval-decision authority frames.

use peritus_types::{ActorId, ApprovalRequestId, CommandId, PolicyId};
use sha2::{Digest, Sha256};

use super::reader::{CanonicalReader, invalid};
use super::value::{
    decode_generation, decode_instant, decode_policy_tier, decode_revision_number, decode_role,
    decode_sha256, exact, instant_bytes, policy_tier_tag, role_tag,
};
use crate::digest::CanonicalEncoder;
use crate::{
    AmendmentIdentity, ApprovalChoice, ApprovalDecision, ApprovalDecisionDigest, ApprovalError,
    ApprovalKeyId, ApprovalRequestDigest, ApprovalSignature, MAX_APPROVAL_DECISION_PREIMAGE_BYTES,
    SignedApprovalDecision,
};

const DECISION_DOMAIN: &[u8] = b"approval-decision-digest";
const SIGNED_DECISION_DOMAIN: &[u8] = b"signed-approval-decision-codec";
const MAX_SIGNED_APPROVAL_DECISION_BYTES: usize = MAX_APPROVAL_DECISION_PREIMAGE_BYTES + 256;

fn encode_decision(decision: &ApprovalDecision) -> Result<Vec<u8>, ApprovalError> {
    if ApprovalDecisionDigest::compute(decision)? != decision.digest() {
        return Err(ApprovalError::DecisionDigestMismatch);
    }

    let mut encoder =
        CanonicalEncoder::record(DECISION_DOMAIN, MAX_APPROVAL_DECISION_PREIMAGE_BYTES)?;
    encoder.field(1, decision.command_id().as_bytes())?;
    encoder.field(2, decision.responder().as_bytes())?;
    encoder.field(3, &[role_tag(decision.approver_role())])?;
    encoder.field(4, decision.request_id().as_bytes())?;
    encoder.field(5, decision.request_digest().sha256().as_bytes())?;
    match decision.choice() {
        ApprovalChoice::Deny => encoder.field(6, &[0])?,
        ApprovalChoice::ApproveOnce => encoder.field(6, &[1])?,
        ApprovalChoice::Amend(identity) => {
            encoder.field(6, &[2])?;
            encoder.field(7, identity.base_policy_id().as_bytes())?;
            encoder.field(8, identity.successor_policy_id().as_bytes())?;
            encoder.field(9, &[policy_tier_tag(identity.tier())])?;
            encoder.field(10, identity.amendment_digest().as_bytes())?;
        }
    }
    encoder.field(11, &instant_bytes(decision.expires_at()))?;
    encoder.field(12, decision.key_id().sha256().as_bytes())?;
    encoder.field(13, &decision.credential_generation().get().to_be_bytes())?;
    encoder.field(14, &decision.registry_revision().get().to_be_bytes())?;
    let bytes = encoder.finish();

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest: [u8; 32] = hasher.finalize().into();
    if digest != *decision.digest().sha256().as_bytes() {
        return Err(ApprovalError::DecisionDigestMismatch);
    }
    Ok(bytes)
}

fn decode_decision(bytes: &[u8]) -> Result<ApprovalDecision, ApprovalError> {
    let mut reader =
        CanonicalReader::record(bytes, DECISION_DOMAIN, MAX_APPROVAL_DECISION_PREIMAGE_BYTES)?;
    let command_id = CommandId::new(exact(reader.field(1)?)?).map_err(|_| invalid())?;
    let responder = ActorId::new(exact(reader.field(2)?)?).map_err(|_| invalid())?;
    let approver_role = decode_role(reader.field(3)?)?;
    let request_id = ApprovalRequestId::new(exact(reader.field(4)?)?).map_err(|_| invalid())?;
    let request_digest = ApprovalRequestDigest::from_sha256(decode_sha256(reader.field(5)?)?);
    let choice = match reader.field(6)? {
        [0] => ApprovalChoice::Deny,
        [1] => ApprovalChoice::ApproveOnce,
        [2] => {
            let base_policy_id = PolicyId::new(exact(reader.field(7)?)?).map_err(|_| invalid())?;
            let successor_policy_id =
                PolicyId::new(exact(reader.field(8)?)?).map_err(|_| invalid())?;
            let tier = decode_policy_tier(reader.field(9)?)?;
            let amendment_digest = decode_sha256(reader.field(10)?)?;
            ApprovalChoice::Amend(
                AmendmentIdentity::new(base_policy_id, successor_policy_id, tier, amendment_digest)
                    .map_err(|_| invalid())?,
            )
        }
        _ => return Err(invalid()),
    };
    let expires_at = decode_instant(reader.field(11)?)?;
    let key_id = ApprovalKeyId::from_sha256(decode_sha256(reader.field(12)?)?);
    let credential_generation = decode_generation(reader.field(13)?)?;
    let registry_revision = decode_revision_number(reader.field(14)?)?;
    reader.finish()?;

    let decision = ApprovalDecision::new(
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
    )
    .map_err(|_| invalid())?;
    let reencoded = encode_decision(&decision).map_err(|_| invalid())?;
    if reencoded != bytes {
        return Err(invalid());
    }
    Ok(decision)
}

/// Encodes one unprivileged decision and its exact signature as a canonical authority frame.
///
/// # Errors
///
/// Returns a bound failure or `DecisionDigestMismatch` when the stored semantic digest is stale.
pub fn encode_signed_decision(signed: &SignedApprovalDecision) -> Result<Vec<u8>, ApprovalError> {
    let decision = encode_decision(signed.decision())?;
    let mut encoder =
        CanonicalEncoder::record(SIGNED_DECISION_DOMAIN, MAX_SIGNED_APPROVAL_DECISION_BYTES)?;
    encoder.field(1, &decision)?;
    encoder.field(2, signed.signature().as_bytes())?;
    Ok(encoder.finish())
}

/// Decodes one exact canonical signed decision and recomputes its semantic decision digest.
///
/// Signature authentication remains the responsibility of `verify_signed_decision`.
///
/// # Errors
///
/// Returns `InvalidCanonicalEncoding` for malformed, noncanonical, trailing, or over-limit input.
pub fn decode_signed_decision(bytes: &[u8]) -> Result<SignedApprovalDecision, ApprovalError> {
    let mut reader =
        CanonicalReader::record(bytes, SIGNED_DECISION_DOMAIN, MAX_SIGNED_APPROVAL_DECISION_BYTES)?;
    let decision = decode_decision(reader.field(1)?)?;
    let signature = ApprovalSignature::from_slice(reader.field(2)?).map_err(|_| invalid())?;
    reader.finish()?;
    let signed = SignedApprovalDecision::new(decision, signature);
    let reencoded = encode_signed_decision(&signed).map_err(|_| invalid())?;
    if reencoded != bytes {
        return Err(invalid());
    }
    Ok(signed)
}
