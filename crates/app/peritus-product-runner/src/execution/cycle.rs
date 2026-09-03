//! One inspect-review-fix cycle of a product run.

use peritus_review::ProductFindingLedger;
use peritus_run_settlement::CandidateStage;

use super::{
    AppliedTurn, ProductRunInput, ProductRunPhase, ProductRunUpdate, RunObserver, RunState,
    check_cancelled,
    checkpoint::{CandidateRecorder, CheckpointEvidence},
    deadline::{self, OpenEndedPhase},
    obligations::{QualificationState, RunObligations},
};
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, budget::RunAccounting, bundle,
    candidate::CandidateBaseline, design, developer_tools::WorkspaceOwnership, gates, review,
};

#[derive(Clone, Default)]
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
    pub(super) qualification: QualificationState,
}

impl CycleInspection {
    pub(super) fn conversation_changed(checked: GateInspection) -> Self {
        Self {
            gates: checked.gates,
            evidence: checked.evidence,
            conversation_changed: true,
            qualification: QualificationState::new(false, false, false),
        }
    }
}

pub(super) fn retained_inspection(
    gate_report: Option<&gates::GateReport>,
    evidence: &RunEvidence,
    recorder: &CandidateRecorder,
) -> Result<CycleInspection, ProductRunnerError> {
    let gates = gate_report.cloned().ok_or_else(|| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InternalInvariant,
            "resume fixer phase",
            "fixer phase has no retained exact-target gate report",
        )
    })?;
    let checkpoint = recorder.checkpoint()?.ok_or_else(|| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InternalInvariant,
            "resume fixer phase",
            "fixer phase has no exact candidate checkpoint",
        )
    })?;
    let identity = checkpoint.identity();
    Ok(CycleInspection {
        gates,
        evidence: evidence.clone(),
        conversation_changed: false,
        qualification: QualificationState::new(
            checkpoint.gates().is_current_and_satisfied(identity),
            checkpoint.obligations().is_current_and_satisfied(identity),
            checkpoint.review().is_current_and_satisfied(identity),
        ),
    })
}

pub(super) struct GateInspection {
    pub(super) gates: gates::GateReport,
    pub(super) gates_satisfied: bool,
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
    recorder: &CandidateRecorder,
) -> Result<AppliedTurn, ProductRunnerError> {
    deadline::require_phase_window(
        input.max_elapsed,
        accounting.remaining(),
        OpenEndedPhase::Writer,
    )?;
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
        recorder,
    )
    .await
}

pub(super) async fn create_design(
    input: &ProductRunInput,
    observe: &RunObserver,
    cycle: u32,
    accounting: &mut RunAccounting,
) -> Result<design::DesignDocument, ProductRunnerError> {
    deadline::require_phase_window(
        input.max_elapsed,
        accounting.remaining(),
        OpenEndedPhase::Design,
    )?;
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

#[allow(clippy::too_many_arguments, reason = "gate execution binds one exact run boundary")]
pub(super) fn inspect_gates(
    input: &ProductRunInput,
    observe: &RunObserver,
    baseline: &CandidateBaseline,
    state: &RunState,
    ownership: &WorkspaceOwnership,
    accounting: &mut RunAccounting,
    recorder: &CandidateRecorder,
    obligations: &RunObligations,
) -> Result<GateInspection, ProductRunnerError> {
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
    let conversation = input.conversation.render();
    let effect_requirement = crate::delivery_requirement::ExternalEffectRequirement::from_task(
        input.delivery_scope,
        &input.task,
    );
    let changed_paths = baseline.changed_paths(&input.workspace_root)?;
    let gate_report = gates::run_with_ownership(
        &input.workspace_root,
        changed_paths,
        ownership,
        input.delivery_scope,
        &conversation,
    )?;
    let mut gate_output = gate_report.output.clone();
    gate_output.push('\n');
    gate_output.push_str(&obligations.render());
    if input.delivery_scope.allows_external_effects()
        && (effect_requirement.is_required() || gate_report.report.changed_paths().is_empty())
    {
        super::acceptance::ExternalEffectEvidence::from_commands(&state.successful_commands)
            .append_report(&mut gate_output, effect_requirement);
    }
    let gates_satisfied = super::acceptance::qualification_ready(
        input.delivery_scope,
        effect_requirement,
        &gate_report,
        &state.successful_commands,
    );
    let evidence = RunEvidence {
        diff: bundle::diff(&input.workspace_root, baseline)?,
        gates: gate_output,
        review: review::render(&state.findings),
        developer_commands: state.developer_evidence.clone(),
    };
    let gate_stage =
        if gates_satisfied { CandidateStage::GatesPassed } else { CandidateStage::SelfChecked };
    let _ = recorder.record(
        gate_stage,
        state.conversation_revision,
        CheckpointEvidence::Gates(gates_satisfied),
    )?;
    Ok(GateInspection {
        gates: gate_report,
        gates_satisfied,
        evidence,
        conversation_changed: input.conversation.revision() != state.conversation_revision,
    })
}

pub(super) async fn apply_fix(
    input: &ProductRunInput,
    observe: &RunObserver,
    inspected: &CycleInspection,
    state: &mut RunState,
    ownership: &mut WorkspaceOwnership,
    accounting: &mut RunAccounting,
    recorder: &CandidateRecorder,
) -> Result<Option<(String, u64)>, ProductRunnerError> {
    deadline::require_phase_window(
        input.max_elapsed,
        accounting.remaining(),
        OpenEndedPhase::Fixer,
    )?;
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
        recorder,
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
            let _ = recorder.record(
                CandidateStage::Changed,
                state.conversation_revision,
                CheckpointEvidence::None,
            )?;
            Ok(None)
        }
        AppliedTurn::Waiting { question, conversation_revision } => {
            Ok(Some((question, conversation_revision)))
        }
    }
}

#[allow(clippy::too_many_arguments, reason = "one effect boundary retains its complete projection")]
pub(super) fn emit(
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
        checkpoint: None,
        remaining_work: Vec::new(),
    });
    Ok(())
}
