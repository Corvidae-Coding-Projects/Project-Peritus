//! Attempt deadline ownership, accounting, and terminal settlement.

use super::terminal_exit::fatal;
use super::{
    ActiveExit, ExecutionContext, ProductRunInput, ProductRunOutcome, ProductRunPhase,
    ProductRunUpdate, ProductRunner, ProductRunnerError, RunAccounting, RunObserver, deadline,
    review, settlement,
};
use std::sync::{Arc, atomic::Ordering};

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
        let max_elapsed = accounting.remaining();
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
            // Keep the long-lived role loop out of every caller's async state while retaining
            // timeout ownership: dropping the timeout still drops the active execution future.
            Box::pin(Self::run_until_terminal(&input, &observe, &mut execution, &mut accounting)),
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
}
