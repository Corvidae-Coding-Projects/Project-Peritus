//! Canonical approval-request authority frames.

use peritus_policy::{ApprovalRequirement, AuthorityInstant, AuthorityTimeState, CapabilityScope};
use peritus_types::{ActionId, ActorId, ApprovalRequestId, Generation};
use sha2::{Digest, Sha256};

use super::reader::{CanonicalReader, invalid};
use super::value::{
    authority_tier_tag, decode_authority_tier, decode_independence_set, decode_instant,
    decode_permissions, decode_producing_participants, decode_review_participants, decode_revision,
    decode_risks, decode_role, decode_roles, decode_sha256, decode_use_limit, decode_validity,
    encode_independence, encode_participants, encode_permissions, encode_risks, encode_roles,
    instant_bytes, revision_bytes, role_tag, use_limit_bytes, validity_bytes,
};
use crate::digest::CanonicalEncoder;
use crate::{
    ActionDigest, ApprovalError, ApprovalRequest, ApprovalRequestDigest,
    MAX_APPROVAL_REQUEST_PREIMAGE_BYTES,
};

const REQUEST_DOMAIN: &[u8] = b"approval-request-digest";
const MAX_APPROVER_ROLES: usize = 11;

/// Encodes one checked request as its canonical domain-separated authority frame.
///
/// # Errors
///
/// Returns a bound failure or `RequestDigestMismatch` when the stored semantic digest is stale.
pub fn encode_approval_request(request: &ApprovalRequest) -> Result<Vec<u8>, ApprovalError> {
    if ApprovalRequestDigest::compute(request)? != request.digest() {
        return Err(ApprovalError::RequestDigestMismatch);
    }

    let mut encoder =
        CanonicalEncoder::record(REQUEST_DOMAIN, MAX_APPROVAL_REQUEST_PREIMAGE_BYTES)?;
    encoder.field(1, request.request_id().as_bytes())?;
    encoder.field(2, request.action_id().as_bytes())?;
    encoder.field(3, request.action_digest().sha256().as_bytes())?;
    encoder.field(4, request.requester().as_bytes())?;
    encoder.field(5, &[role_tag(request.requester_role())])?;
    encoder.field(6, request.scope().actor_id().as_bytes())?;
    encoder.field(7, &[role_tag(request.scope().role())])?;
    encoder.field(8, request.scope().environment_id().as_bytes())?;
    encoder.field(9, &encode_permissions(request.scope().permissions().as_slice())?)?;
    encoder.field(10, &revision_bytes(request.scope().revision()))?;
    encoder.field(11, &validity_bytes(request.scope().validity()))?;
    encoder.field(12, &use_limit_bytes(request.scope().use_limit()))?;
    encoder.field(13, &[authority_tier_tag(request.requirement().minimum_tier())])?;
    encoder.field(14, &encode_roles(request.requirement().approver_roles())?)?;
    encoder.field(15, &encode_independence(request.requirement().independence().as_slice())?)?;
    encoder.field(16, &validity_bytes(request.requirement().validity()))?;
    encoder.field(17, &instant_bytes(request.evaluated_at()))?;
    let mut floor = Vec::with_capacity(16);
    floor.extend_from_slice(&request.authority_time().epoch().get().to_be_bytes());
    floor.extend_from_slice(&request.authority_time().greatest_tick_millis().to_be_bytes());
    encoder.field(18, &floor)?;
    encoder.field(19, &encode_risks(request.risks().as_slice())?)?;
    encoder.field(20, request.risk_details_digest().as_bytes())?;
    encoder.field(21, &encode_participants(request.producing_participants().as_slice())?)?;
    encoder.field(22, &encode_participants(request.review_participants().as_slice())?)?;
    encoder.field(23, &validity_bytes(request.validity()))?;
    let bytes = encoder.finish();

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest: [u8; 32] = hasher.finalize().into();
    if digest != *request.digest().sha256().as_bytes() {
        return Err(ApprovalError::RequestDigestMismatch);
    }
    Ok(bytes)
}

/// Decodes one exact canonical request and recomputes its semantic digest.
///
/// # Errors
///
/// Returns `InvalidCanonicalEncoding` for malformed, noncanonical, trailing, or over-limit input.
pub fn decode_approval_request(bytes: &[u8]) -> Result<ApprovalRequest, ApprovalError> {
    let mut reader =
        CanonicalReader::record(bytes, REQUEST_DOMAIN, MAX_APPROVAL_REQUEST_PREIMAGE_BYTES)?;
    let request_id =
        ApprovalRequestId::new(super::value::exact(reader.field(1)?)?).map_err(|_| invalid())?;
    let action_id = ActionId::new(super::value::exact(reader.field(2)?)?).map_err(|_| invalid())?;
    let action_digest = ActionDigest::from_sha256(decode_sha256(reader.field(3)?)?);
    let requester = ActorId::new(super::value::exact(reader.field(4)?)?).map_err(|_| invalid())?;
    let requester_role = decode_role(reader.field(5)?)?;
    let scope_actor =
        ActorId::new(super::value::exact(reader.field(6)?)?).map_err(|_| invalid())?;
    let scope_role = decode_role(reader.field(7)?)?;
    let environment = peritus_types::EnvironmentId::new(super::value::exact(reader.field(8)?)?)
        .map_err(|_| invalid())?;
    let permissions = decode_permissions(reader.field(9)?)?;
    let revision = decode_revision(reader.field(10)?)?;
    let scope_validity = decode_validity(reader.field(11)?)?;
    let use_limit = decode_use_limit(reader.field(12)?)?;
    let minimum_tier = decode_authority_tier(reader.field(13)?)?;
    let approver_roles = decode_roles(reader.field(14)?, MAX_APPROVER_ROLES)?;
    let independence = decode_independence_set(reader.field(15)?)?;
    let requirement_validity = decode_validity(reader.field(16)?)?;
    let evaluated_at = decode_instant(reader.field(17)?)?;
    let floor = reader.field(18)?;
    if floor.len() != 16 {
        return Err(invalid());
    }
    let challenge_epoch = Generation::new(u64::from_be_bytes(super::value::exact(&floor[..8])?))
        .map_err(|_| invalid())?;
    let challenge_tick_millis = u64::from_be_bytes(super::value::exact(&floor[8..])?);
    let risks = decode_risks(reader.field(19)?)?;
    let risk_details_digest = decode_sha256(reader.field(20)?)?;
    let producing_participants = decode_producing_participants(reader.field(21)?)?;
    let review_participants = decode_review_participants(reader.field(22)?)?;
    let validity = decode_validity(reader.field(23)?)?;
    reader.finish()?;

    let scope = CapabilityScope::new(
        scope_actor,
        scope_role,
        environment,
        permissions,
        revision,
        scope_validity,
        use_limit,
    );
    let requirement =
        ApprovalRequirement::new(minimum_tier, approver_roles, independence, requirement_validity)
            .map_err(|_| invalid())?;
    let authority_time =
        AuthorityTimeState::new(AuthorityInstant::new(challenge_epoch, challenge_tick_millis));
    let request = ApprovalRequest::from_canonical_parts(
        request_id,
        action_id,
        action_digest,
        requester,
        requester_role,
        scope,
        requirement,
        evaluated_at,
        challenge_epoch,
        challenge_tick_millis,
        authority_time,
        risks,
        risk_details_digest,
        producing_participants,
        review_participants,
        validity,
    )
    .map_err(|_| invalid())?;
    let reencoded = encode_approval_request(&request).map_err(|_| invalid())?;
    if reencoded != bytes {
        return Err(invalid());
    }
    Ok(request)
}
