//! Independent review and obligation qualification for a checked candidate.

use peritus_run_settlement::CandidateStage;

use super::{
    ProductRunInput, ProductRunPhase, RunObserver, check_cancelled,
    checkpoint::{CandidateRecorder, CheckpointEvidence},
    cycle::{CycleInspection, GateInspection, emit},
    deadline::{self, OpenEndedPhase},
    obligations::{QualificationState, RunObligations},
    state::RunState,
};
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, budget::RunAccounting, review, reviewer_turn,
};

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "review consumes and binds one exact checked-candidate boundary"
)]
pub(super) async fn complete(
    input: &ProductRunInput,
    observe: &RunObserver,
    state: &mut RunState,
    accounting: &mut RunAccounting,
    recorder: &CandidateRecorder,
    obligations: &RunObligations,
    checked: GateInspection,
) -> Result<CycleInspection, ProductRunnerError> {
    let cycle = state.coordinator.completed_fixer_cycles() + 1;
    emit(
        observe,
        ProductRunPhase::Reviewing,
        cycle,
        "Reviewer is checking the diff and gates; recoverable attempts retry automatically",
        &checked.evidence,
        &state.task_summary,
        Some(&state.findings),
        accounting,
    )?;
    if checked.conversation_changed || input.conversation.revision() != state.conversation_revision
    {
        return Ok(CycleInspection::conversation_changed(checked));
    }
    if checked.gates_satisfied {
        recorder.record_pending_review(state.conversation_revision)?;
    }
    let candidate = recorder.checkpoint()?.ok_or_else(|| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InternalInvariant,
            "bind independent review to candidate",
            "review phase has no exact candidate checkpoint",
        )
    })?;
    check_cancelled(input)?;
    deadline::require_phase_window(
        input.max_elapsed,
        accounting.remaining(),
        OpenEndedPhase::Reviewer,
    )?;
    let page = state.findings.review_page().map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InternalInvariant,
            "select indexed D2 review coverage",
            error.to_string(),
        )
    })?;
    let review_cycle = page.review_cycle();
    let reconciliation = review::render_reconciliation_page(&page);
    let conversation = input.conversation.render();
    let Some(submission) = reviewer_turn::complete(
        input,
        cycle,
        review_cycle,
        reviewer_turn::ReviewEvidence {
            candidate: *candidate.identity(),
            conversation_revision: state.conversation_revision,
            conversation: &conversation,
            diff: &checked.evidence.diff,
            gates: &checked.evidence.gates,
            developer_commands: &checked.evidence.developer_commands,
            prior: &checked.evidence.review,
            reconciliation: &reconciliation,
            coverage_cursor: page.coverage_cursor(),
        },
        accounting,
    )
    .await? else {
        return Ok(CycleInspection::conversation_changed(checked));
    };
    check_cancelled(input)?;
    if input.conversation.revision() != state.conversation_revision {
        return Ok(CycleInspection::conversation_changed(checked));
    }
    let (_, candidate_current) = recorder.record_gates_for_candidate(
        *candidate.identity(),
        state.conversation_revision,
        stage_for_gates(checked.gates_satisfied),
        checked.gates_satisfied,
        checked.gates.execution_context(),
    )?;
    if !candidate_current {
        return Ok(CycleInspection::conversation_changed(checked));
    }
    if submission.coverage_cursor() != Some(page.coverage_cursor()) {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "admit D2 reviewer findings",
            "review coverage cursor is missing or does not match the indexed page",
        ));
    }
    if let Some(publisher) = input.conversation.finding_body_publisher() {
        state.findings.admit_review_page_published(
            review_cycle,
            &page,
            submission,
            publisher,
        )
        .map_err(|error| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::Repository,
                "publish and admit D2 reviewer findings",
                error.to_string(),
            )
        })?;
    } else {
        state.findings.admit_review_page(review_cycle, &page, submission).map_err(|error| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidModelOutput,
                "admit D2 reviewer findings",
                error.to_string(),
            )
        })?;
    }
    let mut evidence = checked.evidence;
    evidence.review = review::render(&state.findings);
    let review_satisfied = !state.findings.has_blockers();
    let obligations_qualified = if let Some(checkpoint) = recorder.checkpoint()? {
        let qualification = obligations.qualify(
            checkpoint.identity(),
            checked.gates_satisfied,
            review_satisfied,
            &format!("{}\n{}", evidence.gates, evidence.review),
        )?;
        RunObligations::append_report(&mut evidence.gates, &qualification);
        let _ = recorder.record(
            stage_for_gates(checked.gates_satisfied),
            state.conversation_revision,
            CheckpointEvidence::Obligations(qualification.qualified()),
        )?;
        let terminal_stage =
            if checked.gates_satisfied && qualification.qualified() && review_satisfied {
                CandidateStage::Qualified
            } else if checked.gates_satisfied {
                CandidateStage::ReviewPending
            } else {
                CandidateStage::SelfChecked
            };
        let _ = recorder.record(
            terminal_stage,
            state.conversation_revision,
            CheckpointEvidence::Review(review_satisfied),
        )?;
        qualification.qualified()
    } else {
        false
    };
    emit(
        observe,
        ProductRunPhase::Reviewing,
        cycle,
        "Fresh typed review completed",
        &evidence,
        &state.task_summary,
        Some(&state.findings),
        accounting,
    )?;
    Ok(CycleInspection {
        gates: checked.gates,
        evidence,
        conversation_changed: false,
        qualification: QualificationState::new(
            checked.gates_satisfied,
            obligations_qualified,
            review_satisfied,
        ),
    })
}

const fn stage_for_gates(passed: bool) -> CandidateStage {
    if passed { CandidateStage::GatesPassed } else { CandidateStage::SelfChecked }
}
