//! E0 production writer-gates-review-fixer composition.

mod acceptance;
mod cancellation;
mod checkpoint;
mod cycle;
mod deadline;
mod fix_progress;
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
    ProductRunPhase, ProductRunQuestion, ProductRunUpdate, RoleProviders, RunObserver,
};

use std::sync::{Arc, atomic::Ordering};

use peritus_obligations::FailureDisposition;
use peritus_orchestrator::ProductionDecision;
use peritus_run_settlement::SettlementCause;

use crate::{
    ProductRunnerError, ProductRunnerErrorKind, budget::RunAccounting,
    developer_tools::WorkspaceOwnership, review,
};
use cycle::{GateInspection, apply_fix, create_design, inspect_gates, retained_inspection};
use fix_progress::FixProgressObservation;
use state::{ExecutionContext, RunState};
use summary::completion_summary;
use terminal_exit::{ActiveExit, fatal};

/// Stateless product-run entry point using the D0/D1/D2/E0 production composition.
pub struct ProductRunner;

impl ProductRunner {
    /// Executes a complete writer-reviewer-fixer loop.
    ///
    /// # Errors
    /// Returns an error only for invalid initial input or an impossible internal invariant. Every
    /// ordinary terminal path is represented by the returned verified settlement.
    #[allow(clippy::too_many_lines, reason = "the E0 effect and decision order remains explicit")]
    pub async fn run(
        input: ProductRunInput,
        observe: RunObserver,
    ) -> Result<ProductRunOutcome, ProductRunnerError> {
        crate::budget::validate_run_horizon(input.max_elapsed)?;
        let mut accounting = match RunAccounting::new(&input.workspace_root, input.max_elapsed) {
            Ok(accounting) => accounting,
            Err(error) => return settlement::from_initial_error(&input, &error),
        };
        if let Err(error) = accounting.check() {
            return settlement::from_initial_error(&input, &error);
        }
        if let Err(error) = crate::trace::prepare(&input.trace_path) {
            return settlement::from_initial_error(&input, &error);
        }
        let mut execution = match ExecutionContext::prepare(&input) {
            Ok(execution) => execution,
            Err(error) => return settlement::from_initial_error(&input, &error),
        };
        let max_elapsed = input.max_elapsed;
        let cancelled = Arc::clone(&input.cancelled);
        let provider_cancellation = input.provider_cancellation.clone();
        let deadline_reached = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let timer_reached = Arc::clone(&deadline_reached);
        let timer_cancelled = Arc::clone(&cancelled);
        let timer_provider_cancellation = provider_cancellation.clone();
        let timer = tokio::spawn(async move {
            tokio::time::sleep(deadline::active_window(max_elapsed)).await;
            timer_reached.store(true, Ordering::SeqCst);
            timer_cancelled.store(true, Ordering::SeqCst);
            let _ = timer_provider_cancellation.cancel();
        });
        let result = tokio::time::timeout(
            max_elapsed,
            Self::run_until_terminal(&input, &observe, &mut execution, &mut accounting),
        )
        .await;
        timer.abort();
        let _ = timer.await;
        let reached = deadline_reached.load(Ordering::SeqCst);
        let terminal = match result {
            Ok(Ok(exit)) => exit.with_deadline(reached),
            Ok(Err(error)) if fatal(&error) => return Err(error),
            Ok(Err(error)) => ActiveExit::from_error(&error, reached, execution.next_phase),
            Err(_) => {
                cancelled.store(true, Ordering::SeqCst);
                let _ = provider_cancellation.cancel();
                ActiveExit::deadline(execution.next_phase)
            }
        };
        let checkpoint = execution.recorder.checkpoint()?;
        observe(ProductRunUpdate {
            phase: ProductRunPhase::Finalizing,
            cycle: execution.completed_cycles().saturating_add(1),
            status: "Refreshing the exact candidate and constructing its terminal handoff"
                .to_owned(),
            diff: execution.evidence.diff.clone(),
            gates: execution.evidence.gates.clone(),
            review: execution.evidence.review.clone(),
            summary: execution
                .state
                .as_ref()
                .map_or_else(String::new, |state| state.task_summary.clone()),
            finding_state: execution
                .state
                .as_ref()
                .map(|state| review::encode_ledger(&state.findings))
                .transpose()?
                .unwrap_or_default(),
            progress: accounting.latest_snapshot(),
            checkpoint,
            remaining_work: Vec::new(),
        });
        settlement::finalize(settlement::FinalizationInput {
            input: &input,
            baseline: &execution.baseline,
            recorder: &execution.recorder,
            design: execution.design.as_ref(),
            state: execution.state.as_ref(),
            diff: &execution.evidence.diff,
            gates: &execution.evidence.gates,
            review: &execution.evidence.review,
            gate_report: execution.gate_report.as_ref(),
            cause: terminal.cause,
            question: terminal.question,
            detail: terminal.detail,
            next_phase: terminal.next_phase,
        })
    }

    #[allow(clippy::too_many_lines, reason = "the E0 effect and decision order remains explicit")]
    async fn run_until_terminal(
        input: &ProductRunInput,
        observe: &RunObserver,
        execution: &mut ExecutionContext,
        accounting: &mut RunAccounting,
    ) -> Result<ActiveExit, ProductRunnerError> {
        let mut workspace_ownership = WorkspaceOwnership::capture(&input.workspace_root);
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
                        state.fix_progress.reset(&input.workspace_root)?;
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
                    if state.fix_progress.observe(&input.workspace_root)?
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
