//! Pure evidence admission planning over checked dependency observations.

use crate::causality::validate_parents;
use crate::freshness::revision_digest;
use crate::provenance::schema_digest;
use crate::{
    CausalLink, EvidenceDraft, EvidenceError, EvidenceErrorKind, EvidenceId, EvidenceRecord,
    JournalProvenance, RecoveryAction,
};
use peritus_artifact_store::ArtifactDigest;
use peritus_codec::{CodecLimits, decode_frame, sha256};
use peritus_journal::{CommittedRecord, IntegrityExport};
use peritus_types::Sha256Digest;
use std::collections::BTreeMap;

/// Pure checked plan ready for one atomic evidence-catalog transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmissionPlan {
    record: EvidenceRecord,
    links: Vec<CausalLink>,
}

impl AdmissionPlan {
    /// Borrows the fully bound immutable record.
    #[must_use]
    pub const fn record(&self) -> &EvidenceRecord {
        &self.record
    }
    /// Borrows canonical direct-cause links.
    #[must_use]
    pub fn causal_links(&self) -> &[CausalLink] {
        &self.links
    }

    pub fn build(
        draft: EvidenceDraft,
        durable: &DurableJournalObservation,
        export: &IntegrityExport,
        provenance_head_digest: Sha256Digest,
        parents: &BTreeMap<EvidenceId, EvidenceRecord>,
    ) -> Result<Self, EvidenceError> {
        let exported = exported_record(export, draft.journal_position())?;
        validate_observation(exported, durable)?;
        let actual_artifacts = export
            .artifact_references()
            .iter()
            .filter(|reference| reference.batch_hash() == durable.batch_hash)
            .map(|reference| ArtifactDigest::from_sha256(reference.artifact_digest()))
            .collect::<Vec<_>>();
        if actual_artifacts != draft.artifacts() || durable.artifacts != actual_artifacts {
            return Err(mismatch("draft, export, and durable journal artifact sets disagree"));
        }
        let expected_revision = revision_digest(draft.revision());
        if expected_revision != durable.revision_digest {
            return Err(EvidenceError::new(
                EvidenceErrorKind::RevisionMismatch,
                RecoveryAction::CorrectInput,
                "plan evidence admission",
                "RevisionTuple does not match the committed journal revision digest",
            ));
        }
        let provenance = JournalProvenance::new(
            durable.global_position,
            durable.event_id,
            durable.event_hash,
            durable.batch_hash,
            provenance_head_digest,
            durable.frame_family,
            durable.frame_schema,
            durable.frame_digest,
            schema_digest(durable.frame_family, durable.frame_schema)?,
            durable.revision_digest,
        );
        let links = validate_parents(draft.id(), durable.global_position, draft.causes(), parents)?;
        Ok(Self { record: EvidenceRecord::from_draft(draft, provenance), links })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableJournalObservation {
    pub global_position: u64,
    pub event_id: peritus_types::EventId,
    pub event_hash: Sha256Digest,
    pub batch_hash: Sha256Digest,
    pub frame_family: u16,
    pub frame_schema: u16,
    pub frame_digest: Sha256Digest,
    pub revision_digest: Sha256Digest,
    pub frame: Vec<u8>,
    pub artifacts: Vec<ArtifactDigest>,
}

fn exported_record(
    export: &IntegrityExport,
    position: u64,
) -> Result<&CommittedRecord, EvidenceError> {
    let index = position
        .checked_sub(1)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| missing("journal position cannot be indexed"))?;
    let record = export.records().get(index).ok_or_else(|| missing("journal position absent"))?;
    if record.global_position() == position {
        Ok(record)
    } else {
        Err(mismatch("integrity export position is not canonical"))
    }
}

fn validate_observation(
    exported: &CommittedRecord,
    durable: &DurableJournalObservation,
) -> Result<(), EvidenceError> {
    let frame = decode_frame(&durable.frame, CodecLimits::PRODUCTION)
        .map_err(|_| mismatch("durable journal frame is not canonical B3"))?;
    if durable.global_position != exported.global_position()
        || durable.event_id != exported.event_id()
        || durable.event_hash != exported.event_hash()
        || durable.frame_digest != exported.frame_digest()
        || durable.revision_digest != exported.revision_digest()
        || durable.frame != exported.frame_bytes()
        || sha256(&durable.frame) != durable.frame_digest
        || frame.header().family() != durable.frame_family
        || frame.header().schema_version() != durable.frame_schema
    {
        Err(mismatch("durable journal row disagrees with checked export"))
    } else {
        Ok(())
    }
}

fn missing(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::MissingJournalRecord,
        RecoveryAction::RepairDependency,
        "plan evidence admission",
        detail,
    )
}

fn mismatch(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::JournalMismatch,
        RecoveryAction::RepairDependency,
        "plan evidence admission",
        detail,
    )
}
