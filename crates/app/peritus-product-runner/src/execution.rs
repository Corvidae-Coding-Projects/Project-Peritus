//! E0 production writer-gates-review-fixer composition.

mod acceptance;
mod cycle;
mod fix_progress;
mod summary;
mod types;

pub use types::{
    ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunOutput,
    ProductRunPhase, ProductRunUpdate, RoleProviders, RunObserver,
};

use std::sync::atomic::Ordering;

use peritus_orchestrator::{ProductionDecision, ProductionRunCoordinator};
use peritus_review::ProductFindingLedger;

use crate::{
    ProductRunnerError, ProductRunnerErrorKind, budget::RunAccounting,
    candidate::CandidateBaseline, design, developer_tools::WorkspaceOwnership, review,
};
use cycle::{apply_fix, create_design, initial_write, inspect_cycle};
use fix_progress::{FixProgress, FixProgressObservation};
use summary::completion_summary;

const MAX_FIX_CYCLES: u32 = 8;

/// Stateless product-run entry point using the D0/D1/D2/E0 production composition.
pub struct ProductRunner;

impl ProductRunner {
    /// Executes a complete writer-reviewer-fixer loop.
    ///
    /// # Errors
    /// Returns a typed failure for provider, model-contract, repository, gate, trace, tool, or
    /// cancellation failures. A candidate is never complete without exact changed-target evidence.
    #[allow(clippy::too_many_lines, reason = "the E0 effect and decision order remains explicit")]
    pub async fn run(
        input: ProductRunInput,
        observe: RunObserver,
    ) -> Result<ProductRunOutcome, ProductRunnerError> {
        let mut accounting = RunAccounting::new();
        accounting.check()?;
        let baseline = CandidateBaseline::capture(&input.workspace_root)?;
        crate::trace::prepare(&input.trace_path)?;
        let mut workspace_ownership = WorkspaceOwnership::capture(&input.workspace_root);
        let design = create_design(&input, &observe, 1, &mut accounting).await?;
        let restored_findings = review::restore_ledger(&input.finding_state)?;
        let prior_findings =
            (restored_findings.cycle() > 0).then(|| review::render(&restored_findings));
        let applied = match initial_write(
            &input,
            &observe,
            design.markdown(),
            prior_findings.as_deref(),
            &mut workspace_ownership,
            &mut accounting,
        )
        .await?
        {
            AppliedTurn::Applied(applied) => applied,
            AppliedTurn::Waiting { question, conversation_revision } => {
                return Ok(ProductRunOutcome::WaitingForUser { question, conversation_revision });
            }
        };
        let mut state = RunState {
            task_summary: applied.summary,
            run_instructions: applied.run_instructions,
            design,
            fix_summaries: Vec::new(),
            tool_calls: applied.tool_calls,
            conversation_revision: applied.conversation_revision,
            findings: restored_findings,
            fix_progress: FixProgress::capture(&input.workspace_root)?,
            coordinator: ProductionRunCoordinator::new(MAX_FIX_CYCLES).map_err(|detail| {
                ProductRunnerError::new(
                    ProductRunnerErrorKind::Gate,
                    "start E0 production coordinator",
                    detail,
                )
            })?,
            developer_evidence: applied.verification_evidence,
            successful_commands: applied.successful_commands,
        };

        loop {
            if input.conversation.revision() != state.conversation_revision {
                state.design = create_design(
                    &input,
                    &observe,
                    state.coordinator.completed_fixer_cycles() + 2,
                    &mut accounting,
                )
                .await?;
                let prior = review::render(&state.findings);
                match crate::turn::complete_developer_turn(
                    &input,
                    &input.providers.writer,
                    "writer-follow-up",
                    state.coordinator.completed_fixer_cycles() + 2,
                    state.design.markdown(),
                    Some(&prior),
                    &mut workspace_ownership,
                    &mut accounting,
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
                    }
                    AppliedTurn::Waiting { question, conversation_revision } => {
                        return Ok(ProductRunOutcome::WaitingForUser {
                            question,
                            conversation_revision,
                        });
                    }
                }
            }
            let inspected = inspect_cycle(
                &input,
                &observe,
                &baseline,
                &mut state,
                &workspace_ownership,
                &mut accounting,
            )
            .await?;
            if inspected.conversation_changed {
                continue;
            }
            if let Some(finding) = state.fix_progress.observe_findings(&state.findings) {
                let location = if finding.location.trim().is_empty() {
                    String::new()
                } else {
                    format!(" at {}", finding.location)
                };
                return Err(ProductRunnerError::new(
                    ProductRunnerErrorKind::Gate,
                    "verify coding run",
                    format!(
                        "blocking review finding remained after two fresh fixer/reviewer cycles: {}{location}; the latest candidate and review evidence were retained for correction or continuation",
                        finding.title,
                    ),
                ));
            }
            match acceptance::decide(
                input.delivery_scope,
                &state.coordinator,
                &inspected.gates,
                &state.findings,
                &state.successful_commands,
            ) {
                ProductionDecision::Accept => {
                    let changed_paths = inspected.gates.report.changed_paths().to_vec();
                    let successful_commands = acceptance::successful_command_lines(
                        input.delivery_scope,
                        &inspected.gates,
                        &state.successful_commands,
                    );
                    let completion = completion_summary(
                        &input.task,
                        &state.task_summary,
                        &state.fix_summaries,
                        &changed_paths,
                        successful_commands.len(),
                        input.delivery_scope,
                    );
                    let summary = format!(
                        "{completion}\n\nDetailed design: {}",
                        state.design.path().display()
                    );
                    return Ok(ProductRunOutcome::Complete(ProductRunOutput {
                        design_path: state.design.path().to_owned(),
                        summary,
                        diff: inspected.evidence.diff,
                        gates: inspected.evidence.gates,
                        review: inspected.evidence.review,
                        changed_paths,
                        successful_commands,
                        run_instructions: state.run_instructions,
                        fixer_cycles: state.coordinator.completed_fixer_cycles(),
                        conversation_revision: state.conversation_revision,
                    }));
                }
                ProductionDecision::Fix => {
                    if let Some(waiting) = apply_fix(
                        &input,
                        &observe,
                        &inspected,
                        &mut state,
                        &mut workspace_ownership,
                        &mut accounting,
                    )
                    .await?
                    {
                        return Ok(waiting);
                    }
                    if state.fix_progress.observe(&input.workspace_root)?
                        == FixProgressObservation::Exhausted
                    {
                        return Err(ProductRunnerError::new(
                            ProductRunnerErrorKind::Gate,
                            "verify coding run",
                            "two consecutive fixer cycles made no candidate change while exact checks or blocking findings remained",
                        ));
                    }
                }
                ProductionDecision::Exhausted => {
                    return Err(ProductRunnerError::new(
                        ProductRunnerErrorKind::Gate,
                        "verify coding run",
                        "exact-target checks or conserved blocking findings remain after the configured fixer cycles",
                    ));
                }
            }
        }
    }
}

