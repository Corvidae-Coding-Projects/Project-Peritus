//! Canonical B2 contract component encodings.

#![allow(
    clippy::missing_errors_doc,
    reason = "contract component codecs use the shared CodecError and checked SpecError vocabularies"
)]

use super::dto::{GateDefinitionDto, ReviewPolicyDto};
use crate::primitive::{read_digest, read_id, write_digest, write_id};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_spec::{
    ContentReference, ContractDocuments, EvidenceRequirement, EvidenceRequirementId,
    EvidenceSource, ExportClassification, FindingSeverity, GateDefinition, GateExecutionPlan,
    GateFreshnessScope, GateSuccessRule, HumanApprovalPolicy, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, SpecError, WaiverPolicy,
};
use peritus_types::{EnvironmentId, GateId};

pub fn write_content_ref(
    writer: &mut CanonicalWriter,
    value: ContentReference,
) -> Result<(), CodecError> {
    write_digest(writer, &value.digest())
}

pub fn read_content_ref(reader: &mut CanonicalReader<'_>) -> Result<ContentReference, CodecError> {
    Ok(ContentReference::new(read_digest(reader)?))
}

pub fn write_documents(
    writer: &mut CanonicalWriter,
    value: ContractDocuments,
) -> Result<(), CodecError> {
    for reference in [
        value.objective(),
        value.user_visible_behavior(),
        value.repository_roots(),
        value.permitted_change_surface(),
        value.resource_budget_policy(),
        value.security_approval_policy(),
        value.completion_conditions(),
        value.failure_conditions(),
    ] {
        write_content_ref(writer, reference)?;
    }
    Ok(())
}

pub fn read_documents(reader: &mut CanonicalReader<'_>) -> Result<ContractDocuments, CodecError> {
    Ok(ContractDocuments::new(
        read_content_ref(reader)?,
        read_content_ref(reader)?,
        read_content_ref(reader)?,
        read_content_ref(reader)?,
        read_content_ref(reader)?,
        read_content_ref(reader)?,
        read_content_ref(reader)?,
        read_content_ref(reader)?,
    ))
}

pub fn write_gate(
    writer: &mut CanonicalWriter,
    value: &GateDefinitionDto,
) -> Result<(), CodecError> {
    write_id(writer, value.id.as_bytes())?;
    write_gate_plan(writer, value.plan)?;
    writer.write_collection_len(value.dependencies.len())?;
    for id in &value.dependencies {
        write_id(writer, id.as_bytes())?;
    }
    writer.write_collection_len(value.required_evidence.len())?;
    for id in &value.required_evidence {
        write_digest(writer, &id.digest())?;
    }
    Ok(())
}

pub fn read_gate(reader: &mut CanonicalReader<'_>) -> Result<GateDefinitionDto, CodecError> {
    let id = read_id(reader, GateId::new)?;
    let plan = read_gate_plan(reader)?;
    let dependency_count = reader.read_collection_len()?;
    let mut dependencies = Vec::with_capacity(dependency_count);
    for _ in 0..dependency_count {
        dependencies.push(read_id(reader, GateId::new)?);
    }
    let evidence_count = reader.read_collection_len()?;
    let mut required_evidence = Vec::with_capacity(evidence_count);
    for _ in 0..evidence_count {
        required_evidence.push(EvidenceRequirementId::new(read_digest(reader)?));
    }
    Ok(GateDefinitionDto { id, plan, dependencies, required_evidence })
}

pub fn try_gate(value: GateDefinitionDto) -> Result<GateDefinition, SpecError> {
    GateDefinition::new(value.id, value.plan, value.dependencies, value.required_evidence)
}

fn write_gate_plan(
    writer: &mut CanonicalWriter,
    value: GateExecutionPlan,
) -> Result<(), CodecError> {
    write_content_ref(writer, value.action())?;
    write_id(writer, value.environment().as_bytes())?;
    write_content_ref(writer, value.inputs())?;
    write_content_ref(writer, value.parser())?;
    match value.success_rule() {
        GateSuccessRule::ExitCodeZero => writer.write_u16(1)?,
        GateSuccessRule::Predicate(reference) => {
            writer.write_u16(2)?;
            write_content_ref(writer, reference)?;
        }
    }
    writer.write_u64(value.timeout_ms())?;
    write_content_ref(writer, value.resources())?;
    writer.write_u16(match value.freshness() {
        GateFreshnessScope::ExactRevisionTuple => 1,
        GateFreshnessScope::WorkspaceContent => 2,
    })
}

fn read_gate_plan(reader: &mut CanonicalReader<'_>) -> Result<GateExecutionPlan, CodecError> {
    let start = reader.offset();
    let action = read_content_ref(reader)?;
    let environment = read_id(reader, EnvironmentId::new)?;
    let inputs = read_content_ref(reader)?;
    let parser = read_content_ref(reader)?;
    let success_offset = reader.offset();
    let success = match reader.read_u16()? {
        1 => GateSuccessRule::ExitCodeZero,
        2 => GateSuccessRule::Predicate(read_content_ref(reader)?),
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, success_offset)),
    };
    let timeout_ms = reader.read_u64()?;
    let resources = read_content_ref(reader)?;
    let freshness_offset = reader.offset();
    let freshness = match reader.read_u16()? {
        1 => GateFreshnessScope::ExactRevisionTuple,
        2 => GateFreshnessScope::WorkspaceContent,
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, freshness_offset)),
    };
    GateExecutionPlan::new(
        action,
        environment,
        inputs,
        parser,
        success,
        timeout_ms,
        resources,
        freshness,
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, start))
}

