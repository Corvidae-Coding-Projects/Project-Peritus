//! Versioned persistence for the otherwise opaque product-run continuation.

use std::path::PathBuf;

use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceRecord, EvidenceStatus,
    QualificationEvidence,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use serde::Deserialize;
use serde::Serialize;

use super::{ProductRunResume, RoleKnowledge};
use crate::{
    ProductRunPhase, ProductRunnerError, ProductRunnerErrorKind,
    candidate::CandidateBaseline,
    developer_tools::{CommandPurpose, SuccessfulCommand},
};

const DURABLE_VERSION: u16 = 1;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DurableResume {
    version: u16,
    checkpoint: DurableCheckpoint,
    baseline_head: String,
    next_phase: u16,
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
    developer_evidence: String,
    successful_commands: Vec<DurableCommand>,
    fixer_cycles: u32,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DurableCheckpoint {
    identity: DurableIdentity,
    stage: u16,
    gates: DurableEvidence,
    obligations: DurableEvidence,
    review: DurableEvidence,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DurableIdentity {
    run_id: [u8; 16],
    workspace_id: [u8; 16],
    candidate_digest: [u8; 32],
    conversation_revision: u64,
    checkpoint_sequence: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DurableEvidence {
    status: u16,
    provenance: Option<DurableIdentity>,
    value: Option<u16>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DurableCommand {
    command: String,
    purpose: u16,
}

pub(super) fn encode(resume: &ProductRunResume) -> Result<Vec<u8>, ProductRunnerError> {
    let payload = DurableResume {
        version: DURABLE_VERSION,
        checkpoint: DurableCheckpoint::from_checkpoint(&resume.checkpoint),
        baseline_head: resume.baseline.head().to_owned(),
        next_phase: phase_tag(resume.next_phase),
        design_path: resume.design_path.clone(),
        design_markdown: resume.design_markdown.clone(),
        design_revision: resume.design_revision,
        task_summary: resume.task_summary.clone(),
        run_instructions: resume.run_instructions.clone(),
        fix_summaries: resume.fix_summaries.clone(),
        tool_calls: resume.tool_calls,
        finding_state: resume.finding_state.clone(),
        diff: resume.diff.clone(),
        gates: resume.gates.clone(),
        review: resume.review.clone(),
        developer_evidence: resume.developer_evidence.clone(),
        successful_commands: resume
            .successful_commands
            .iter()
            .map(DurableCommand::from_command)
            .collect(),
        fixer_cycles: resume.fixer_cycles,
    };
    serde_json::to_vec(&payload).map_err(|error| durable_error(error.to_string()))
}

pub(super) fn decode(
    bytes: &[u8],
    transcript: &str,
) -> Result<ProductRunResume, ProductRunnerError> {
    let payload: DurableResume =
        serde_json::from_slice(bytes).map_err(|error| durable_error(error.to_string()))?;
    if payload.version != DURABLE_VERSION {
        return Err(durable_error("unsupported durable resume version"));
    }
    let checkpoint = payload.checkpoint.into_checkpoint()?;
    let next_phase = restored_phase(payload.next_phase)?;
    let baseline = CandidateBaseline::restored(payload.baseline_head)?;
    let successful_commands = payload
        .successful_commands
        .into_iter()
        .map(DurableCommand::into_command)
        .collect::<Result<Vec<_>, _>>()?;
    let knowledge = RoleKnowledge::capture(
        *checkpoint.identity(),
        transcript,
        &payload.design_markdown,
        &payload.finding_state,
        &payload.developer_evidence,
    )?;
    Ok(ProductRunResume {
        checkpoint,
        baseline,
        next_phase,
        design_path: payload.design_path,
        design_markdown: payload.design_markdown,
        design_revision: payload.design_revision,
        task_summary: payload.task_summary,
        run_instructions: payload.run_instructions,
        fix_summaries: payload.fix_summaries,
        tool_calls: payload.tool_calls,
        finding_state: payload.finding_state,
        diff: payload.diff,
        gates: payload.gates,
        review: payload.review,
        // Deterministic gates are intentionally reacquired after a process restart rather than
        // reviving an effectful report whose plan contains private runtime state.
        gate_report: None,
        developer_evidence: payload.developer_evidence,
        successful_commands,
        fixer_cycles: payload.fixer_cycles,
        knowledge,
    })
}

impl DurableCheckpoint {
    fn from_checkpoint(value: &CandidateCheckpoint) -> Self {
        Self {
            identity: DurableIdentity::from_identity(*value.identity()),
            stage: value.stage().tag(),
            gates: DurableEvidence::from_evidence(value.gates()),
            obligations: DurableEvidence::from_evidence(value.obligations()),
            review: DurableEvidence::from_evidence(value.review()),
        }
    }

    fn into_checkpoint(self) -> Result<CandidateCheckpoint, ProductRunnerError> {
        let identity = self.identity.into_identity()?;
        let stage = CandidateStage::from_tag(self.stage)
            .ok_or_else(|| durable_error("invalid candidate stage"))?;
        CandidateCheckpoint::new(
            identity,
            stage,
            self.gates.into_evidence()?,
            self.obligations.into_evidence()?,
            self.review.into_evidence()?,
        )
        .map_err(|error| durable_error(error.to_string()))
    }
}

impl DurableIdentity {
    const fn from_identity(value: CandidateIdentity) -> Self {
        Self {
            run_id: *value.run_id().as_bytes(),
            workspace_id: *value.workspace_id().as_bytes(),
            candidate_digest: *value.candidate_digest().as_bytes(),
            conversation_revision: value.conversation_revision(),
            checkpoint_sequence: value.checkpoint_sequence(),
        }
    }

    fn into_identity(self) -> Result<CandidateIdentity, ProductRunnerError> {
        let run_id =
            RunId::new(self.run_id).map_err(|_| durable_error("invalid retained run identity"))?;
        let workspace_id = WorkspaceId::new(self.workspace_id)
            .map_err(|_| durable_error("invalid retained workspace identity"))?;
        CandidateIdentity::new(
            run_id,
            workspace_id,
            Sha256Digest::new(self.candidate_digest),
            self.conversation_revision,
            self.checkpoint_sequence,
        )
        .map_err(|error| durable_error(error.to_string()))
    }
}

impl DurableEvidence {
    fn from_evidence(value: &EvidenceStatus<QualificationEvidence>) -> Self {
        Self {
            status: value.tag(),
            provenance: value
                .record()
                .map(|record| DurableIdentity::from_identity(*record.provenance())),
            value: value.record().map(|record| record.value().tag()),
        }
    }

    fn into_evidence(self) -> Result<EvidenceStatus<QualificationEvidence>, ProductRunnerError> {
        if self.status == 1 {
            if self.provenance.is_some() || self.value.is_some() {
                return Err(durable_error("missing evidence retained an unexpected value"));
            }
            return Ok(EvidenceStatus::Missing);
        }
        let provenance = self
            .provenance
            .ok_or_else(|| durable_error("retained evidence omitted its provenance"))?
            .into_identity()?;
        let value = QualificationEvidence::from_tag(
            self.value.ok_or_else(|| durable_error("retained evidence omitted its value"))?,
        )
        .ok_or_else(|| durable_error("retained evidence has an invalid value"))?;
        let record = EvidenceRecord::new(provenance, value);
        match self.status {
            2 => Ok(EvidenceStatus::Current(record)),
            3 => Ok(EvidenceStatus::Failed(record)),
            4 => Ok(EvidenceStatus::Stale(record)),
            _ => Err(durable_error("retained evidence has an invalid status")),
        }
    }
}

impl DurableCommand {
    fn from_command(value: &SuccessfulCommand) -> Self {
        let purpose = match value.purpose {
            CommandPurpose::ExternalEffect => 1,
            CommandPurpose::Verification => 2,
        };
        Self { command: value.command.clone(), purpose }
    }

    fn into_command(self) -> Result<SuccessfulCommand, ProductRunnerError> {
        let purpose = match self.purpose {
            1 => CommandPurpose::ExternalEffect,
            2 => CommandPurpose::Verification,
            _ => return Err(durable_error("retained command has an invalid purpose")),
        };
        Ok(SuccessfulCommand { command: self.command, purpose })
    }
}

const fn phase_tag(value: ProductRunPhase) -> u16 {
    match value {
        ProductRunPhase::Designing => 1,
        ProductRunPhase::Writing => 2,
        ProductRunPhase::Checking => 3,
        ProductRunPhase::Reviewing => 4,
        ProductRunPhase::Fixing => 5,
        ProductRunPhase::Verifying => 6,
        ProductRunPhase::Finalizing => 7,
        ProductRunPhase::Complete => 8,
    }
}

fn restored_phase(tag: u16) -> Result<ProductRunPhase, ProductRunnerError> {
    match tag {
        1 => Ok(ProductRunPhase::Designing),
        2 => Ok(ProductRunPhase::Writing),
        // Effectful gate reports are intentionally fresh after process restart. All later phases
        // therefore continue at Checking while retaining their earlier design and writer state.
        3..=8 => Ok(ProductRunPhase::Checking),
        _ => Err(durable_error("retained continuation has an invalid phase")),
    }
}

fn durable_error(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "restore durable product-run continuation",
        detail,
    )
}
