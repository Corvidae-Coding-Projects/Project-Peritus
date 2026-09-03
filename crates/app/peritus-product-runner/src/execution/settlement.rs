//! Final candidate refresh and honest terminal settlement construction.

use core::fmt::Write as _;

use peritus_run_settlement::{
    CandidateStage, EvidenceStatus, QualificationEvidence, RunDisposition, SettlementCause,
    SettlementReducer,
};

use super::{
    ProductRunInput, ProductRunOutcome, ProductRunOutput, ProductRunPhase, ProductRunQuestion,
    RunState,
    checkpoint::CandidateRecorder,
    resume::{ProductRunResume, ResumeCapture},
};
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, bundle, candidate::CandidateBaseline,
    design::DesignDocument,
};

/// Latest effectful state available to the finalization arbiter.
pub(super) struct FinalizationInput<'a> {
    pub(super) input: &'a ProductRunInput,
    pub(super) baseline: &'a CandidateBaseline,
    pub(super) recorder: &'a CandidateRecorder,
    pub(super) design: Option<&'a DesignDocument>,
    pub(super) state: Option<&'a RunState>,
    pub(super) diff: &'a str,
    pub(super) gates: &'a str,
    pub(super) review: &'a str,
    pub(super) gate_report: Option<&'a crate::gates::GateReport>,
    pub(super) cause: SettlementCause,
    pub(super) question: Option<(String, u64)>,
    pub(super) detail: Option<String>,
    pub(super) next_phase: ProductRunPhase,
}

/// Converts an ordinary failure before the active loop into an honest no-candidate or retained
/// resume-candidate settlement.
pub(super) fn from_initial_error(
    input: &ProductRunInput,
    error: &ProductRunnerError,
) -> Result<ProductRunOutcome, ProductRunnerError> {
    if fatal(error) {
        return Err(ProductRunnerError::new(error.kind(), error.operation(), error.detail()));
    }
    let cause = cause_from_error(error, false);
    let mut reducer = SettlementReducer::new();
    let candidate = if let Some(resume) = input.resume.as_ref() {
        reducer.observe(*resume.checkpoint()).map_err(invariant)?;
        Some(ProductRunOutput {
            design_path: resume.design_path().clone(),
            summary: resume.task_summary().to_owned(),
            diff: resume.diff().to_owned(),
            gates: resume.gates().to_owned(),
            review: resume.review().to_owned(),
            changed_paths: resume
                .gate_report()
                .map_or_else(Vec::new, |report| report.report.changed_paths().to_vec()),
            successful_commands: resume
                .successful_commands()
                .iter()
                .map(|command| command.command.clone())
                .collect(),
            run_instructions: resume.run_instructions().to_owned(),
            fixer_cycles: resume.fixer_cycles(),
            conversation_revision: resume.checkpoint().identity().conversation_revision(),
        })
    } else {
        None
    };
    let checkpoint = reducer.checkpoint().copied();
    let settlement = reducer.settle(cause).map_err(invariant)?;
    Ok(ProductRunOutcome {
        settlement,
        candidate,
        question: None,
        detail: Some(format!("{}: {}", error.operation(), error.detail())),
        remaining_work: remaining_work(checkpoint.as_ref(), cause),
        resume: input.resume.clone(),
    })
}

