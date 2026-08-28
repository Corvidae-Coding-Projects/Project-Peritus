//! Writer-reviewer-fixer run orchestration.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::RunId;
use serde::Deserialize;

use crate::{ProductRunnerError, ProductRunnerErrorKind, bundle, gates, plan, provider};

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
    ) -> Result<ProductRunOutput, ProductRunnerError> {
        let applied = initial_write(&input, &observe).await?;
        let mut state = RunState {
            summary: applied.summary,
            changed_files: applied.changed_files,
            fixer_cycles: 0,
        };

        loop {
            let inspected = inspect_cycle(&input, &observe, &state).await?;
            if inspected.passed {
                emit(
                    &observe,
                    ProductRunPhase::Complete,
                    inspected.cycle,
                    "Run completed with passing checks",
                    &inspected.evidence,
                    &state.summary,
                );
                return Ok(ProductRunOutput {
                    summary: state.summary,
                    diff: inspected.evidence.diff,
                    gates: inspected.evidence.gates,
                    review: inspected.evidence.review,
                    changed_files: state.changed_files,
                    fixer_cycles: state.fixer_cycles,
                });
            }
            if state.fixer_cycles >= MAX_FIX_CYCLES {
                return Err(ProductRunnerError::new(
                    ProductRunnerErrorKind::Gate,
                    "verify coding run",
                    "checks or blocking review findings remain after two fixer cycles",
                ));
            }
            apply_fix(&input, &observe, &inspected, &mut state).await?;
        }
    }
}

struct RunState {
    summary: String,
    changed_files: usize,
    fixer_cycles: u32,
}

#[derive(Default)]
struct RunEvidence {
    diff: String,
    gates: String,
    review: String,
}

struct CycleInspection {
    cycle: u32,
    passed: bool,
    evidence: RunEvidence,
}

async fn initial_write(
    input: &ProductRunInput,
    observe: &RunObserver,
) -> Result<plan::AppliedPlan, ProductRunnerError> {
    check_cancelled(input)?;
    emit(
        observe,
        ProductRunPhase::Writing,
        1,
        "Writer is preparing the implementation",
        &RunEvidence::default(),
        "",
    );
    let initial = bundle::build(&input.workspace_root, &input.task)?;
    let response = provider::complete(
        input.providers.writer.as_ref(),
        request_name(input.run_id, "writer", 1),
        writer_system(),
        writer_user(&input.task, &initial.prompt),
        input.provider_cancellation.clone(),
    )
    .await?;
    check_cancelled(input)?;
    plan::apply(&input.workspace_root, &response)
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
    evidence.review = provider::complete(
        input.providers.reviewer.as_ref(),
        request_name(input.run_id, "reviewer", cycle),
        reviewer_system(),
        reviewer_user(&input.task, &evidence.diff, &evidence.gates),
        input.provider_cancellation.clone(),
    )
    .await?;
    let review = parse_review(&evidence.review)?;
    emit(observe, ProductRunPhase::Reviewing, cycle, "Review completed", &evidence, &state.summary);
    Ok(CycleInspection { cycle, passed: gate_report.passed && !review.blocking, evidence })
}

async fn apply_fix(
    input: &ProductRunInput,
    observe: &RunObserver,
    inspected: &CycleInspection,
    state: &mut RunState,
) -> Result<(), ProductRunnerError> {
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
    let context = bundle::build(&input.workspace_root, &input.task)?;
    let response = provider::complete(
        input.providers.fixer.as_ref(),
        request_name(input.run_id, "fixer", state.fixer_cycles),
        writer_system(),
        fixer_user(
            &input.task,
            &context.prompt,
            &inspected.evidence.diff,
            &inspected.evidence.gates,
            &inspected.evidence.review,
        ),
        input.provider_cancellation.clone(),
    )
    .await?;
    let applied = plan::apply(&input.workspace_root, &response)?;
    state.changed_files = state.changed_files.saturating_add(applied.changed_files);
    state.summary = applied.summary;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewResult {
    summary: String,
    blocking: bool,
    #[serde(default)]
    findings: Vec<String>,
}

fn parse_review(value: &str) -> Result<ReviewResult, ProductRunnerError> {
    let start = value.find('{').ok_or_else(|| invalid_review("review contains no JSON object"))?;
    let end = value
        .rfind('}')
        .ok_or_else(|| invalid_review("review contains no complete JSON object"))?;
    let review: ReviewResult = serde_json::from_str(&value[start..=end]).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "parse reviewer result",
            error.to_string(),
        )
    })?;
    if review.summary.trim().is_empty() || review.findings.len() > 128 {
        return Err(invalid_review("review summary is empty or has too many findings"));
    }
    Ok(review)
}

fn check_cancelled(input: &ProductRunInput) -> Result<(), ProductRunnerError> {
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

fn request_name(run_id: RunId, role: &str, cycle: u32) -> String {
    let mut value = String::from("peritus-");
    for byte in run_id.as_bytes() {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    format!("{value}-{role}-{cycle}")
}

fn writer_system() -> String {
    "You are the implementation role in a coding harness. Return only one JSON object with this exact shape: {\"summary\":\"...\",\"files\":[{\"path\":\"relative/path\",\"content\":\"complete replacement contents\"}],\"deletions\":[\"relative/path\"]}. Make a substantial, maintainable implementation. Preserve unrelated code. Do not use markdown fences. Do not merely explain the work.".to_owned()
}

fn reviewer_system() -> String {
    "You are an independent code reviewer. Return only one JSON object with this exact shape: {\"summary\":\"...\",\"blocking\":false,\"findings\":[\"specific finding\"]}. Mark blocking true only for correctness, requested-behavior, build, or test failures that should prevent accepting the implementation. Do not invent obscure hypothetical threats or demand unrelated redesign. Do not use markdown fences.".to_owned()
}

fn writer_user(task: &str, bundle: &str) -> String {
    format!("Task:\n{task}\n\nRepository context:\n{bundle}")
}
fn reviewer_user(task: &str, diff: &str, gates: &str) -> String {
    format!("Task:\n{task}\n\nDiff:\n{diff}\n\nChecks:\n{gates}")
}
fn fixer_user(task: &str, bundle: &str, diff: &str, gates: &str, review: &str) -> String {
    format!(
        "Task:\n{task}\n\nCurrent repository:\n{bundle}\n\nCurrent diff:\n{diff}\n\nChecks:\n{gates}\n\nReview:\n{review}\n\nReturn the complete replacement file plan that fixes the real failures and blocking findings."
    )
}

fn invalid_review(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "validate reviewer result",
        detail,
    )
}
