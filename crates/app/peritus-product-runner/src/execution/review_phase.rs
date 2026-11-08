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
    if checked.conversation_changed || input.requirements_revision() != state.conversation_revision
    {
        return Ok(CycleInspection::conversation_changed(checked));
    }
    if checked.gates_satisfied {
        recorder.record_pending_review(state.conversation_revision)?;
    }
    check_cancelled(input)?;
    deadline::require_phase_window(
        input.max_elapsed,
        accounting.remaining(),
        OpenEndedPhase::Reviewer,
    )?;
    let review_cycle = state.findings.cycle().checked_add(1).ok_or_else(|| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "start D2 reviewer",
            "review cycle counter is exhausted; no fresh review can be requested",
        )
    })?;
    let conversation = input.conversation.render();
    let mut delivery = crate::delivery::Delivery::load(input, &state.design)?;
    let delivery_candidate = crate::delivery::candidate(input)?;
    let submission = reviewer_turn::complete(
        input,
        cycle,
        review_cycle,
        reviewer_turn::ReviewEvidence {
            conversation: &conversation,
            design: &state.design,
            diff: &checked.evidence.diff,
            gates: &checked.evidence.gates,
            developer_commands: &checked.evidence.developer_commands,
            prior: &checked.evidence.review,
            delivery: Some(reviewer_turn::DeliveryReviewContext {
                ledger: &delivery,
                scope_only: false,
                candidate: &delivery_candidate,
                commands: &state.successful_commands,
            }),
        },
        accounting,
    )
    .await?;
    if input.requirements_revision() != state.conversation_revision {
        return Ok(CycleInspection::conversation_changed(checked));
    }
    let capability_review = submission
        .delivery
        .ok_or_else(|| crate::delivery::invalid("candidate capability review is missing"))?;
    let capability_qualified =
        delivery.qualify(capability_review, &delivery_candidate, &state.successful_commands)?;
    state.findings.admit_review(review_cycle, submission.submission).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "admit D2 reviewer findings",
            error.to_string(),
        )
    })?;
    let mut evidence = checked.evidence;
    evidence.review = review::render(&state.findings) + &delivery.prompt()?;
    let review_satisfied =
        !state.findings.open_findings().any(peritus_review::ProductFinding::blocking);
    let obligations_qualified = if let Some(checkpoint) = recorder.checkpoint()? {
        let qualification = obligations.qualify(
            checkpoint.identity(),
            checked.gates_satisfied,
            review_satisfied,
            &format!("{}\n{}", evidence.gates, evidence.review),
            &delivery,
            &delivery_candidate,
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
            obligations_qualified && capability_qualified,
            review_satisfied,
        ),
    })
}

const fn stage_for_gates(passed: bool) -> CandidateStage {
    if passed { CandidateStage::GatesPassed } else { CandidateStage::SelfChecked }
}
