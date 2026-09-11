//! E0 production writer-gates-review-fixer composition.

mod acceptance;
mod cancellation;
mod candidate_digest;
mod checkpoint;
mod conversation;
mod cycle;
mod deadline;
mod fix_progress;
mod folder;
mod obligations;
mod resume;
mod review_phase;
mod settlement;
mod state;
mod summary;
mod terminal_exit;
mod turn_result;
mod types;

pub use cancellation::check_cancelled;
pub use checkpoint::CandidateRecorder;
pub use resume::ProductRunResume;
pub use turn_result::{AppliedTurn, AppliedWrite};
pub use types::{
    ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunOutput,
    ProductRunPhase, ProductRunQuestion, ProductRunUpdate, ProductRunner, RoleProviders,
    RunObserver, WorkspaceMutationKind,
};

use peritus_obligations::FailureDisposition;
use peritus_orchestrator::ProductionDecision;
use peritus_run_settlement::SettlementCause;

use crate::{ProductRunnerError, ProductRunnerErrorKind, budget::RunAccounting, review};
use cycle::{GateInspection, apply_fix, create_design, inspect_gates, retained_inspection};
use fix_progress::FixProgressObservation;
use state::{ExecutionContext, RunState};
use summary::completion_summary;
use terminal_exit::ActiveExit;
mod driver;

