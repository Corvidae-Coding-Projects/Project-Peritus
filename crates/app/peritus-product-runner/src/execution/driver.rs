//! Attempt deadline ownership, accounting, and terminal settlement.

use super::{
    ActiveExit, ExecutionContext, ProductRunInput, ProductRunOutcome, ProductRunPhase,
    ProductRunUpdate, ProductRunner, ProductRunnerError, RunAccounting, RunObserver,
    review, settlement,
};

enum ActiveExecutionResult {
    Returned(Result<ActiveExit, ProductRunnerError>),
    Deadline,
}

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
        if let Err(error) = crate::budget::validate_run_horizon(input.max_elapsed) {
            return settlement::from_initial_error(&input, &error);
        }
        let accounting = match input.accounting() {
            Ok(accounting) => accounting,
            Err(error) => return settlement::from_initial_error(&input, &error),
        };
        Box::pin(Self::run_accounted(input, observe, accounting)).await
    }

    #[allow(clippy::too_many_lines, reason = "the E0 effect and decision order remains explicit")]
    pub(super) async fn run_accounted(
        input: ProductRunInput,
        observe: RunObserver,
        mut accounting: RunAccounting,
    ) -> Result<ProductRunOutcome, ProductRunnerError> {
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
        let provider_cancellation = input.provider_cancellation.clone();
        // The caller-selected deadline owns active execution only. Terminal projection and
        // settlement below are a separate owner state, so they never need a synthetic reserve.
        let active = if let Some(max_elapsed) = accounting.remaining() {
            match tokio::time::timeout(
                max_elapsed,
                // Keep the long-lived role loop out of every caller's async state while retaining
                // timeout ownership: dropping the timeout still drops the active execution future.
                Box::pin(Self::run_until_terminal(
                    &input,
                    &observe,
                    &mut execution,
                    &mut accounting,
                )),
            )
            .await
            {
                Ok(result) => ActiveExecutionResult::Returned(result),
                Err(_) => {
                    let _ = provider_cancellation.cancel();
                    ActiveExecutionResult::Deadline
                }
            }
        } else {
            ActiveExecutionResult::Returned(Box::pin(Self::run_until_terminal(
                &input,
                &observe,
                &mut execution,
                &mut accounting,
            ))
            .await)
        };
        let mut terminal = match active {
            ActiveExecutionResult::Returned(Ok(exit)) => exit,
            ActiveExecutionResult::Returned(Err(error)) => {
                ActiveExit::from_error(&error, execution.next_phase)
            }
            ActiveExecutionResult::Deadline => ActiveExit::deadline(execution.next_phase),
        };
        // From this point forward the runner owns finalization independently of the active
        // deadline. The returned settlement lets the host persist any remaining goal/reply work
        // as a durable completion obligation.
        match execution.refresh_obligation_contract(&input) {
            Ok(true) => {
                terminal.next_phase = execution.next_phase;
                terminal.retain_finalization_failure(&ProductRunnerError::new(
                    crate::ProductRunnerErrorKind::InvalidPrecondition,
                    "adopt governing input before finalization",
                    "the governing source root changed after the active phase; continue this retained run against the pending input",
                ));
            }
            Ok(false) => {}
            Err(error) => terminal.retain_finalization_failure(&error),
        }
        let (checkpoint, checkpoint_failure) = execution.recorder.checkpoint_for_handoff();
        if let Some(error) = checkpoint_failure {
            terminal.retain_finalization_failure(&error);
        }
        let finding_state = execution
            .state
            .as_ref()
            .map(|state| review::encode_ledger(&state.findings))
            .transpose();
        // A failed progress projection cannot prevent the authoritative terminal handoff. Do
        // not publish an empty or predecessor ledger as if it described the current findings.
        if let Ok(finding_state) = &finding_state {
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
                finding_state: finding_state
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| input.finding_state.clone()),
                progress: accounting.latest_snapshot(),
                checkpoint,
                remaining_work: Vec::new(),
            });
        }
        if let Err(error) = finding_state {
            terminal.retain_finalization_failure(&error);
        }
        settlement::finalize_current(
            settlement::FinalizationInput {
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
            },
            &execution.obligations,
        )
    }
}
