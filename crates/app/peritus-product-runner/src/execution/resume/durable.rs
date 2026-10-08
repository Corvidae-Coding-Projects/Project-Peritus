//! Canonical persistence for the otherwise opaque product-run continuation.

use std::{io::Cursor, path::PathBuf};

use peritus_review::{
    ProductFinding, ProductFindingBodyFields, ProductFindingBodyReference,
    ProductFindingCategory, ProductFindingFieldReference, ProductFindingLedger,
    ProductFindingState, ProductReviewSummaryReference,
};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceDependencies, EvidenceRecord,
    EvidenceStatus, QualificationEvidence,
};
use peritus_spec::FindingSeverity;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeSeq as _};

use super::{ProductRunResume, RetainedDesign, RetainedFindings, RoleKnowledge};
use crate::{
    ProductRunPhase, ProductRunnerError, ProductRunnerErrorKind,
    candidate::CandidateBaseline,
    developer_tools::{CommandPurpose, SuccessfulCommand},
};

const DURABLE_VERSION: u16 = 6;
const VERSION_FIVE: u16 = 5;
const VERSION_FOUR: u16 = 4;
const VERSION_THREE: u16 = 3;
const LEGACY_DURABLE_VERSION: u16 = 2;

#[derive(Deserialize)]
struct DurableVersion {
    version: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableResume {
    version: u16,
    checkpoint: DurableCheckpoint,
    baseline_head: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    managed_baseline: Option<crate::candidate::managed::ManagedBaseline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    in_place_baseline: Option<crate::workspace_delivery::scope::ScopedBaseline>,
    next_phase: u16,
    #[serde(default)]
    design_path: Option<PathBuf>,
    #[serde(default)]
    design_markdown: Option<String>,
    #[serde(default)]
    design_revision: Option<u64>,
    #[serde(default)]
    design: Option<DurableDesign>,
    task_summary: String,
    run_instructions: String,
    fix_summaries: Vec<String>,
    tool_calls: u64,
    #[serde(alias = "finding_state")]
    findings: DurableFindings,
    diff: String,
    gates: String,
    review: String,
    #[serde(default)]
    retained_gate_state: Option<super::gate_state::RetainedGateState>,
    #[serde(default)]
    retained_gate_state_digest: Option<[u8; 32]>,
    developer_evidence: String,
    successful_commands: Vec<DurableCommand>,
    fixer_cycles: u32,
    #[serde(default)]
    obligation_source_root: Option<[u8; 32]>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct DurableResumeRef<'a> {
    version: u16,
    checkpoint: DurableCheckpoint,
    baseline_head: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    managed_baseline: Option<&'a crate::candidate::managed::ManagedBaseline>,
    #[serde(skip_serializing_if = "Option::is_none")]
    in_place_baseline: Option<&'a crate::workspace_delivery::scope::ScopedBaseline>,
    next_phase: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    design: Option<DurableDesignRef<'a>>,
    task_summary: &'a str,
    run_instructions: &'a str,
    fix_summaries: &'a [String],
    tool_calls: u64,
    findings: DurableFindingsRef<'a>,
    diff: &'a str,
    gates: &'a str,
    review: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    retained_gate_state: Option<&'a super::gate_state::RetainedGateState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retained_gate_state_digest: Option<[u8; 32]>,
    developer_evidence: &'a str,
    successful_commands: DurableCommands<'a>,
    fixer_cycles: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    obligation_source_root: Option<[u8; 32]>,
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
    content_digest: [u8; 32],
    repository_digest: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_digest: Option<[u8; 32]>,
    requirements_revision: u64,
    checkpoint_sequence: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DurableEvidence {
    status: u16,
    provenance: Option<DurableIdentity>,
    value: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dependencies: Option<u16>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableCommand {
    command: String,
    purpose: u16,
}

struct DurableCommands<'a>(&'a [SuccessfulCommand]);

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct DurableCommandRef<'a> {
    command: &'a str,
    purpose: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableDesign {
    path: PathBuf,
    markdown: String,
    conversation_revision: u64,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct DurableDesignRef<'a> {
    path: &'a std::path::Path,
    markdown: &'a str,
    conversation_revision: u64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DurableFindings {
    Legacy(String),
    VersionThree(DurableFindingsV3),
}

#[derive(Deserialize)]
#[serde(tag = "representation", content = "encoded", rename_all = "snake_case")]
enum DurableFindingsV3 {
    Typed(DurableFindingLedger),
    Encoded(String),
}

#[derive(Serialize)]
#[serde(tag = "representation", content = "encoded", rename_all = "snake_case")]
enum DurableFindingsRef<'a> {
    Typed(DurableFindingLedgerRef<'a>),
    Encoded(&'a str),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingLedger {
    cycle: u32,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    summary_body: Option<DurableReviewSummary>,
    #[serde(default)]
    coverage_after: Option<[u8; 32]>,
    #[serde(default)]
    pending_fixer: Vec<[u8; 32]>,
    findings: Vec<DurableFinding>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingLedgerRef<'a> {
    cycle: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary_body: Option<DurableReviewSummaryRef<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    coverage_after: Option<[u8; 32]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pending_fixer: Vec<[u8; 32]>,
    findings: DurableFindingSequence<'a>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableFinding {
    #[serde(default)]
    identity: Option<[u8; 32]>,
    #[serde(default)]
    identity_version: Option<u8>,
    #[serde(default)]
    provenance: Option<String>,
    category: String,
    severity: u8,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    reproduction: Option<String>,
    #[serde(default)]
    remediation: Option<String>,
    #[serde(default)]
    body: Option<DurableFindingBody>,
    state: u8,
    state_cycle: u32,
    first_cycle: u32,
    last_cycle: u32,
}

struct DurableFindingSequence<'a>(&'a ProductFindingLedger);

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingRef<'a> {
    identity: [u8; 32],
    identity_version: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<&'a str>,
    category: &'static str,
    severity: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reproduction: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remediation: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<DurableFindingBodyRef<'a>>,
    state: u8,
    state_cycle: u32,
    first_cycle: u32,
    last_cycle: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingBody {
    digest: [u8; 32],
    bytes: u64,
    source_ordinal: u64,
    fields: DurableFindingBodyFields,
    title_preview: DurableFindingPreview,
    provenance_preview: DurableFindingPreview,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableReviewSummary {
    digest: [u8; 32],
    bytes: u64,
    source_ordinal: u64,
    summary: DurableFindingField,
    preview: DurableFindingPreview,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct DurableReviewSummaryRef<'a> {
    digest: [u8; 32],
    bytes: u64,
    source_ordinal: u64,
    summary: DurableFindingField,
    preview: DurableFindingPreviewRef<'a>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingBodyRef<'a> {
    digest: [u8; 32],
    bytes: u64,
    source_ordinal: u64,
    fields: DurableFindingBodyFields,
    title_preview: DurableFindingPreviewRef<'a>,
    provenance_preview: DurableFindingPreviewRef<'a>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingBodyFields {
    title: DurableFindingField,
    description: DurableFindingField,
    location: DurableFindingField,
    provenance: DurableFindingField,
    reproduction: DurableFindingField,
    remediation: DurableFindingField,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingField {
    offset: u64,
    bytes: u64,
    digest: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingPreview {
    text: String,
    truncated: bool,
    binding: [u8; 32],
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct DurableFindingPreviewRef<'a> {
    text: &'a str,
    truncated: bool,
    binding: [u8; 32],
}

pub(super) fn encode(resume: &ProductRunResume) -> Result<Vec<u8>, ProductRunnerError> {
    let mut bytes = Vec::new();
    encode_into(resume, &mut bytes)?;
    Ok(bytes)
}

pub(super) fn encode_into(
    resume: &ProductRunResume,
    writer: impl std::io::Write,
) -> Result<(), ProductRunnerError> {
    let retained_gate_state = resume
        .gate_report
        .as_ref()
        .map(|report| super::gate_state::RetainedGateState::capture(report, &resume.checkpoint))
        .transpose()?;
    let retained_gate_state_digest = retained_gate_state
        .as_ref()
        .map(|state| state.binding_digest().into_bytes());
    let (version, findings) = match (
        resume.finding_ledger(),
        resume.retained_findings().encoded(),
    ) {
        (Some(ledger), None) => (if ledger.has_inline_finding_bodies() {
            VERSION_FOUR
        } else if ledger.has_inline_summary() {
            VERSION_FIVE
        } else {
            DURABLE_VERSION
        }, DurableFindingsRef::Typed(DurableFindingLedgerRef {
            cycle: ledger.cycle(),
            summary: ledger.inline_review_summary(),
            summary_body: ledger
                .review_summary_reference()
                .map(DurableReviewSummaryRef::from_reference),
            coverage_after: ledger.coverage_after().map(Sha256Digest::into_bytes),
            pending_fixer: ledger
                .pending_fixer()
                .iter()
                .copied()
                .map(Sha256Digest::into_bytes)
                .collect(),
            findings: DurableFindingSequence(ledger),
        })),
        (None, Some(encoded)) => (DURABLE_VERSION, DurableFindingsRef::Encoded(encoded)),
        (Some(_), Some(_)) | (None, None) => {
            return Err(durable_error(
                "retained findings have no exact typed or encoded representation",
            ));
        }
    };
    let payload = DurableResumeRef {
        version,
        checkpoint: DurableCheckpoint::from_checkpoint(&resume.checkpoint),
        baseline_head: resume.baseline.head(),
        managed_baseline: resume.baseline.managed(),
        in_place_baseline: resume.baseline.scope(),
        next_phase: phase_tag(resume.next_phase),
        design: resume.design().map(|design| DurableDesignRef {
            path: design.path(),
            markdown: design.markdown(),
            conversation_revision: design.conversation_revision(),
        }),
        task_summary: &resume.task_summary,
        run_instructions: &resume.run_instructions,
        fix_summaries: &resume.fix_summaries,
        tool_calls: resume.tool_calls,
        findings,
        diff: &resume.diff,
        gates: &resume.gates,
        review: &resume.review,
        retained_gate_state: retained_gate_state.as_ref(),
        retained_gate_state_digest,
        developer_evidence: &resume.developer_evidence,
        successful_commands: DurableCommands(&resume.successful_commands),
        fixer_cycles: resume.fixer_cycles,
        obligation_source_root: resume.obligation_source_root().map(Sha256Digest::into_bytes),
    };
    serde_json::to_writer(writer, &payload).map_err(|error| durable_error(error.to_string()))
}

pub(super) fn decode(
    bytes: &[u8],
    transcript: &str,
) -> Result<ProductRunResume, ProductRunnerError> {
    decode_inner(Cursor::new(bytes), transcript, true)
}

pub(super) fn decode_retained(
    bytes: &[u8],
    transcript: &str,
) -> Result<ProductRunResume, ProductRunnerError> {
    decode_retained_from(Cursor::new(bytes), transcript)
}

pub(super) fn decode_retained_from(
    reader: impl std::io::Read,
    transcript: &str,
) -> Result<ProductRunResume, ProductRunnerError> {
    decode_inner(reader, transcript, false)
}

pub(super) fn version_from(
    reader: impl std::io::Read,
) -> Result<u16, ProductRunnerError> {
    serde_json::from_reader::<_, DurableVersion>(reader)
        .map(|value| value.version)
        .map_err(|error| durable_error(error.to_string()))
}

pub(super) const fn version_supported(version: u16) -> bool {
    matches!(
        version,
        LEGACY_DURABLE_VERSION
            | VERSION_THREE
            | VERSION_FOUR
            | VERSION_FIVE
            | DURABLE_VERSION
    )
}

fn decode_inner(
    reader: impl std::io::Read,
    transcript: &str,
    reconcile_execution: bool,
) -> Result<ProductRunResume, ProductRunnerError> {
    let payload: DurableResume =
        serde_json::from_reader(reader).map_err(|error| durable_error(error.to_string()))?;
    if !version_supported(payload.version) {
        return Err(durable_error("unsupported durable resume version"));
    }
    if payload.version == LEGACY_DURABLE_VERSION
        && (payload.design.is_some()
            || payload.retained_gate_state.is_some()
            || payload.retained_gate_state_digest.is_some()
            || payload.obligation_source_root.is_some())
    {
        return Err(durable_error(
            "legacy continuation contains fields from a newer durable version",
        ));
    }
    if payload.version >= VERSION_THREE
        && (payload.design_path.is_some()
            || payload.design_markdown.is_some()
            || payload.design_revision.is_some())
    {
        return Err(durable_error(
            "indexed continuation contains legacy design fields",
        ));
    }
    let checkpoint = payload.checkpoint.into_checkpoint()?;
    let next_phase = restored_phase(payload.next_phase, payload.version >= VERSION_THREE)?;
    let design = if payload.version == LEGACY_DURABLE_VERSION {
        match (
            payload.design_path,
            payload.design_markdown,
            payload.design_revision,
        ) {
            (Some(path), Some(markdown), Some(revision)) => {
                Some(RetainedDesign::new(path, markdown, revision)?)
            }
            _ => {
                return Err(durable_error(
                    "legacy continuation omitted part of its required design",
                ));
            }
        }
    } else {
        payload
            .design
            .map(|design| {
                RetainedDesign::new(
                    design.path,
                    design.markdown,
                    design.conversation_revision,
                )
            })
            .transpose()?
    };
    let durable_version = payload.version;
    let findings = match (durable_version, payload.findings) {
        (LEGACY_DURABLE_VERSION, DurableFindings::Legacy(encoded)) => {
            RetainedFindings::from_encoded(encoded)
        }
        (VERSION_THREE, DurableFindings::VersionThree(DurableFindingsV3::Typed(ledger))) => {
            RetainedFindings::from_ledger(ledger.into_ledger(VERSION_THREE)?)
        }
        (VERSION_FOUR, DurableFindings::VersionThree(DurableFindingsV3::Typed(ledger))) => {
            RetainedFindings::from_ledger(ledger.into_ledger(VERSION_FOUR)?)
        }
        (VERSION_FIVE, DurableFindings::VersionThree(DurableFindingsV3::Typed(ledger))) => {
            RetainedFindings::from_ledger(ledger.into_ledger(VERSION_FIVE)?)
        }
        (DURABLE_VERSION, DurableFindings::VersionThree(DurableFindingsV3::Typed(ledger))) => {
            RetainedFindings::from_ledger(ledger.into_ledger(DURABLE_VERSION)?)
        }
        (VERSION_THREE, DurableFindings::VersionThree(DurableFindingsV3::Encoded(encoded)))
        | (VERSION_FOUR, DurableFindings::VersionThree(
            DurableFindingsV3::Encoded(encoded),
        ))
        | (VERSION_FIVE, DurableFindings::VersionThree(
            DurableFindingsV3::Encoded(encoded),
        ))
        | (DURABLE_VERSION, DurableFindings::VersionThree(
            DurableFindingsV3::Encoded(encoded),
        )) => {
            if crate::review::restore_ledger(&encoded).is_ok() {
                return Err(durable_error(
                    "encoded finding fallback contains a valid typed ledger",
                ));
            }
            RetainedFindings::Encoded(encoded)
        }
        _ => return Err(durable_error("finding representation does not match durable version")),
    };
    let baseline = match payload.in_place_baseline {
        Some(scope) if payload.baseline_head.is_empty() => CandidateBaseline::in_place(scope),
        Some(_) => return Err(durable_error("resume mixes Git and in-place baselines")),
        None => CandidateBaseline::restored(payload.baseline_head)?,
    };
    let baseline = baseline.with_managed(payload.managed_baseline)?;
    let successful_commands = payload
        .successful_commands
        .into_iter()
        .map(DurableCommand::into_command)
        .collect::<Result<Vec<_>, _>>()?;
    let knowledge = match (design.as_ref(), findings.knowledge_projection().ok()) {
        (Some(design), Some(findings)) => RoleKnowledge::capture(
            *checkpoint.identity(),
            transcript,
            design.markdown(),
            &findings,
            &payload.developer_evidence,
        )
        .ok(),
        _ => None,
    };
    let gate_state_digest = payload.retained_gate_state_digest.map(Sha256Digest::new);
    let gate_report = match (payload.retained_gate_state, gate_state_digest) {
        (Some(state), Some(expected)) => {
            if state.binding_digest() != expected {
                return Err(durable_error(
                    "gate-state section digest differs from its continuation descriptor",
                ));
            }
            Some(state.restore(&checkpoint)?)
        }
        (None, None) => None,
        (Some(_), None) | (None, Some(_)) => {
            return Err(durable_error(
                "gate-state section and descriptor must be present together",
            ));
        }
    };
    let mut resume = ProductRunResume {
        checkpoint,
        baseline,
        next_phase,
        design,
        task_summary: payload.task_summary,
        run_instructions: payload.run_instructions,
        fix_summaries: payload.fix_summaries,
        tool_calls: payload.tool_calls,
        findings,
        diff: payload.diff,
        gates: payload.gates,
        review: payload.review,
        gate_report,
        developer_evidence: payload.developer_evidence,
        successful_commands,
        fixer_cycles: payload.fixer_cycles,
        obligation_source_root: payload.obligation_source_root.map(Sha256Digest::new),
        knowledge,
    };
    resume.next_phase = if resume.design.is_none() {
        ProductRunPhase::Designing
    } else {
        super::phase_with_retained_evidence(
            resume.next_phase,
            &resume.checkpoint,
            resume.gate_report.as_ref(),
        )
    };
    if reconcile_execution {
        let checkpoint = crate::ProductRunner::reconcile_checkpoint_after_restart(
            resume.checkpoint,
        )?;
        resume = resume.reconcile_candidate(checkpoint)?;
    }
    Ok(resume)
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
    fn from_identity(value: CandidateIdentity) -> Self {
        Self {
            run_id: *value.run_id().as_bytes(),
            workspace_id: *value.workspace_id().as_bytes(),
            content_digest: value.content_digest().into_bytes(),
            repository_digest: value.repository_digest().into_bytes(),
            execution_digest: value.execution_digest().map(Sha256Digest::into_bytes),
            requirements_revision: value.requirements_revision(),
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
            Sha256Digest::new(self.content_digest),
            Sha256Digest::new(self.repository_digest),
            self.execution_digest.map(Sha256Digest::new),
            self.requirements_revision,
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
            dependencies: value.record().map(|record| record.dependencies().tag()),
        }
    }

    fn into_evidence(self) -> Result<EvidenceStatus<QualificationEvidence>, ProductRunnerError> {
        if self.status == 1 {
            if self.provenance.is_some() || self.value.is_some() || self.dependencies.is_some() {
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
        let dependencies = EvidenceDependencies::from_tag(
            self.dependencies
                .ok_or_else(|| durable_error("retained evidence omitted its dependencies"))?,
        )
        .ok_or_else(|| durable_error("retained evidence dependencies are invalid"))?;
        let record = EvidenceRecord::new(provenance, dependencies, value);
        match self.status {
            2 => Ok(EvidenceStatus::Current(record)),
            3 => Ok(EvidenceStatus::Failed(record)),
            4 => Ok(EvidenceStatus::Stale(record)),
            _ => Err(durable_error("retained evidence has an invalid status")),
        }
    }
}

impl DurableCommand {
    fn into_command(self) -> Result<SuccessfulCommand, ProductRunnerError> {
        let purpose = match self.purpose {
            1 => CommandPurpose::ExternalEffect,
            2 => CommandPurpose::Verification,
            _ => return Err(durable_error("retained command has an invalid purpose")),
        };
        Ok(SuccessfulCommand { command: self.command, purpose })
    }
}

impl Serialize for DurableCommands<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for command in self.0 {
            let purpose = match command.purpose {
                CommandPurpose::ExternalEffect => 1,
                CommandPurpose::Verification => 2,
            };
            sequence.serialize_element(&DurableCommandRef {
                command: &command.command,
                purpose,
            })?;
        }
        sequence.end()
    }
}

impl DurableFindingLedger {
    fn into_ledger(self, version: u16) -> Result<ProductFindingLedger, ProductRunnerError> {
        if version == VERSION_THREE
            && (self.coverage_after.is_some() || !self.pending_fixer.is_empty())
        {
            return Err(durable_error(
                "version-three finding ledger contains indexed review cursors",
            ));
        }
        let findings = self
            .findings
            .into_iter()
            .map(|finding| finding.into_finding(version))
            .collect::<Result<Vec<_>, _>>()?;
        let coverage_after = self.coverage_after.map(Sha256Digest::new);
        let pending_fixer = self.pending_fixer.into_iter().map(Sha256Digest::new).collect();
        let restored = match (version, self.summary, self.summary_body) {
            (
                LEGACY_DURABLE_VERSION | VERSION_THREE | VERSION_FOUR | VERSION_FIVE,
                Some(summary),
                None,
            ) => {
                ProductFindingLedger::restore_durable(
                    self.cycle,
                    summary,
                    coverage_after,
                    pending_fixer,
                    findings,
                )
            }
            (DURABLE_VERSION, Some(summary), None) if self.cycle == 0 => {
                ProductFindingLedger::restore_durable(
                    self.cycle,
                    summary,
                    coverage_after,
                    pending_fixer,
                    findings,
                )
            }
            (DURABLE_VERSION, None, Some(summary)) => {
                ProductFindingLedger::restore_durable_stored(
                    self.cycle,
                    summary.into_reference(self.cycle)?,
                    coverage_after,
                    pending_fixer,
                    findings,
                )
            }
            _ => return Err(durable_error("retained review summary authority is invalid")),
        };
        restored.map_err(|error| durable_error(error.to_string()))
    }
}

impl DurableFinding {
    fn into_finding(self, version: u16) -> Result<ProductFinding, ProductRunnerError> {
        let category = ProductFindingCategory::parse(&self.category)
            .ok_or_else(|| durable_error("retained finding category is invalid"))?;
        let severity = match self.severity {
            1 => FindingSeverity::Advisory,
            2 => FindingSeverity::Low,
            3 => FindingSeverity::Medium,
            4 => FindingSeverity::High,
            5 => FindingSeverity::Critical,
            _ => return Err(durable_error("retained finding severity is invalid")),
        };
        let state = match (self.state, self.state_cycle) {
            (1, 0) => ProductFindingState::Open,
            // Fix proposals retain the independent fixer-cycle coordinate; first/last cycle are
            // review history and therefore do not bound this value.
            (2, cycle) if cycle > 0 => {
                ProductFindingState::FixProposed { cycle }
            }
            (3, cycle) if cycle > 0 => ProductFindingState::ResolutionConfirmed { cycle },
            _ => return Err(durable_error("retained finding state is invalid")),
        };
        if version >= VERSION_FIVE {
            if self.provenance.is_some()
                || self.title.is_some()
                || self.description.is_some()
                || self.location.is_some()
                || self.reproduction.is_some()
                || self.remediation.is_some()
            {
                return Err(durable_error(
                    "compact finding contains legacy inline body fields",
                ));
            }
            let identity = self
                .identity
                .ok_or_else(|| durable_error("compact finding identity is incomplete"))?;
            let identity_version = self
                .identity_version
                .ok_or_else(|| durable_error("compact finding identity is incomplete"))?;
            let body = self
                .body
                .ok_or_else(|| durable_error("compact finding body is missing"))?;
            let reference = body.into_reference(Sha256Digest::new(identity), identity_version)?;
            return ProductFinding::restore_stored(
                Sha256Digest::new(identity),
                identity_version,
                category,
                severity,
                reference,
                state,
                self.first_cycle,
                self.last_cycle,
            )
            .map_err(|error| durable_error(error.to_string()));
        }
        if self.body.is_some() {
            return Err(durable_error(
                "legacy finding contains a version-five body descriptor",
            ));
        }
        let title = self.title.ok_or_else(|| durable_error("retained finding title is missing"))?;
        let description = self
            .description
            .ok_or_else(|| durable_error("retained finding description is missing"))?;
        let location = self
            .location
            .ok_or_else(|| durable_error("retained finding location is missing"))?;
        let reproduction = self
            .reproduction
            .ok_or_else(|| durable_error("retained finding reproduction is missing"))?;
        let remediation = self
            .remediation
            .ok_or_else(|| durable_error("retained finding remediation is missing"))?;
        let restored = match (version, self.identity, self.identity_version, self.provenance) {
            (VERSION_THREE, None, None, None) => ProductFinding::restore(
                category,
                severity,
                title,
                description,
                location,
                reproduction,
                remediation,
                state,
                self.first_cycle,
                self.last_cycle,
            ),
            (VERSION_FOUR, Some(identity), Some(identity_version), Some(provenance)) => {
                ProductFinding::restore_indexed(
                    Sha256Digest::new(identity),
                    identity_version,
                    provenance,
                    category,
                    severity,
                    title,
                    description,
                    location,
                    reproduction,
                    remediation,
                    state,
                    self.first_cycle,
                    self.last_cycle,
                )
            }
            (VERSION_THREE, _, _, _) => {
                return Err(durable_error(
                    "version-three finding contains indexed identity fields",
                ));
            }
            (VERSION_FOUR, _, _, _) => {
                return Err(durable_error("version-four finding identity is incomplete"));
            }
            _ => return Err(durable_error("finding identity version is unsupported")),
        };
        restored.map_err(|error| durable_error(error.to_string()))
    }
}

impl DurableFindingBody {
    fn into_reference(
        self,
        finding_id: Sha256Digest,
        identity_version: u8,
    ) -> Result<ProductFindingBodyReference, ProductRunnerError> {
        let fields = ProductFindingBodyFields::restore(
            self.fields.title.into_reference(),
            self.fields.description.into_reference(),
            self.fields.location.into_reference(),
            self.fields.provenance.into_reference(),
            self.fields.reproduction.into_reference(),
            self.fields.remediation.into_reference(),
        )
        .map_err(|error| durable_error(error.to_string()))?;
        ProductFindingBodyReference::restore(
            finding_id,
            identity_version,
            Sha256Digest::new(self.digest),
            self.bytes,
            self.source_ordinal,
            fields,
            self.title_preview.text,
            self.title_preview.truncated,
            Sha256Digest::new(self.title_preview.binding),
            self.provenance_preview.text,
            self.provenance_preview.truncated,
            Sha256Digest::new(self.provenance_preview.binding),
        )
        .map_err(|error| durable_error(error.to_string()))
    }
}

impl DurableReviewSummary {
    fn into_reference(
        self,
        review_cycle: u32,
    ) -> Result<ProductReviewSummaryReference, ProductRunnerError> {
        ProductReviewSummaryReference::restore(
            review_cycle,
            Sha256Digest::new(self.digest),
            self.bytes,
            self.source_ordinal,
            self.summary.into_reference(),
            self.preview.text,
            self.preview.truncated,
            Sha256Digest::new(self.preview.binding),
        )
        .map_err(|error| durable_error(error.to_string()))
    }
}

impl DurableFindingField {
    const fn into_reference(self) -> ProductFindingFieldReference {
        ProductFindingFieldReference::new(
            self.offset,
            self.bytes,
            Sha256Digest::new(self.digest),
        )
    }

    const fn from_reference(reference: ProductFindingFieldReference) -> Self {
        Self {
            offset: reference.offset(),
            bytes: reference.bytes(),
            digest: reference.digest().into_bytes(),
        }
    }
}

impl<'a> DurableFindingBodyRef<'a> {
    fn from_reference(reference: &'a ProductFindingBodyReference) -> Self {
        let fields = reference.fields();
        Self {
            digest: reference.digest().into_bytes(),
            bytes: reference.bytes(),
            source_ordinal: reference.source_ordinal(),
            fields: DurableFindingBodyFields {
                title: DurableFindingField::from_reference(fields.title()),
                description: DurableFindingField::from_reference(fields.description()),
                location: DurableFindingField::from_reference(fields.location()),
                provenance: DurableFindingField::from_reference(fields.provenance()),
                reproduction: DurableFindingField::from_reference(fields.reproduction()),
                remediation: DurableFindingField::from_reference(fields.remediation()),
            },
            title_preview: DurableFindingPreviewRef {
                text: reference.title_preview().text(),
                truncated: reference.title_preview().truncated(),
                binding: reference.title_preview().binding().into_bytes(),
            },
            provenance_preview: DurableFindingPreviewRef {
                text: reference.provenance_preview().text(),
                truncated: reference.provenance_preview().truncated(),
                binding: reference.provenance_preview().binding().into_bytes(),
            },
        }
    }
}

impl<'a> DurableReviewSummaryRef<'a> {
    fn from_reference(reference: &'a ProductReviewSummaryReference) -> Self {
        Self {
            digest: reference.digest().into_bytes(),
            bytes: reference.bytes(),
            source_ordinal: reference.source_ordinal(),
            summary: DurableFindingField::from_reference(reference.summary()),
            preview: DurableFindingPreviewRef {
                text: reference.preview().text(),
                truncated: reference.preview().truncated(),
                binding: reference.preview().binding().into_bytes(),
            },
        }
    }
}

impl Serialize for DurableFindingSequence<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(None)?;
        for finding in self.0.findings() {
            let severity = match finding.severity() {
                FindingSeverity::Advisory => 1,
                FindingSeverity::Low => 2,
                FindingSeverity::Medium => 3,
                FindingSeverity::High => 4,
                FindingSeverity::Critical => 5,
            };
            let (state, state_cycle) = match finding.state() {
                ProductFindingState::Open => (1, 0),
                ProductFindingState::FixProposed { cycle } => (2, cycle),
                ProductFindingState::ResolutionConfirmed { cycle } => (3, cycle),
            };
            let (provenance, title, description, location, reproduction, remediation, body) =
                if let Some(reference) = finding.body_reference() {
                    (
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(DurableFindingBodyRef::from_reference(reference)),
                    )
                } else {
                    (
                        Some(finding.provenance()),
                        Some(finding.title()),
                        Some(finding.description()),
                        Some(finding.location()),
                        Some(finding.reproduction()),
                        Some(finding.remediation()),
                        None,
                    )
                };
            sequence.serialize_element(&DurableFindingRef {
                identity: finding.id().into_bytes(),
                identity_version: finding.identity_version(),
                provenance,
                category: finding.category().as_str(),
                severity,
                title,
                description,
                location,
                reproduction,
                remediation,
                body,
                state,
                state_cycle,
                first_cycle: finding.first_cycle(),
                last_cycle: finding.last_cycle(),
            })?;
        }
        sequence.end()
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

fn restored_phase(
    tag: u16,
    retains_gate_state: bool,
) -> Result<ProductRunPhase, ProductRunnerError> {
    match (tag, retains_gate_state) {
        (1, _) => Ok(ProductRunPhase::Designing),
        (2, _) => Ok(ProductRunPhase::Writing),
        // Effectful gate reports are intentionally fresh after process restart. All later phases
        // in legacy payloads therefore continue at Checking. Version three and later restore an
        // exact candidate-bound report below before its later phase can be reused.
        (3..=8, false) => Ok(ProductRunPhase::Checking),
        (3, true) => Ok(ProductRunPhase::Checking),
        (4, true) => Ok(ProductRunPhase::Reviewing),
        (5, true) => Ok(ProductRunPhase::Fixing),
        (6, true) => Ok(ProductRunPhase::Verifying),
        (7, true) => Ok(ProductRunPhase::Finalizing),
        (8, true) => Ok(ProductRunPhase::Complete),
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
