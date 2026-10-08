//! Final candidate refresh and honest terminal settlement construction.

use core::fmt::Write as _;

use peritus_run_settlement::{
    CandidateStage, EvidenceStatus, QualificationEvidence, SettlementCause, SettlementReducer,
};

use super::{
    ProductRunInput, ProductRunOutcome, ProductRunOutput, ProductRunPhase, ProductRunQuestion,
    RunState,
    checkpoint::CandidateRecorder,
    obligations::RunObligations,
    resume::{
        ProductRunResume, RetainedDesign, RetainedFindings, RetainedResumeCapture,
    },
};
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, ProductRunnerFailureCause, bundle,
    candidate::CandidateBaseline,
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
    if fatal(error) && input.resume.is_none() {
        return Err(error.clone());
    }
    let mut cause = cause_from_error(error, false);
    let mut detail = error.settlement_detail();
    let mut reducer = SettlementReducer::new();
    let candidate = if let Some(resume) = input.resume.as_ref() {
        let identity = resume.checkpoint().identity();
        if identity.run_id() != input.run_id || identity.workspace_id() != input.workspace_id {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "restore retained candidate settlement",
                "refusing a resume from another logical run or workspace",
            ));
        }
        if identity.requirements_revision() != input.conversation.revision() {
            if cause != SettlementCause::Cancellation {
                cause = SettlementCause::Recovery;
            }
            let _ = write!(
                detail,
                "; retained requirements revision {} differs from current revision {}",
                identity.requirements_revision(),
                input.conversation.revision(),
            );
        }
        if resume.baseline().scope() != input.in_place_scope().as_ref() {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "restore workspace delivery",
                "refusing a resume from another delivery scope",
            ));
        }
        reducer.observe(*resume.checkpoint()).map_err(invariant)?;
        let changed_paths = match resume.baseline().changed_paths(&input.workspace_root) {
            Ok(paths) => Some(paths),
            Err(error) => {
                if cause != SettlementCause::Cancellation {
                    cause = SettlementCause::Recovery;
                }
                let _ = write!(
                    detail,
                    "; retained candidate material unavailable: {}",
                    error.settlement_detail(),
                );
                None
            }
        };
        let design_path = resume.design().map(|design| design.path().to_owned());
        if design_path.is_none() {
            if cause != SettlementCause::Cancellation {
                cause = SettlementCause::Recovery;
            }
            detail.push_str(
                "; retained candidate has no completed design; the same run must resume design",
            );
        }
        if resume.finding_ledger().is_none() {
            if cause != SettlementCause::Cancellation {
                cause = SettlementCause::Recovery;
            }
            detail.push_str(
                "; retained authoritative finding bytes are unavailable as a typed ledger",
            );
        }
        match (changed_paths, design_path) {
            (Some(changed_paths), Some(design_path)) => Some(ProductRunOutput {
                design_path,
                summary: resume.task_summary().to_owned(),
                diff: resume.diff().to_owned(),
                gates: resume.gates().to_owned(),
                review: resume.review().to_owned(),
                // Durable resumes deliberately reacquire effectful gate reports. Candidate paths
                // must therefore come from the retained baseline, not that optional runtime report.
                changed_paths,
                successful_commands: resume
                    .successful_commands()
                    .iter()
                    .map(|command| command.command.clone())
                    .collect(),
                run_instructions: resume.run_instructions().to_owned(),
                fixer_cycles: resume.fixer_cycles(),
                conversation_revision: resume.checkpoint().identity().requirements_revision(),
            }),
            _ => None,
        }
    } else {
        None
    };
    let checkpoint = reducer.checkpoint().copied();
    let settlement = reducer.settle(cause).map_err(invariant)?;
    let mut remaining_work = remaining_work(checkpoint.as_ref(), cause);
    append_resume_work(&mut remaining_work, input.resume.as_ref());
    Ok(ProductRunOutcome {
        settlement,
        candidate,
        question: None,
        detail: Some(detail),
        remaining_work,
        resume: input.resume.clone(),
    })
}

