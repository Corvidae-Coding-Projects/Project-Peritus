//! E0 production writer-gates-review-fixer composition.

mod summary;
mod types;

pub use types::{
    ConversationView, ProductRunInput, ProductRunOutcome, ProductRunOutput, ProductRunPhase,
    ProductRunUpdate, RoleProviders, RunObserver,
};

use std::sync::atomic::Ordering;

use peritus_orchestrator::{ProductionDecision, ProductionRunCoordinator};
use peritus_review::ProductFindingLedger;

use crate::{
    ProductRunnerError, ProductRunnerErrorKind, bundle, candidate::CandidateBaseline, gates,
    provider, review,
};
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
        let baseline = CandidateBaseline::capture(&input.workspace_root)?;
        let restored_findings = review::restore_ledger(&input.finding_state)?;
        let prior_findings =
            (restored_findings.cycle() > 0).then(|| review::render(&restored_findings));
        let applied = match initial_write(&input, &observe, prior_findings.as_deref()).await? {
            AppliedTurn::Applied(applied) => applied,
            AppliedTurn::Waiting { question, conversation_revision } => {
                return Ok(ProductRunOutcome::WaitingForUser { question, conversation_revision });
            }
        };
        let mut state = RunState {
            task_summary: applied.summary,
            run_instructions: applied.run_instructions,
            fix_summaries: Vec::new(),
            tool_calls: applied.tool_calls,
            conversation_revision: applied.conversation_revision,
            findings: restored_findings,
            coordinator: ProductionRunCoordinator::new(MAX_FIX_CYCLES).map_err(|detail| {
                ProductRunnerError::new(
                    ProductRunnerErrorKind::Gate,
                    "start E0 production coordinator",
                    detail,
                )
            })?,
        };

        loop {
            if input.conversation.revision() != state.conversation_revision {
                let prior = review::render(&state.findings);
                match crate::turn::complete_developer_turn(
                    &input,
                    input.providers.writer.as_ref(),
                    "writer-follow-up",
                    state.coordinator.completed_fixer_cycles() + 2,
                    Some(&prior),
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
                    }
                    AppliedTurn::Waiting { question, conversation_revision } => {
                        return Ok(ProductRunOutcome::WaitingForUser {
                            question,
                            conversation_revision,
                        });
                    }
                }
            }
            let inspected = inspect_cycle(&input, &observe, &baseline, &mut state).await?;
            if inspected.conversation_changed {
                continue;
            }
            match state.coordinator.decide(&inspected.gates.report, &state.findings) {
                ProductionDecision::Accept => {
                    let changed_paths = inspected.gates.report.changed_paths().to_vec();
                    let successful_commands = inspected
                        .gates
                        .report
                        .records()
                        .iter()
                        .map(|record| record.command.clone())
                        .collect::<Vec<_>>();
                    let summary = completion_summary(
                        &input.task,
                        &state.task_summary,
                        &state.fix_summaries,
                        &changed_paths,
                        successful_commands.len(),
                    );
                    return Ok(ProductRunOutcome::Complete(ProductRunOutput {
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
                    if let Some(waiting) =
                        apply_fix(&input, &observe, &inspected, &mut state).await?
                    {
                        return Ok(waiting);
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
    fix_summaries: Vec<String>,
    tool_calls: u32,
    conversation_revision: u64,
    findings: ProductFindingLedger,
    coordinator: ProductionRunCoordinator,
}

#[derive(Default)]
struct RunEvidence {
    diff: String,
    gates: String,
    review: String,
}

struct CycleInspection {
    gates: gates::GateReport,
    evidence: RunEvidence,
    conversation_changed: bool,
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

async fn initial_write(
    input: &ProductRunInput,
    observe: &RunObserver,
    findings: Option<&str>,
) -> Result<AppliedTurn, ProductRunnerError> {
    check_cancelled(input)?;
    emit(
        observe,
        ProductRunPhase::Writing,
        1,
        "Writer is inspecting and implementing in the managed workspace",
        &RunEvidence::default(),
        "",
        None,
    )?;
    crate::turn::complete_developer_turn(
        input,
        input.providers.writer.as_ref(),
        "writer",
        1,
        findings,
    )
    .await
}

async fn inspect_cycle(
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
        "Reviewer is checking the diff, target coverage, and conserved findings",
        &evidence,
        &state.task_summary,
        Some(&state.findings),
    )?;
    if input.conversation.revision() != state.conversation_revision {
        return Ok(CycleInspection { gates: gate_report, evidence, conversation_changed: true });
    }
    let conversation = input.conversation.render();
    let raw_review = provider::complete(
        input.providers.reviewer.as_ref(),
        crate::turn::request_name(input.run_id, "reviewer", cycle),
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
    if input.conversation.revision() != state.conversation_revision {
        return Ok(CycleInspection { gates: gate_report, evidence, conversation_changed: true });
    }
    let review_cycle = state.findings.cycle().saturating_add(1);
    let submission = review::parse(&raw_review, review_cycle)?;
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

async fn apply_fix(
    input: &ProductRunInput,
    observe: &RunObserver,
    inspected: &CycleInspection,
    state: &mut RunState,
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
        Some(&findings),
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
