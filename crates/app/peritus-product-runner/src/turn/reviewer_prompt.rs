//! Independent reviewer instructions and exact evidence projection.

use super::{ReviewerPrompt, evidence};
use peritus_agent::{DeveloperLoopError, estimate_developer_request_tokens};
use peritus_model_protocol::{BoundedText, ContentBlock, Message, ProtocolLimits, Role};
use std::time::Duration;

pub fn reviewer_system(remaining: Option<Duration>) -> String {
    let instructions = format!(
        "You are the independent D2 reviewer in a coding harness. When repository grounding has not yet been established for this candidate, begin by requesting the declared read-only `workspace_list` host function through the model tool-call interface, then use `workspace_search` and `workspace_read` as needed before reaching a verdict. Retain established grounding across provider segments and submission-format corrections. A formatting error does not invalidate a successful inventory or unchanged source observations; inspect missing evidence and changed candidate facts rather than repeating those reads. Peritus executes these requests and returns their results on later turns; they are not provider-native tools. Use them to inspect the authoritative source inputs, exact changed files, current permission metadata, and relevant surrounding repository context. Do not rely on the writer's account of files you can inspect. Inspect the original conversation, exact diff, and exact-target gate evidence; the design is a proposal, not authority. When the caller explicitly authorizes external-effect delivery, evaluate the retained structured command observations against the requested effect: require a relevant successful action and a later fresh state or end-to-end verification, but do not invent a missing repository diff as a finding. Git diff modes distinguish executable from non-executable files and do not encode exact POSIX permissions such as 0600; use Peritus workspace metadata when exact permissions matter. Verify every explicit requested path, field, value, operation, and scoped rule against the result. When the request requires an output component to match a named source, treat the complete selected source value as that component and apply only transformations the request names; outside knowledge that labels part of the source as a tag, wrapper, metadata, artifact, or non-native content is not authority to delete it. Reject self-authored checks that merely prove the implementation agrees with its own interpretation. A non-advisory finding must identify an unmet explicit requirement, a failed deterministic gate, or a concrete contradiction. Do not replace one reasonable reading of a grammatically ambiguous compound phrase with another merely because you prefer a narrower scope. Unless another authoritative source or deterministic gate resolves that scope, preserve a candidate that satisfies a reasonable reading and report the ambiguity only as advisory; a blocking interpretation finding must show the candidate violates every reasonable reading. Do not settle whether a trailing modifier distributes over coordinated list items by assuming that distribution and then citing an earlier item's lack of the modifier's property; independently consider distributive and nearest-item attachments. Do not broaden a named rule category to semantically related concepts without an authoritative label, taxonomy, or membership definition. Treat optional richer traces, duplicated corroboration, and evidence-presentation improvements as advisory, never as reasons for repeated fixer cycles. Accept contemporaneous process metrics unless contradicted; do not rerun stateful external operations merely to reproduce one-shot transient failures. Return one complete JSON object with this shape when it fits one response: {{\"summary\":\"...\",\"findings\":[{{\"category\":\"correctness|requested_behavior|build_coverage|test_coverage|security|maintainability|documentation\",\"severity\":\"advisory|low|medium|high|critical\",\"title\":\"stable concise identity\",\"description\":\"specific observed problem\",\"location\":\"stable path and symbol or empty\",\"reproduction\":\"exact evidence or command\",\"remediation\":\"specific required fix\"}}],\"coverage_cursor\":\"exact current indexed-page cursor\"}}. When the declared review_submission tools are present and the logical object does not fit one response, stage its decoded fields incrementally and finish it through the host instead of truncating or re-emitting the whole object. Do not return a blocking Boolean; policy derives blocker status from typed fields. Use a stable file and symbol location as provenance; findings with the same title in different files are distinct. Repeat every still-present finding in the current indexed reconciliation page using the same title and location. Omit a finding from that page only after independently confirming its fix in the fresh diff and evidence. Return the exact coverage_cursor to limit that omission assertion to the current page; findings outside it remain conserved and must not be repeated. A legacy response without coverage_cursor asserts full-ledger coverage. Do not invent obscure hypothetical threats or demand unrelated redesign. Do not use markdown fences.\n\n{}",
        crate::engineering_workflow::reviewer(),
    );
    if let Some(remaining) = remaining {
        format!(
            "This independent review begins with approximately {} seconds left in the shared caller window. Inspect decisive evidence first and return a grounded verdict without optional rechecks.\n\n{instructions}",
            remaining.as_secs()
        )
    } else {
        instructions
    }
}

#[derive(Clone, Copy)]
pub struct ReviewDelivery {
    pub scope: crate::ProductDeliveryScope,
    pub effect_requirement: crate::delivery_requirement::ExternalEffectRequirement,
}

/// Bounds the initial evidence after charging the system policy, prompt framing and tools.
///
/// # Errors
/// Rejects excessive protocol text or a profile with no room for independent review evidence.
pub fn reviewer_user(prompt: &ReviewerPrompt<'_>) -> Result<String, DeveloperLoopError> {
    reviewer_user_with_catalog(prompt, "")
}

