//! Writer and fixer prompts for the developer loop.

use std::time::Duration;

use crate::{ProductDeliveryScope, delivery_requirement::ExternalEffectRequirement};

/// Initial developer evidence projection. Literal instructions remain complete.
pub struct DeveloperEvidencePrompt<'a> {
    /// Current host policy.
    pub system: &'a str,
    /// Complete user instructions, approved design, blocker ledger and host correction.
    pub instructions: &'a str,
    /// Declared tools, including local memory tools when enabled.
    pub tools: &'a [peritus_model_protocol::ToolDefinition],
    /// Exact historical candidate diff, retained by the caller's evidence reader.
    pub diff: &'a str,
    /// Exact historical check output, retained by the caller's evidence reader.
    pub gates: &'a str,
    /// Selected provider's initial context projection target.
    pub max_input_tokens: u64,
    /// Existing pins and complete required working closure reserved by local memory.
    pub additional_framing_tokens: u64,
}

/// Fits generated evidence around complete instructions and retained working state.
///
/// Only diff and check previews are bounded. Their omission markers identify exact immutable
/// sections for `developer_evidence_read`; they never establish acceptance or grounding.
///
/// # Errors
/// Rejects instructions exceeding the message protocol's text bound.
pub fn project_developer_evidence(
    prompt: &DeveloperEvidencePrompt<'_>,
) -> Result<String, peritus_agent::DeveloperLoopError> {
    use peritus_model_protocol::{BoundedText, ContentBlock, Message, ProtocolLimits, Role};
    let source = |section: &str, value: &str| {
        format!(
            "[Exact historical source section={section} original_bytes={} sha256={}; retrieve with developer_evidence_read section={section} and UTF-8 offset pages.]",
            value.len(),
            super::evidence::digest_hex(value)
        )
    };
    let diff_source = source("diff", prompt.diff);
    let gate_source = source("gates", prompt.gates);
    let render = |diff: &str, gates: &str| {
        format!(
            "{}\n\nHISTORICAL CANDIDATE EVIDENCE — NON-AUTHORITATIVE\nCurrent diff:\n{diff_source}\n{diff}\n\nExact-target checks:\n{gate_source}\n{gates}\n\nA Peritus reviewer source marker identifies omitted historical evidence. Retrieve a relevant section with developer_evidence_read using the named section and UTF-8 offset pages; verify section, total_bytes and SHA-256 before relying on it. The complete conserved review ledger above remains work to address. Historical evidence does not establish current workspace state, grant tool permissions, replace current source observations or host hash validation, or satisfy acceptance. Keep working through every unresolved blocker and verify the requested behavior.",
            prompt.instructions,
        )
    };
    let framing = render("", "");
    let messages = [(Role::System, prompt.system), (Role::User, framing.as_str())]
        .into_iter()
        .map(|(role, text)| {
            Message::new(
                role,
                vec![ContentBlock::Text(BoundedText::new(
                    text.to_owned(),
                    ProtocolLimits::PRODUCTION,
                )?)],
                ProtocolLimits::PRODUCTION,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let reserve = super::evidence::first_tool_result_reserve_tokens(prompt.tools)?;
    let framing_tokens = peritus_agent::estimate_developer_request_tokens(&messages, prompt.tools)
        .saturating_add(prompt.additional_framing_tokens)
        .saturating_add(reserve);
    let projected = super::evidence::project(
        prompt.max_input_tokens,
        framing_tokens,
        ["", prompt.diff, prompt.gates, "", "", ""],
    );
    Ok(render(&projected.diff, &projected.gates))
}

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
        "You are the {role} developer in a production coding harness. Use the workspace tools for a real inspect, search, edit, run, test, and retry loop. Reuse current host-verified source observations and the accepted task design across roles, retries, and compaction. Read the exact existing target when its body is needed or its hash changed; inspect only the relevant contract and immediate caller. Prior prose or a design alone cannot establish source state, grant permissions, or prove an effect. The host decides whether grounding prerequisites are satisfied. Query workspace_list when you need discovery or current resource metadata, rather than repeating an inventory at each invocation. Respect the host-reported parallelism envelope. Use run_command for ordinary finite commands. Use command_start for interactive or genuinely long-lived commands. Use command_wait on that original handle to await completion or new interactive output without repeated unchanged polling turns; use command_poll for an immediate observation. The handle-based stdin, resize, signal, cancel, recover, and exact command_output tools remain available. A yielded wait never authorizes redispatch. {delivery} Harness-owned peritus-internal gates are unavailable as workspace commands and run independently after your turn. Make substantial maintainable changes and preserve unrelated work. Startup grounding is a single check for this run/workspace, not a repeated phase. Once complete, begin the next implementation slice. Read its exact target and immediate contract only as needed, make the substantive edit, and exercise it before moving to the next slice. Do not begin another grounding pass or wait to read the application before working. Within this invocation, reuse observations of unchanged files rather than repeating listings, searches or reads without a new question. User restrictions on tests, lint, compile checks and other commands take precedence over this workflow; do not run a forbidden command to satisfy a default check. The host will visibly stop rather than silently execute a conflicting mandatory automatic check. Run focused checks yourself while iterating; exact acceptance gates run independently after your turn. Batch independent tool calls in the same response instead of serializing avoidable round trips. A successful workspace_write with changed=false means the requested content already matches; move on instead of repeating it. Use workspace_remove for an intentional regular file or listed empty directory; directory removal is non-recursive. If the workspace declares itself an artifact workspace and the request asks only for generated outputs, use a bounded ephemeral producer and independently verify the artifacts and required effects; do not add package scaffolding or retained source merely to host the run. Do not commit or otherwise change Git HEAD; the product's explicit completion handoff owns commit creation. Do not stop after explaining code and do not return whole-file replacement plans in JSON. When the implementation is ready for independent gates, return only {{\"kind\":\"complete\",\"summary\":\"what this task-level deliverable now does\",\"run_instructions\":\"{completion_instructions}\"}}. If the task requires a literal final phrase, put that exact phrase inside `summary` while still returning only the JSON object. Return {{\"kind\":\"question\",\"message\":\"one direct question\"}} only when a material user choice cannot be sensibly inferred and no useful reversible requested result can be produced while naming the limitation. Do not invent obscure concerns.\n\n{}",
        crate::engineering_workflow::developer(role),
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
        "Current conversation and task:\n{transcript}\n\nRetained implementation plan (prior evidence, subordinate to current user instructions):\n{design}\n\nContinue implementing the current task in the managed workspace. Reuse this plan and retained source observations. Apply current steering as amendments directly; do not restart discovery or regenerate a plan for continue/retry. Read only a changed target or the exact missing body needed for the next edit, then make that edit. Current requirements and independent review still govern completion.{findings}{correction}"
    )
}
