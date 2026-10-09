//! Pure canonical portable bundle planning.

use super::format::{invalid, overflow};
use crate::manifest::{
    ArtifactManifestEntry, EvidenceManifest, JournalManifestEntry, RecordManifestEntry,
};
use crate::{
    EvidenceError, EvidenceErrorKind, EvidenceId, EvidenceRecord, EvidenceStore, Freshness,
    RecoveryAction,
};
use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_journal::IntegrityExport;
use peritus_types::RevisionTuple;
use std::collections::{BTreeMap, BTreeSet};

/// Explicit portable bundle resource limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "the max prefix distinguishes enforced ceilings from observed sizes"
)]
pub struct BundleLimits {
    max_entries: u64,
    max_entry_bytes: u64,
    max_bundle_bytes: u64,
}

impl BundleLimits {
    /// Creates positive explicit bundle limits.
    ///
    /// # Errors
    ///
    /// Rejects any zero bound.
    pub fn new(
        max_entries: u64,
        max_entry_bytes: u64,
        max_bundle_bytes: u64,
    ) -> Result<Self, EvidenceError> {
        if max_entries == 0 || max_entry_bytes == 0 || max_bundle_bytes == 0 {
            Err(invalid("bundle limits must be positive"))
        } else {
            Ok(Self { max_entries, max_entry_bytes, max_bundle_bytes })
        }
    }
    /// Returns the collection-entry bound.
    #[must_use]
    pub const fn max_entries(self) -> u64 {
        self.max_entries
    }
    /// Returns the per-entry byte bound.
    #[must_use]
    pub const fn max_entry_bytes(self) -> u64 {
        self.max_entry_bytes
    }
    /// Returns the complete stream byte bound.
    #[must_use]
    pub const fn max_bundle_bytes(self) -> u64 {
        self.max_bundle_bytes
    }
}

impl Default for BundleLimits {
    fn default() -> Self {
        Self { max_entries: 4_096, max_entry_bytes: 1 << 30, max_bundle_bytes: 4 << 30 }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PlannedFrame {
    pub(super) entry: JournalManifestEntry,
    pub(super) bytes: Vec<u8>,
}

/// Immutable deterministic portable bundle plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BundlePlan {
    manifest: EvidenceManifest,
    records: Vec<EvidenceRecord>,
    frames: Vec<PlannedFrame>,
}

impl BundlePlan {
    /// Returns the canonical manifest.
    #[must_use]
    pub const fn manifest(&self) -> &EvidenceManifest {
        &self.manifest
    }
    /// Borrows canonical records in identity order.
    #[must_use]
    pub fn records(&self) -> &[EvidenceRecord] {
        &self.records
    }

