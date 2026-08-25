//! Canonical request-specific evidence manifest.

use peritus_evidence::EvidenceRecord;
use peritus_spec::EvidenceRequirementId;
use peritus_types::{EventId, GateId, RevisionTuple, RunId, Sha256Digest};

use crate::error::reject;
use crate::{ActiveAttempt, GateArtifact, GateError, GateRejection};

use super::{GateEvidenceReceipt, MAX_PUBLISHED_GATE_EVIDENCE};

const MANIFEST_DOMAIN: &[u8] = b"peritus-d1-gate-evidence-publication-v1\0";

/// Inert normalized publication input produced only after a passing result event commits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidencePublication {
    run_id: RunId,
    gate_id: GateId,
    attempt: ActiveAttempt,
    revision: RevisionTuple,
    result_event: EventId,
    result_position: u64,
    result_digest: Sha256Digest,
    required: Vec<EvidenceRequirementId>,
    quality_artifacts: Vec<GateArtifact>,
    manifest_digest: Sha256Digest,
}

impl EvidencePublication {
    #[allow(clippy::too_many_arguments, reason = "every durable publication binding is explicit")]
    pub(crate) fn new(
        run_id: RunId,
        gate_id: GateId,
        attempt: ActiveAttempt,
        revision: RevisionTuple,
        result_event: EventId,
        result_position: u64,
        result_digest: Sha256Digest,
        required: Vec<EvidenceRequirementId>,
        quality_artifacts: Vec<GateArtifact>,
    ) -> Result<Self, GateError> {
        if result_position == 0
            || required.len() > MAX_PUBLISHED_GATE_EVIDENCE
            || quality_artifacts.len() > crate::outcome::MAX_GATE_ARTIFACTS
            || required.windows(2).any(|pair| pair[0] >= pair[1])
            || quality_artifacts.windows(2).any(|pair| pair[0] >= pair[1])
            || quality_artifacts.windows(2).any(|pair| pair[0].digest() == pair[1].digest())
        {
            return Err(reject(
                GateRejection::LimitExceeded,
                "gate evidence publication exceeds a bound or is not canonical",
            ));
        }
        let mut publication = Self {
            run_id,
            gate_id,
            attempt,
            revision,
            result_event,
            result_position,
            result_digest,
            required,
            quality_artifacts,
            manifest_digest: Sha256Digest::new([0; 32]),
        };
        publication.manifest_digest = peritus_codec::sha256(&publication.manifest_bytes());
        Ok(publication)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_wire(
        run_id: RunId,
        gate_id: GateId,
        attempt: ActiveAttempt,
        revision: RevisionTuple,
        result_event: EventId,
        result_position: u64,
        result_digest: Sha256Digest,
        required: Vec<EvidenceRequirementId>,
        quality_artifacts: Vec<GateArtifact>,
        advertised_digest: Sha256Digest,
    ) -> Result<Self, GateError> {
        let publication = Self::new(
            run_id,
            gate_id,
            attempt,
            revision,
            result_event,
            result_position,
            result_digest,
            required,
            quality_artifacts,
        )?;
        if publication.manifest_digest != advertised_digest {
            return Err(reject(
                GateRejection::EvidenceInvalid,
                "decoded evidence publication manifest digest differs",
            ));
        }
        Ok(publication)
    }

    /// Creates a request-bound receipt from the admitted C0 evidence records.
    ///
    /// # Errors
    /// Rejects missing, extra, stale, reordered, reused, or non-manifest-bound evidence records.
    pub fn receipt_from_records(
        &self,
        records: Vec<(EvidenceRequirementId, EvidenceRecord)>,
    ) -> Result<GateEvidenceReceipt, GateError> {
        GateEvidenceReceipt::from_publication(self, records)
    }

    /// Returns canonical bytes that must be finalized under [`Self::manifest_digest`].
    #[must_use]
    pub fn manifest_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(512);
        bytes.extend_from_slice(MANIFEST_DOMAIN);
        bytes.extend_from_slice(self.run_id.as_bytes());
        bytes.extend_from_slice(self.gate_id.as_bytes());
        bytes.extend_from_slice(self.attempt.execution_id().as_bytes());
        bytes.extend_from_slice(&self.attempt.ordinal().get().to_be_bytes());
        bytes.extend_from_slice(self.attempt.action_id().as_bytes());
        bytes.extend_from_slice(self.attempt.prepared_digest().as_bytes());
        bytes.extend_from_slice(self.attempt.replay_digest().as_bytes());
        bytes.extend_from_slice(self.attempt.snapshot_digest().as_bytes());
        append_revision(&mut bytes, self.revision);
        bytes.extend_from_slice(self.result_event.as_bytes());
        bytes.extend_from_slice(&self.result_position.to_be_bytes());
        bytes.extend_from_slice(self.result_digest.as_bytes());
        append_len(&mut bytes, self.required.len());
        for requirement in &self.required {
            bytes.extend_from_slice(requirement.digest().as_bytes());
        }
        append_len(&mut bytes, self.quality_artifacts.len());
        for artifact in &self.quality_artifacts {
            bytes.extend_from_slice(artifact.digest().as_bytes());
            bytes.extend_from_slice(&artifact.size().to_be_bytes());
            append_text(&mut bytes, artifact.media_type());
            append_text(&mut bytes, artifact.label());
            bytes.push(1); // D1 admits only complete quality artifacts.
            bytes.extend_from_slice(self.attempt.action_id().as_bytes());
            bytes.extend_from_slice(self.attempt.prepared_digest().as_bytes());
        }
        bytes
    }

    /// Returns the owning run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }

    /// Returns the exact gate identity.
    #[must_use]
    pub const fn gate_id(&self) -> GateId {
        self.gate_id
    }

    /// Returns every durable attempt and artifact-provenance binding.
    #[must_use]
    pub const fn attempt(&self) -> ActiveAttempt {
        self.attempt
    }

    /// Returns the exact revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }

    /// Returns the durable result event.
    #[must_use]
    pub const fn result_event(&self) -> EventId {
        self.result_event
    }

    /// Returns the durable result global position.
    #[must_use]
    pub const fn result_position(&self) -> u64 {
        self.result_position
    }

    /// Returns the complete C4 result digest.
    #[must_use]
    pub const fn result_digest(&self) -> Sha256Digest {
        self.result_digest
    }

    /// Returns the clean snapshot binding digest.
    #[must_use]
    pub const fn snapshot_digest(&self) -> Sha256Digest {
        self.attempt.snapshot_digest()
    }

    /// Borrows required declarations in canonical order.
    #[must_use]
    pub fn required(&self) -> &[EvidenceRequirementId] {
        &self.required
    }

    /// Borrows the exact complete quality artifact set.
    #[must_use]
    pub fn quality_artifacts(&self) -> &[GateArtifact] {
        &self.quality_artifacts
    }

    /// Returns the digest of the canonical publication manifest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }
}

fn append_revision(bytes: &mut Vec<u8>, revision: RevisionTuple) {
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    bytes.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
    bytes.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
}

fn append_text(bytes: &mut Vec<u8>, value: &str) {
    append_len(bytes, value.len());
    bytes.extend_from_slice(value.as_bytes());
}

fn append_len(bytes: &mut Vec<u8>, value: usize) {
    bytes.extend_from_slice(&u64::try_from(value).unwrap_or(u64::MAX).to_be_bytes());
}