/// Uses actual model headroom and retains exact handles independently of inline previews.
///
/// # Errors
/// Rejects framing that exceeds the actual model or physical protocol text allowance.
pub fn reviewer_user_with_catalog(
    prompt: &ReviewerPrompt<'_>,
    catalog: &str,
) -> Result<String, DeveloperLoopError> {
    let empty = evidence::project(0, 0, [""; 6]);
    let framing = render(prompt, &empty, catalog);
    let framing_tokens = request_tokens(prompt, &framing)?;
    let request_target = evidence::request_target(prompt.max_input_tokens);
    if framing_tokens > request_target {
        return Err(DeveloperLoopError::Context(format!(
            "reviewer policy, exact evidence catalog and tool framing use {framing_tokens} estimated tokens, exceeding the actual {}-token model input allowance",
            prompt.max_input_tokens
        )));
    }
    let values = [prompt.transcript, prompt.diff, prompt.gates, prompt.developer_evidence,
        prompt.prior, prompt.correction.unwrap_or_default()];
    // Charge one actual model-visible observation page. The page executors reserve the source
    // annotation inside this physical boundary before context ingestion appends it. The initial
    // prompt is retained while tools run; filling its entire input window would prevent even the
    // next observation from being admitted. This is no lifetime work quota.
    let observation_bytes = peritus_agent::developer_tool_page_bytes(
        prompt.max_input_tokens, ProtocolLimits::PRODUCTION,
    );
    let mut budget = usize::try_from(request_target.saturating_sub(framing_tokens).saturating_mul(3))
        .unwrap_or(usize::MAX)
        .saturating_sub(observation_bytes)
        .min(ProtocolLimits::PRODUCTION.max_text_bytes().saturating_sub(framing.len()));
    loop {
        let projected = evidence::project_bytes(budget, values);
        let rendered = render(prompt, &projected, catalog);
        let bytes_over = rendered.len().saturating_sub(ProtocolLimits::PRODUCTION.max_text_bytes());
        if bytes_over == 0 {
            let tokens = request_tokens(prompt, &rendered)?;
            if tokens <= request_target { return Ok(rendered); }
            if budget == 0 {
                return Err(DeveloperLoopError::Context(
                    "reviewer framing exceeds the actual model input allowance".to_owned(),
                ));
            }
            budget = budget.saturating_sub(
                usize::try_from(tokens.saturating_sub(request_target).saturating_mul(3))
                    .unwrap_or(usize::MAX).max(1),
            );
        } else {
            if budget == 0 {
                return Err(DeveloperLoopError::Context(
                    "reviewer framing exceeds one physical protocol text value".to_owned(),
                ));
            }
            budget = budget.saturating_sub(bytes_over);
        }
    }
}

fn request_tokens(prompt: &ReviewerPrompt<'_>, rendered: &str) -> Result<u64, DeveloperLoopError> {
    let messages = [(Role::System, prompt.system), (Role::User, rendered)]
        .into_iter().map(|(role, text)| Message::new(
            role, vec![ContentBlock::Text(BoundedText::new(text.to_owned(), ProtocolLimits::PRODUCTION)?)],
            ProtocolLimits::PRODUCTION,
        )).collect::<Result<Vec<_>, _>>()?;
    Ok(estimate_developer_request_tokens(&messages, prompt.tools))
}

fn render(prompt: &ReviewerPrompt<'_>, projected: &evidence::ReviewerEvidence, catalog: &str) -> String {
    let correction = if projected.correction.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nHarness correction from the previous rejected review:\n{}",
            projected.correction
        )
    };
    let delivery = match prompt.delivery.scope {
        crate::ProductDeliveryScope::WorkspaceChanges => {
            "Delivery scope: exact workspace changes. An empty candidate cannot pass."
        }
        crate::ProductDeliveryScope::AuthorizedExternalEffects => {
            if prompt.delivery.effect_requirement.is_required() {
                "Delivery scope: the operational request requires a live caller-authorized external effect even when supporting workspace files changed. Require relevant successful external_effect command evidence followed by successful fresh verification command evidence. A setup script, README, or instructions alone are not the requested configured state."
            } else {
                "Delivery scope: caller-authorized external effects. When the candidate has no workspace changes, require relevant successful external_effect command evidence followed by successful fresh verification command evidence; do not require a synthetic file change."
            }
        }
    };
    format!(
        "Conversation:\n{}\n\n{delivery}\n\nCurrent diff:\n{}\n\nExact-target checks:\n{}\n\nDeveloper command observations:\n{}\n\nConserved finding history:\n{}\n\nEstablish repository grounding with a workspace_list host-function call and independent reads of authoritative workspace inputs and exact changed files. Reuse retained grounding for this candidate across segment and typed-submission repairs; additional reads should resolve missing evidence or changed facts. Developer command observations are real bounded process results: confirm that each claimed acceptance command exercises the explicit requested behavior and reject circular mocks, irrelevant success, or verification that predates the effect. For every conserved finding, read each cited current workspace file before repeating the finding; prior diff and finding text can predate fixer writes and do not prove that a defect remains.{correction}",
        projected.transcript, projected.diff, projected.gates, projected.developer, projected.prior,
    )
    + catalog
}
