//! Reviewed, explicitly authorized investigations which produce no patch candidate.

use super::{
    ProductRunInput, ProductRunPhase, RunObserver, check_cancelled,
    checkpoint::CandidateRecorder,
    cycle::{RunEvidence, emit},
    deadline::{self, OpenEndedPhase},
    obligations::RunObligations,
    state::RunState,
};
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, budget::RunAccounting,
    candidate::CandidateBaseline, developer_tools::CommandPurpose, review, reviewer_turn,
};

pub(super) fn eligible(
    input: &ProductRunInput,
    baseline: &CandidateBaseline,
    recorder: &CandidateRecorder,
) -> Result<bool, ProductRunnerError> {
    let authority = input.conversation.reference_authority_context();
    Ok(authority.contains("PERITUS HARNESS EVALUATION\n")
        && authority.contains("If it is not, report that result without inventing a patch.")
        && recorder.checkpoint()?.is_none()
        && baseline.changed_paths(&input.workspace_root)?.is_empty())
}

pub(super) enum ReviewResult {
    Complete,
    ConversationChanged,
    FixRequired,
}

#[allow(clippy::too_many_arguments, reason = "review binds the exact no-patch investigation")]
pub(super) async fn review(
    input: &ProductRunInput,
    observe: &RunObserver,
    state: &mut RunState,
    accounting: &mut RunAccounting,
    baseline: &CandidateBaseline,
    obligations: &RunObligations,
    evidence: &mut RunEvidence,
) -> Result<ReviewResult, ProductRunnerError> {
    check_cancelled(input)?;
    if !state
        .successful_commands
        .iter()
        .any(|command| command.purpose == CommandPurpose::Verification)
        || state
            .successful_commands
            .iter()
            .any(|command| command.purpose == CommandPurpose::ExternalEffect)
    {
        return Err(incomplete(
            "A no-patch conclusion requires successful real-interface verification and no external effect.",
        ));
    }
    let source = baseline.content_digest(&input.workspace_root)?;
    let cycle = state.coordinator.completed_fixer_cycles().saturating_add(1);
    let revision = state.conversation_revision;
    evidence.diff.clear();
    evidence.developer_commands.clone_from(&state.developer_evidence);
    evidence.gates = format!(
        "No-patch investigation — no candidate qualification or promotion authority.\nUnchanged source digest: {source:?}\nConversation revision: {revision}\nWriter conclusion: {}\n\n{}\nIndependently verify that the allegation is not reproduced by the actual operations, that the current public request permits no patch, and that no requested work is omitted. Do not demand invented edits or unit-test suites. Block completion if the real receipts do not support this conclusion.",
        state.task_summary,
        obligations.render(),
    );
    evidence.review = review::render(&state.findings);
    emit(
        observe,
        ProductRunPhase::Reviewing,
        cycle,
        "Reviewing the real investigation; no patch candidate was generated",
        evidence,
        &state.task_summary,
        Some(&state.findings),
        accounting,
    )?;
    if input.conversation.revision() != revision {
        return Ok(ReviewResult::ConversationChanged);
    }
    deadline::require_phase_window(
        input.max_elapsed,
        accounting.remaining(),
        OpenEndedPhase::Reviewer,
    )?;
    let review_cycle = state.findings.cycle().saturating_add(1);
    let conversation = input.conversation.render();
    let submission = reviewer_turn::complete(
        input,
        cycle,
        review_cycle,
        reviewer_turn::ReviewEvidence {
            conversation: &conversation,
            diff: &evidence.diff,
            gates: &evidence.gates,
            developer_commands: &evidence.developer_commands,
            prior: &evidence.review,
        },
        accounting,
    )
    .await?;
    check_cancelled(input)?;
    if input.conversation.revision() != revision {
        return Ok(ReviewResult::ConversationChanged);
    }
    state
        .findings
        .admit_review(review_cycle, submission)
        .map_err(|error| incomplete(&error.to_string()))?;
    evidence.review = review::render(&state.findings);
    let clean = baseline.changed_paths(&input.workspace_root)?.is_empty()
        && baseline.content_digest(&input.workspace_root)? == source;
    let satisfied = !state.findings.open_findings().any(peritus_review::ProductFinding::blocking);
    emit(
        observe,
        ProductRunPhase::Reviewing,
        cycle,
        "Independent no-patch review completed",
        evidence,
        &state.task_summary,
        Some(&state.findings),
        accounting,
    )?;
    if !clean {
        return Err(incomplete(
            "The no-patch investigation changed source during independent review; no candidate was accepted.",
        ));
    }
    if !satisfied {
        return Ok(ReviewResult::FixRequired);
    }
    state.task_summary =
        format!("Investigation complete; no patch candidate generated. {}", state.task_summary);
    Ok(ReviewResult::Complete)
}

fn incomplete(detail: &str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "review no-patch investigation",
        detail,
    )
}
