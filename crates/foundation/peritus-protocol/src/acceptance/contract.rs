//! Digest-bound complete canonical acceptance contracts.

#![allow(
    clippy::missing_errors_doc,
    reason = "contract codecs use the shared CodecError and checked SpecError vocabularies"
)]

use super::dto::{GateDefinitionDto, ReviewPolicyDto};
use super::values::{
    read_approval_policy, read_content_ref, read_documents, read_evidence, read_gate, read_review,
    read_waiver_policy, try_gate, try_review, write_approval_policy, write_content_ref,
    write_documents, write_evidence, write_gate, write_review, write_waiver_policy,
};
use crate::SCHEMA_V1;
use crate::primitive::{read_digest, read_id, write_digest, write_id};
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
    CodecLimits, canonical_sha256,
};
use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContractDocuments, EvidenceRequirement,
    Exclusion, GateGraph, HumanApprovalPolicy, Requirement, RequirementId, SpecError, WaiverPolicy,
};
use peritus_types::{AcceptanceSpecId, Sha256Digest};

#[derive(Clone, Debug, Eq, PartialEq)]
struct AcceptanceContractContent {
    id: AcceptanceSpecId,
    documents: ContractDocuments,
    requirements: Vec<Requirement>,
    exclusions: Vec<Exclusion>,
    assumptions: Vec<Assumption>,
    gates: Vec<GateDefinitionDto>,
    review_policy: ReviewPolicyDto,
    evidence_requirements: Vec<EvidenceRequirement>,
    completion_policy: CompletionPolicy,
    approval_policy: HumanApprovalPolicy,
    waiver_policy: WaiverPolicy,
}

/// Complete stable B2 acceptance contract with a verified content digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptanceContractDto {
    /// Immutable acceptance-specification identity.
    pub id: AcceptanceSpecId,
    /// SHA-256 of the complete family-30 content frame.
    pub content_digest: Sha256Digest,
    /// Required immutable contract documents.
    pub documents: ContractDocuments,
    /// Canonical requirements.
    pub requirements: Vec<Requirement>,
    /// Canonical exclusions.
    pub exclusions: Vec<Exclusion>,
    /// Canonical assumptions.
    pub assumptions: Vec<Assumption>,
    /// Complete gate graph definitions.
    pub gates: Vec<GateDefinitionDto>,
    /// Complete review policy.
    pub review_policy: ReviewPolicyDto,
    /// Canonical evidence declarations.
    pub evidence_requirements: Vec<EvidenceRequirement>,
    /// Retry and review-cycle bounds.
    pub completion_policy: CompletionPolicy,
    /// Explicit final human-approval policy.
    pub approval_policy: HumanApprovalPolicy,
    /// Explicit blocker-waiver policy.
    pub waiver_policy: WaiverPolicy,
}

impl AcceptanceContractDto {
    /// Builds a contract DTO and hashes every non-self-referential field.
    #[allow(clippy::too_many_arguments, reason = "all frozen contract components stay explicit")]
    pub fn new(
        id: AcceptanceSpecId,
        documents: ContractDocuments,
        requirements: Vec<Requirement>,
        exclusions: Vec<Exclusion>,
        assumptions: Vec<Assumption>,
        gates: Vec<GateDefinitionDto>,
        review_policy: ReviewPolicyDto,
        evidence_requirements: Vec<EvidenceRequirement>,
        completion_policy: CompletionPolicy,
        approval_policy: HumanApprovalPolicy,
        waiver_policy: WaiverPolicy,
        limits: CodecLimits,
    ) -> Result<Self, CodecError> {
        let content = AcceptanceContractContent {
            id,
            documents,
            requirements: requirements.clone(),
            exclusions: exclusions.clone(),
            assumptions: assumptions.clone(),
            gates: gates.clone(),
            review_policy: review_policy.clone(),
            evidence_requirements: evidence_requirements.clone(),
            completion_policy,
            approval_policy,
            waiver_policy,
        };
        let content_digest = canonical_sha256(&content, limits)?;
        Ok(Self {
            id,
            content_digest,
            documents,
            requirements,
            exclusions,
            assumptions,
            gates,
            review_policy,
            evidence_requirements,
            completion_policy,
            approval_policy,
            waiver_policy,
        })
    }

