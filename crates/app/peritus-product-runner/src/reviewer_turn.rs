//! Fresh tool-capable, read-only independent review turns.

use peritus_agent::{DeveloperLoop, DeveloperLoopLimits, DeveloperLoopRequest};
use peritus_review::ProductReviewSubmission;

use crate::developer_tools::{WorkspaceDeveloperTools, read_only_definitions};
use crate::execution::{ProductRunInput, check_cancelled};
use crate::trace::FileDeveloperTrace;
use crate::{ProductRunnerError, ProductRunnerErrorKind, review, turn};

const MAX_INVALID_REVIEWS: u8 = 3;
const MAX_REVIEWER_TURNS: u16 = 32;
const MAX_REVIEWER_TOOL_CALLS: u32 = 256;

/// Runs a fresh reviewer with bounded read-only workspace tools and parses its typed submission.
pub async fn complete(
    input: &ProductRunInput,
    cycle: u32,
    review_cycle: u32,
    conversation: &str,
    diff: &str,
    gates: &str,
    prior: &str,
) -> Result<ProductReviewSubmission, ProductRunnerError> {
    let mut correction = None;
    for attempt in 1..=MAX_INVALID_REVIEWS {
        check_cancelled(input)?;
        let prompt = turn::reviewer_user(conversation, diff, gates, prior, correction.as_deref());
        let media = crate::workspace_media::discover(
            &input.workspace_root,
            conversation,
            input.providers.reviewer.profile(),
        )?;
        let (prompt, attachments) = media.into_parts(prompt);
        let mut tools = WorkspaceDeveloperTools::read_only(input.workspace_root.clone());
        let mut trace = FileDeveloperTrace::new(input.trace_path.clone());
        let result = DeveloperLoop::run(
            input.providers.reviewer.as_ref(),
            DeveloperLoopRequest {
                request_prefix: format!(
                    "{}-invocation-{attempt}",
                    turn::request_name(input.run_id, "reviewer", cycle),
                ),
                system: turn::reviewer_system(),
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
        .await
        .map_err(|error| turn::developer_error(&error))?;
        check_cancelled(input)?;
        let submission = tools
            .grounding()
            .validate()
            .map_err(grounding)
            .and_then(|()| review::parse(&result.text, review_cycle));
        match submission {
            Ok(submission) => return Ok(submission),
            Err(error) if attempt < MAX_INVALID_REVIEWS => {
                correction = Some(correction_prompt(&error));
            }
            Err(error) => return Err(error),
        }
    }
    Err(ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "complete independent review",
        "reviewer attempts were exhausted",
    ))
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
