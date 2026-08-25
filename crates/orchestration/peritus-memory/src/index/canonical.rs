//! Versioned canonical bytes and real SHA-256 for rebuilt indexes.

use crate::{
    ClaimType, DeletionReason, MemoryRecord, MemoryScope, MemoryState, MemoryTombstone, Observation,
    QuarantineReason, ScopeKind, SourceProvenance,
};
#[cfg(not(verus_only))]
use peritus_codec::sha256;
use peritus_policy::ActorRole;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

const INDEX_DOMAIN: [u8; 23] = *b"peritus-memory-index\0v1";

fn canonical_bytes(active: &[MemoryRecord], tombstones: &[MemoryTombstone]) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend_from_slice(&INDEX_DOMAIN);
    push_len(&mut output, active.len());
    let mut record_index = 0;
    while record_index < active.len()
        invariant record_index <= active.len(),
        decreases active.len() - record_index,
    {
        push_record(&mut output, &active[record_index]);
        record_index += 1;
    }
    push_len(&mut output, tombstones.len());
    let mut tombstone_index = 0;
    while tombstone_index < tombstones.len()
        invariant tombstone_index <= tombstones.len(),
        decreases tombstones.len() - tombstone_index,
    {
        push_tombstone(&mut output, tombstones[tombstone_index]);
        tombstone_index += 1;
    }
    output
}

fn push_record(output: &mut Vec<u8>, record: &MemoryRecord) {
    output.extend_from_slice(record.id().as_bytes());
    push_u64(output, record.revision().get());
    push_scope(output, record.scope());
    output.push(claim_tag(record.material().claim_type()));
    output.extend_from_slice(record.content_digest().as_bytes());
    output.push(provenance_tag(record.material().provenance()));
    push_u32(output, record.material().estimated_tokens());
    push_len(output, record.material().content().len());
    output.extend_from_slice(record.material().content());
    let source_events = record.evidence().source_events().values();
    let source_len = source_events.len();
    push_len(output, source_len);
    let mut source_index = 0;
    while source_index < source_len
        invariant
            source_index <= source_len,
            source_len == source_events@.len(),
        decreases source_len - source_index,
    {
        output.extend_from_slice(source_events[source_index].as_bytes());
        source_index += 1;
    }
    let supporting = record.evidence().supporting().values();
    let supporting_len = supporting.len();
    push_len(output, supporting_len);
    let mut support_index = 0;
    while support_index < supporting_len
        invariant
            support_index <= supporting_len,
            supporting_len == supporting@.len(),
        decreases supporting_len - support_index,
    {
        output.extend_from_slice(supporting[support_index].as_bytes());
        support_index += 1;
    }
    let contradicting = record.evidence().contradicting().values();
    let contradicting_len = contradicting.len();
    push_len(output, contradicting_len);
    let mut contradiction_index = 0;
    while contradiction_index < contradicting_len
        invariant
            contradiction_index <= contradicting_len,
            contradicting_len == contradicting@.len(),
        decreases contradicting_len - contradiction_index,
    {
        output.extend_from_slice(contradicting[contradiction_index].as_bytes());
        contradiction_index += 1;
    }
    push_observation(output, record.timing().created());
    push_optional_observation(output, record.timing().reviewed());
    push_optional_observation(output, record.timing().expires());
    let features = record.features().values();
    let features_len = features.len();
    push_len(output, features_len);
    let mut feature_index = 0;
    while feature_index < features_len
        invariant
            feature_index <= features_len,
            features_len == features@.len(),
        decreases features_len - feature_index,
    {
        let feature = features[feature_index];
        output.extend_from_slice(feature.key().as_bytes());
        output.extend_from_slice(feature.digest().as_bytes());
        push_u16(output, feature.weight().basis_points().get());
        feature_index += 1;
    }
    output.push(state_tag(record.lifecycle().state()));
    push_u16(output, record.lifecycle().confidence().basis_points().get());
    push_u16(output, record.lifecycle().feedback().positive());
    push_u16(output, record.lifecycle().feedback().negative());
    push_optional_observation(output, record.lifecycle().state_observation());
    match record.lifecycle().quarantine_reason() {
        Some(reason) => { output.push(1); output.push(quarantine_tag(reason)); }
        None => output.push(0),
    }
    match record.lifecycle().superseded_by() {
        Some(id) => { output.push(1); output.extend_from_slice(id.as_bytes()); }
        None => output.push(0),
    }
}