impl ProductRunner {
    #[allow(clippy::too_many_lines, reason = "the E0 effect and decision order remains explicit")]
    async fn run_until_terminal(
        input: &ProductRunInput,
        observe: &RunObserver,
        execution: &mut ExecutionContext,
        accounting: &mut RunAccounting,
    ) -> Result<ActiveExit, ProductRunnerError> {
        let mut workspace_ownership = input.ownership();
        if let Some((question, revision)) = execution
            .prepare_active_state(input, observe, &mut workspace_ownership, accounting)
            .await?
        {
            return Ok(ActiveExit::waiting(question, revision, ProductRunPhase::Writing));
        }

        loop {
            if execution.next_phase == ProductRunPhase::Finalizing {
                return Ok(ActiveExit::completed());
            }
            let state = execution.state.as_mut().ok_or_else(|| {
                ProductRunnerError::new(
                    ProductRunnerErrorKind::InternalInvariant,
                    "resume product run",
                    "an executable phase has no retained run state",
                )
            })?;
            if input.conversation.revision() != state.conversation_revision {
                execution.next_phase = ProductRunPhase::Designing;
                state.design = create_design(
                    input,
                    observe,
                    state.coordinator.completed_fixer_cycles() + 2,
                    accounting,
                )
                .await?;
                execution.design = Some(state.design.clone());
                execution.next_phase = ProductRunPhase::Writing;
                let prior = review::render(&state.findings);
                match crate::turn::complete_developer_turn(
                    input,
                    &input.providers.writer,
                    "writer-follow-up",
                    state.coordinator.completed_fixer_cycles() + 2,
                    state.design.markdown(),
                    Some(&prior),
                    &mut workspace_ownership,
                    accounting,
                    &execution.recorder,
                )
                .await?
                {
                    AppliedTurn::Applied(applied) => {
                        if state.findings.open_findings().next().is_some() {
                            state.findings.record_fixer_proposal(
                                state.coordinator.completed_fixer_cycles() + 1,
                            );
                        }
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
                        state.fix_progress.reset(input.checkpoint()?);
                        execution.next_phase = ProductRunPhase::Checking;
                    }
                    AppliedTurn::Waiting { question, conversation_revision } => {
                        return Ok(ActiveExit::waiting(
                            question,
                            conversation_revision,
                            ProductRunPhase::Writing,
                        ));
                    }
                }
                continue;
            }
            let inspected = match execution.next_phase {
                ProductRunPhase::Checking | ProductRunPhase::Verifying => {
                    let checked = inspect_gates(
                        input,
                        observe,
                        &execution.baseline,
                        state,
                        &workspace_ownership,
                        accounting,
                        &execution.recorder,
                        &execution.obligations,
                    )?;
                    execution.gate_report = Some(checked.gates.clone());
                    execution.evidence = checked.evidence.clone();
                    if checked.conversation_changed {
                        execution.next_phase = ProductRunPhase::Designing;
                    } else {
                        execution.next_phase = ProductRunPhase::Reviewing;
                    }
                    continue;
                }
                ProductRunPhase::Reviewing => {
                    let checked = GateInspection {
                        gates: execution.gate_report.clone().ok_or_else(|| {
                            ProductRunnerError::new(
                                ProductRunnerErrorKind::InternalInvariant,
                                "resume reviewer phase",
                                "reviewer phase has no retained exact-target gate report",
                            )
                        })?,
                        gates_satisfied: execution.recorder.checkpoint()?.is_some_and(
                            |checkpoint| {
                                checkpoint.gates().is_current_and_satisfied(checkpoint.identity())
                            },
                        ),
                        evidence: execution.evidence.clone(),
                        conversation_changed: false,
                    };
                    let inspected = review_phase::complete(
                        input,
                        observe,
                        state,
                        accounting,
                        &execution.recorder,
                        &execution.obligations,
                        checked,
                    )
                    .await?;
                    execution.evidence = inspected.evidence.clone();
                    if inspected.conversation_changed {
                        execution.next_phase = ProductRunPhase::Designing;
                        continue;
                    }
                    inspected
                }
                ProductRunPhase::Fixing => {
                    let inspected = retained_inspection(
                        execution.gate_report.as_ref(),
                        &execution.evidence,
                        &execution.recorder,
                    )?;
                    if let Some((question, revision)) = apply_fix(
                        input,
                        observe,
                        &inspected,
                        state,
                        &mut workspace_ownership,
                        accounting,
                        &execution.recorder,
                    )
                    .await?
                    {
                        return Ok(ActiveExit::waiting(
                            question,
                            revision,
                            ProductRunPhase::Fixing,
                        ));
                    }
                    execution.next_phase = ProductRunPhase::Verifying;
                    if state.fix_progress.observe(input.checkpoint()?)
                        == FixProgressObservation::Exhausted
                    {
                        return Ok(ActiveExit::stopped(
                            SettlementCause::Gate,
                            "two consecutive fixer cycles made no candidate change while exact checks or blocking findings remained".to_owned(),
                            ProductRunPhase::Fixing,
                        ));
                    }
                    continue;
                }
                ProductRunPhase::Finalizing => return Ok(ActiveExit::completed()),
                ProductRunPhase::Designing
                | ProductRunPhase::Writing
                | ProductRunPhase::Complete => {
                    return Err(ProductRunnerError::new(
                        ProductRunnerErrorKind::InternalInvariant,
                        "advance product run",
                        "active loop entered a phase without its required transition",
                    ));
                }
            };
            if let Some(finding) = state.fix_progress.observe_findings(&state.findings) {
                let location = if finding.location.trim().is_empty() {
                    String::new()
                } else {
                    format!(" at {}", finding.location)
                };
                return Ok(ActiveExit::stopped(
                    SettlementCause::Review,
                    format!(
                        "blocking review finding remained after two fresh fixer/reviewer cycles: {}{location}",
                        finding.title,
                    ),
                    ProductRunPhase::Fixing,
                ));
            }
            let effect_requirement =
                crate::delivery_requirement::ExternalEffectRequirement::from_task(
                    input.delivery_scope,
                    &input.task,
                );
            match acceptance::decide(
                input.delivery_scope,
                effect_requirement,
                &state.coordinator,
                &inspected.gates,
                &state.findings,
                &state.successful_commands,
            ) {
                ProductionDecision::Accept if inspected.qualification.all_satisfied() => {
                    let changed_paths = inspected.gates.report.changed_paths().to_vec();
                    let successful_commands = acceptance::successful_command_lines(
                        input.delivery_scope,
                        effect_requirement,
                        &inspected.gates,
                        &state.successful_commands,
                    );
                    state.task_summary = completion_summary(
                        &input.task,
                        &state.task_summary,
                        &state.fix_summaries,
                        &changed_paths,
                        successful_commands.len(),
                        input.delivery_scope,
                        effect_requirement,
                    );
                    execution.next_phase = ProductRunPhase::Finalizing;
                    return Ok(ActiveExit::completed());
                }
                ProductionDecision::Accept | ProductionDecision::Fix => {
                    if inspected.qualification.fixer_disposition(false)
                        != FailureDisposition::RequestFixer
                    {
                        return Ok(ActiveExit::stopped(
                            SettlementCause::InternalInvariant,
                            "non-candidate failure was refused fixer routing".to_owned(),
                            execution.next_phase,
                        ));
                    }
                    execution.next_phase = ProductRunPhase::Fixing;
                }
                ProductionDecision::Exhausted => {
                    return Ok(ActiveExit::stopped(
                        SettlementCause::Gate,
                        "exact-target checks or conserved blocking findings remain after the configured fixer cycles".to_owned(),
                        ProductRunPhase::Fixing,
                    ));
                }
            }
        }
    }
}
