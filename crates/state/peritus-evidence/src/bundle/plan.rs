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
use peritus_types::{RevisionTuple, Sha256Digest};
use std::collections::{BTreeMap, BTreeSet};

/// Explicit portable bundle resource limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "the max prefix distinguishes enforced ceilings from observed sizes"
)]
pub struct BundleLimits {
    max_entries: Option<u64>,
    max_entry_bytes: Option<u64>,
    max_bundle_bytes: Option<u64>,
}

impl BundleLimits {
    /// Creates positive explicit limits for every export dimension.
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
            Ok(Self {
                max_entries: Some(max_entries),
                max_entry_bytes: Some(max_entry_bytes),
                max_bundle_bytes: Some(max_bundle_bytes),
            })
        }
    }

    /// Creates independently optional caller-selected limits.
    ///
    /// # Errors
    ///
    /// Rejects a selected zero bound.
    pub fn optional(
        max_entries: Option<u64>,
        max_entry_bytes: Option<u64>,
        max_bundle_bytes: Option<u64>,
    ) -> Result<Self, EvidenceError> {
        if [max_entries, max_entry_bytes, max_bundle_bytes]
            .into_iter()
            .flatten()
            .any(|limit| limit == 0)
        {
            return Err(invalid("selected bundle limits must be positive"));
        }
        Ok(Self { max_entries, max_entry_bytes, max_bundle_bytes })
    }

    /// Returns the caller-selected collection-entry bound.
    #[must_use]
    pub const fn max_entries(self) -> Option<u64> {
        self.max_entries
    }
    /// Returns the caller-selected per-entry byte bound.
    #[must_use]
    pub const fn max_entry_bytes(self) -> Option<u64> {
        self.max_entry_bytes
    }
    /// Returns the caller-selected complete stream byte bound.
    #[must_use]
    pub const fn max_bundle_bytes(self) -> Option<u64> {
        self.max_bundle_bytes
    }

    pub(super) fn check_entries(self, count: u64) -> Result<(), EvidenceError> {
        if self.max_entries.is_some_and(|limit| count > limit) {
            Err(invalid("bundle collection bound exceeded"))
        } else {
            Ok(())
        }
    }

    pub(super) fn check_entry_bytes(self, bytes: u64) -> Result<(), EvidenceError> {
        if self.max_entry_bytes.is_some_and(|limit| bytes > limit) {
            Err(invalid("bundle entry exceeds byte limit"))
        } else {
            Ok(())
        }
    }

    pub(super) fn check_bundle_bytes(self, bytes: u64) -> Result<(), EvidenceError> {
        if self.max_bundle_bytes.is_some_and(|limit| bytes > limit) {
            Err(invalid("bundle exceeds complete byte limit"))
        } else {
            Ok(())
        }
    }
}

impl Default for BundleLimits {
    fn default() -> Self {
        Self { max_entries: None, max_entry_bytes: None, max_bundle_bytes: None }
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
    export_id: Sha256Digest,
    byte_count: u64,
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
    /// Returns the stable identity of these exact deterministic export bytes.
    #[must_use]
    pub const fn export_id(&self) -> Sha256Digest {
        self.export_id
    }
    /// Returns the exact complete serialized byte count.
    #[must_use]
    pub const fn byte_count(&self) -> u64 {
        self.byte_count
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
        if !crate::verified::bundle_plan_shape(record_count, frame_count) {
            return Err(invalid("bundle section shape is invalid"));
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
        let byte_count = serialized_size(&manifest, &records, &frames, limits)?;
        let export_id = export_identity(manifest.root_digest(), byte_count);
        Ok(Self { manifest, records, frames, export_id, byte_count })
    }

    pub(super) fn frames(&self) -> &[PlannedFrame] {
        &self.frames
    }

    pub(super) fn validate_limits(&self, limits: BundleLimits) -> Result<(), EvidenceError> {
        let byte_count = serialized_size(&self.manifest, &self.records, &self.frames, limits)?;
        if byte_count == self.byte_count {
            Ok(())
        } else {
            Err(invalid("bundle serialized plan changed"))
        }
    }
}

fn serialized_size(
    manifest: &EvidenceManifest,
    records: &[EvidenceRecord],
    frames: &[PlannedFrame],
    limits: BundleLimits,
) -> Result<u64, EvidenceError> {
    let record_count = u64::try_from(records.len()).map_err(|_| overflow("record count"))?;
    let frame_count = u64::try_from(frames.len()).map_err(|_| overflow("frame count"))?;
    let artifact_count = u64::try_from(manifest.artifacts().len())
        .map_err(|_| overflow("artifact count"))?;
    limits.check_entries(record_count)?;
    limits.check_entries(frame_count)?;
    limits.check_entries(artifact_count)?;

    let mut total = 8_u64;
    let manifest_size = manifest.canonical_size()?;
    limits.check_entry_bytes(manifest_size)?;
    total = add(total, 8, "manifest length")?;
    total = add(total, manifest_size, "manifest bytes")?;
    total = add(total, 8, "record count")?;
    for record in records {
        let size = record.canonical_size()?;
        limits.check_entry_bytes(size)?;
        total = add(total, 8, "record length")?;
        total = add(total, size, "record bytes")?;
    }
    total = add(total, 8, "frame count")?;
    for frame in frames {
        let size = u64::try_from(frame.bytes.len()).map_err(|_| overflow("frame bytes"))?;
        limits.check_entry_bytes(size)?;
        total = add(total, 16, "frame header")?;
        total = add(total, size, "frame bytes")?;
    }
    total = add(total, 8, "artifact count")?;
    for artifact in manifest.artifacts() {
        limits.check_entry_bytes(artifact.size())?;
        total = add(total, 40, "artifact header")?;
        total = add(total, artifact.size(), "artifact bytes")?;
    }
    total = add(total, 32, "bundle root")?;
    limits.check_bundle_bytes(total)?;
    Ok(total)
}

fn add(total: u64, value: u64, field: &'static str) -> Result<u64, EvidenceError> {
    total.checked_add(value).ok_or_else(|| overflow(field))
}

fn export_identity(root: Sha256Digest, byte_count: u64) -> Sha256Digest {
    let mut bytes = b"peritus-evidence-export-v1\0".to_vec();
    bytes.extend_from_slice(root.as_bytes());
    bytes.extend_from_slice(&byte_count.to_be_bytes());
    peritus_codec::sha256(&bytes)
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
