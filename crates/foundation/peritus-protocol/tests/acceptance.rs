//! B2 acceptance-contract wire and digest-binding tests.

use peritus_codec::{CodecErrorKind, CodecLimits, decode_message, encode_message};
use peritus_protocol::{AcceptanceContractDto, GateDefinitionDto, ReviewPolicyDto};
use peritus_spec::{
    Assumption, CompletionPolicy, ContentReference, ContractDocuments, EvidenceRequirement,
    EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification, FindingSeverity,
    GateExecutionPlan, GateFreshnessScope, GateSuccessRule, HumanApprovalPolicy, Requirement,
    RequirementId, ReviewCategory, ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{AcceptanceSpecId, EnvironmentId, GateId, Sha256Digest};

const LIMITS: CodecLimits = CodecLimits::PRODUCTION;

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
const fn content(value: u8) -> ContentReference {
    ContentReference::new(digest(value))
}
const fn evidence_id(value: u8) -> EvidenceRequirementId {
    EvidenceRequirementId::new(digest(value))
}
const fn category(value: u8) -> ReviewCategory {
    ReviewCategory::new(digest(value))
}
fn gate_id(value: u8) -> GateId {
    GateId::new([value; 16]).expect("gate id")
}

fn gate(value: u8, dependencies: Vec<GateId>) -> GateDefinitionDto {
    GateDefinitionDto {
        id: gate_id(value),
        plan: GateExecutionPlan::new(
            content(value),
            EnvironmentId::new([value; 16]).expect("environment"),
            content(value + 1),
            content(value + 2),
            GateSuccessRule::ExitCodeZero,
            10_000,
            content(value + 3),
            GateFreshnessScope::ExactRevisionTuple,
        )
        .expect("plan"),
        dependencies,
        required_evidence: vec![evidence_id(value)],
    }
}

fn contract() -> AcceptanceContractDto {
    let documents = ContractDocuments::new(
        content(70),
        content(71),
        content(72),
        content(73),
        content(74),
        content(75),
        content(76),
        content(77),
    );
    AcceptanceContractDto::new(
        AcceptanceSpecId::new([1; 16]).expect("contract id"),
        documents,
        vec![Requirement::new(RequirementId::new(digest(1)), content(21))],
        vec![Exclusion::new(content(31))],
        vec![Assumption::new(content(41))],
        vec![gate(1, vec![]), gate(2, vec![gate_id(1)])],
        ReviewPolicyDto {
            required_categories: vec![category(1), category(2)],
            reviewer_quorum: 2,
            independence: ReviewerIndependence::new(true, true, true, false, false, true),
            blocking_severity: FindingSeverity::High,
        },
        vec![
            EvidenceRequirement::new(
                evidence_id(1),
                content(1),
                EvidenceSource::Gate(gate_id(1)),
                ExportClassification::Internal,
            ),
            EvidenceRequirement::new(
                evidence_id(2),
                content(2),
                EvidenceSource::Gate(gate_id(2)),
                ExportClassification::Internal,
            ),
            EvidenceRequirement::new(
                evidence_id(3),
                content(3),
                EvidenceSource::Review(category(1)),
                ExportClassification::Internal,
            ),
            EvidenceRequirement::new(
                evidence_id(4),
                content(4),
                EvidenceSource::Review(category(2)),
                ExportClassification::Internal,
            ),
            EvidenceRequirement::new(
                evidence_id(5),
                content(5),
                EvidenceSource::HumanApproval,
                ExportClassification::Restricted,
            ),
            EvidenceRequirement::new(
                evidence_id(6),
                content(6),
                EvidenceSource::WaiverAuthorization,
                ExportClassification::Restricted,
            ),
        ],
        CompletionPolicy::new(2, 4).expect("completion"),
        HumanApprovalPolicy::Required(content(60)),
        WaiverPolicy::Allowed { authority: content(61), evidence: evidence_id(6) },
        LIMITS,
    )
    .expect("contract dto")
}

#[test]
fn complete_contract_roundtrips_and_reconstructs_checked_domain() {
    let value = contract();
    let bytes = encode_message(&value, LIMITS).expect("encode");
    let decoded: AcceptanceContractDto = decode_message(&bytes, LIMITS).expect("decode");
    assert_eq!(decoded, value);
    let domain = decoded.try_into_domain(LIMITS).expect("checked domain contract");
    assert_eq!(domain.id(), value.id);
    assert_eq!(domain.content_digest(), value.content_digest);
}

#[test]
fn contract_digest_rejects_any_corrupted_advertised_value() {
    let value = contract();
    let mut bytes = encode_message(&value, LIMITS).expect("encode");
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    assert_eq!(
        decode_message::<AcceptanceContractDto>(&bytes, LIMITS)
            .expect_err("digest mismatch")
            .kind(),
        CodecErrorKind::InvalidDomainValue
    );
}