/// Refreshes the exact diff and settles from the strongest current checkpoint.
pub(super) fn finalize(
    mut request: FinalizationInput<'_>,
) -> Result<ProductRunOutcome, ProductRunnerError> {
    let conversation_revision = request.input.conversation.revision();
    let mut cause = request.cause;
    let mut detail = request.detail.take();
    if let Err(error) = request.recorder.refresh(conversation_revision) {
        if fatal(&error) {
            return Err(error);
        }
        if cause == SettlementCause::Completed {
            cause = cause_from_error(&error, false);
        }
        append_detail(&mut detail, &error);
    }
    let checkpoint = request.recorder.checkpoint()?;
    let remaining_work = remaining_work(checkpoint.as_ref(), cause);
    let candidate = if let Some(checkpoint) = checkpoint.as_ref() {
        let material = capture_candidate_material(&request);
        for error in &material.errors {
            if cause == SettlementCause::Completed {
                cause = cause_from_error(error, false);
            }
            append_detail(&mut detail, error);
        }
        Some(candidate_output(
            &request,
            checkpoint.stage(),
            &remaining_work,
            material.changed_paths,
            material.diff,
        )?)
    } else {
        None
    };
    let settlement = request.recorder.settle(cause)?;
    let resume = match (checkpoint, request.design) {
        (Some(checkpoint), Some(design))
            if settlement.disposition() != RunDisposition::Accepted =>
        {
            Some(ProductRunResume::capture(ResumeCapture {
                checkpoint,
                baseline: request.baseline.clone(),
                next_phase: request.next_phase,
                design_path: design.path().to_owned(),
                design_markdown: design.markdown().to_owned(),
                design_revision: design.conversation_revision(),
                task_summary: request
                    .state
                    .map_or_else(String::new, |state| state.task_summary.clone()),
                run_instructions: request
                    .state
                    .map_or_else(default_run_instructions, |state| state.run_instructions.clone()),
                fix_summaries: request
                    .state
                    .map_or_else(Vec::new, |state| state.fix_summaries.clone()),
                tool_calls: request.state.map_or(0, |state| state.tool_calls),
                finding_state: request
                    .state
                    .map(developer_findings)
                    .transpose()?
                    .unwrap_or_else(|| request.input.finding_state.clone()),
                diff: candidate.as_ref().map_or_else(String::new, |output| output.diff.clone()),
                gates: request.gates.to_owned(),
                review: request.review.to_owned(),
                gate_report: request.gate_report.cloned(),
                developer_evidence: request
                    .state
                    .map_or_else(String::new, |state| state.developer_evidence.clone()),
                successful_commands: request
                    .state
                    .map_or_else(Vec::new, |state| state.successful_commands.clone()),
                fixer_cycles: request
                    .state
                    .map_or(0, |state| state.coordinator.completed_fixer_cycles()),
                transcript: request.input.conversation.render(),
            })?)
        }
        _ => None,
    };
    let question = request
        .question
        .map(|(message, revision)| ProductRunQuestion { message, conversation_revision: revision });
    Ok(ProductRunOutcome { settlement, candidate, question, detail, remaining_work, resume })
}

struct CandidateMaterial {
    changed_paths: Vec<std::path::PathBuf>,
    diff: String,
    errors: Vec<ProductRunnerError>,
}

fn capture_candidate_material(request: &FinalizationInput<'_>) -> CandidateMaterial {
    let mut errors = Vec::new();
    let changed_paths =
        request.baseline.changed_paths(&request.input.workspace_root).unwrap_or_else(|error| {
            errors.push(error);
            request
                .gate_report
                .map_or_else(Vec::new, |report| report.report.changed_paths().to_vec())
        });
    let diff =
        bundle::diff(&request.input.workspace_root, request.baseline).unwrap_or_else(|error| {
            errors.push(error);
            request.diff.to_owned()
        });
    CandidateMaterial { changed_paths, diff, errors }
}

fn candidate_output(
    request: &FinalizationInput<'_>,
    candidate_stage: CandidateStage,
    remaining_work: &[String],
    changed_paths: Vec<std::path::PathBuf>,
    diff: String,
) -> Result<ProductRunOutput, ProductRunnerError> {
    let design = request.design.ok_or_else(|| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InternalInvariant,
            "finalize candidate handoff",
            "a workspace candidate exists without its completed design",
        )
    })?;
    let effect_requirement = crate::delivery_requirement::ExternalEffectRequirement::from_task(
        request.input.delivery_scope,
        &request.input.task,
    );
    let successful_commands = match (request.gate_report, request.state) {
        (Some(gate_report), Some(run_state)) => super::acceptance::successful_command_lines(
            request.input.delivery_scope,
            effect_requirement,
            gate_report,
            &run_state.successful_commands,
        ),
        (None, Some(run_state)) => {
            run_state.successful_commands.iter().map(|command| command.command.clone()).collect()
        }
        (_, None) => Vec::new(),
    };
    let mut summary = request.state.map_or_else(
        || "Peritus retained the strongest workspace candidate before the run stopped.".to_owned(),
        |state| state.task_summary.clone(),
    );
    if !remaining_work.is_empty() {
        summary.push_str(" Remaining work: ");
        summary.push_str(&remaining_work.join("; "));
    }
    let _ = write!(summary, " Candidate stage: {candidate_stage:?}.");
    Ok(ProductRunOutput {
        design_path: design.path().to_owned(),
        summary,
        diff,
        gates: request.gates.to_owned(),
        review: request.review.to_owned(),
        changed_paths,
        successful_commands,
        run_instructions: request
            .state
            .map_or_else(default_run_instructions, |state| state.run_instructions.clone()),
        fixer_cycles: request.state.map_or(0, |state| state.coordinator.completed_fixer_cycles()),
        conversation_revision: request.input.conversation.revision(),
    })
}

