//! Fresh tool-capable, read-only independent review turns.

use peritus_agent::{DeveloperLoop, DeveloperLoopLimits, DeveloperLoopRequest};
use peritus_review::ProductReviewSubmission;

use crate::budget::RunAccounting;
use crate::developer_tools::{WorkspaceDeveloperTools, read_only_definitions};
use crate::execution::{ProductRunInput, check_cancelled};
use crate::trace::FileDeveloperTrace;
use crate::{ProductRunnerError, ProductRunnerErrorKind, review, turn};

const MAX_INVALID_REVIEWS: u8 = 3;
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
    let mut invalid_reviews = 0_u8;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut invocation = 0_u32;
    loop {
        check_cancelled(input)?;
        invocation = invocation.saturating_add(1);
        let prompt = turn::reviewer_user(&turn::ReviewerPrompt {
            transcript: evidence.conversation,
            diff: evidence.diff,
            gates: evidence.gates,
            developer_evidence: evidence.developer_commands,
            prior: evidence.prior,
            max_input_tokens: providers.current().profile().limits().max_input_tokens(),
            delivery: turn::ReviewDelivery {
                scope: input.delivery_scope,
                effect_requirement:
                    crate::delivery_requirement::ExternalEffectRequirement::from_task(
                        input.delivery_scope,
                        &input.task,
                    ),
            },
            correction: correction.as_deref(),
        });
        let media = match crate::workspace_media::discover(
            &input.workspace_root,
            evidence.conversation,
            providers.current().profile(),
        ) {
            Ok(media) => media,
            Err(error) if let Some(switch) = providers.advance_for_capability(&error) => {
                crate::failover::record_switch(input, "reviewer", cycle, accounting, switch)?;
                continue;
            }
            Err(error) => return Err(error),
        };
        let (prompt, attachments) = media.into_parts(prompt);
        let mut tools = WorkspaceDeveloperTools::read_only(input.workspace_root.clone());
        let mut trace = FileDeveloperTrace::new(input.trace_path.clone());
        let result = DeveloperLoop::run(
            providers.current(),
            DeveloperLoopRequest {
                request_prefix: format!(
                    "{}-invocation-{invocation}",
                    turn::request_name(input.run_id, "reviewer", cycle),
                ),
                system: turn::reviewer_system(accounting.remaining()),
                prompt,
                attachments,
                tools: read_only_definitions()?,
                limits: DeveloperLoopLimits::new(MAX_REVIEWER_TURNS, MAX_REVIEWER_TOOL_CALLS)
                    .and_then(|limits| limits.with_max_output_tokens(4_096))
                    .map_err(|error| turn::developer_error(&error))?,
                cancellation: input.provider_cancellation.clone(),
            },
            &mut tools,
            &mut trace,
        )
        .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if let Some(reason) = provider_recovery.retry(&error) {
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
        provider_recovery.reset();
        accounting.record(&result)?;
        check_cancelled(input)?;
        let submission = tools
            .grounding()
            .validate()
            .map_err(grounding)
            .and_then(|()| review::parse(&result.text, review_cycle));
        match submission {
            Ok(submission) => return Ok(submission),
            Err(error) => {
                invalid_reviews = invalid_reviews.saturating_add(1);
                if invalid_reviews >= MAX_INVALID_REVIEWS {
                    return Err(error);
                }
                correction = Some(correction_prompt(&error));
            }
        }
    }
}

fn correction_prompt(error: &ProductRunnerError) -> String {
    format!(
        "The previous review was rejected during {}: {}. Request a fresh workspace_list call through the declared host tool-call interface, read the authoritative source inputs and exact changed files needed to verify the request, then return the complete typed reviewer JSON object.",
        error.operation(),
        error.detail(),
    )
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
