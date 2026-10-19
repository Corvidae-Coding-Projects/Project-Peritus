//! Public host observations. Never infer success, reveal arguments, or impersonate model reasoning.

use peritus_app_protocol::{ProductActivity, ProductActivityKind, ProductInteractionMode};

use super::{InteractionOptions, ProductRunServiceError};

pub(super) const fn starting(mode: ProductInteractionMode) -> &'static str {
    match mode {
        ProductInteractionMode::Chat => "I'm working on your reply. I'll keep you updated as I go.",
        ProductInteractionMode::Plan => {
            "I'll work through the plan with you without changing files."
        }
        ProductInteractionMode::Design => {
            "I'm helping you shape the design. You can revise any decision before starting implementation."
        }
        ProductInteractionMode::Review => {
            "I'll review the evidence and share what I find, without changing files."
        }
        ProductInteractionMode::Build => {
            "I'll work through the design, implementation, checks, and review, and keep you updated along the way."
        }
    }
}

pub(super) fn waiting(
    options: &mut InteractionOptions,
    elapsed_seconds: u64,
    progress: peritus_agent::DeveloperProviderProgress,
) -> Result<(), ProductRunServiceError> {
    use peritus_agent::DeveloperProviderProgress;
    const WAIT_DETAIL: &str = "Live provider observation. Structured tool proposals run only after complete response validation.";
    let text = match progress {
        DeveloperProviderProgress::AwaitingResponse => {
            format!("Waiting for provider output ({elapsed_seconds}s elapsed).")
        }
        DeveloperProviderProgress::Accepted => format!(
            "Provider runtime accepted the request; waiting for generation ({elapsed_seconds}s elapsed)."
        ),
        DeveloperProviderProgress::Reasoning => {
            format!("Provider is reasoning ({elapsed_seconds}s elapsed).")
        }
        DeveloperProviderProgress::Generating { output_bytes } => format!(
            "Receiving structured response: {output_bytes} bytes ({elapsed_seconds}s elapsed). Validation follows completion."
        ),
    };
    if let Some(last) = options.activities.last_mut()
        && last.kind() == ProductActivityKind::Status
        && last.detail() == WAIT_DETAIL
    {
        // A long wait must not evict the actual conversation with repeated heartbeat entries.
        *last = ProductActivity::new(
            last.sequence(),
            ProductActivityKind::Status,
            text,
            WAIT_DETAIL.to_owned(),
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        return Ok(());
    }
    options.append(ProductActivityKind::Status, &text, WAIT_DETAIL)
}

pub(super) fn review_retry(
    options: &mut InteractionOptions,
    next_attempt: u64,
    reason: peritus_agent::DeveloperReviewRetryReason,
) -> Result<(), ProductRunServiceError> {
    let reason = match reason {
        peritus_agent::DeveloperReviewRetryReason::MissingGrounding => {
            "The independent reviewer did not inspect the required repository evidence."
        }
        peritus_agent::DeveloperReviewRetryReason::InvalidSubmission => {
            "The independent reviewer returned an incomplete or invalid review."
        }
    };
    // Public assistant narration is visible in ordinary clients. Keep host notices separate
    // from streamed provider text and from the unchanged private response trace.
    options.append(
        ProductActivityKind::Assistant,
        &format!("{reason} Retrying review (attempt {next_attempt}) with fresh repository reads. Your existing work is retained."),
        "Host recovery notice",
    )
}