    /// Verifies the digest and reconstructs the complete checked contract.
    pub fn try_into_domain(
        self,
        limits: CodecLimits,
    ) -> Result<AcceptanceContract, AcceptanceContractConversionError> {
        self.verify_digest(limits).map_err(AcceptanceContractConversionError::Codec)?;
        let gates =
            GateGraph::new(self.gates.into_iter().map(try_gate).collect::<Result<Vec<_>, _>>()?)?;
        let review_policy = try_review(self.review_policy)?;
        AcceptanceContract::new(
            self.id,
            self.content_digest,
            self.documents,
            self.requirements,
            self.exclusions,
            self.assumptions,
            gates,
            review_policy,
            self.evidence_requirements,
            self.completion_policy,
            self.approval_policy,
            self.waiver_policy,
        )
        .map_err(AcceptanceContractConversionError::Spec)
    }

    /// Imports a checked domain contract only when its advertised digest matches canonical bytes.
    pub fn try_from_domain(
        value: &AcceptanceContract,
        limits: CodecLimits,
    ) -> Result<Self, CodecError> {
        let result = Self {
            id: value.id(),
            content_digest: value.content_digest(),
            documents: value.documents(),
            requirements: value.requirements().to_vec(),
            exclusions: value.exclusions().to_vec(),
            assumptions: value.assumptions().to_vec(),
            gates: value.gates().definitions().iter().map(GateDefinitionDto::from).collect(),
            review_policy: value.review_policy().into(),
            evidence_requirements: value.evidence_requirements().to_vec(),
            completion_policy: value.completion_policy(),
            approval_policy: value.approval_policy(),
            waiver_policy: value.waiver_policy(),
        };
        result.verify_digest(limits)?;
        Ok(result)
    }

    fn content(&self) -> AcceptanceContractContent {
        AcceptanceContractContent {
            id: self.id,
            documents: self.documents,
            requirements: self.requirements.clone(),
            exclusions: self.exclusions.clone(),
            assumptions: self.assumptions.clone(),
            gates: self.gates.clone(),
            review_policy: self.review_policy.clone(),
            evidence_requirements: self.evidence_requirements.clone(),
            completion_policy: self.completion_policy,
            approval_policy: self.approval_policy,
            waiver_policy: self.waiver_policy,
        }
    }

    fn verify_digest(&self, limits: CodecLimits) -> Result<(), CodecError> {
        if canonical_sha256(&self.content(), limits)? == self.content_digest {
            Ok(())
        } else {
            Err(CodecError::at(CodecErrorKind::InvalidDomainValue, 0))
        }
    }
}

/// Checked contract conversion failure without assigning authority to decoded bytes.
#[derive(Debug)]
pub enum AcceptanceContractConversionError {
    /// Canonical digest or encoding failure.
    Codec(CodecError),
    /// Checked B2 contract validation failure.
    Spec(SpecError),
}

impl From<SpecError> for AcceptanceContractConversionError {
    fn from(error: SpecError) -> Self {
        Self::Spec(error)
    }
}

impl CanonicalEncode for AcceptanceContractContent {
    const FAMILY: u16 = 30;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_content(writer, self)
    }
}

impl CanonicalEncode for AcceptanceContractDto {
    const FAMILY: u16 = 31;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_content(writer, &self.content())?;
        write_digest(writer, &self.content_digest)
    }
}

