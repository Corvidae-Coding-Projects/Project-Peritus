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
) -> Result<(), ProductRunServiceError> {
    const WAIT_DETAIL: &str = "Waiting for public provider output; no new result is available yet.";
    let text = format!(
        "I'm still waiting for the provider's response ({elapsed_seconds}s so far). I'll show it when it arrives."
    );
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