pub fn write_review(
    writer: &mut CanonicalWriter,
    value: &ReviewPolicyDto,
) -> Result<(), CodecError> {
    writer.write_collection_len(value.required_categories.len())?;
    for category in &value.required_categories {
        write_digest(writer, &category.digest())?;
    }
    writer.write_u16(value.reviewer_quorum)?;
    let independence = value.independence;
    for required in [
        independence.requires_distinct_reviewers(),
        independence.requires_independence_from_producer(),
        independence.requires_distinct_contexts(),
        independence.requires_distinct_model_families(),
        independence.requires_distinct_providers(),
        independence.requires_no_shared_ancestry(),
    ] {
        writer.write_bool(required)?;
    }
    writer.write_u16(severity_tag(value.blocking_severity))
}

pub fn read_review(reader: &mut CanonicalReader<'_>) -> Result<ReviewPolicyDto, CodecError> {
    let count = reader.read_collection_len()?;
    let mut required_categories = Vec::with_capacity(count);
    for _ in 0..count {
        required_categories.push(ReviewCategory::new(read_digest(reader)?));
    }
    Ok(ReviewPolicyDto {
        required_categories,
        reviewer_quorum: reader.read_u16()?,
        independence: ReviewerIndependence::new(
            reader.read_bool()?,
            reader.read_bool()?,
            reader.read_bool()?,
            reader.read_bool()?,
            reader.read_bool()?,
            reader.read_bool()?,
        ),
        blocking_severity: read_severity(reader)?,
    })
}

pub fn try_review(value: ReviewPolicyDto) -> Result<ReviewPolicy, SpecError> {
    ReviewPolicy::new(
        value.required_categories,
        value.reviewer_quorum,
        value.independence,
        value.blocking_severity,
    )
}

pub fn write_evidence(
    writer: &mut CanonicalWriter,
    value: EvidenceRequirement,
) -> Result<(), CodecError> {
    write_digest(writer, &value.id().digest())?;
    write_content_ref(writer, value.description())?;
    match value.source() {
        EvidenceSource::General => writer.write_u16(1)?,
        EvidenceSource::Gate(id) => {
            writer.write_u16(2)?;
            write_id(writer, id.as_bytes())?;
        }
        EvidenceSource::Review(category) => {
            writer.write_u16(3)?;
            write_digest(writer, &category.digest())?;
        }
        EvidenceSource::HumanApproval => writer.write_u16(4)?,
        EvidenceSource::WaiverAuthorization => writer.write_u16(5)?,
    }
    writer.write_u16(match value.export_classification() {
        ExportClassification::Public => 1,
        ExportClassification::Internal => 2,
        ExportClassification::Restricted => 3,
    })
}

pub fn read_evidence(reader: &mut CanonicalReader<'_>) -> Result<EvidenceRequirement, CodecError> {
    let id = EvidenceRequirementId::new(read_digest(reader)?);
    let description = read_content_ref(reader)?;
    let source_offset = reader.offset();
    let source = match reader.read_u16()? {
        1 => EvidenceSource::General,
        2 => EvidenceSource::Gate(read_id(reader, GateId::new)?),
        3 => EvidenceSource::Review(ReviewCategory::new(read_digest(reader)?)),
        4 => EvidenceSource::HumanApproval,
        5 => EvidenceSource::WaiverAuthorization,
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, source_offset)),
    };
    let export_offset = reader.offset();
    let export = match reader.read_u16()? {
        1 => ExportClassification::Public,
        2 => ExportClassification::Internal,
        3 => ExportClassification::Restricted,
        _ => return Err(CodecError::at(CodecErrorKind::UnknownTag, export_offset)),
    };
    Ok(EvidenceRequirement::new(id, description, source, export))
}

pub fn write_approval_policy(
    writer: &mut CanonicalWriter,
    value: HumanApprovalPolicy,
) -> Result<(), CodecError> {
    match value {
        HumanApprovalPolicy::NotRequired => writer.write_u16(1),
        HumanApprovalPolicy::Required(reference) => {
            writer.write_u16(2)?;
            write_content_ref(writer, reference)
        }
    }
}

pub fn read_approval_policy(
    reader: &mut CanonicalReader<'_>,
) -> Result<HumanApprovalPolicy, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(HumanApprovalPolicy::NotRequired),
        2 => Ok(HumanApprovalPolicy::Required(read_content_ref(reader)?)),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

pub fn write_waiver_policy(
    writer: &mut CanonicalWriter,
    value: WaiverPolicy,
) -> Result<(), CodecError> {
    match value {
        WaiverPolicy::Forbidden => writer.write_u16(1),
        WaiverPolicy::Allowed { authority, evidence } => {
            writer.write_u16(2)?;
            write_content_ref(writer, authority)?;
            write_digest(writer, &evidence.digest())
        }
    }
}

pub fn read_waiver_policy(reader: &mut CanonicalReader<'_>) -> Result<WaiverPolicy, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(WaiverPolicy::Forbidden),
        2 => Ok(WaiverPolicy::Allowed {
            authority: read_content_ref(reader)?,
            evidence: EvidenceRequirementId::new(read_digest(reader)?),
        }),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn severity_tag(value: FindingSeverity) -> u16 {
    match value {
        FindingSeverity::Advisory => 1,
        FindingSeverity::Low => 2,
        FindingSeverity::Medium => 3,
        FindingSeverity::High => 4,
        FindingSeverity::Critical => 5,
    }
}

fn read_severity(reader: &mut CanonicalReader<'_>) -> Result<FindingSeverity, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(FindingSeverity::Advisory),
        2 => Ok(FindingSeverity::Low),
        3 => Ok(FindingSeverity::Medium),
        4 => Ok(FindingSeverity::High),
        5 => Ok(FindingSeverity::Critical),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