fn append_detail(detail: &mut Option<String>, error: &ProductRunnerError) {
    let diagnostic = format!("{}: {}", error.operation(), error.detail());
    match detail {
        Some(detail) => {
            detail.push_str("; finalization fallback: ");
            detail.push_str(&diagnostic);
        }
        None => *detail = Some(diagnostic),
    }
}

fn invariant(error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "construct product-run settlement",
        error.to_string(),
    )
}

const fn fatal(error: &ProductRunnerError) -> bool {
    matches!(
        error.kind(),
        ProductRunnerErrorKind::InvalidPrecondition | ProductRunnerErrorKind::InternalInvariant
    )
}

pub(super) fn cause_from_error(
    error: &ProductRunnerError,
    deadline_reached: bool,
) -> SettlementCause {
    if deadline_reached {
        return SettlementCause::Deadline;
    }
    match error.kind() {
        ProductRunnerErrorKind::InvalidPrecondition | ProductRunnerErrorKind::InternalInvariant => {
            SettlementCause::InternalInvariant
        }
        ProductRunnerErrorKind::Repository => SettlementCause::Repository,
        ProductRunnerErrorKind::Provider if context_failure(error.detail()) => {
            SettlementCause::Context
        }
        ProductRunnerErrorKind::Provider => SettlementCause::Provider,
        ProductRunnerErrorKind::InvalidModelOutput => SettlementCause::Adapter,
        ProductRunnerErrorKind::Apply => SettlementCause::Recovery,
        ProductRunnerErrorKind::Gate => SettlementCause::Gate,
        ProductRunnerErrorKind::Budget => SettlementCause::Deadline,
        ProductRunnerErrorKind::Cancelled => SettlementCause::Cancellation,
    }
}

fn context_failure(detail: &str) -> bool {
    let normalized = detail.to_ascii_lowercase();
    normalized.contains("context window")
        || normalized.contains("context limit")
        || normalized.contains("context overflow")
}

fn remaining_work(
    checkpoint: Option<&peritus_run_settlement::CandidateCheckpoint>,
    cause: SettlementCause,
) -> Vec<String> {
    let Some(checkpoint) = checkpoint else { return Vec::new() };
    let identity = checkpoint.identity();
    let mut remaining = Vec::new();
    if !satisfied(checkpoint.gates(), identity) {
        remaining.push("run the exact deterministic gates for the current candidate".to_owned());
    }
    if !satisfied(checkpoint.obligations(), identity) {
        remaining.push("satisfy every current public requirement obligation".to_owned());
    }
    if !satisfied(checkpoint.review(), identity) {
        remaining.push("complete a current independent blocker-free review".to_owned());
    }
    match cause {
        SettlementCause::Provider => {
            remaining
                .push("recover the selected provider and resume the interrupted phase".to_owned());
        }
        SettlementCause::Deadline => {
            remaining.push("continue with a fresh phase budget".to_owned());
        }
        SettlementCause::Recovery => {
            remaining.push("reconcile the interrupted command boundary".to_owned());
        }
        _ => {}
    }
    remaining
}

fn satisfied(
    status: &EvidenceStatus<QualificationEvidence>,
    identity: &peritus_run_settlement::CandidateIdentity,
) -> bool {
    status.is_current_and_satisfied(identity)
}

fn developer_findings(state: &RunState) -> Result<String, ProductRunnerError> {
    crate::review::encode_ledger(&state.findings)
}

fn default_run_instructions() -> String {
    "Resume this Peritus run to finish verification and acceptance.".to_owned()
}

#[cfg(test)]
#[path = "settlement/tests.rs"]
mod tests;
