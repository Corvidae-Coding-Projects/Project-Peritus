//! Strict receipt construction and replay decoding.

use std::collections::BTreeSet;

use peritus_evidence::EvidenceRecord;
use peritus_spec::EvidenceRequirementId;
use peritus_types::{EventId, GateExecutionId, GateId, RevisionTuple, RunId, Sha256Digest};

use crate::error::reject;
use crate::{ActiveAttempt, GateArtifact, GateError, GateRejection};

use super::{EvidencePublication, MAX_PUBLISHED_GATE_EVIDENCE, PublishedGateEvidence};

/// Complete idempotent evidence-publication receipt for one passing attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateEvidenceReceipt {
    publication: Box<EvidencePublication>,
    evidence: Vec<PublishedGateEvidence>,
    receipt_digest: Sha256Digest,
}

impl GateEvidenceReceipt {
    pub(super) fn from_publication(
        publication: &EvidencePublication,
        records: Vec<(EvidenceRequirementId, EvidenceRecord)>,
    ) -> Result<Self, GateError> {
        if records.len() > MAX_PUBLISHED_GATE_EVIDENCE
            || records.len() != publication.required().len()
        {
            return Err(reject(
                GateRejection::EvidenceInvalid,
                "admitted gate evidence does not cover the exact required set",
            ));
        }
        let mut evidence = Vec::with_capacity(records.len());
        for (index, (requirement, record)) in records.into_iter().enumerate() {
            if publication.required().get(index) != Some(&requirement)
                || record.revision() != &publication.revision()
                || !record
                    .artifacts()
                    .iter()
                    .any(|artifact| artifact.as_bytes() == publication.manifest_digest().as_bytes())
            {
                return Err(reject(
                    GateRejection::EvidenceInvalid,
                    "gate evidence requirement, revision, or publication manifest differs",
                ));
            }
            let provenance = record.provenance();
            evidence.push(PublishedGateEvidence::from_parts(
                requirement,
                record.id(),
                record.record_digest(),
                provenance.global_position(),
                provenance.event_id(),
            ));
        }
        Self::finish(publication.clone(), evidence, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_wire(
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
        evidence: Vec<PublishedGateEvidence>,
        advertised_receipt: Sha256Digest,
    ) -> Result<Self, GateError> {
        let publication = EvidencePublication::from_wire(
            run_id,
            gate_id,
            attempt,
            revision,
            result_event,
            result_position,
            result_digest,
            required,
            quality_artifacts,
            manifest_digest,
        )?;
        Self::finish(publication, evidence, Some(advertised_receipt))
    }

    fn finish(
        publication: EvidencePublication,
        evidence: Vec<PublishedGateEvidence>,
        advertised_receipt: Option<Sha256Digest>,
    ) -> Result<Self, GateError> {
        let exact_requirements = evidence.len() == publication.required().len()
            && evidence
                .iter()
                .zip(publication.required())
                .all(|(item, requirement)| item.requirement_id() == *requirement);
        if evidence.len() > MAX_PUBLISHED_GATE_EVIDENCE
            || !exact_requirements
            || !evidence_is_one_to_one(&evidence)
            || evidence.iter().any(|item| item.journal_position() == 0)
            || evidence.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(reject(
                GateRejection::EvidenceInvalid,
                "gate evidence receipt is incomplete, reused, or noncanonical",
            ));
        }
        let receipt_digest = receipt_digest(publication.manifest_digest(), &evidence);
        if advertised_receipt.is_some_and(|advertised| advertised != receipt_digest) {
            return Err(reject(
                GateRejection::EvidenceInvalid,
                "decoded gate evidence receipt digest differs",
            ));
        }
        Ok(Self { publication: Box::new(publication), evidence, receipt_digest })
    }

    /// Borrows the complete request-specific publication binding.
    #[must_use]
    pub fn publication(&self) -> &EvidencePublication {
        &self.publication
    }

    /// Returns the owning run identity.
    #[must_use]
    pub fn run_id(&self) -> RunId {
        self.publication.run_id()
    }

    /// Returns the exact gate identity.
    #[must_use]
    pub fn gate_id(&self) -> GateId {
        self.publication.gate_id()
    }

    /// Returns the exact passing execution identity.
    #[must_use]
    pub fn execution_id(&self) -> GateExecutionId {
        self.publication.attempt().execution_id()
    }

    /// Returns the complete attempt and artifact-provenance binding.
    #[must_use]
    pub fn attempt(&self) -> ActiveAttempt {
        self.publication.attempt()
    }

    /// Returns the complete revision tuple.
    #[must_use]
    pub fn revision(&self) -> RevisionTuple {
        self.publication.revision()
    }

    /// Returns the exact durable result event.
    #[must_use]
    pub fn result_event(&self) -> EventId {
        self.publication.result_event()
    }

    /// Returns the exact durable result global position.
    #[must_use]
    pub fn result_position(&self) -> u64 {
        self.publication.result_position()
    }

    /// Returns the complete C4 terminal digest.
    #[must_use]
    pub fn result_digest(&self) -> Sha256Digest {
        self.publication.result_digest()
    }

    /// Returns the clean snapshot binding digest.
    #[must_use]
    pub fn snapshot_digest(&self) -> Sha256Digest {
        self.publication.snapshot_digest()
    }

    /// Borrows the exact quality artifact set.
    #[must_use]
    pub fn quality_artifacts(&self) -> &[GateArtifact] {
        self.publication.quality_artifacts()
    }

    /// Returns the canonical normalized manifest artifact digest.
    #[must_use]
    pub fn manifest_digest(&self) -> Sha256Digest {
        self.publication.manifest_digest()
    }

    /// Borrows evidence observations in canonical requirement order.
    #[must_use]
    pub fn evidence(&self) -> &[PublishedGateEvidence] {
        &self.evidence
    }

    /// Returns the digest of the publication binding and every admitted record.
    #[must_use]
    pub const fn receipt_digest(&self) -> Sha256Digest {
        self.receipt_digest
    }
}

fn evidence_is_one_to_one(evidence: &[PublishedGateEvidence]) -> bool {
    let mut identities = BTreeSet::new();
    let mut record_digests = BTreeSet::new();
    let mut provenance = BTreeSet::new();
    evidence.iter().all(|item| {
        identities.insert(item.evidence_id())
            && record_digests.insert(item.record_digest())
            && provenance.insert((item.journal_position(), item.producing_event()))
    })
}

fn receipt_digest(
    manifest_digest: Sha256Digest,
    evidence: &[PublishedGateEvidence],
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(96 + evidence.len() * 96);
    bytes.extend_from_slice(b"peritus-d1-gate-evidence-receipt-v2\0");
    bytes.extend_from_slice(manifest_digest.as_bytes());
    bytes.extend_from_slice(&u64::try_from(evidence.len()).unwrap_or(u64::MAX).to_be_bytes());
    for item in evidence {
        bytes.extend_from_slice(item.requirement_id().digest().as_bytes());
        bytes.extend_from_slice(item.evidence_id().as_bytes());
        bytes.extend_from_slice(item.record_digest().as_bytes());
        bytes.extend_from_slice(&item.journal_position().to_be_bytes());
        bytes.extend_from_slice(item.producing_event().as_bytes());
    }
    peritus_codec::sha256(&bytes)
}

#[cfg(test)]
mod tests {
    use peritus_quality_policy::GateAttemptOrdinal;
    use peritus_spec::EvidenceRequirementId;
    use peritus_types::{ActionId, EventId, GateExecutionId, RevisionTuple, Sha256Digest};

