//! Role-specific, digest-bound knowledge retained for safe continuation.

use peritus_role::HarnessRole;
use peritus_run_knowledge::{
    CurrentKnowledgeState, InvalidationRequest, KnowledgeBinding, KnowledgeChange, KnowledgeLimits,
    KnowledgeSection, KnowledgeSectionId, KnowledgeSectionKind, KnowledgeSourceId,
    RunKnowledgeSnapshot, SourceDigest, plan_invalidation,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_types::Sha256Digest;

use super::{
    ProductRunResume,
    hashing::{digest, digest_pair},
};
use crate::{ProductRunnerError, ProductRunnerErrorKind, execution::ProductRunPhase};

const INVENTORY_SECTION: u8 = 1;
const FILE_MAP_SECTION: u8 = 2;
const REQUIREMENTS_SECTION: u8 = 3;
const DESIGN_SECTION: u8 = 4;
const FINDINGS_SECTION: u8 = 5;
const EVIDENCE_SECTION: u8 = 6;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RoleKnowledge {
    writer: RunKnowledgeSnapshot,
    reviewer: RunKnowledgeSnapshot,
    fixer: RunKnowledgeSnapshot,
}

impl RoleKnowledge {
    pub(super) fn capture(
        candidate: CandidateIdentity,
        transcript: &str,
        design: &str,
        findings: &str,
        evidence: &str,
    ) -> Result<Self, ProductRunnerError> {
        let sources = sources(candidate.candidate_digest(), transcript, design, findings)?;
        Ok(Self {
            writer: snapshot(candidate, HarnessRole::Writer, &sources, design, findings, evidence)?,
            reviewer: snapshot(
                candidate,
                HarnessRole::Reviewer,
                &sources,
                design,
                findings,
                evidence,
            )?,
            fixer: snapshot(candidate, HarnessRole::Fixer, &sources, design, findings, evidence)?,
        })
    }
}

impl ProductRunResume {
    pub(in crate::execution) fn plan(
        &self,
        current: CandidateIdentity,
        transcript: &str,
    ) -> Result<ProductRunPhase, ProductRunnerError> {
        let sources = sources(
            current.candidate_digest(),
            transcript,
            &self.design_markdown,
            &self.finding_state,
        )?;
        let change = knowledge_change(self.checkpoint.identity(), &current);
        let state = CurrentKnowledgeState::new(current, sources, limits()).map_err(invariant)?;
        let request = InvalidationRequest::new(state, change, Vec::new()).map_err(invariant)?;
        let writer = plan_invalidation(&self.knowledge.writer, &request).map_err(invariant)?;
        if !writer.is_reused(section_id(DESIGN_SECTION)?) {
            return Ok(ProductRunPhase::Designing);
        }
        let phase_snapshot = match self.next_phase {
            ProductRunPhase::Reviewing => &self.knowledge.reviewer,
            ProductRunPhase::Fixing => &self.knowledge.fixer,
            _ => &self.knowledge.writer,
        };
        let phase_plan = plan_invalidation(phase_snapshot, &request).map_err(invariant)?;
        if phase_plan.accounting().invalidated() == 0 {
            Ok(self.next_phase)
        } else {
            Ok(ProductRunPhase::Checking)
        }
    }
}

fn snapshot(
    candidate: CandidateIdentity,
    role: HarnessRole,
    sources: &[SourceDigest],
    design: &str,
    findings: &str,
    evidence: &str,
) -> Result<RunKnowledgeSnapshot, ProductRunnerError> {
    let inventory = section(
        INVENTORY_SECTION,
        KnowledgeSectionKind::RepositoryInventory,
        candidate.candidate_digest(),
        candidate,
        role,
        vec![sources[0]],
        Vec::new(),
    )?;
    let file_map = section(
        FILE_MAP_SECTION,
        KnowledgeSectionKind::RelevantFileMap,
        digest_pair(
            b"file-map",
            candidate.candidate_digest().as_bytes(),
            sources[1].content_digest().as_bytes(),
        ),
        candidate,
        role,
        vec![sources[0], sources[1]],
        vec![inventory.id()],
    )?;
    let requirements = section(
        REQUIREMENTS_SECTION,
        KnowledgeSectionKind::LiteralRequirementLedger,
        sources[1].content_digest(),
        candidate,
        role,
        vec![sources[1]],
        Vec::new(),
    )?;
    let design_section = section(
        DESIGN_SECTION,
        KnowledgeSectionKind::DesignSection,
        digest(b"design", design.as_bytes()),
        candidate,
        role,
        vec![sources[1], sources[2]],
        vec![inventory.id(), file_map.id(), requirements.id()],
    )?;
    let findings_section = section(
        FINDINGS_SECTION,
        KnowledgeSectionKind::ResolvedFinding,
        digest(b"findings", findings.as_bytes()),
        candidate,
        role,
        vec![sources[0], sources[3]],
        vec![inventory.id(), requirements.id()],
    )?;
    let evidence_section = section(
        EVIDENCE_SECTION,
        KnowledgeSectionKind::CandidateEvidenceIndex,
        digest(b"evidence", evidence.as_bytes()),
        candidate,
        role,
        vec![sources[0]],
        vec![inventory.id(), requirements.id()],
    )?;
    RunKnowledgeSnapshot::new(
        candidate,
        role,
        inventory.id(),
        file_map.id(),
        requirements.id(),
        vec![inventory, file_map, requirements, design_section, findings_section, evidence_section],
        limits(),
    )
    .map_err(invariant)
}

fn section(
    id: u8,
    kind: KnowledgeSectionKind,
    section_digest: Sha256Digest,
    candidate: CandidateIdentity,
    role: HarnessRole,
    sources: Vec<SourceDigest>,
    dependencies: Vec<KnowledgeSectionId>,
) -> Result<KnowledgeSection, ProductRunnerError> {
    let binding =
        KnowledgeBinding::new(candidate, role, candidate.checkpoint_sequence(), sources, limits())
            .map_err(invariant)?;
    KnowledgeSection::new(section_id(id)?, kind, section_digest, binding, dependencies, limits())
        .map_err(invariant)
}

fn sources(
    candidate_digest: Sha256Digest,
    transcript: &str,
    design: &str,
    findings: &str,
) -> Result<Vec<SourceDigest>, ProductRunnerError> {
    Ok(vec![
        SourceDigest::new(source_id(1)?, candidate_digest),
        SourceDigest::new(source_id(2)?, digest(b"conversation", transcript.as_bytes())),
        SourceDigest::new(source_id(3)?, digest(b"design", design.as_bytes())),
        SourceDigest::new(source_id(4)?, digest(b"findings", findings.as_bytes())),
    ])
}

fn knowledge_change(previous: &CandidateIdentity, current: &CandidateIdentity) -> KnowledgeChange {
    if previous.conversation_revision() != current.conversation_revision() {
        KnowledgeChange::ConversationRevision
    } else if !previous.same_candidate(current) {
        KnowledgeChange::CandidateRevision
    } else {
        KnowledgeChange::ProviderFailure
    }
}

fn limits() -> KnowledgeLimits {
    KnowledgeLimits::new(16, 16, 8, 8).expect("static knowledge limits are valid")
}

fn section_id(value: u8) -> Result<KnowledgeSectionId, ProductRunnerError> {
    KnowledgeSectionId::new([value; 16]).map_err(invariant)
}

fn source_id(value: u8) -> Result<KnowledgeSourceId, ProductRunnerError> {
    KnowledgeSourceId::new([value; 16]).map_err(invariant)
}

fn invariant(error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "plan digest-bound product-run resume",
        error.to_string(),
    )
}
