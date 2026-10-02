//! Fresh tool-capable, read-only independent review turns.

use peritus_agent::{DeveloperLoopLimits, DeveloperLoopRequest};
use peritus_review::ProductReviewSubmission;

use crate::budget::RunAccounting;
use crate::developer_tools::{WorkspaceDeveloperTools, read_only_definitions};
use crate::execution::{ProductRunInput, check_cancelled};
use crate::{ProductRunnerError, ProductRunnerErrorKind, review, turn};

const MAX_REVIEWER_TURNS: u16 = 32;
const MAX_REVIEWER_TOOL_CALLS: u32 = 256;

pub struct ReviewEvidence<'a> {
    pub conversation: &'a str,
    pub diff: &'a str,
    pub gates: &'a str,
    pub developer_commands: &'a str,
    pub prior: &'a str,
}

/// Runs a fresh reviewer with bounded read-only workspace tools and parses its typed submission.
pub async fn complete(
    input: &ProductRunInput,
    cycle: u32,
    review_cycle: u32,
    evidence: ReviewEvidence<'_>,
    accounting: &mut RunAccounting,
) -> Result<ProductReviewSubmission, ProductRunnerError> {
    let mut providers =
        crate::failover::ProviderCursor::new(&input.providers.reviewer, &input.providers.fallbacks);
    let mut correction = None;
    let memory = input.working_memory("reviewer")?;
    let mut rejected_reviews = 0_u64;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut invocation = 0_u32;
    loop {
        check_cancelled(input)?;
        crate::failover::bypass_open_circuit(input, "reviewer", cycle, accounting, &mut providers)?;
        invocation = invocation.saturating_add(1);
        let request = prepare_request(
            input,
            &evidence,
            providers.current().profile().limits().max_input_tokens(),
            correction.as_deref(),
            accounting.remaining(),
            memory.as_ref(),
        )?;
        let media = match input.media(evidence.conversation, providers.current().profile()) {
            Ok(media) => media,
            Err(error) if let Some(switch) = providers.advance_for_capability(&error) => {
                crate::failover::record_switch(input, "reviewer", cycle, accounting, switch)?;
                continue;
            }
            Err(error) => return Err(error),
        };
        let (prompt, attachments) = media.into_parts(request.prompt);
        let mut tools = input.configure_tools(
            WorkspaceDeveloperTools::read_only(input.workspace_root.clone())
                .with_task_contract(evidence.conversation),
        );
        let result = crate::local_context::run_live_invocation(
            providers.current(),
            DeveloperLoopRequest {
                request_prefix: format!(
                    "{}-invocation-{invocation}",
                    turn::request_name(input.run_id, "reviewer", cycle),
                ),
                system: request.system,
                prompt,
                attachments,
                tools: request.tools,
                limits: reviewer_limits()?,
                cancellation: input.provider_cancellation.clone(),
            },
            &mut tools,
            crate::local_context::InvocationAccounting {
                trace_path: &input.trace_path,
                accounting,
            },
            memory.as_ref(),
            input.conversation.interaction(),
            peritus_agent::DeveloperModelRole::Reviewer,
        )
        .await;
        accounting.check()?;
        let result = match result {
            Ok(result) => result,
            Err(peritus_agent::DeveloperLoopError::SegmentExhausted) => {
                accounting.record_role_retry()?;
                provider_recovery.reset();
                correction = Some(crate::failover::RoleRecovery::correction("segment_boundary"));
                continue;
            }
            Err(error) => {
                if let Some(reason) = provider_recovery.retry(&error) {
                    accounting.record_role_retry()?;
                    correction = Some(crate::failover::RoleRecovery::correction(reason));
                    continue;
                }
                if let Some(switch) = providers.advance(&error) {
                    crate::failover::record_switch(input, "reviewer", cycle, accounting, switch)?;
                    provider_recovery.reset();
                    correction = None;
                    continue;
                }
                return Err(turn::developer_error(&error));
            }
        };
        crate::failover::record_provider_success(accounting, &providers, &mut provider_recovery);
        check_cancelled(input)?;
        let submission = grounded_submission(&tools, &result.text, review_cycle);
        match submission {
            Ok(submission) => return Ok(submission),
            Err(rejected) => {
                rejected_reviews = rejected_reviews.saturating_add(1);
                correction =
                    Some(record_rejected_review(input, rejected_reviews, &rejected, accounting)?);
            }
        }
    }
}

struct ReviewRequest {
    system: String,
    prompt: String,
    tools: Vec<peritus_model_protocol::ToolDefinition>,
}

