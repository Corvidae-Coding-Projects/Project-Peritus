//! Canonical private frames for durable review command indexes.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_spec::ReviewCategory;

use crate::history::index::{
    IndexedCycle, IndexedCyclePhase, IndexedFact, IndexedFinding, ReviewIndexKind,
    ReviewIndexManifest, ReviewIndexNode, ReviewIndexNodePayload, ReviewQuorumIndex,
};

/// Family-55 schema-3 radix node.
pub(crate) struct ReviewIndexNodeFrame(pub(crate) ReviewIndexNode);

impl ReviewIndexNodeFrame {
    pub(crate) fn into_node(self) -> ReviewIndexNode {
        self.0
    }
}

impl CanonicalEncode for ReviewIndexNodeFrame {
    const FAMILY: u16 = 55;
    const SCHEMA_VERSION: u16 = 3;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let node = &self.0;
        super::write_id(writer, node.run_id().as_bytes())?;
        writer.write_u8(kind_tag(node.kind()))?;
        writer.write_bytes(node.prefix())?;
        writer.write_u64(node.through_sequence())?;
        super::write_id(writer, node.through_event().as_bytes())?;
        match node.payload() {
            ReviewIndexNodePayload::Branch(children) => {
                writer.write_u8(1)?;
                writer.write_collection_len(children.len())?;
                for child in children {
                    writer.write_u8(*child)?;
                }
            }
            ReviewIndexNodePayload::Cycles(cycles) => {
                writer.write_u8(2)?;
                writer.write_collection_len(cycles.len())?;
                for cycle in cycles {
                    write_indexed_cycle(writer, cycle)?;
                }
            }
            ReviewIndexNodePayload::Findings(findings) => {
                writer.write_u8(3)?;
                writer.write_collection_len(findings.len())?;
                for finding in findings {
                    super::write_digest(writer, finding.binding_digest())?;
                    super::state::write_finding(writer, finding.finding())?;
                }
            }
            ReviewIndexNodePayload::Facts(facts) => {
                writer.write_u8(4)?;
                writer.write_collection_len(facts.len())?;
                for fact in facts {
                    write_fact(writer, fact)?;
                }
            }
        }
        Ok(())
    }
}

impl CanonicalDecode for ReviewIndexNodeFrame {
    const FAMILY: u16 = 55;
    const SCHEMA_VERSION: u16 = 3;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let run_id = super::read_run_id(reader)?;
        let kind_offset = reader.offset();
        let kind = match reader.read_u8()? {
            1 => ReviewIndexKind::Cycles,
            2 => ReviewIndexKind::Findings,
            3 => ReviewIndexKind::Facts,
            _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, kind_offset)),
        };
        let prefix = reader.read_bytes()?.to_vec();
        let maximum_prefix = match kind {
            ReviewIndexKind::Cycles | ReviewIndexKind::Findings => 16,
            ReviewIndexKind::Facts => 113,
        };
        if prefix.len() > maximum_prefix {
            return Err(super::invalid(reader));
        }
        let sequence_offset = reader.offset();
        let sequence = reader.read_u64()?;
        if sequence == 0 {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset));
        }
        let event_id = super::read_event_id(reader)?;
        let payload_offset = reader.offset();
        let payload = match reader.read_u8()? {
            1 => {
                if prefix.len() == maximum_prefix {
                    return Err(super::invalid(reader));
                }
                let count = super::bounded_len(reader, 256, 1)?;
                let mut children = reader.reserve_collection(count)?;
                for _ in 0..count {
                    children.push(reader.read_u8()?);
                }
                if children.is_empty() || children.windows(2).any(|pair| pair[0] >= pair[1]) {
                    return Err(super::invalid(reader));
                }
                ReviewIndexNodePayload::Branch(children)
            }
            2 if kind == ReviewIndexKind::Cycles => {
                let count = reader.read_collection_len(16)?;
                let mut cycles = reader.reserve_collection(count)?;
                for _ in 0..count {
                    cycles.push(read_indexed_cycle(reader)?);
                }
                if cycles.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
                    return Err(super::invalid(reader));
                }
                ReviewIndexNodePayload::Cycles(cycles)
            }
            3 if kind == ReviewIndexKind::Findings => {
                let count = reader.read_collection_len(16)?;
                let mut findings = reader.reserve_collection(count)?;
                for _ in 0..count {
                    findings.push(IndexedFinding::new(
                        super::read_digest(reader)?,
                        super::state::read_finding(reader)?,
                    ));
                }
                if findings.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
                    return Err(super::invalid(reader));
                }
                ReviewIndexNodePayload::Findings(findings)
            }
            4 if kind == ReviewIndexKind::Facts => {
                let count = reader.read_collection_len(17)?;
                let mut facts = reader.reserve_collection(count)?;
                for _ in 0..count {
                    facts.push(read_fact(reader)?);
                }
                if facts
                    .windows(2)
                    .any(|pair| pair[0].key_bytes() >= pair[1].key_bytes())
                {
                    return Err(super::invalid(reader));
                }
                ReviewIndexNodePayload::Facts(facts)
            }
            _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, payload_offset)),
        };
        Ok(Self(ReviewIndexNode::from_parts(
            run_id, kind, prefix, sequence, event_id, payload,
        )))
    }
}

