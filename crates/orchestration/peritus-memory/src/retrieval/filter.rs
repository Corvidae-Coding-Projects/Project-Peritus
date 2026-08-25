//! Ordered fail-closed retrieval filters.

#![allow(clippy::collapsible_if, reason = "the pinned Verus frontend lacks Rust let-chains")]

use super::output::{ExclusionReason, dominant_tombstone, state_reason};
use super::types::{RetrievalPolicy, RetrievalQuery};
use crate::{BasisPoints, MemoryError, MemoryErrorKind, MemoryField, MemoryRecord};
use peritus_role::MemoryVisibility;
use vstd::prelude::*;

verus! {

pub(super) fn exclusion(
    record: &MemoryRecord,
    tombstones: &[crate::MemoryTombstone],
    policy: &RetrievalPolicy,
    query: &RetrievalQuery,
) -> Result<Option<ExclusionReason>, MemoryError> {
    if let Some(tombstone) = dominant_tombstone(record.id(), tombstones) {
        if tombstone.last_known_revision() == record.revision()
            && tombstone.prior_digest() != record.content_digest()
        {
            return Err(MemoryError::memory(
                MemoryErrorKind::TombstoneDigestMismatch,
                MemoryField::Tombstones,
                record.id(),
            ));
        }
        if tombstone.dominates(record) {
            return Ok(Some(ExclusionReason::Tombstoned));
        }
    }
    if !record.scope().compatible_with(query.scope(), policy.scope_policy()) {
        return Ok(Some(ExclusionReason::ScopeMismatch));
    }
    if query.role().context().memory_visibility() != MemoryVisibility::EvidenceBacked {
        return Ok(Some(ExclusionReason::RolePolicy));
    }
    if let Some(reason) = state_reason(record.lifecycle().state()) {
        return Ok(Some(reason));
    }
    if record.latest_observation() > query.observation() {
        return Ok(Some(ExclusionReason::FutureObservation));
    }
    if let Some(expiry) = record.timing().expires() {
        if expiry <= query.observation() {
            return Ok(Some(ExclusionReason::ExpiryReached));
        }
    }
    if record.lifecycle().confidence() < policy.limits().minimum_confidence() {
        return Ok(Some(ExclusionReason::BelowConfidence));
    }
    if !policy.accepted_claims().contains(record.material().claim_type()) {
        return Ok(Some(ExclusionReason::UnsupportedClaim));
    }
    if record.evidence().supporting().is_empty() {
        return Ok(Some(ExclusionReason::UnsupportedEvidence));
    }
    let required = query.required_features().values();
    let required_len = required.len();
    let mut required_index = 0;
    while required_index < required_len
        invariant
            required_index <= required_len,
            required_len == required@.len(),
        decreases required_len - required_index,
    {
        if record.features().get(required[required_index]).is_none() {
            return Ok(Some(ExclusionReason::MissingRequiredFeature));
        }
        required_index += 1;
    }
    if review_is_stale(record, policy, query) {
        return Ok(Some(ExclusionReason::StaleReview));
    }
    if let Some(threshold) = policy.feedback().negative_quarantine_at() {
        if record.lifecycle().feedback().negative_ratio() >= threshold {
            return Ok(Some(ExclusionReason::NegativeFeedback));
        }
    }
    if let Some(threshold) = policy.feedback().contradiction_quarantine_at() {
        if contradiction_ratio(record)? >= threshold {
            return Ok(Some(ExclusionReason::Contradiction));
        }
    }
    Ok(None)
}

const fn review_is_stale(
    record: &MemoryRecord,
    policy: &RetrievalPolicy,
    query: &RetrievalQuery,
) -> bool {
    let Some(max_age) = policy.limits().max_review_age() else { return false };
    let Some(reviewed) = record.timing().reviewed() else { return true };
    if reviewed.epoch() != query.observation().epoch() {
        return true;
    }
    let reviewed_tick = reviewed.tick();
    let query_tick = query.observation().tick();
    if reviewed_tick > query_tick {
        return true;
    }
    query_tick - reviewed_tick > max_age
}

pub(super) fn contradiction_ratio(record: &MemoryRecord) -> Result<BasisPoints, MemoryError> {
    let supporting = record.evidence().supporting().values().len() as u64;
    let contradicting = record.evidence().contradicting().values().len() as u64;
    let total = supporting.checked_add(contradicting).ok_or(MemoryError::field(
        MemoryErrorKind::ArithmeticOverflow,
        MemoryField::Score,
    ))?;
    if total == 0 {
        return Ok(BasisPoints::ZERO);
    }
    let scaled = contradicting.checked_mul(10_000).ok_or(MemoryError::field(
        MemoryErrorKind::ArithmeticOverflow,
        MemoryField::Score,
    ))? / total;
    let Ok(converted) = u16::try_from(scaled) else {
        return Err(MemoryError::field(
            MemoryErrorKind::ArithmeticOverflow,
            MemoryField::Score,
        ));
    };
    BasisPoints::new(converted)
}

} // verus!