fn prepare_request(
    input: &ProductRunInput,
    evidence: &ReviewEvidence<'_>,
    max_input_tokens: u64,
    correction: Option<&str>,
    remaining: Option<std::time::Duration>,
    memory: Option<&crate::local_context::LocalContextHandle>,
) -> Result<ReviewRequest, ProductRunnerError> {
    let system = turn::reviewer_system(remaining) + input.delivery_instructions();
    let tools = read_only_definitions()?;
    let mut budget_tools = tools.clone();
    if let Some(memory) = memory {
        budget_tools
            .extend(memory.tool_definitions().map_err(|error| turn::developer_error(&error))?);
    }
    let prompt = turn::reviewer_user(&turn::ReviewerPrompt {
        system: &system,
        tools: &budget_tools,
        transcript: &input.conversation.stable_request_context(),
        diff: evidence.diff,
        gates: evidence.gates,
        developer_evidence: evidence.developer_commands,
        prior: evidence.prior,
        max_input_tokens,
        delivery: turn::ReviewDelivery {
            scope: input.delivery_scope,
            effect_requirement: crate::delivery_requirement::ExternalEffectRequirement::from_task(
                input.delivery_scope,
                &input.task,
            ),
        },
        correction,
    })
    .map_err(|error| turn::developer_error(&error))?;
    Ok(ReviewRequest { system, prompt, tools })
}

fn grounded_submission(
    tools: &WorkspaceDeveloperTools,
    text: &str,
    cycle: u32,
) -> Result<ProductReviewSubmission, RejectedReview> {
    tools.grounding().validate().map_err(|detail| RejectedReview {
        error: grounding(detail),
        reason: peritus_agent::DeveloperReviewRetryReason::MissingGrounding,
    })?;
    review::parse(text, cycle).map_err(|error| RejectedReview {
        error,
        reason: peritus_agent::DeveloperReviewRetryReason::InvalidSubmission,
    })
}

struct RejectedReview {
    error: ProductRunnerError,
    reason: peritus_agent::DeveloperReviewRetryReason,
}

fn record_rejected_review(
    input: &ProductRunInput,
    rejected_reviews: u64,
    rejected: &RejectedReview,
    accounting: &mut RunAccounting,
) -> Result<String, ProductRunnerError> {
    accounting.record_role_retry()?;
    if let Some(port) = input.conversation.interaction() {
        port.observe(peritus_agent::DeveloperActivity::ReviewRetry {
            next_attempt: rejected_reviews.saturating_add(1),
            reason: rejected.reason,
        })
        .map_err(|error| turn::developer_error(&error))?;
    }
    Ok(correction_prompt(&rejected.error))
}

fn correction_prompt(error: &ProductRunnerError) -> String {
    format!(
        "The previous review was rejected during {}: {}. Request a fresh workspace_list call through the declared host tool-call interface, read the authoritative source inputs and exact changed files needed to verify the request, then return the complete typed reviewer JSON object.",
        error.operation(),
        error.detail(),
    )
}

fn reviewer_limits() -> Result<DeveloperLoopLimits, ProductRunnerError> {
    DeveloperLoopLimits::new(MAX_REVIEWER_TURNS, MAX_REVIEWER_TOOL_CALLS)
        .and_then(|limits| limits.with_max_output_tokens(4_096))
        .map_err(|error| turn::developer_error(&error))
}

fn grounding(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "ground independent review in repository evidence",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ungrounded_review_has_a_typed_public_reason_without_response_bytes() {
        let root = tempfile::tempdir().expect("workspace");
        let tools = WorkspaceDeveloperTools::read_only(root.path().to_owned());
        let Err(rejected) = grounded_submission(&tools, "PRIVATE_RESPONSE_CANARY", 1) else {
            panic!("uninspected review cannot be accepted")
        };
        assert_eq!(rejected.reason, peritus_agent::DeveloperReviewRetryReason::MissingGrounding);
        assert!(!rejected.error.detail().contains("PRIVATE_RESPONSE_CANARY"));
    }

    #[test]
    fn rejected_review_requires_fresh_authoritative_reads() {
        let error = grounding("repository grounding requires a successful workspace listing");
        let correction = correction_prompt(&error);

        assert!(correction.contains("fresh workspace_list call"));
        assert!(correction.contains("host tool-call interface"));
        assert!(correction.contains("authoritative source inputs"));
        assert!(correction.contains("exact changed files"));
        assert!(correction.contains(error.detail()));
    }
}