/// Family-55 schema-4 exact index frontier.
pub(crate) struct ReviewIndexManifestFrame(pub(crate) ReviewIndexManifest);

impl ReviewIndexManifestFrame {
    pub(crate) const fn manifest(&self) -> ReviewIndexManifest {
        self.0
    }
}

impl CanonicalEncode for ReviewIndexManifestFrame {
    const FAMILY: u16 = 55;
    const SCHEMA_VERSION: u16 = 4;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_manifest(writer, self.0)
    }
}

impl CanonicalDecode for ReviewIndexManifestFrame {
    const FAMILY: u16 = 55;
    const SCHEMA_VERSION: u16 = 4;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        read_manifest(reader).map(Self)
    }
}

/// Family-55 schema-5 bounded quorum index bound to one manifest frontier.
pub(crate) struct ReviewQuorumIndexFrame {
    manifest: ReviewIndexManifest,
    quorum: ReviewQuorumIndex,
}

impl ReviewQuorumIndexFrame {
    pub(crate) const fn new(
        manifest: ReviewIndexManifest,
        quorum: ReviewQuorumIndex,
    ) -> Self {
        Self { manifest, quorum }
    }

    pub(crate) const fn manifest(&self) -> ReviewIndexManifest {
        self.manifest
    }

    pub(crate) const fn quorum(&self) -> &ReviewQuorumIndex {
        &self.quorum
    }

    pub(crate) fn into_quorum(self) -> ReviewQuorumIndex {
        self.quorum
    }
}

impl CanonicalEncode for ReviewQuorumIndexFrame {
    const FAMILY: u16 = 55;
    const SCHEMA_VERSION: u16 = 5;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_manifest(writer, self.manifest)?;
        super::write_digest(writer, self.quorum.binding_digest())?;
        let members = self.quorum.members().collect::<Vec<_>>();
        writer.write_collection_len(members.len())?;
        for member in members {
            write_indexed_cycle(writer, member)?;
        }
        super::state::write_quorum(writer, self.quorum.report())
    }
}

impl CanonicalDecode for ReviewQuorumIndexFrame {
    const FAMILY: u16 = 55;
    const SCHEMA_VERSION: u16 = 5;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let manifest = read_manifest(reader)?;
        let binding_digest = super::read_digest(reader)?;
        let count = super::bounded_len(reader, usize::from(crate::ReviewLimits::MAX_CYCLES), 16)?;
        let mut members = reader.reserve_collection(count)?;
        for _ in 0..count {
            members.push(read_indexed_cycle(reader)?);
        }
        if members.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
            return Err(super::invalid(reader));
        }
        let report = super::state::read_quorum(reader)?;
        Ok(Self {
            manifest,
            quorum: ReviewQuorumIndex::from_parts(binding_digest, members, report),
        })
    }
}