fn push_scope(output: &mut Vec<u8>, scope: &MemoryScope) {
    output.push(scope_tag(scope.kind()));
    match scope.project() {
        Some(id) => { output.push(1); output.extend_from_slice(id.as_bytes()); }
        None => output.push(0),
    }
    match scope.workspace() {
        Some(id) => { output.push(1); output.extend_from_slice(id.as_bytes()); }
        None => output.push(0),
    }
    match scope.repository() {
        Some(id) => { output.push(1); output.extend_from_slice(id.as_bytes()); }
        None => output.push(0),
    }
    match scope.actor() {
        Some(id) => { output.push(1); output.extend_from_slice(id.as_bytes()); }
        None => output.push(0),
    }
    match scope.role() {
        Some(role) => { output.push(1); output.push(role_tag(role)); }
        None => output.push(0),
    }
}

fn push_tombstone(output: &mut Vec<u8>, tombstone: MemoryTombstone) {
    output.extend_from_slice(tombstone.memory_id().as_bytes());
    push_u64(output, tombstone.last_known_revision().get());
    push_observation(output, tombstone.deletion_observation());
    output.push(deletion_tag(tombstone.reason()));
    output.extend_from_slice(tombstone.prior_digest().as_bytes());
}

fn push_optional_observation(output: &mut Vec<u8>, observation: Option<Observation>) {
    match observation {
        Some(value) => { output.push(1); push_observation(output, value); }
        None => output.push(0),
    }
}

fn push_observation(output: &mut Vec<u8>, observation: Observation) {
    push_u64(output, observation.epoch());
    push_u64(output, observation.tick());
}

fn push_len(output: &mut Vec<u8>, value: usize) { push_u64(output, value as u64); }
#[allow(clippy::cast_possible_truncation, reason = "each cast selects a shifted byte")]
fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&[(value >> 8) as u8, value as u8]);
}

#[allow(clippy::cast_possible_truncation, reason = "each cast selects a shifted byte")]
fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&[
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ]);
}

#[allow(clippy::cast_possible_truncation, reason = "each cast selects a shifted byte")]
fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&[
        (value >> 56) as u8,
        (value >> 48) as u8,
        (value >> 40) as u8,
        (value >> 32) as u8,
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ]);
}

const fn claim_tag(value: ClaimType) -> u8 {
    match value {
        ClaimType::Fact => 0,
        ClaimType::Preference => 1,
        ClaimType::Procedure => 2,
        ClaimType::Outcome => 3,
        ClaimType::Warning => 4,
        ClaimType::Constraint => 5,
        ClaimType::Hypothesis => 6,
    }
}

const fn provenance_tag(value: SourceProvenance) -> u8 {
    match value {
        SourceProvenance::Repository => 0,
        SourceProvenance::Tool => 1,
        SourceProvenance::Provider => 2,
        SourceProvenance::External => 3,
        SourceProvenance::Agent => 4,
        SourceProvenance::Review => 5,
        SourceProvenance::User => 6,
    }
}

const fn scope_tag(value: ScopeKind) -> u8 {
    match value {
        ScopeKind::Project => 0,
        ScopeKind::Workspace => 1,
        ScopeKind::Repository => 2,
        ScopeKind::Actor => 3,
        ScopeKind::Role => 4,
    }
}

const fn state_tag(value: MemoryState) -> u8 {
    match value {
        MemoryState::Active => 0,
        MemoryState::Quarantined => 1,
        MemoryState::Expired => 2,
        MemoryState::Superseded => 3,
    }
}

const fn quarantine_tag(value: QuarantineReason) -> u8 {
    match value {
        QuarantineReason::Contradiction => 0,
        QuarantineReason::NegativeFeedback => 1,
        QuarantineReason::Unsupported => 2,
        QuarantineReason::SuspectedPoisoning => 3,
        QuarantineReason::ManualReview => 4,
    }
}

const fn deletion_tag(value: DeletionReason) -> u8 {
    match value {
        DeletionReason::UserRequest => 0,
        DeletionReason::RetentionPolicy => 1,
        DeletionReason::InvalidContent => 2,
        DeletionReason::ScopeRemoved => 3,
    }
}

const fn role_tag(value: ActorRole) -> u8 {
    match value {
        ActorRole::Writer => 0,
        ActorRole::Fixer => 1,
        ActorRole::Reviewer => 2,
        ActorRole::Evaluator => 3,
        ActorRole::GateRunner => 4,
        ActorRole::Orchestrator => 5,
        ActorRole::EvolutionAgent => 6,
        ActorRole::HumanAuthority => 7,
        ActorRole::DaemonService => 8,
        ActorRole::ProviderToolWorker => 9,
        ActorRole::Plugin => 10,
    }
}

} // verus!

// The real SHA-256 call is the audited hybrid boundary; canonical byte construction above remains
// executable Verus code.
#[cfg(not(verus_only))]
pub(super) fn index_digest(
    active: &[MemoryRecord],
    tombstones: &[MemoryTombstone],
) -> Sha256Digest {
    let bytes = canonical_bytes(active, tombstones);
    sha256(bytes.as_slice())
}
