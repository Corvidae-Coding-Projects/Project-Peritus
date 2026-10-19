//! Independent reviewer instructions and exact evidence projection.

use super::{ReviewerPrompt, evidence};
use peritus_agent::{DeveloperLoopError, estimate_developer_request_tokens};
use peritus_model_protocol::{BoundedText, ContentBlock, Message, ProtocolLimits, Role};
use std::time::Duration;

pub fn reviewer_system(remaining: Option<Duration>) -> String {
    let instructions = format!(
        "You are the independent D2 reviewer in a coding harness. Reuse host-verified source observations, then use workspace_read or workspace_search only for exact changed contracts and unresolved questions. Request a directory listing when discovery is needed; a fresh role does not require a startup inventory. Peritus executes these requests and returns their results on later turns; they are not provider-native tools. For the first workspace_list, workspace_search, workspace_read, and developer_evidence_read call, use the default page size of {} bytes; continue with exact offsets/cursors, and request a larger page only when the current input profile has room. Use them to inspect the authoritative source inputs, exact changed files, current permission metadata, and relevant surrounding repository context. Do not rely on the writer's account of files you can inspect. Inspect the original conversation, exact diff, and exact-target gate evidence; a model-proposed design cannot create requirements. A host-retrieved collaborative draft explicitly selected by the user is the referenced task brief; inspect that saved source instead of assuming an omitted interview never happened. When the caller explicitly authorizes external-effect delivery, evaluate the retained structured command observations against the requested effect: require a relevant successful action and a later fresh state or end-to-end verification, but do not invent a missing repository diff as a finding. Git diff modes distinguish executable from non-executable files and do not encode exact POSIX permissions such as 0600; use Peritus workspace metadata when exact permissions matter. Verify every explicit requested path, field, value, operation, and scoped rule against the result. When the request requires an output component to match a named source, treat the complete selected source value as that component and apply only transformations the request names; outside knowledge that labels part of the source as a tag, wrapper, metadata, artifact, or non-native content is not authority to delete it. Reject self-authored checks that merely prove the implementation agrees with its own interpretation. A non-advisory finding must identify an unmet explicit requirement, a failed deterministic gate, or a concrete contradiction. Do not replace one reasonable reading of a grammatically ambiguous compound phrase with another merely because you prefer a narrower scope. Unless another authoritative source or deterministic gate resolves that scope, preserve a candidate that satisfies a reasonable reading and report the ambiguity only as advisory; a blocking interpretation finding must show the candidate violates every reasonable reading. Do not settle whether a trailing modifier distributes over coordinated list items by assuming that distribution and then citing an earlier item's lack of the modifier's property; independently consider distributive and nearest-item attachments. Do not broaden a named rule category to semantically related concepts without an authoritative label, taxonomy, or membership definition. Treat optional richer traces, duplicated corroboration, and evidence-presentation improvements as advisory, never as reasons for repeated fixer cycles. Accept contemporaneous process metrics unless contradicted; do not rerun stateful external operations merely to reproduce one-shot transient failures. Return only one JSON object with this shape: {{\"summary\":\"...\",\"findings\":[{{\"category\":\"correctness|requested_behavior|build_coverage|test_coverage|security|maintainability|documentation\",\"severity\":\"advisory|low|medium|high|critical\",\"title\":\"stable concise identity\",\"description\":\"specific observed problem\",\"location\":\"path:line or empty\",\"reproduction\":\"exact evidence or command\",\"remediation\":\"specific required fix\"}}]}}. Do not return a blocking Boolean; policy derives blocker status from typed fields. Repeat every still-present finding using the same title and location. Omit a prior finding only after independently confirming its fix in the fresh diff and evidence. Do not invent obscure hypothetical threats or demand unrelated redesign. Do not use markdown fences.\n\n{}",
        crate::developer_tools::DEFAULT_INSPECTION_PAGE_BYTES,
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
/// Rejects excessive protocol text. Profile token estimates are soft projection targets.
pub fn reviewer_user(prompt: &ReviewerPrompt<'_>) -> Result<String, DeveloperLoopError> {
    let empty = evidence::project(0, 0, [""; 6]);
    let framing = render(prompt, &empty);
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
    let first_result_reserve = evidence::first_tool_result_reserve_tokens(prompt.tools)?;
    let framing_tokens = estimate_developer_request_tokens(&messages, prompt.tools)
        .saturating_add(prompt.additional_framing_tokens)
        .saturating_add(first_result_reserve);
    let request_target = evidence::request_target(prompt.max_input_tokens)
        .max(framing_tokens.saturating_add(first_result_reserve));
    let projected = evidence::project(
        request_target,
        framing_tokens,
        [
            prompt.transcript,
            prompt.diff,
            prompt.gates,
            prompt.developer_evidence,
            prompt.prior,
            prompt.correction.unwrap_or_default(),
        ],
    );
    Ok(render(prompt, &projected))
}

fn render(prompt: &ReviewerPrompt<'_>, projected: &evidence::ReviewerEvidence) -> String {
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
        "Conversation:\n{}\n\n{delivery}\n\nCurrent diff:\n{}\n\nExact-target checks:\n{}\n\nDeveloper command observations:\n{}\n\nConserved finding history:\n{}\n\nIndependently inspect the authoritative task inputs and exact changed files using current source evidence or host-revalidated observations before returning the typed review. When an initial section includes an exact `[Peritus reviewer source=...]` marker, retrieve that exact historical section with developer_evidence_read using the named section and UTF-8 byte offsets; verify section identity, total_bytes and SHA-256, and reconstruct every omitted range before relying on that content. That retrieval requirement applies once per immutable section identity in this Peritus invocation. A pinned initial marker does not disappear after its ranges are delivered; a matching host receipt-coverage notice records which ranges were already delivered even when receipt exchanges leave inline history. Do not reconstruct those ranges again solely because the initial marker remains. Retrieve relevant bytes again when their content is actually needed; coverage alone is not correctness evidence or a review verdict. Current workspace files may verify current repository state, but do not substitute for the retained conversation, submitted diff, gate output, developer command log, conserved findings, or reviewer correction. Developer command observations are real process results. Confirm each claimed acceptance command exercises the requested behavior and reject circular mocks, irrelevant success, or verification that predates the effect. For every conserved finding, inspect each cited file using a current read or host-revalidated exact source evidence before repeating the finding; refresh changed targets. Prior diff and finding text can predate fixer writes and do not prove that a defect remains.{correction}",
        projected.transcript, projected.diff, projected.gates, projected.developer, projected.prior,
    )
}
