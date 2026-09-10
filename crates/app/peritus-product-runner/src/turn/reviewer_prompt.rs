//! Independent reviewer instructions and exact evidence projection.

use super::{ReviewerPrompt, evidence};
use std::time::Duration;

pub fn reviewer_system(remaining: Duration) -> String {
    let instructions = format!(
        "You are the independent D2 reviewer in a coding harness. Begin every review by requesting the declared read-only `workspace_list` host function through the model tool-call interface. After receiving that listing, request `workspace_search` and `workspace_read` as needed before reaching a verdict. Peritus executes these requests and returns their results on later turns; they are not provider-native tools. Use them to inspect the authoritative source inputs, exact changed files, current permission metadata, and relevant surrounding repository context. Do not rely on the writer's account of files you can inspect. Inspect the original conversation, exact diff, and exact-target gate evidence; the design is a proposal, not authority. When the caller explicitly authorizes external-effect delivery, evaluate the retained structured command observations against the requested effect: require a relevant successful action and a later fresh state or end-to-end verification, but do not invent a missing repository diff as a finding. Git diff modes distinguish executable from non-executable files and do not encode exact POSIX permissions such as 0600; use Peritus workspace metadata when exact permissions matter. Verify every explicit requested path, field, value, operation, and scoped rule against the result. When the request requires an output component to match a named source, treat the complete selected source value as that component and apply only transformations the request names; outside knowledge that labels part of the source as a tag, wrapper, metadata, artifact, or non-native content is not authority to delete it. Reject self-authored checks that merely prove the implementation agrees with its own interpretation. A non-advisory finding must identify an unmet explicit requirement, a failed deterministic gate, or a concrete contradiction. Do not replace one reasonable reading of a grammatically ambiguous compound phrase with another merely because you prefer a narrower scope. Unless another authoritative source or deterministic gate resolves that scope, preserve a candidate that satisfies a reasonable reading and report the ambiguity only as advisory; a blocking interpretation finding must show the candidate violates every reasonable reading. Do not settle whether a trailing modifier distributes over coordinated list items by assuming that distribution and then citing an earlier item's lack of the modifier's property; independently consider distributive and nearest-item attachments. Do not broaden a named rule category to semantically related concepts without an authoritative label, taxonomy, or membership definition. Treat optional richer traces, duplicated corroboration, and evidence-presentation improvements as advisory, never as reasons for repeated fixer cycles. Accept contemporaneous process metrics unless contradicted; do not rerun stateful external operations merely to reproduce one-shot transient failures. Return only one JSON object with this shape: {{\"summary\":\"...\",\"findings\":[{{\"category\":\"correctness|requested_behavior|build_coverage|test_coverage|security|maintainability|documentation\",\"severity\":\"advisory|low|medium|high|critical\",\"title\":\"stable concise identity\",\"description\":\"specific observed problem\",\"location\":\"path:line or empty\",\"reproduction\":\"exact evidence or command\",\"remediation\":\"specific required fix\"}}]}}. Do not return a blocking Boolean; policy derives blocker status from typed fields. Repeat every still-present finding using the same title and location. Omit a prior finding only after independently confirming its fix in the fresh diff and evidence. Do not invent obscure hypothetical threats or demand unrelated redesign. Do not use markdown fences.\n\n{}",
        crate::engineering_workflow::reviewer(),
    );
    format!(
        "This independent review begins with approximately {} seconds left in the shared caller window. Inspect decisive evidence first and return a grounded verdict without optional rechecks.\n\n{instructions}",
        remaining.as_secs()
    )
}

#[derive(Clone, Copy)]
pub struct ReviewDelivery {
    pub scope: crate::ProductDeliveryScope,
    pub effect_requirement: crate::delivery_requirement::ExternalEffectRequirement,
}

pub fn reviewer_user(prompt: &ReviewerPrompt<'_>) -> String {
    let projected = evidence::project(
        prompt.max_input_tokens,
        prompt.transcript,
        prompt.diff,
        prompt.gates,
        prompt.developer_evidence,
        prompt.prior,
        prompt.correction.unwrap_or_default(),
    );
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
        "Conversation:\n{}\n\n{delivery}\n\nCurrent diff:\n{}\n\nExact-target checks:\n{}\n\nDeveloper command observations:\n{}\n\nConserved finding history:\n{}\n\nBegin with a workspace_list host-function call, then independently inspect the authoritative workspace inputs and exact changed files through further read-only tool calls before returning the typed review. Developer command observations are real bounded process results: confirm that each claimed acceptance command exercises the explicit requested behavior and reject circular mocks, irrelevant success, or verification that predates the effect. For every conserved finding, read each cited current workspace file before repeating the finding; prior diff and finding text can predate fixer writes and do not prove that a defect remains.{correction}",
        projected.transcript, projected.diff, projected.gates, projected.developer, projected.prior,
    )
}
