//! Writer-reviewer-fixer run orchestration.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{ProductRunnerError, ProductRunnerErrorKind, bundle, gates, provider};
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::RunId;

const MAX_FIX_CYCLES: u32 = 2;

/// Concrete product-run phase emitted to the daemon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductRunPhase {
    /// Writer model and edit-plan application.
    Writing,
    /// Initial repository checks.
    Checking,
    /// Independent review.
    Reviewing,
    /// Fixer model and edit-plan application.
    Fixing,
    /// Final repository checks.
    Verifying,
    /// Passing terminal state.
    Complete,
}

/// One progress observation emitted at a completed effect boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunUpdate {
    /// Current phase.
    pub phase: ProductRunPhase,
    /// One-based implementation cycle.
    pub cycle: u32,
    /// Current operation in user language.
    pub status: String,
    /// Current bounded diff.
    pub diff: String,
    /// Latest gate output.
    pub gates: String,
    /// Latest review output.
    pub review: String,
    /// Interim summary.
    pub summary: String,
}

/// Observer invoked synchronously after each durable daemon-visible boundary.
pub type RunObserver = Arc<dyn Fn(ProductRunUpdate) + Send + Sync>;

/// Live daemon-owned conversation supplied to every model turn.
pub trait ConversationView: Send + Sync {
    /// Monotonic revision incremented whenever the user adds context.
    fn revision(&self) -> u64;
    /// Human-readable chronological transcript for the next model turn.
    fn render(&self) -> String;
}

/// Explicit writer, reviewer, and fixer provider instances.
pub struct RoleProviders {
    /// Writer model adapter.
    pub writer: Arc<dyn ModelProvider>,
    /// Independent reviewer adapter.
    pub reviewer: Arc<dyn ModelProvider>,
    /// Fixer model adapter.
    pub fixer: Arc<dyn ModelProvider>,
}

/// Fully resolved input supplied by the daemon authority boundary.
pub struct ProductRunInput {
    /// Stable run identity.
    pub run_id: RunId,
    /// Canonical managed-worktree root.
    pub workspace_root: PathBuf,
    /// Natural-language coding task.
    pub task: String,
    /// Live conversation, including the original task and all follow-ups.
    pub conversation: Arc<dyn ConversationView>,
    /// Role provider adapters.
    pub providers: RoleProviders,
    /// Shared cancellation state.
    pub cancelled: Arc<AtomicBool>,
    /// Provider cancellation token.
    pub provider_cancellation: CancellationToken,
}

/// Successful terminal result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunOutput {
    /// Writer/fixer summary.
    pub summary: String,
    /// Final bounded diff.
    pub diff: String,
    /// Final passing gate output.
    pub gates: String,
    /// Final review.
    pub review: String,
    /// Total files changed across plans.
    pub changed_files: usize,
    /// Number of fixer cycles used.
    pub fixer_cycles: u32,
    /// Conversation revision incorporated by the accepted implementation.
    pub conversation_revision: u64,
}

/// A completed run or a material question that needs a user reply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductRunOutcome {
    /// Passing implementation and review evidence.
    Complete(ProductRunOutput),
    /// The writer cannot proceed without one material user choice.
    WaitingForUser {
        /// Direct question to present in the run conversation.
        question: String,
        /// Conversation revision on which the question was based.
        conversation_revision: u64,
    },
}

/// Stateless product-run coordinator.
pub struct ProductRunner;

