//! Canonical replay, tombstone dominance, and posting-list construction.

use super::types::{ClaimPosting, FeaturePosting, MemoryIndex, ScopePosting};
use crate::retrieval::MAX_RETRIEVAL_INPUTS;
use crate::{
    ClaimType, FeatureKey, MemoryError, MemoryErrorKind, MemoryField, MemoryRecord, MemoryScope,
    MemoryState, MemoryTombstone,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

#[allow(clippy::needless_pass_by_value, reason = "rebuild consumes the canonical replay input")]
pub(super) fn rebuild_unhashed(
    records: Vec<MemoryRecord>,
    tombstones: Vec<MemoryTombstone>,
) -> Result<MemoryIndex, MemoryError> {
    if records.len() > MAX_RETRIEVAL_INPUTS {
        return Err(MemoryError::field(MemoryErrorKind::LimitExceeded, MemoryField::Records));
    }
    if tombstones.len() > MAX_RETRIEVAL_INPUTS {
        return Err(MemoryError::field(MemoryErrorKind::LimitExceeded, MemoryField::Tombstones));
    }
    validate_records(&records)?;
    validate_tombstones(&tombstones)?;
    validate_tombstone_digests(&records, &tombstones)?;

    let mut active = Vec::new();
    let mut index = 0;
    while index < records.len()
        invariant index <= records.len(),
        decreases records.len() - index,
    {
        let mut next = index + 1;
        while next < records.len() && records[next].id() == records[index].id()
            invariant index < next <= records.len(),
            decreases records.len() - next,
        {
            next += 1;
        }
        let latest = &records[next - 1];
        if latest.lifecycle().state() == MemoryState::Active
            && !is_tombstoned(latest, &tombstones)
        {
            active.push(latest.clone());
        }
        index = next;
    }

    let scopes = build_scope_postings(&active);
    let claims = build_claim_postings(&active);
    let features = build_feature_postings(&active);
    // The H-class wrapper replaces this sentinel with real SHA-256 before returning the index.
    let unhashed = Sha256Digest::new([0; 32]);
    Ok(MemoryIndex::from_parts(active, tombstones, scopes, claims, features, unhashed))
}

fn validate_records(records: &[MemoryRecord]) -> Result<(), MemoryError> {
    if records.len() < 2 {
        return Ok(());
    }
    let mut index = 1;
    while index < records.len()
        invariant 1 <= index <= records.len(),
        decreases records.len() - index,
    {
        let previous = &records[index - 1];
        let current = &records[index];
        if previous.id() > current.id()
            || previous.id() == current.id() && previous.revision() > current.revision()
        {
            return Err(MemoryError::memory(
                MemoryErrorKind::NonCanonicalOrder,
                MemoryField::Records,
                current.id(),
            ));
        }
        if previous.id() == current.id() && previous.revision() == current.revision() {
            return Err(MemoryError::memory(
                MemoryErrorKind::ConflictingRevision,
                MemoryField::Records,
                current.id(),
            ));
        }
        index += 1;
    }
    Ok(())
}

fn validate_tombstones(tombstones: &[MemoryTombstone]) -> Result<(), MemoryError> {
    if tombstones.len() < 2 {
        return Ok(());
    }
    let mut index = 1;
    while index < tombstones.len()
        invariant 1 <= index <= tombstones.len(),
        decreases tombstones.len() - index,
    {
        let previous = tombstones[index - 1];
        let current = tombstones[index];
        if previous.memory_id() > current.memory_id()
            || previous.memory_id() == current.memory_id()
                && previous.last_known_revision() > current.last_known_revision()
        {
            return Err(MemoryError::memory(
                MemoryErrorKind::NonCanonicalOrder,
                MemoryField::Tombstones,
                current.memory_id(),
            ));
        }
        if previous.memory_id() == current.memory_id()
            && previous.last_known_revision() == current.last_known_revision()
        {
            return Err(MemoryError::memory(
                MemoryErrorKind::ConflictingRevision,
                MemoryField::Tombstones,
                current.memory_id(),
            ));
        }
        index += 1;
    }
    Ok(())
}

fn validate_tombstone_digests(
    records: &[MemoryRecord],
    tombstones: &[MemoryTombstone],
) -> Result<(), MemoryError> {
    let mut tombstone_index = 0;
    while tombstone_index < tombstones.len()
        invariant tombstone_index <= tombstones.len(),
        decreases tombstones.len() - tombstone_index,
    {
        let tombstone = tombstones[tombstone_index];
        let mut record_index = 0;
        while record_index < records.len()
            invariant record_index <= records.len(),
            decreases records.len() - record_index,
        {
            let record = &records[record_index];
            if record.id() == tombstone.memory_id()
                && record.revision() == tombstone.last_known_revision()
                && record.content_digest() != tombstone.prior_digest()
            {
                return Err(MemoryError::memory(
                    MemoryErrorKind::TombstoneDigestMismatch,
                    MemoryField::Tombstones,
                    record.id(),
                ));
            }
            record_index += 1;
        }
        tombstone_index += 1;
    }
    Ok(())
}

fn is_tombstoned(record: &MemoryRecord, tombstones: &[MemoryTombstone]) -> bool {
    let mut index = 0;
    while index < tombstones.len()
        invariant index <= tombstones.len(),
        decreases tombstones.len() - index,
    {
        if tombstones[index].dominates(record) {
            return true;
        }
        index += 1;
    }
    false
}

fn build_scope_postings(records: &[MemoryRecord]) -> Vec<ScopePosting> {
    let mut postings: Vec<ScopePosting> = Vec::new();
    let mut record_index = 0;
    while record_index < records.len()
        invariant record_index <= records.len(),
        decreases records.len() - record_index,
    {
        let scope = *records[record_index].scope();
        let position = scope_position(&postings, scope);
        if position > postings.len() {
            return Vec::new();
        }
        if position < postings.len() && postings[position].scope() == &scope {
            postings[position].push(records[record_index].id());
        } else {
            postings.insert(position, ScopePosting::new(scope, vec![records[record_index].id()]));
        }
        record_index += 1;
    }
    postings
}

fn build_claim_postings(records: &[MemoryRecord]) -> Vec<ClaimPosting> {
    let mut postings: Vec<ClaimPosting> = Vec::new();
    let mut record_index = 0;
    while record_index < records.len()
        invariant record_index <= records.len(),
        decreases records.len() - record_index,
    {
        let claim = records[record_index].material().claim_type();
        let position = claim_position(&postings, claim);
        if position > postings.len() {
            return Vec::new();
        }
        if position < postings.len() && postings[position].claim_type() == claim {
            postings[position].push(records[record_index].id());
        } else {
            postings.insert(position, ClaimPosting::new(claim, vec![records[record_index].id()]));
        }
        record_index += 1;
    }
    postings
}

fn build_feature_postings(records: &[MemoryRecord]) -> Vec<FeaturePosting> {
    let mut postings: Vec<FeaturePosting> = Vec::new();
    let mut record_index = 0;
    while record_index < records.len()
        invariant record_index <= records.len(),
        decreases records.len() - record_index,
    {
        let features = records[record_index].features().values();
        let features_len = features.len();
        let mut feature_index = 0;
        while feature_index < features_len
            invariant
                record_index < records.len(),
                feature_index <= features_len,
                features_len == features@.len(),
            decreases features_len - feature_index,
        {
            let key = features[feature_index].key();
            let position = feature_position(&postings, key);
            if position > postings.len() {
                return Vec::new();
            }
            if position < postings.len() && postings[position].key() == key {
                postings[position].push(records[record_index].id());
            } else {
                postings.insert(
                    position,
                    FeaturePosting::new(key, vec![records[record_index].id()]),
                );
            }
            feature_index += 1;
        }
        record_index += 1;
    }
    postings
}

fn scope_position(postings: &[ScopePosting], value: MemoryScope) -> usize {
    let mut position = 0;
    while position < postings.len() && postings[position].scope() < &value
        invariant position <= postings.len(),
        decreases postings.len() - position,
    {
        position += 1;
    }
    position
}

fn claim_position(postings: &[ClaimPosting], value: ClaimType) -> usize {
    let mut position = 0;
    while position < postings.len() && postings[position].claim_type() < value
        invariant position <= postings.len(),
        decreases postings.len() - position,
    {
        position += 1;
    }
    position
}

fn feature_position(postings: &[FeaturePosting], value: FeatureKey) -> usize {
    let mut position = 0;
    while position < postings.len() && postings[position].key() < value
        invariant position <= postings.len(),
        decreases postings.len() - position,
    {
        position += 1;
    }
    position
}

} // verus!
