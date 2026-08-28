//! One inspect-review-fix cycle of a product run.

use peritus_review::ProductFindingLedger;

use super::{
    AppliedTurn, ProductRunInput, ProductRunOutcome, ProductRunPhase, ProductRunUpdate,
    RunObserver, RunState, check_cancelled,
};
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, bundle, candidate::CandidateBaseline, design,
    developer_tools::WorkspaceOwnership, gates, provider, review,
};

#[derive(Default)]
pub(super) struct RunEvidence {
    pub(super) diff: String,
    pub(super) gates: String,
    pub(super) review: String,
}

pub(super) struct CycleInspection {
    pub(super) gates: gates::GateReport,
    pub(super) evidence: RunEvidence,
    pub(super) conversation_changed: bool,
}

const MAX_INVALID_REVIEWS: u8 = 3;

pub(super) async fn initial_write(
    input: &ProductRunInput,
    observe: &RunObserver,
    design: &str,
    findings: Option<&str>,
    ownership: &mut WorkspaceOwnership,
) -> Result<AppliedTurn, ProductRunnerError> {
    check_cancelled(input)?;
    emit(
        observe,
        ProductRunPhase::Writing,
        1,
        "Writer is inspecting and implementing; recoverable provider attempts retry automatically",
        &RunEvidence::default(),
        "",
        None,
    )?;
    crate::turn::complete_developer_turn(
        input,
        input.providers.writer.as_ref(),
        "writer",
        1,
        design,
        findings,
        ownership,
    )
    .await
}

pub(super) async fn create_design(
    input: &ProductRunInput,
    observe: &RunObserver,
    cycle: u32,
) -> Result<design::DesignDocument, ProductRunnerError> {
    check_cancelled(input)?;
    emit(
        observe,
        ProductRunPhase::Designing,
        cycle,
        "Inspecting the repository and writing a design; recoverable attempts retry automatically",
        &RunEvidence::default(),
        "The writer will implement against this design after it is published.",
        None,
    )?;
    let document = design::create(input, input.providers.writer.as_ref(), cycle).await?;
    let status = format!("Detailed design ready at {}", document.path().display());
    let summary = format!(
        "Design covers conversation revision {} and is ready for implementation.",
        document.conversation_revision()
    );
    emit(
        observe,
        ProductRunPhase::Designing,
        cycle,
        &status,
        &RunEvidence::default(),
        &summary,
        None,
    )?;
    Ok(document)
}

pub(super) async fn inspect_cycle(
    input: &ProductRunInput,
    observe: &RunObserver,
    baseline: &CandidateBaseline,
    state: &mut RunState,
) -> Result<CycleInspection, ProductRunnerError> {
    let phase = if state.coordinator.completed_fixer_cycles() == 0 {
        ProductRunPhase::Checking
    } else {
        ProductRunPhase::Verifying
    };
    let cycle = state.coordinator.completed_fixer_cycles() + 1;
    emit(
        observe,
        phase,
        cycle,
        "Running checks for every exact changed target",
        &RunEvidence::default(),
        &state.task_summary,
        Some(&state.findings),
    )?;
    check_cancelled(input)?;
    let changed_paths = baseline.changed_paths(&input.workspace_root)?;
    let gate_report = gates::run(&input.workspace_root, changed_paths)?;
    let mut evidence = RunEvidence {
        diff: bundle::diff(&input.workspace_root)?,
        gates: gate_report.output.clone(),
        review: review::render(&state.findings),
    };
    emit(
        observe,
        ProductRunPhase::Reviewing,
        cycle,
        "Reviewer is checking the diff and gates; recoverable attempts retry automatically",
        &evidence,
        &state.task_summary,
        Some(&state.findings),
    )?;
    if input.conversation.revision() != state.conversation_revision {
        return Ok(CycleInspection { gates: gate_report, evidence, conversation_changed: true });
    }
    let conversation = input.conversation.render();
    let mut output_attempt = 0_u8;
    let submission = loop {
        output_attempt = output_attempt.saturating_add(1);
        let request = crate::turn::request_name(input.run_id, "reviewer", cycle);
        let raw_review = provider::complete(
            input.providers.reviewer.as_ref(),
            format!("{request}-output-{output_attempt}"),
            crate::turn::reviewer_system(),
            crate::turn::reviewer_user(
                &conversation,
                &evidence.diff,
                &evidence.gates,
                &evidence.review,
            ),
            input.provider_cancellation.clone(),
        )
        .await?;
        let review_cycle = state.findings.cycle().saturating_add(1);
        match review::parse(&raw_review, review_cycle) {
            Ok(submission) => break submission,
            Err(_) if output_attempt < MAX_INVALID_REVIEWS => {}
            Err(error) => return Err(error),
        }
    };
    if input.conversation.revision() != state.conversation_revision {
        return Ok(CycleInspection { gates: gate_report, evidence, conversation_changed: true });
    }
    let review_cycle = state.findings.cycle().saturating_add(1);
    state.findings.admit_review(review_cycle, submission).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "admit D2 reviewer findings",
            error.to_string(),
        )
    })?;
    evidence.review = review::render(&state.findings);
    emit(
        observe,
        ProductRunPhase::Reviewing,
        cycle,
        "Fresh typed review completed",
        &evidence,
        &state.task_summary,
        Some(&state.findings),
    )?;
    Ok(CycleInspection { gates: gate_report, evidence, conversation_changed: false })
}