    use super::*;
    use crate::test_support as support;

    #[test]
    fn two_requirements_cannot_reuse_one_evidence_record() {
        let fixture = support::fixture(1);
        let attempt = ActiveAttempt::new(
            GateExecutionId::new([31; 16]).expect("execution"),
            GateAttemptOrdinal::new(1).expect("ordinal"),
            ActionId::new([32; 16]).expect("action"),
            support::digest(33),
            support::digest(34),
            fixture.snapshot,
        );
        let requirements = [
            EvidenceRequirementId::new(support::digest(35)),
            EvidenceRequirementId::new(support::digest(36)),
        ];
        let publication = EvidencePublication::new(
            fixture.run_id,
            fixture.first,
            attempt,
            fixture.revision,
            EventId::new([37; 16]).expect("result event"),
            38,
            support::digest(39),
            requirements.to_vec(),
            Vec::new(),
        )
        .expect("publication");
        let record = evidence_record(fixture.revision, publication.manifest_digest());
        let error = publication
            .receipt_from_records(vec![
                (requirements[0], record.clone()),
                (requirements[1], record),
            ])
            .expect_err("one record cannot discharge two requirements");
        assert_eq!(error.kind(), crate::GateErrorKind::Rejected(GateRejection::EvidenceInvalid));
    }

    fn evidence_record(revision: RevisionTuple, manifest: Sha256Digest) -> EvidenceRecord {
        let mut body = b"peritus-evidence-record-v1\0".to_vec();
        body.extend_from_slice(&[40; 16]);
        put_text(&mut body, "execution-result");
        put_text(&mut body, "local-runner");
        put_revision(&mut body, revision);
        body.extend_from_slice(&43_u64.to_be_bytes());
        body.extend_from_slice(&[42; 16]);
        for digest in [44_u8, 45, 46] {
            body.extend_from_slice(support::digest(digest).as_bytes());
        }
        body.extend_from_slice(&50_u16.to_be_bytes());
        body.extend_from_slice(&1_u16.to_be_bytes());
        for digest in [47_u8, 48, 49, 50] {
            body.extend_from_slice(support::digest(digest).as_bytes());
        }
        body.extend_from_slice(&1_u64.to_be_bytes());
        body.extend_from_slice(manifest.as_bytes());
        body.extend_from_slice(&0_u64.to_be_bytes());

        let mut portable = Vec::with_capacity(body.len() + 40);
        portable.extend_from_slice(&u64::try_from(body.len()).expect("body length").to_be_bytes());
        portable.extend_from_slice(&body);
        portable.extend_from_slice(peritus_codec::sha256(&body).as_bytes());
        EvidenceRecord::verify_portable(&portable).expect("portable evidence record")
    }

    fn put_text(bytes: &mut Vec<u8>, value: &str) {
        bytes.extend_from_slice(&u64::try_from(value.len()).expect("text length").to_be_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }

    fn put_revision(bytes: &mut Vec<u8>, revision: RevisionTuple) {
        bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
        bytes.extend_from_slice(revision.harness_id().as_bytes());
        bytes.extend_from_slice(revision.workspace_id().as_bytes());
        bytes.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
        bytes.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
        bytes.extend_from_slice(revision.policy_id().as_bytes());
        bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
    }
}