impl ProductRunner {
    /// Executes a complete writer-reviewer-fixer loop.
    ///
    /// # Errors
    ///
    /// Returns a typed failure for provider, model-contract, repository, gate, apply, or
    /// cancellation failures. Test failures are offered to the fixer before becoming terminal.
    pub async fn run(
        input: ProductRunInput,
        observe: RunObserver,
    ) -> Result<ProductRunOutcome, ProductRunnerError> {
        let applied = match initial_write(&input, &observe).await? {
            AppliedTurn::Applied(applied) => applied,
            AppliedTurn::Waiting { question, conversation_revision } => {
                return Ok(ProductRunOutcome::WaitingForUser { question, conversation_revision });
            }
        };
        let mut state = RunState {
            summary: applied.summary,
            changed_files: applied.changed_files,
            fixer_cycles: 0,
            conversation_revision: applied.conversation_revision,
        };

        loop {
            if input.conversation.revision() != state.conversation_revision {
                match initial_write(&input, &observe).await? {
                    AppliedTurn::Applied(applied) => {
                        state.summary = applied.summary;
                        state.changed_files =
                            state.changed_files.saturating_add(applied.changed_files);
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
            let inspected = inspect_cycle(&input, &observe, &state).await?;
            if inspected.conversation_changed {
                continue;
            }
            if inspected.passed {
                return Ok(ProductRunOutcome::Complete(ProductRunOutput {
                    summary: state.summary,
                    diff: inspected.evidence.diff,
                    gates: inspected.evidence.gates,
                    review: inspected.evidence.review,
                    changed_files: state.changed_files,
                    fixer_cycles: state.fixer_cycles,
                    conversation_revision: state.conversation_revision,
                }));
            }
            if state.fixer_cycles >= MAX_FIX_CYCLES {
                return Err(ProductRunnerError::new(
                    ProductRunnerErrorKind::Gate,
                    "verify coding run",
                    "checks or blocking review findings remain after two fixer cycles",
                ));
            }
            if let Some(waiting) = apply_fix(&input, &observe, &inspected, &mut state).await? {
                return Ok(waiting);
            }
        }
    }
}

struct RunState {
    summary: String,
    changed_files: usize,
    fixer_cycles: u32,
    conversation_revision: u64,
}

#[derive(Default)]
struct RunEvidence {
    diff: String,
    gates: String,
    review: String,
}

struct CycleInspection {
    passed: bool,
    evidence: RunEvidence,
    conversation_changed: bool,
}

pub struct AppliedWrite {
    pub summary: String,
    pub changed_files: usize,
    pub conversation_revision: u64,
}

pub enum AppliedTurn {
    Applied(AppliedWrite),
    Waiting { question: String, conversation_revision: u64 },
}

async fn initial_write(
    input: &ProductRunInput,
    observe: &RunObserver,
) -> Result<AppliedTurn, ProductRunnerError> {
    check_cancelled(input)?;
    emit(
        observe,
        ProductRunPhase::Writing,
        1,
        "Writer is preparing the implementation",
        &RunEvidence::default(),
        "",
    );
    crate::turn::complete_plan(input, input.providers.writer.as_ref(), "writer", 1, None).await
}

async fn inspect_cycle(
    input: &ProductRunInput,
    observe: &RunObserver,
    state: &RunState,
) -> Result<CycleInspection, ProductRunnerError> {
    let phase = if state.fixer_cycles == 0 {
        ProductRunPhase::Checking
    } else {
        ProductRunPhase::Verifying
    };
    let cycle = state.fixer_cycles + 1;
    emit(
        observe,
        phase,
        cycle,
        "Running repository checks",
        &RunEvidence::default(),
        &state.summary,
    );
    check_cancelled(input)?;
    let gate_report = gates::run(&input.workspace_root)?;
    let mut evidence = RunEvidence {
        diff: bundle::diff(&input.workspace_root)?,
        gates: gate_report.output,
        review: String::new(),
    };
    emit(
        observe,
        ProductRunPhase::Reviewing,
        cycle,
        "Reviewer is inspecting the diff and checks",
        &evidence,
        &state.summary,
    );
    if input.conversation.revision() != state.conversation_revision {
        return Ok(CycleInspection { passed: false, evidence, conversation_changed: true });
    }
    let conversation = input.conversation.render();
    evidence.review = provider::complete(
        input.providers.reviewer.as_ref(),
        crate::turn::request_name(input.run_id, "reviewer", cycle),
        crate::turn::reviewer_system(),
        crate::turn::reviewer_user(&conversation, &evidence.diff, &evidence.gates),
        input.provider_cancellation.clone(),
    )
    .await?;
    if input.conversation.revision() != state.conversation_revision {
        return Ok(CycleInspection { passed: false, evidence, conversation_changed: true });
    }
    let review = crate::review::parse(&evidence.review)?;
    emit(observe, ProductRunPhase::Reviewing, cycle, "Review completed", &evidence, &state.summary);
    Ok(CycleInspection {
        passed: gate_report.passed && !review.blocking,
        evidence,
        conversation_changed: false,
    })
}

async fn apply_fix(
    input: &ProductRunInput,
    observe: &RunObserver,
    inspected: &CycleInspection,
    state: &mut RunState,
) -> Result<Option<ProductRunOutcome>, ProductRunnerError> {
    state.fixer_cycles += 1;
    emit(
        observe,
        ProductRunPhase::Fixing,
        state.fixer_cycles + 1,
        "Fixer is addressing checks and review findings",
        &inspected.evidence,
        &state.summary,
    );
    check_cancelled(input)?;
    let findings = format!(
        "Current diff:\n{}\n\nChecks:\n{}\n\nReview:\n{}",
        inspected.evidence.diff, inspected.evidence.gates, inspected.evidence.review
    );
    let turn = crate::turn::complete_plan(
        input,
        input.providers.fixer.as_ref(),
        "fixer",
        state.fixer_cycles,
        Some(&findings),
    )
    .await?;
    match turn {
        AppliedTurn::Applied(applied) => {
            state.changed_files = state.changed_files.saturating_add(applied.changed_files);
            state.summary = applied.summary;
            state.conversation_revision = applied.conversation_revision;
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
) {
    observer(ProductRunUpdate {
        phase,
        cycle,
        status: status.to_owned(),
        diff: evidence.diff.clone(),
        gates: evidence.gates.clone(),
        review: evidence.review.clone(),
        summary: summary.to_owned(),
    });
}