struct RunState {
    task_summary: String,
    run_instructions: String,
    design: design::DesignDocument,
    fix_summaries: Vec<String>,
    tool_calls: u32,
    conversation_revision: u64,
    findings: ProductFindingLedger,
    fix_progress: FixProgress,
    coordinator: ProductionRunCoordinator,
    developer_evidence: String,
    successful_commands: Vec<crate::developer_tools::SuccessfulCommand>,
}

pub struct AppliedWrite {
    /// Task-level summary returned by this developer turn.
    pub summary: String,
    /// Concrete command or steps for running the result.
    pub run_instructions: String,
    /// Number of actual developer-tool calls executed.
    pub tool_calls: u32,
    /// Conversation revision incorporated by the turn.
    pub conversation_revision: u64,
    /// Bounded structured command requests and observations from this developer turn.
    pub verification_evidence: String,
    /// Successful, explicitly classified developer commands retained for delivery evidence.
    pub(crate) successful_commands: Vec<crate::developer_tools::SuccessfulCommand>,
}

pub enum AppliedTurn {
    /// The model performed work and returned a terminal summary.
    Applied(AppliedWrite),
    /// The model requires one material answer before continuing.
    Waiting {
        /// Direct question for the user.
        question: String,
        /// Conversation revision on which the question was based.
        conversation_revision: u64,
    },
}

pub fn check_cancelled(input: &ProductRunInput) -> Result<(), ProductRunnerError> {
    if input.cancelled.load(Ordering::Acquire) || input.provider_cancellation.is_cancelled() {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Cancelled,
            "execute coding run",
            "run was cancelled",
        ))
    } else {
        Ok(())
    }
}
