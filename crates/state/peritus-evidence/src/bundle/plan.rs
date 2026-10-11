//! Pure canonical portable bundle planning.

use super::format::{invalid, overflow};
use crate::manifest::{
    ArtifactManifestEntry, EvidenceManifest, JournalManifestEntry, RecordManifestEntry,
};
use crate::{
    EvidenceError, EvidenceErrorKind, EvidenceId, EvidenceRecord, EvidenceStore, Freshness,
    FreshnessRequirement, RecoveryAction,
};
use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_journal::{CommittedRecord, IntegrityExport};
use peritus_types::{RevisionTuple, Sha256Digest};
use std::collections::{BTreeMap, BTreeSet};

/// Explicit portable bundle resource limits.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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
        authority_revision: RevisionTuple,
        authority_records: Vec<EvidenceId>,
        export: &IntegrityExport,
        artifact_sizes: BTreeMap<ArtifactDigest, u64>,
        limits: BundleLimits,
    ) -> Result<Self, EvidenceError> {
        records.sort_by_key(EvidenceRecord::id);
        if records.is_empty() || records.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
            return Err(invalid("bundle evidence identities are empty, duplicate, or reordered"));
        }
        if authority_records.is_empty()
            || authority_records.windows(2).any(|pair| pair[0] >= pair[1])
            || authority_records
                .iter()
                .any(|id| records.binary_search_by_key(id, EvidenceRecord::id).is_err())
        {
            return Err(invalid("bundle authority identities are empty, absent, or reordered"));
        }
        if records.iter().any(|record| {
            record.provenance().revision_digest()
                != crate::freshness::revision_digest(record.revision())
        }) {
            return Err(invalid("bundle record revision disagrees with original provenance"));
        }
        for id in &authority_records {
            let index = records
                .binary_search_by_key(id, EvidenceRecord::id)
                .map_err(|_| invalid("bundle authority record is absent"))?;
            if !crate::verified::revisions_equal(records[index].revision(), &authority_revision) {
                return Err(invalid("bundle authority record has a different revision"));
            }
        }
        super::format::validate_ancestry(&records, &authority_records)?;
        let frames = planned_frames(&records, export, &artifact_sizes)?;
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
        if !crate::verified::bundle_plan_shape(record_count, frame_count, artifact_count, u64::MAX)
        {
            return Err(invalid("bundle section shape is invalid"));
        }
        for (previous, current) in [(0, 1), (1, 2), (2, 3)] {
            if !crate::verified::bundle_section_transition(previous, current) {
                return Err(invalid("bundle section order is invalid"));
            }
        }
        let manifest = EvidenceManifest::build(
            authority_revision,
            authority_records,
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

fn planned_frames(
    records: &[EvidenceRecord],
    export: &IntegrityExport,
    artifact_sizes: &BTreeMap<ArtifactDigest, u64>,
) -> Result<Vec<PlannedFrame>, EvidenceError> {
    let mut provenance_by_position = BTreeMap::new();
    let mut expected_artifacts = BTreeSet::new();
    for record in records {
        let provenance = record.provenance();
        if let Some(existing) =
            provenance_by_position.insert(provenance.global_position(), provenance)
            && (
                existing.event_id(),
                existing.event_hash(),
                existing.frame_digest(),
                existing.schema_digest(),
                existing.revision_digest(),
                existing.batch_hash(),
            ) != (
                provenance.event_id(),
                provenance.event_hash(),
                provenance.frame_digest(),
                provenance.schema_digest(),
                provenance.revision_digest(),
                provenance.batch_hash(),
            )
        {
            return Err(invalid("records disagree on shared journal provenance"));
        }
        expected_artifacts.extend(record.artifacts().iter().copied());
    }
    if expected_artifacts != artifact_sizes.keys().copied().collect() {
        return Err(invalid("artifact metadata does not exactly cover record references"));
    }
    let mut frames = Vec::with_capacity(provenance_by_position.len());
    for (position, provenance) in provenance_by_position {
        let index = export
            .records()
            .binary_search_by_key(&position, CommittedRecord::global_position)
            .map_err(|_| invalid("record frame is absent from scoped export"))?;
        let record = export
            .records()
            .get(index)
            .ok_or_else(|| invalid("record frame is absent from export"))?;
        if record.event_id() != provenance.event_id()
            || record.event_hash() != provenance.event_hash()
            || record.frame_digest() != provenance.frame_digest()
            || record.revision_digest() != provenance.revision_digest()
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
    Ok(frames)
}

fn serialized_size(
    manifest: &EvidenceManifest,
    records: &[EvidenceRecord],
    frames: &[PlannedFrame],
    limits: BundleLimits,
) -> Result<u64, EvidenceError> {
    let record_count = u64::try_from(records.len()).map_err(|_| overflow("record count"))?;
    let frame_count = u64::try_from(frames.len()).map_err(|_| overflow("frame count"))?;
    let artifact_count =
        u64::try_from(manifest.artifacts().len()).map_err(|_| overflow("artifact count"))?;
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
    /// Plans a canonical bundle for present authority and its complete causal history.
    ///
    /// `ids` names the records asserted as current authority. Their transitive parents are loaded
    /// automatically as historical provenance with their original revision bindings.
    ///
    /// # Errors
    ///
    /// Rejects empty/noncanonical authority identities, missing closure records, stale authority,
    /// any durable invalidation, journal mismatch, unavailable artifact metadata, and selected limits.
    /// Artifact bytes are authenticated once in private staging during assembly.
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
        let authority_records = ids.to_vec();
        let authority: BTreeSet<_> = ids.iter().copied().collect();
        let snapshot = self.causal_snapshot(ids, current_revision)?;
        let mut records = Vec::with_capacity(snapshot.len());
        let mut artifact_digests = BTreeSet::new();
        let mut artifact_sizes = BTreeMap::new();
        for (record, freshness) in snapshot {
            let id = record.id();
            let requirement = if authority.contains(&id) {
                FreshnessRequirement::PresentAuthority
            } else {
                FreshnessRequirement::HistoricalProvenance
            };
            if !freshness.satisfies(requirement) {
                let detail = match freshness {
                    Freshness::RevisionStale(_) => {
                        "present authority evidence is stale for the selected revision"
                    }
                    Freshness::Invalidated(_) => {
                        "bundle evidence was explicitly and durably invalidated"
                    }
                    Freshness::Current => "bundle evidence does not satisfy its required role",
                };
                return Err(EvidenceError::new(
                    EvidenceErrorKind::StaleEvidence,
                    RecoveryAction::ObtainFreshEvidence,
                    "plan portable evidence bundle",
                    detail,
                ));
            }
            artifact_digests.extend(record.artifacts().iter().copied());
            records.push(record);
        }
        for digest in artifact_digests {
            let metadata = artifacts
                .metadata(digest)
                .map_err(|error| EvidenceError::artifact("read bundle artifact metadata", error))?
                .filter(peritus_artifact_store::ArtifactMetadata::is_referenceable)
                .ok_or_else(|| {
                    EvidenceError::new(
                        EvidenceErrorKind::MissingArtifact,
                        RecoveryAction::RepairDependency,
                        "plan evidence bundle",
                        "artifact is not finalized and active",
                    )
                })?;
            artifact_sizes.insert(digest, metadata.size());
        }
        BundlePlan::build(
            records,
            *current_revision,
            authority_records,
            export,
            artifact_sizes,
            limits,
        )
    }
}