    pub(crate) fn build(
        mut records: Vec<EvidenceRecord>,
        export: &IntegrityExport,
        artifact_sizes: BTreeMap<ArtifactDigest, u64>,
        limits: BundleLimits,
    ) -> Result<Self, EvidenceError> {
        records.sort_by_key(EvidenceRecord::id);
        if records.is_empty() || records.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
            return Err(invalid("bundle evidence identities are empty, duplicate, or reordered"));
        }
        let revision = *records[0].revision();
        if records
            .iter()
            .any(|record| !crate::verified::revisions_equal(record.revision(), &revision))
        {
            return Err(invalid("bundle records do not share one exact revision"));
        }
        super::format::validate_ancestry(&records)?;
        let mut positions = BTreeSet::new();
        let mut expected_artifacts = BTreeSet::new();
        for record in &records {
            positions.insert(record.provenance().global_position());
            expected_artifacts.extend(record.artifacts().iter().copied());
        }
        if expected_artifacts != artifact_sizes.keys().copied().collect() {
            return Err(invalid("artifact metadata does not exactly cover record references"));
        }
        let mut frames = Vec::with_capacity(positions.len());
        for position in positions {
            let index = usize::try_from(
                position.checked_sub(1).ok_or_else(|| invalid("zero frame position"))?,
            )
            .map_err(|_| overflow("frame position exceeds usize"))?;
            let record = export
                .records()
                .get(index)
                .ok_or_else(|| invalid("record frame is absent from export"))?;
            let provenance = records
                .iter()
                .find(|value| value.provenance().global_position() == position)
                .map(EvidenceRecord::provenance)
                .ok_or_else(|| invalid("frame has no record provenance"))?;
            if record.event_id() != provenance.event_id()
                || record.event_hash() != provenance.event_hash()
                || record.frame_digest() != provenance.frame_digest()
            {
                return Err(invalid("bundle frame disagrees with evidence provenance"));
            }
            let frame_size = u64::try_from(record.frame_bytes().len())
                .map_err(|_| overflow("frame size exceeds u64"))?;
            frames.push(PlannedFrame {
                entry: JournalManifestEntry::new(
                    position,
                    record.event_id(),
                    record.event_hash(),
                    record.frame_digest(),
                    provenance.schema_digest(),
                    frame_size,
                ),
                bytes: record.frame_bytes().to_vec(),
            });
        }
        let record_entries = records
            .iter()
            .map(|record| RecordManifestEntry::new(record.id(), record.record_digest()))
            .collect();
        let journal_entries = frames.iter().map(|frame| frame.entry).collect();
        let artifact_entries = artifact_sizes
            .into_iter()
            .map(|(digest, size)| ArtifactManifestEntry::new(digest, size))
            .collect::<Vec<_>>();
        let record_count =
            u64::try_from(records.len()).map_err(|_| overflow("record count exceeds u64"))?;
        let frame_count =
            u64::try_from(frames.len()).map_err(|_| overflow("frame count exceeds u64"))?;
        let artifact_count = u64::try_from(artifact_entries.len())
            .map_err(|_| overflow("artifact count exceeds u64"))?;
        if !crate::verified::bundle_plan_shape(
            record_count,
            frame_count,
            artifact_count,
            limits.max_entries(),
        ) {
            return Err(invalid("bundle collection bound exceeded"));
        }
        for (previous, current) in [(0, 1), (1, 2), (2, 3)] {
            if !crate::verified::bundle_section_transition(previous, current) {
                return Err(invalid("bundle section order is invalid"));
            }
        }
        let manifest = EvidenceManifest::build(
            revision,
            export.report().journal_head_digest(),
            record_entries,
            journal_entries,
            artifact_entries,
        )?;
        Ok(Self { manifest, records, frames })
    }

    pub(super) fn frames(&self) -> &[PlannedFrame] {
        &self.frames
    }
}

impl EvidenceStore {
    /// Plans a canonical current bundle without opening output files.
    ///
    /// # Errors
    ///
    /// Rejects empty/noncanonical identities, missing or stale records, journal mismatch, artifact
    /// corruption, and configured bundle limits.
    pub fn plan_bundle(
        &self,
        ids: &[EvidenceId],
        current_revision: &RevisionTuple,
        export: &IntegrityExport,
        artifacts: &ArtifactStore,
        limits: BundleLimits,
    ) -> Result<BundlePlan, EvidenceError> {
        if ids.is_empty() || ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(invalid("bundle identities must be nonempty and strictly ordered"));
        }
        let mut records = Vec::with_capacity(ids.len());
        let mut artifact_sizes = BTreeMap::new();
        for id in ids {
            let record =
                self.load(*id)?.ok_or_else(|| invalid("bundle evidence record is missing"))?;
            if !matches!(self.freshness(*id, current_revision)?, Freshness::Current) {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::StaleEvidence,
                    RecoveryAction::ObtainFreshEvidence,
                    "plan portable evidence bundle",
                    "bundle evidence is stale or invalidated",
                ));
            }
            for digest in record.artifacts() {
                let metadata = artifacts
                    .verify(*digest)
                    .map_err(|error| EvidenceError::artifact("verify bundle artifact", error))?;
                artifact_sizes.insert(*digest, metadata.size());
            }
            records.push(record);
        }
        BundlePlan::build(records, export, artifact_sizes, limits)
    }
}
