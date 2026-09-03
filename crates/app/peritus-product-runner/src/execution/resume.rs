//! Digest-valid phase resume planning over retained product-run state.

use std::path::PathBuf;

use peritus_role::HarnessRole;
use peritus_run_knowledge::{
    CurrentKnowledgeState, InvalidationRequest, KnowledgeBinding, KnowledgeChange, KnowledgeLimits,
    KnowledgeSection, KnowledgeSectionId, KnowledgeSectionKind, KnowledgeSourceId,
    RunKnowledgeSnapshot, SourceDigest, plan_invalidation,
};
use peritus_run_settlement::{CandidateCheckpoint, CandidateIdentity};
use peritus_types::Sha256Digest;

mod hashing;

use hashing::{digest, digest_pair};

use super::ProductRunPhase;
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, candidate::CandidateBaseline,
    developer_tools::SuccessfulCommand,
};

const INVENTORY_SECTION: u8 = 1;
const FILE_MAP_SECTION: u8 = 2;
const REQUIREMENTS_SECTION: u8 = 3;
const DESIGN_SECTION: u8 = 4;
const FINDINGS_SECTION: u8 = 5;
const EVIDENCE_SECTION: u8 = 6;

/// Opaque retained state sufficient to continue at the first stale or missing phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunResume {
    checkpoint: CandidateCheckpoint,
    baseline: CandidateBaseline,
    next_phase: ProductRunPhase,
    design_path: PathBuf,
    design_markdown: String,
    design_revision: u64,
    task_summary: String,
    run_instructions: String,
    fix_summaries: Vec<String>,
    tool_calls: u32,
    finding_state: String,
    diff: String,
    gates: String,
    review: String,
    gate_report: Option<crate::gates::GateReport>,
    developer_evidence: String,
    successful_commands: Vec<SuccessfulCommand>,
    fixer_cycles: u32,
    knowledge: RoleKnowledge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RoleKnowledge {
    writer: RunKnowledgeSnapshot,
    reviewer: RunKnowledgeSnapshot,
    fixer: RunKnowledgeSnapshot,
}

/// Complete retained execution values copied into a resume handoff.
pub(super) struct ResumeCapture {
    pub(super) checkpoint: CandidateCheckpoint,
    pub(super) baseline: CandidateBaseline,
    pub(super) next_phase: ProductRunPhase,
    pub(super) design_path: PathBuf,
    pub(super) design_markdown: String,
    pub(super) design_revision: u64,
    pub(super) task_summary: String,
    pub(super) run_instructions: String,
    pub(super) fix_summaries: Vec<String>,
    pub(super) tool_calls: u32,
    pub(super) finding_state: String,
    pub(super) diff: String,
    pub(super) gates: String,
    pub(super) review: String,
    pub(super) gate_report: Option<crate::gates::GateReport>,
    pub(super) developer_evidence: String,
    pub(super) successful_commands: Vec<SuccessfulCommand>,
    pub(super) fixer_cycles: u32,
    pub(super) transcript: String,
}

impl ProductRunResume {
    pub(super) fn capture(values: ResumeCapture) -> Result<Self, ProductRunnerError> {
        let knowledge = RoleKnowledge::capture(
            *values.checkpoint.identity(),
            &values.transcript,
            &values.design_markdown,
            &values.finding_state,
            &values.developer_evidence,
        )?;
        Ok(Self {
            checkpoint: values.checkpoint,
            baseline: values.baseline,
            next_phase: values.next_phase,
            design_path: values.design_path,
            design_markdown: values.design_markdown,
            design_revision: values.design_revision,
            task_summary: values.task_summary,
            run_instructions: values.run_instructions,
            fix_summaries: values.fix_summaries,
            tool_calls: values.tool_calls,
            finding_state: values.finding_state,
            diff: values.diff,
            gates: values.gates,
            review: values.review,
            gate_report: values.gate_report,
            developer_evidence: values.developer_evidence,
            successful_commands: values.successful_commands,
            fixer_cycles: values.fixer_cycles,
            knowledge,
        })
    }

    /// Exact candidate checkpoint at the interruption boundary.
    #[must_use]
    pub const fn checkpoint(&self) -> &CandidateCheckpoint {
        &self.checkpoint
    }

    pub(super) const fn baseline(&self) -> &CandidateBaseline {
        &self.baseline
    }

    /// First phase that was stale or incomplete when the run stopped.
    #[must_use]
    pub const fn next_phase(&self) -> ProductRunPhase {
        self.next_phase
    }

    pub(super) fn plan(
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

    pub(super) const fn design_path(&self) -> &PathBuf {
        &self.design_path
    }

    pub(super) fn design_markdown(&self) -> &str {
        &self.design_markdown
    }

    pub(super) const fn design_revision(&self) -> u64 {
        self.design_revision
    }

    pub(super) fn task_summary(&self) -> &str {
        &self.task_summary
    }

    pub(super) fn run_instructions(&self) -> &str {
        &self.run_instructions
    }

    pub(super) fn fix_summaries(&self) -> &[String] {
        &self.fix_summaries
    }

    pub(super) const fn tool_calls(&self) -> u32 {
        self.tool_calls
    }

    pub(super) fn finding_state(&self) -> &str {
        &self.finding_state
    }

    pub(super) fn developer_evidence(&self) -> &str {
        &self.developer_evidence
    }

    pub(super) fn diff(&self) -> &str {
        &self.diff
    }

    pub(super) fn gates(&self) -> &str {
        &self.gates
    }

    pub(super) fn review(&self) -> &str {
        &self.review
    }

    pub(super) const fn gate_report(&self) -> Option<&crate::gates::GateReport> {
        self.gate_report.as_ref()
    }

    pub(super) fn successful_commands(&self) -> &[SuccessfulCommand] {
        &self.successful_commands
    }

    pub(super) const fn fixer_cycles(&self) -> u32 {
        self.fixer_cycles
    }
}

impl RoleKnowledge {
    fn capture(
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

#[cfg(test)]
mod tests;