/// Refreshes and settles under the exact live governing-source contract.
pub(super) fn finalize_current(
    request: FinalizationInput<'_>,
    obligations: &RunObligations,
) -> Result<ProductRunOutcome, ProductRunnerError> {
    finalize_inner(request, Some(obligations))
}

/// Legacy unit fixtures exercise the same protected finalizer without a host source catalog.
#[cfg(test)]
pub(super) fn finalize(
    request: FinalizationInput<'_>,
) -> Result<ProductRunOutcome, ProductRunnerError> {
    finalize_inner(request, None)
}

fn finalize_inner(
    mut request: FinalizationInput<'_>,
    obligations: Option<&RunObligations>,
) -> Result<ProductRunOutcome, ProductRunnerError> {
    let conversation_revision = request.input.conversation.revision();
    let mut cause = request.cause;
    let mut detail = request.detail.take();
    if let Err(error) = request.recorder.refresh_for_finalization(conversation_revision) {
        if cause != SettlementCause::Cancellation {
            // A failed refresh cannot certify that retained qualification is still current.
            cause = SettlementCause::Recovery;
        }
        append_detail(&mut detail, &error);
    }
    let (checkpoint, checkpoint_failure) = request.recorder.checkpoint_for_handoff();
    if let Some(error) = checkpoint_failure {
        if cause != SettlementCause::Cancellation {
            cause = SettlementCause::Recovery;
        }
        append_detail(&mut detail, &error);
    }
    let candidate = if let Some(checkpoint) = checkpoint.as_ref() {
        let material = capture_candidate_material(&request);
        for error in &material.errors {
            if cause != SettlementCause::Cancellation {
                cause = SettlementCause::Recovery;
            }
            append_detail(&mut detail, error);
        }
        let remaining_work = remaining_work(Some(checkpoint), cause);
        match candidate_output(
            &request,
            checkpoint.stage(),
            &remaining_work,
            material.changed_paths,
            material.diff,
        ) {
            Ok(output) => Some(output),
            Err(error) => {
                if cause != SettlementCause::Cancellation {
                    cause = SettlementCause::Recovery;
                }
                append_detail(&mut detail, &error);
                None
            }
        }
    } else {
        None
    };
    let design = request
        .design
        .or_else(|| request.state.map(|state| &state.design))
        .map(RetainedDesign::from_document);
    let findings = retained_findings(&request);
    let mut resume = checkpoint.map(|checkpoint| {
        let (resume, projection_failure) =
            ProductRunResume::capture_retained(RetainedResumeCapture {
            checkpoint,
            baseline: request.baseline.clone(),
            next_phase: request.next_phase,
            design,
            task_summary: request
                .state
                .map_or_else(String::new, |state| state.task_summary.clone()),
            run_instructions: request
                .state
                .map_or_else(default_run_instructions, |state| state.run_instructions.clone()),
            fix_summaries: request.state.map_or_else(Vec::new, |state| state.fix_summaries.clone()),
            tool_calls: request.state.map_or(0, |state| state.tool_calls),
            findings,
            diff: candidate
                .as_ref()
                .map_or_else(|| request.diff.to_owned(), |output| output.diff.clone()),
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
            });
        if let Some(error) = projection_failure {
            if cause != SettlementCause::Cancellation {
                cause = SettlementCause::Recovery;
            }
            append_detail(&mut detail, &error);
        }
        resume
    });
    if let (Some(resume), Some(obligations)) = (&mut resume, obligations) {
        let revision = resume.checkpoint().identity().requirements_revision();
        let source_root = obligations.source_root_for_retained_revision(request.input, revision);
        let current = request.input.conversation.revision() == revision
            && obligations.source_contract_is_current(request.input)
            && (obligations.source_root_digest().is_none() || source_root.is_some());
        if current {
            resume.set_obligation_source_root(source_root);
        } else {
            if cause != SettlementCause::Cancellation {
                cause = SettlementCause::Recovery;
            }
            append_detail(&mut detail, &governing_input_changed());
            resume.set_obligation_source_root(None);
        }
    }
    // Own the complete raw continuation before consuming the predecessor's sole settlement.
    // Failure to derive a role index must not discard those authoritative values.
    let (settlement, settlement_failure) = request.recorder.settle_for_handoff(cause)?;
    if let Some(error) = settlement_failure {
        append_detail(&mut detail, &error);
    }
    let mut remaining_work = remaining_work(settlement.checkpoint(), settlement.cause());
    append_resume_work(&mut remaining_work, resume.as_ref());
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

fn retained_findings(request: &FinalizationInput<'_>) -> RetainedFindings {
    if let Some(state) = request.state {
        RetainedFindings::from_ledger(state.findings.clone())
    } else if let Some(resume) = request.input.resume.as_ref() {
        resume.retained_findings().clone()
    } else {
        RetainedFindings::from_encoded(request.input.finding_state.clone())
    }
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
    let design = request
        .design
        .or_else(|| request.state.map(|state| &state.design))
        .ok_or_else(|| {
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

pub(super) fn append_detail(detail: &mut Option<String>, error: &ProductRunnerError) {
    let diagnostic = error.settlement_detail();
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

fn governing_input_changed() -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "finalize governing conversation authority",
        "the governing source root or revision changed during finalization; the retained run must adopt the pending input before acceptance",
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
    if deadline_reached || error.failure_cause() == ProductRunnerFailureCause::SelectedDeadline {
        return SettlementCause::Deadline;
    }
    match error.failure_cause() {
        ProductRunnerFailureCause::ContextPreparation => return SettlementCause::Context,
        ProductRunnerFailureCause::AccountingRepresentation
        | ProductRunnerFailureCause::ResourceObservation
        | ProductRunnerFailureCause::UnclassifiedBudget => return SettlementCause::Recovery,
        _ => {}
    }
    match error.kind() {
        ProductRunnerErrorKind::InvalidPrecondition | ProductRunnerErrorKind::InternalInvariant => {
            SettlementCause::Recovery
        }
        ProductRunnerErrorKind::Repository => SettlementCause::Repository,
        ProductRunnerErrorKind::Provider => SettlementCause::Provider,
        ProductRunnerErrorKind::InvalidModelOutput => SettlementCause::Adapter,
        ProductRunnerErrorKind::Apply => SettlementCause::Recovery,
        ProductRunnerErrorKind::Gate => SettlementCause::Gate,
        ProductRunnerErrorKind::Budget => SettlementCause::Recovery,
        ProductRunnerErrorKind::Cancelled => SettlementCause::Cancellation,
    }
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
            remaining.push(
                "review the selected elapsed policy and resume this retained run".to_owned(),
            );
        }
        SettlementCause::Recovery => {
            remaining.push(
                "reconcile the retained failure and outstanding effects before resuming".to_owned(),
            );
        }
        SettlementCause::Adapter => {
            remaining.push(
                "resume the retained host work and return a valid terminal report".to_owned(),
            );
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

fn append_resume_work(remaining: &mut Vec<String>, resume: Option<&ProductRunResume>) {
    let Some(resume) = resume else { return };
    if resume.design().is_none() {
        remaining.push("resume the retained design phase on this same run".to_owned());
    }
    if resume.finding_ledger().is_none() {
        remaining.push(
            "restore the retained authoritative finding ledger before review or acceptance"
                .to_owned(),
        );
    }
}

fn default_run_instructions() -> String {
    "Resume this Peritus run to finish verification and acceptance.".to_owned()
}

#[cfg(test)]
#[path = "settlement/tests.rs"]
mod tests;
