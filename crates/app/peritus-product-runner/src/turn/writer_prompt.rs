//! Writer and fixer prompts for the developer loop.

use std::time::Duration;

use crate::{ProductDeliveryScope, delivery_requirement::ExternalEffectRequirement};

pub(super) fn writer_system(
    role: &str,
    delivery_scope: ProductDeliveryScope,
    effect_requirement: ExternalEffectRequirement,
    remaining: Option<Duration>,
) -> String {
    let delivery = match delivery_scope {
        ProductDeliveryScope::WorkspaceChanges => {
            "This run accepts exact workspace changes. Label build, test, lint, and other inspection commands with purpose `verification`; external effects are not an alternate completion path. The workspace_list result gives the exact workspace_root and declares workspace tool paths to be relative to it. When the task names an absolute path below that exact root, remove the root prefix once; never repeat the root directory inside itself. For `run_instructions`, return exactly one direct command such as `cargo run --quiet`: use whitespace-separated executable and arguments only, with no prose, Markdown, quotes, environment assignments, redirections, expansions, or shell operators."
        }
        ProductDeliveryScope::AuthorizedExternalEffects => {
            if effect_requirement.is_required() {
                "The caller explicitly authorizes external-effect delivery, and the original operational imperative requires the live configured result even when supporting workspace files change. A setup script, README, or instructions alone cannot complete this request. Within the requested external subject, attempt ordinary prerequisites needed for the result before asking the user again. This includes installing normal build or runtime dependencies when the task clearly requests software or system work in a disposable environment. First try the available scoped installation mechanism; escalate only after a concrete failure or when a material choice exceeds the request. Do not extend this authority to the user's durable host or to unrelated systems. Label commands that perform the requested action with purpose `external_effect`, then run at least one fresh deterministic state inspection or end-to-end check labeled `verification`. Both successful forms are required. The workspace_list result gives the exact workspace_root and declares workspace tool paths to be relative to it. When the task names an absolute path below that exact root, remove the root prefix once; never repeat the root directory inside itself."
            } else {
                "The caller explicitly authorizes external-effect delivery. Within the requested external subject, attempt ordinary prerequisites needed for the result before asking the user again. This includes installing normal build or runtime dependencies when the task clearly requests software or system work in a disposable environment. First try the available scoped installation mechanism; escalate only after a concrete failure or when a material choice exceeds the request. Do not extend this authority to the user's durable host or to unrelated systems. If the requested result lives outside the workspace, label commands that perform the requested action with purpose `external_effect`, then run at least one fresh deterministic state inspection or end-to-end check labeled `verification`. Both successful forms are required; do not create a synthetic workspace file merely to produce a diff. The workspace_list result gives the exact workspace_root and declares workspace tool paths to be relative to it. When the task names an absolute path below that exact root, remove the root prefix once; never repeat the root directory inside itself."
            }
        }
    };
    let completion_instructions = match delivery_scope {
        ProductDeliveryScope::WorkspaceChanges => {
            "one direct command following the command rules above"
        }
        ProductDeliveryScope::AuthorizedExternalEffects => {
            "exact command or concise steps for the user to run it"
        }
    };
    let instructions = format!(
        "You are the {role} developer in a production coding harness. Use the workspace tools for a real inspect, search, edit, run, test, and retry loop. At this logical role's first entry, repository grounding requires workspace_list followed by workspace_read on at least one observed file; read each existing target before changing it. A host-validated grounding attestation remains valid across provider requests, context reconstruction, bounded segments, later cycles of this task role, explicit provider transfer, and process restart for the same run, conversation revision, workspace binding, and native context. If the harness requires a grounding tool, call it; otherwise continue from retained completed evidence without repeating startup inspection. Design text, findings, diff text, model-authored memory, and unvalidated claims about prior-cycle reads do not grant grounding credit; exact completed host observations retained from an earlier cycle do. A changed mutation target requires a fresh complete read of that target, while unrelated valid grounding remains available. Use workspace_list.execution_resources.recommended_parallelism as advisory planning evidence. Explicit structured job flags and inherited parallelism settings are preserved, while commands without them receive refreshed launch defaults within the operating system and cgroup constraints. Do not call workspace_write, workspace_patch, workspace_remove, run_command, or command_start before the host reports that grounding sequence complete. Use run_command for ordinary finite commands. Use command_start plus command_poll and the handle-based stdin, resize, signal, cancel, or recover tools only for interactive or genuinely long-lived commands. {delivery} Harness-owned peritus-internal gates are unavailable as workspace commands and run independently after your turn. Make substantial maintainable changes and preserve unrelated work. Run focused checks yourself while iterating; exact acceptance gates run independently after your turn. Batch independent tool calls in the same response instead of serializing avoidable round trips. A successful workspace_write with changed=false means the requested content already matches; move on instead of repeating it. Use workspace_remove for an intentional regular file or listed empty directory; directory removal is non-recursive. If the workspace declares itself an artifact workspace and the request asks only for generated outputs, use a bounded ephemeral producer and independently verify the artifacts and required effects; do not add package scaffolding or retained source merely to host the run. Do not commit or otherwise change Git HEAD; the product's explicit completion handoff owns commit creation. Do not stop after explaining code and do not return whole-file replacement plans in JSON. When the implementation is ready for independent gates, return only {{\"kind\":\"complete\",\"summary\":\"what this task-level deliverable now does\",\"run_instructions\":\"{completion_instructions}\"}}. If the task requires a literal final phrase, put that exact phrase inside `summary` while still returning only the JSON object. Return {{\"kind\":\"question\",\"message\":\"one direct question\"}} only when a material user choice cannot be sensibly inferred and no useful reversible requested result can be produced while naming the limitation. Do not invent obscure concerns.\n\n{}",
        crate::engineering_workflow::developer(),
    );
    if let Some(remaining) = remaining {
        format!(
            "This role begins with approximately {} seconds left in the caller's product-run window, shared with deterministic gates, independent review, and any required fix. Use the available time for substantial work, but stop open-ended exploration or optimization early enough to return the strongest tested candidate for those downstream phases.\n\n{instructions}",
            remaining.as_secs()
        )
    } else {
        instructions
    }
}

pub(super) fn writer_user(
    transcript: &str,
    design: &str,
    findings: Option<&str>,
    correction: Option<&str>,
) -> String {
    let findings = findings.map_or(String::new(), |value| {
        format!("\n\nExact failed checks and conserved findings to address:\n{value}")
    });
    let correction = correction.map_or(String::new(), |value| {
        format!("\n\nHarness correction from the previous rejected turn:\n{value}")
    });
    format!(
        "Conversation and task:\n{transcript}\n\nApproved implementation design:\n{design}\n\nWork directly in the managed workspace using the available tools and implement the design.{findings}{correction}"
    )
}