pub(super) async fn apply_fix(
    input: &ProductRunInput,
    observe: &RunObserver,
    inspected: &CycleInspection,
    state: &mut RunState,
    ownership: &mut WorkspaceOwnership,
) -> Result<Option<ProductRunOutcome>, ProductRunnerError> {
    let fixer_cycle = state.coordinator.completed_fixer_cycles() + 1;
    emit(
        observe,
        ProductRunPhase::Fixing,
        fixer_cycle + 1,
        "Fixer is addressing exact check failures and every conserved blocker",
        &inspected.evidence,
        &state.task_summary,
        Some(&state.findings),
    )?;
    check_cancelled(input)?;
    let findings = format!(
        "Current diff:\n{}\n\nExact-target checks:\n{}\n\nConserved review ledger:\n{}",
        inspected.evidence.diff, inspected.evidence.gates, inspected.evidence.review,
    );
    let turn = crate::turn::complete_developer_turn(
        input,
        input.providers.fixer.as_ref(),
        "fixer",
        fixer_cycle,
        state.design.markdown(),
        Some(&findings),
        ownership,
    )
    .await?;
    match turn {
        AppliedTurn::Applied(applied) => {
            state.findings.record_fixer_proposal(fixer_cycle);
            state.coordinator.record_fixer_completed();
            state.fix_summaries.push(applied.summary);
            state.run_instructions = applied.run_instructions;
            state.tool_calls = state.tool_calls.saturating_add(applied.tool_calls);
            state.conversation_revision = applied.conversation_revision;
            emit(
                observe,
                ProductRunPhase::Fixing,
                fixer_cycle + 1,
                "Fixer completed; fresh verification is pending",
                &inspected.evidence,
                &state.task_summary,
                Some(&state.findings),
            )?;
            Ok(None)
        }
        AppliedTurn::Waiting { question, conversation_revision } => {
            Ok(Some(ProductRunOutcome::WaitingForUser { question, conversation_revision }))
        }
    }
}

fn emit(
    observer: &RunObserver,
    phase: ProductRunPhase,
    cycle: u32,
    status: &str,
    evidence: &RunEvidence,
    summary: &str,
    findings: Option<&ProductFindingLedger>,
) -> Result<(), ProductRunnerError> {
    let finding_state = findings.map(review::encode_ledger).transpose()?.unwrap_or_default();
    observer(ProductRunUpdate {
        phase,
        cycle,
        status: status.to_owned(),
        diff: evidence.diff.clone(),
        gates: evidence.gates.clone(),
        review: evidence.review.clone(),
        summary: summary.to_owned(),
        finding_state,
    });
    Ok(())
}
