//! One inspect-review-fix cycle of a product run.

use peritus_review::ProductFindingLedger;

use super::{
    AppliedTurn, ProductRunInput, ProductRunOutcome, ProductRunPhase, ProductRunUpdate,
    RunObserver, RunState, check_cancelled,
};
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, budget::RunAccounting, bundle,
    candidate::CandidateBaseline, design, developer_tools::WorkspaceOwnership, gates, review,
    reviewer_turn,
};

#[derive(Default)]
pub(super) struct RunEvidence {
    pub(super) diff: String,
    pub(super) gates: String,
    pub(super) review: String,
    pub(super) developer_commands: String,
}

pub(super) struct CycleInspection {
    pub(super) gates: gates::GateReport,
    pub(super) evidence: RunEvidence,
    pub(super) conversation_changed: bool,
}

pub(super) async fn initial_write(
    input: &ProductRunInput,
    observe: &RunObserver,
    design: &str,
    findings: Option<&str>,
    ownership: &mut WorkspaceOwnership,
    accounting: &mut RunAccounting,
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
        accounting,
    )?;
    crate::turn::complete_developer_turn(
        input,
        &input.providers.writer,
        "writer",
        1,
        design,
        findings,
        ownership,
        accounting,
    )
    .await
}

pub(super) async fn create_design(
    input: &ProductRunInput,
    observe: &RunObserver,
    cycle: u32,
    accounting: &mut RunAccounting,
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
        accounting,
    )?;
    let document = design::create(
        input,
        &input.providers.writer,
        &input.providers.fallbacks,
        cycle,
        accounting,
    )
    .await?;
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
        accounting,
    )?;
    Ok(document)
}

pub(super) async fn inspect_cycle(
    input: &ProductRunInput,
    observe: &RunObserver,
    baseline: &CandidateBaseline,
    state: &mut RunState,
    ownership: &WorkspaceOwnership,
    accounting: &mut RunAccounting,
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
        accounting,
    )?;
    check_cancelled(input)?;
    let changed_paths = baseline.changed_paths(&input.workspace_root)?;
    let gate_report = gates::run_with_ownership(
        &input.workspace_root,
        changed_paths,
        ownership,
        input.delivery_scope,
    )?;
    let mut gate_output = gate_report.output.clone();
    if input.delivery_scope.allows_external_effects()
        && gate_report.report.changed_paths().is_empty()
    {
        super::acceptance::ExternalEffectEvidence::from_commands(&state.successful_commands)
            .append_report(&mut gate_output);
    }
    let mut evidence = RunEvidence {
        diff: bundle::diff(&input.workspace_root)?,
        gates: gate_output,
        review: review::render(&state.findings),
        developer_commands: state.developer_evidence.clone(),
    };
    emit(
        observe,
        ProductRunPhase::Reviewing,
        cycle,
        "Reviewer is checking the diff and gates; recoverable attempts retry automatically",
        &evidence,
        &state.task_summary,
        Some(&state.findings),
        accounting,
    )?;
    if input.conversation.revision() != state.conversation_revision {
        return Ok(CycleInspection { gates: gate_report, evidence, conversation_changed: true });
    }
    let conversation = input.conversation.render();
    let review_cycle = state.findings.cycle().saturating_add(1);
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
    if input.conversation.revision() != state.conversation_revision {
        return Ok(CycleInspection { gates: gate_report, evidence, conversation_changed: true });
    }
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
        accounting,
    )?;
    Ok(CycleInspection { gates: gate_report, evidence, conversation_changed: false })
}

pub(super) async fn apply_fix(
    input: &ProductRunInput,
    observe: &RunObserver,
    inspected: &CycleInspection,
    state: &mut RunState,
    ownership: &mut WorkspaceOwnership,
    accounting: &mut RunAccounting,
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
        accounting,
    )?;
    check_cancelled(input)?;
    let findings = format!(
        "Current diff:\n{}\n\nExact-target checks:\n{}\n\nConserved review ledger:\n{}",
        inspected.evidence.diff, inspected.evidence.gates, inspected.evidence.review,
    );
    let turn = crate::turn::complete_developer_turn(
        input,
        &input.providers.fixer,
        "fixer",
        fixer_cycle,
        state.design.markdown(),
        Some(&findings),
        ownership,
        accounting,
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
            crate::developer_tools::merge_rendered(
                &mut state.developer_evidence,
                &applied.verification_evidence,
            );
            crate::developer_tools::merge_successful(
                &mut state.successful_commands,
                &applied.successful_commands,
            );
            emit(
                observe,
                ProductRunPhase::Fixing,
                fixer_cycle + 1,
                "Fixer completed; fresh verification is pending",
                &inspected.evidence,
                &state.task_summary,
                Some(&state.findings),
                accounting,
            )?;
            Ok(None)
        }
        AppliedTurn::Waiting { question, conversation_revision } => {
            Ok(Some(ProductRunOutcome::WaitingForUser { question, conversation_revision }))
        }
    }
}

#[allow(clippy::too_many_arguments, reason = "one effect boundary retains its complete projection")]
fn emit(
    observer: &RunObserver,
    phase: ProductRunPhase,
    cycle: u32,
    status: &str,
    evidence: &RunEvidence,
    summary: &str,
    findings: Option<&ProductFindingLedger>,
    accounting: &mut RunAccounting,
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
        progress: accounting.snapshot()?,
    });
    Ok(())
}
