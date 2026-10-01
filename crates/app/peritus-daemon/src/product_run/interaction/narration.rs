//! Public host observations. Never infer success, reveal arguments, or impersonate model reasoning.

use peritus_app_protocol::{ProductActivity, ProductActivityKind, ProductInteractionMode};

use super::{InteractionOptions, ProductRunServiceError};

pub(super) const fn starting(mode: ProductInteractionMode) -> &'static str {
    match mode {
        ProductInteractionMode::Chat => "I'm working on your reply. I'll keep you updated as I go.",
        ProductInteractionMode::Plan => {
            "I'll work through the plan with you without changing files."
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

#[cfg(not(verus_only))]
pub(super) fn review_retry(
    options: &mut InteractionOptions,
    next_attempt: u8,
    max_attempts: u8,
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
        &format!("{reason} Retrying review (attempt {next_attempt} of {max_attempts}) with fresh repository reads. Your existing work is retained."),
        "Host recovery notice",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_app_protocol::ProductRoleModels;

    #[test]
    fn review_retry_is_visible_and_separate_from_provider_text_and_wait_status() {
        use peritus_agent::DeveloperReviewRetryReason::{InvalidSubmission, MissingGrounding};
        let mut options =
            InteractionOptions::test(ProductInteractionMode::Build, ProductRoleModels::default());
        options.text(b"Original reviewer output").expect("provider text");
        review_retry(&mut options, 2, 3, MissingGrounding).expect("grounding notice");
        waiting(&mut options, 20).expect("status");
        review_retry(&mut options, 3, 3, InvalidSubmission).expect("schema notice");
        options.text(b"Corrected reviewer output").expect("provider text");
        assert_eq!(options.activities.len(), 5);
        assert_eq!(options.activities[0].text(), "Original reviewer output");
        assert_eq!(options.activities[4].text(), "Corrected reviewer output");
        for (index, attempt, reason) in
            [(1, "2 of 3", "repository evidence"), (3, "3 of 3", "invalid review")]
        {
            let notice = &options.activities[index];
            assert_eq!(notice.kind(), ProductActivityKind::Assistant);
            assert_eq!(notice.detail(), "Host recovery notice");
            assert!(notice.text().contains(attempt));
            assert!(notice.text().contains(reason));
            assert!(notice.text().contains("existing work is retained"));
        }
    }

    #[test]
    fn repeated_waits_preserve_the_conversation_and_update_one_notice() {
        let mut options =
            InteractionOptions::test(ProductInteractionMode::Chat, ProductRoleModels::default());
        options.append(ProductActivityKind::User, "Check my code", "").expect("user");
        waiting(&mut options, 20).expect("first wait");
        for seconds in (40..=4000).step_by(20) {
            waiting(&mut options, seconds).expect("continued wait");
        }
        assert_eq!(options.activities.len(), 2);
        assert_eq!(options.activities[0].text(), "Check my code");
        assert_eq!(options.activities[1].sequence(), 2);
        assert!(options.activities[1].text().contains("4000s"));
        options.text(b"Here is what I found.").expect("public response");
        assert_eq!(options.activities.last().expect("response").text(), "Here is what I found.");
    }

    #[test]
    fn chat_start_does_not_claim_implementation() {
        assert!(!starting(ProductInteractionMode::Chat).contains("implementation"));
    }
}