impl CanonicalDecode for AcceptanceContractDto {
    const FAMILY: u16 = 31;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let start = reader.offset();
        let content = read_content(reader)?;
        let value = Self {
            id: content.id,
            content_digest: read_digest(reader)?,
            documents: content.documents,
            requirements: content.requirements,
            exclusions: content.exclusions,
            assumptions: content.assumptions,
            gates: content.gates,
            review_policy: content.review_policy,
            evidence_requirements: content.evidence_requirements,
            completion_policy: content.completion_policy,
            approval_policy: content.approval_policy,
            waiver_policy: content.waiver_policy,
        };
        value
            .clone()
            .try_into_domain(CodecLimits::PRODUCTION)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, start))?;
        Ok(value)
    }
}

fn write_content(
    writer: &mut CanonicalWriter,
    value: &AcceptanceContractContent,
) -> Result<(), CodecError> {
    write_id(writer, value.id.as_bytes())?;
    write_documents(writer, value.documents)?;
    writer.write_collection_len(value.requirements.len())?;
    for requirement in &value.requirements {
        write_digest(writer, &requirement.id().digest())?;
        write_content_ref(writer, requirement.content())?;
    }
    writer.write_collection_len(value.exclusions.len())?;
    for exclusion in &value.exclusions {
        write_content_ref(writer, exclusion.content())?;
    }
    writer.write_collection_len(value.assumptions.len())?;
    for assumption in &value.assumptions {
        write_content_ref(writer, assumption.content())?;
    }
    writer.write_collection_len(value.gates.len())?;
    for gate in &value.gates {
        writer.nested(|writer| write_gate(writer, gate))?;
    }
    writer.nested(|writer| write_review(writer, &value.review_policy))?;
    writer.write_collection_len(value.evidence_requirements.len())?;
    for evidence in &value.evidence_requirements {
        writer.nested(|writer| write_evidence(writer, *evidence))?;
    }
    writer.write_u16(value.completion_policy.max_gate_attempts())?;
    writer.write_u16(value.completion_policy.max_review_cycles())?;
    write_approval_policy(writer, value.approval_policy)?;
    write_waiver_policy(writer, value.waiver_policy)
}

fn read_content(reader: &mut CanonicalReader<'_>) -> Result<AcceptanceContractContent, CodecError> {
    let id = read_id(reader, AcceptanceSpecId::new)?;
    let documents = read_documents(reader)?;
    let requirement_count = reader.read_collection_len()?;
    let mut requirements = Vec::with_capacity(requirement_count);
    for _ in 0..requirement_count {
        requirements.push(Requirement::new(
            RequirementId::new(read_digest(reader)?),
            read_content_ref(reader)?,
        ));
    }
    let exclusion_count = reader.read_collection_len()?;
    let mut exclusions = Vec::with_capacity(exclusion_count);
    for _ in 0..exclusion_count {
        exclusions.push(Exclusion::new(read_content_ref(reader)?));
    }
    let assumption_count = reader.read_collection_len()?;
    let mut assumptions = Vec::with_capacity(assumption_count);
    for _ in 0..assumption_count {
        assumptions.push(Assumption::new(read_content_ref(reader)?));
    }
    let gate_count = reader.read_collection_len()?;
    let mut gates = Vec::with_capacity(gate_count);
    for _ in 0..gate_count {
        gates.push(reader.nested(read_gate)?);
    }
    let review_policy = reader.nested(read_review)?;
    let evidence_count = reader.read_collection_len()?;
    let mut evidence_requirements = Vec::with_capacity(evidence_count);
    for _ in 0..evidence_count {
        evidence_requirements.push(reader.nested(read_evidence)?);
    }
    let completion_offset = reader.offset();
    let completion_policy = CompletionPolicy::new(reader.read_u16()?, reader.read_u16()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, completion_offset))?;
    Ok(AcceptanceContractContent {
        id,
        documents,
        requirements,
        exclusions,
        assumptions,
        gates,
        review_policy,
        evidence_requirements,
        completion_policy,
        approval_policy: read_approval_policy(reader)?,
        waiver_policy: read_waiver_policy(reader)?,
    })
}