fn write_manifest(
    writer: &mut CanonicalWriter,
    manifest: ReviewIndexManifest,
) -> Result<(), CodecError> {
    super::write_id(writer, manifest.run_id().as_bytes())?;
    writer.write_u64(manifest.sequence())?;
    super::write_id(writer, manifest.event_id().as_bytes())?;
    super::write_digest(writer, manifest.state_digest())?;
    super::write_digest(writer, manifest.binding_digest())
}

fn read_manifest(reader: &mut CanonicalReader<'_>) -> Result<ReviewIndexManifest, CodecError> {
    let run_id = super::read_run_id(reader)?;
    let offset = reader.offset();
    let sequence = reader.read_u64()?;
    if sequence == 0 {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    Ok(ReviewIndexManifest::from_parts(
        run_id,
        sequence,
        super::read_event_id(reader)?,
        super::read_digest(reader)?,
        super::read_digest(reader)?,
    ))
}

fn write_indexed_cycle(
    writer: &mut CanonicalWriter,
    cycle: &IndexedCycle,
) -> Result<(), CodecError> {
    super::state::write_assignment(writer, cycle.assignment())?;
    writer.write_u8(match cycle.phase() {
        IndexedCyclePhase::Assigned => 1,
        IndexedCyclePhase::Submitted => 2,
        IndexedCyclePhase::Cancelled => 3,
        IndexedCyclePhase::Invalidated => 4,
    })?;
    writer.write_collection_len(cycle.submitted_categories().len())?;
    for category in cycle.submitted_categories() {
        super::write_digest(writer, category.digest())?;
    }
    writer.write_option_tag(cycle.review_digest().is_some())?;
    if let Some(digest) = cycle.review_digest() {
        super::write_digest(writer, digest)?;
    }
    Ok(())
}

fn read_indexed_cycle(reader: &mut CanonicalReader<'_>) -> Result<IndexedCycle, CodecError> {
    let assignment = super::state::read_assignment(reader)?;
    let phase_offset = reader.offset();
    let phase = match reader.read_u8()? {
        1 => IndexedCyclePhase::Assigned,
        2 => IndexedCyclePhase::Submitted,
        3 => IndexedCyclePhase::Cancelled,
        4 => IndexedCyclePhase::Invalidated,
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, phase_offset)),
    };
    let categories = super::read_digests(
        reader,
        crate::ReviewLimits::MAX_CATEGORIES,
        ReviewCategory::new,
    )?;
    let review_digest = reader
        .read_option_tag()?
        .then(|| super::read_digest(reader))
        .transpose()?;
    let submission_shape = if matches!(phase, IndexedCyclePhase::Submitted) {
        !categories.is_empty() && review_digest.is_some()
    } else {
        categories.is_empty() && review_digest.is_none()
    };
    if !submission_shape || categories.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(super::invalid(reader));
    }
    Ok(IndexedCycle::from_parts(assignment, phase, categories, review_digest))
}

fn write_fact(writer: &mut CanonicalWriter, fact: &IndexedFact) -> Result<(), CodecError> {
    match fact {
        IndexedFact::WaiverRequest { approval_request_id } => {
            writer.write_u8(1)?;
            super::write_id(writer, approval_request_id.as_bytes())
        }
        IndexedFact::WaiverObservation { finding_id, revision } => {
            writer.write_u8(2)?;
            super::write_id(writer, finding_id.as_bytes())?;
            super::write_revision(writer, *revision)
        }
    }
}

fn read_fact(reader: &mut CanonicalReader<'_>) -> Result<IndexedFact, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(IndexedFact::waiver_request(super::read_approval_id(reader)?)),
        2 => Ok(IndexedFact::waiver_observation(
            super::read_finding_id(reader)?,
            super::read_revision(reader)?,
        )),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn kind_tag(kind: ReviewIndexKind) -> u8 {
    match kind {
        ReviewIndexKind::Cycles => 1,
        ReviewIndexKind::Findings => 2,
        ReviewIndexKind::Facts => 3,
    }
}
