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

pub(super) fn tool_started(name: &str) -> &'static str {
    match name {
        "workspace_list" => "Looking through the workspace files.",
        "workspace_read" => "Reading the relevant file contents.",
        "workspace_search" => "Searching the workspace for relevant code and context.",
        "workspace_write" => "Writing the proposed changes.",
        "run_command" => "Running a command; I'll check its result before continuing.",
        _ => "Working through the next step.",
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

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_app_protocol::ProductRoleModels;

    #[test]
    fn repeated_waits_preserve_the_conversation_and_update_one_notice() {
        let mut options =
            InteractionOptions::new(ProductInteractionMode::Chat, ProductRoleModels::default());
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
    fn tool_narration_does_not_echo_private_or_unknown_names() {
        assert_eq!(tool_started("workspace_read"), "Reading the relevant file contents.");
        assert_eq!(tool_started("private-tool-name"), "Working through the next step.");
        assert!(!starting(ProductInteractionMode::Chat).contains("implementation"));
    }
}
