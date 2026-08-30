//! Tool-capable writer/fixer turns and independent reviewer prompts.

use std::sync::Arc;

use peritus_agent::{
    DeveloperLoop, DeveloperLoopError, DeveloperLoopLimits, DeveloperLoopOutcome,
    DeveloperLoopRequest,
};
use peritus_provider_core::ModelProvider;
use peritus_types::RunId;

use crate::budget::RunAccounting;
use crate::developer_tools::{
    WorkspaceDeveloperTools, WorkspaceOwnership, definitions, merge_rendered,
};
use crate::execution::{AppliedTurn, AppliedWrite, ProductRunInput, check_cancelled};
use crate::progress::WorkspaceCheckpoint;
use crate::trace::FileDeveloperTrace;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

mod provider;
mod terminal;

use terminal::TerminalTurn;

const MAX_UNPRODUCTIVE_TERMINALS: u8 = 3;

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the ordered role invocation and repository-progress state machine remain explicit"
)]
pub async fn complete_developer_turn(
    input: &ProductRunInput,
    primary: &Arc<dyn ModelProvider>,
    role: &str,
    cycle: u32,
    design: &str,
    findings: Option<&str>,
    ownership: &mut WorkspaceOwnership,
    accounting: &mut RunAccounting,
) -> Result<AppliedTurn, ProductRunnerError> {
    let mut providers = crate::failover::ProviderCursor::new(primary, &input.providers.fallbacks);
    let mut checkpoint = WorkspaceCheckpoint::capture(&input.workspace_root)?;
    let mut invocation = 0_u32;
    let mut unproductive_terminals = 0_u8;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut verification_evidence = String::new();
    let mut successful_commands = Vec::new();
    let (mut correction, mut pending_question) = (None, None);
    loop {
        check_cancelled(input)?;
        invocation = invocation.saturating_add(1);
        let revision = input.conversation.revision();
        let identity = DeveloperInvocation { role, cycle, invocation };
        let Some((result, tools)) = run_selected_invocation(
            input,
            &mut providers,
            identity,
            InvocationContext { design, findings, correction: correction.as_deref(), ownership },
            accounting,
        )
        .await?
        else {
            continue;
        };
        provider::record_accounting(&result, accounting)?;
        *ownership = tools.ownership().clone();
        merge_rendered(&mut verification_evidence, &tools.verification_evidence());
        crate::developer_tools::merge_successful(
            &mut successful_commands,
            &tools.successful_commands(),
        );
        check_cancelled(input)?;
        if input.conversation.revision() != revision {
            checkpoint = WorkspaceCheckpoint::capture(&input.workspace_root)?;
            provider_recovery.reset();
            unproductive_terminals = 0;
            (correction, pending_question) = (None, None);
            continue;
        }
        let resolution = provider::resolve(
            input,
            &mut providers,
            identity,
            result,
            &mut checkpoint,
            &mut provider_recovery,
            accounting,
        )?;
        let Some(result) = provider::apply(
            resolution,
            &mut correction,
            &mut pending_question,
            &mut unproductive_terminals,
        ) else {
            continue;
        };
        let terminal = match parse_grounded_terminal(&tools, &result) {
            Ok(terminal) => terminal,
            Err(error) => {
                let current = WorkspaceCheckpoint::capture(&input.workspace_root)?;
                if current != checkpoint {
                    checkpoint = current;
                    unproductive_terminals = 0;
                    (correction, pending_question) = (None, None);
                    continue;
                }
                unproductive_terminals = unproductive_terminals.saturating_add(1);
                if unproductive_terminals < MAX_UNPRODUCTIVE_TERMINALS {
                    correction = Some(correction_prompt(&error));
                    continue;
                }
                return Err(error);
            }
        };
        return match terminal {
            TerminalTurn::Complete(summary) => Ok(AppliedTurn::Applied(AppliedWrite {
                summary: summary.0,
                run_instructions: summary.1,
                tool_calls: result.tool_calls,
                conversation_revision: revision,
                verification_evidence,
                successful_commands,
            })),
            TerminalTurn::Question(question) => {
                let current = WorkspaceCheckpoint::capture(&input.workspace_root)?;
                if retry_unverified_question(
                    &question,
                    current == checkpoint,
                    pending_question.as_deref(),
                    &mut unproductive_terminals,
                ) {
                    correction = Some(question_confirmation_prompt(&question));
                    pending_question = Some(question);
                    continue;
                }
                Ok(AppliedTurn::Waiting { question, conversation_revision: revision })
            }
        };
    }
}

fn parse_grounded_terminal(
    tools: &WorkspaceDeveloperTools,
    result: &DeveloperLoopOutcome,
) -> Result<TerminalTurn, ProductRunnerError> {
    tools.grounding().validate().map_err(|detail| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "ground developer turn in repository evidence",
            detail,
        )
    })?;
    terminal::parse(&result.text)
}

#[derive(Clone, Copy)]
struct DeveloperInvocation<'a> {
    role: &'a str,
    cycle: u32,
    invocation: u32,
}

struct InvocationContext<'a> {
    design: &'a str,
    findings: Option<&'a str>,
    correction: Option<&'a str>,
    ownership: &'a WorkspaceOwnership,
}

async fn run_selected_invocation(
    input: &ProductRunInput,
    providers: &mut crate::failover::ProviderCursor<'_>,
    identity: DeveloperInvocation<'_>,
    context: InvocationContext<'_>,
    accounting: &mut RunAccounting,
) -> Result<
    Option<(Result<DeveloperLoopOutcome, DeveloperLoopError>, WorkspaceDeveloperTools)>,
    ProductRunnerError,
> {
    let result = run_developer_invocation(
        input,
        providers.current(),
        identity,
        context.design,
        context.findings,
        context.correction,
        context.ownership,
    )
    .await;
    match result {
        Ok(result) => Ok(Some(result)),
        Err(error) if let Some(switch) = providers.advance_for_capability(&error) => {
            crate::failover::record_switch(
                input,
                identity.role,
                identity.cycle,
                accounting,
                switch,
            )?;
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

async fn run_developer_invocation(
    input: &ProductRunInput,
    model: &dyn ModelProvider,
    identity: DeveloperInvocation<'_>,
    design: &str,
    findings: Option<&str>,
    correction: Option<&str>,
    ownership: &WorkspaceOwnership,
) -> Result<
    (Result<DeveloperLoopOutcome, DeveloperLoopError>, WorkspaceDeveloperTools),
    ProductRunnerError,
> {
    let transcript = input.conversation.render();
    let media =
        crate::workspace_media::discover(&input.workspace_root, &transcript, model.profile())?;
    let prompt = writer_user(&transcript, design, findings, correction);
    let (prompt, attachments) = media.into_parts(prompt);
    let prefix = request_name(input.run_id, identity.role, identity.cycle);
    let request_prefix = format!("{prefix}-invocation-{}", identity.invocation);
    let mut tools = WorkspaceDeveloperTools::with_ownership(
        input.workspace_root.clone(),
        ownership.clone(),
        input.trace_path.with_extension("effects.bin"),
        request_prefix.clone(),
    );
    let mut trace = FileDeveloperTrace::new(input.trace_path.clone());
    let result = DeveloperLoop::run(
        model,
        DeveloperLoopRequest {
            request_prefix,
            system: writer_system(identity.role, input.delivery_scope),
            prompt,
            attachments,
            tools: definitions()?,
            limits: DeveloperLoopLimits::new(48, 512).map_err(|error| developer_error(&error))?,
            cancellation: input.provider_cancellation.clone(),
        },
        &mut tools,
        &mut trace,
    )
    .await;
    Ok((result, tools))
}

pub fn request_name(run_id: RunId, role: &str, cycle: u32) -> String {
    let mut value = String::from("peritus-");
    for byte in run_id.as_bytes() {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    format!("{value}-{role}-{cycle}")
}

pub fn reviewer_system() -> String {
    format!(
        "You are the independent D2 reviewer in a coding harness. Begin every review by requesting the declared read-only `workspace_list` host function through the model tool-call interface. After receiving that listing, request `workspace_search` and `workspace_read` as needed before reaching a verdict. Peritus executes these requests and returns their results on later turns; they are not provider-native tools. Use them to inspect the authoritative source inputs, exact changed files, current permission metadata, and relevant surrounding repository context. Do not rely on the writer's account of files you can inspect. Inspect the original conversation, exact diff, and exact-target gate evidence; the design is a proposal, not authority. When the caller explicitly authorizes external-effect delivery, evaluate the retained structured command observations against the requested effect: require a relevant successful action and a later fresh state or end-to-end verification, but do not invent a missing repository diff as a finding. Git diff modes distinguish executable from non-executable files and do not encode exact POSIX permissions such as 0600; use Peritus workspace metadata when exact permissions matter. Verify every explicit requested path, field, value, operation, and scoped rule against the result. When the request requires an output component to match a named source, treat the complete selected source value as that component and apply only transformations the request names; outside knowledge that labels part of the source as a tag, wrapper, metadata, artifact, or non-native content is not authority to delete it. Reject self-authored checks that merely prove the implementation agrees with its own interpretation. A non-advisory finding must identify an unmet explicit requirement, a failed deterministic gate, or a concrete contradiction. Do not replace one reasonable reading of a grammatically ambiguous compound phrase with another merely because you prefer a narrower scope. Unless another authoritative source or deterministic gate resolves that scope, preserve a candidate that satisfies a reasonable reading and report the ambiguity only as advisory; a blocking interpretation finding must show the candidate violates every reasonable reading. Do not settle whether a trailing modifier distributes over coordinated list items by assuming that distribution and then citing an earlier item's lack of the modifier's property; independently consider distributive and nearest-item attachments. Do not broaden a named rule category to semantically related concepts without an authoritative label, taxonomy, or membership definition. Treat optional richer traces, duplicated corroboration, and evidence-presentation improvements as advisory, never as reasons for repeated fixer cycles. Accept contemporaneous process metrics unless contradicted; do not rerun stateful external operations merely to reproduce one-shot transient failures. Return only one JSON object with this shape: {{\"summary\":\"...\",\"findings\":[{{\"category\":\"correctness|requested_behavior|build_coverage|test_coverage|security|maintainability|documentation\",\"severity\":\"advisory|low|medium|high|critical\",\"title\":\"stable concise identity\",\"description\":\"specific observed problem\",\"location\":\"path:line or empty\",\"reproduction\":\"exact evidence or command\",\"remediation\":\"specific required fix\"}}]}}. Do not return a blocking Boolean; policy derives blocker status from typed fields. Repeat every still-present finding using the same title and location. Omit a prior finding only after independently confirming its fix in the fresh diff and evidence. Do not invent obscure hypothetical threats or demand unrelated redesign. Do not use markdown fences.\n\n{}",
        crate::engineering_workflow::reviewer(),
    )
}

pub fn reviewer_user(
    transcript: &str,
    diff: &str,
    gates: &str,
    developer_evidence: &str,
    prior: &str,
    delivery_scope: super::ProductDeliveryScope,
    correction: Option<&str>,
) -> String {
    let correction = correction.map_or(String::new(), |value| {
        format!("\n\nHarness correction from the previous rejected review:\n{value}")
    });
    let delivery = match delivery_scope {
        super::ProductDeliveryScope::WorkspaceChanges => {
            "Delivery scope: exact workspace changes. An empty candidate cannot pass."
        }
        super::ProductDeliveryScope::AuthorizedExternalEffects => {
            "Delivery scope: caller-authorized external effects. When the candidate has no workspace changes, require relevant successful external_effect command evidence followed by successful fresh verification command evidence; do not require a synthetic file change."
        }
    };
    format!(
        "Conversation:\n{transcript}\n\n{delivery}\n\nCurrent diff:\n{diff}\n\nExact-target checks:\n{gates}\n\nDeveloper command observations:\n{developer_evidence}\n\nConserved finding history:\n{prior}\n\nBegin with a workspace_list host-function call, then independently inspect the authoritative workspace inputs and exact changed files through further read-only tool calls before returning the typed review. Developer command observations are real bounded process results: confirm that each claimed acceptance command exercises the explicit requested behavior and reject circular mocks, irrelevant success, or verification that predates the effect. For every conserved finding, read each cited current workspace file before repeating the finding; prior diff and finding text can predate fixer writes and do not prove that a defect remains.{correction}"
    )
}

fn writer_system(role: &str, delivery_scope: super::ProductDeliveryScope) -> String {
    let delivery = match delivery_scope {
        super::ProductDeliveryScope::WorkspaceChanges => {
            "This run accepts exact workspace changes. Label build, test, lint, and other inspection commands with purpose `verification`; external effects are not an alternate completion path."
        }
        super::ProductDeliveryScope::AuthorizedExternalEffects => {
            "The caller explicitly authorizes external-effect delivery. Within the requested external subject, attempt ordinary prerequisites needed for the result before asking the user again. This includes installing normal build or runtime dependencies when the task clearly requests software or system work in a disposable environment. First try the available scoped installation mechanism; escalate only after a concrete failure or when a material choice exceeds the request. Do not extend this authority to the user's durable host or to unrelated systems. If the requested result lives outside the workspace, label commands that perform the requested action with purpose `external_effect`, then run at least one fresh deterministic state inspection or end-to-end check labeled `verification`. Both successful forms are required; do not create a synthetic workspace file merely to produce a diff."
        }
    };
    format!(
        "You are the {role} developer in a production coding harness. Use the workspace tools for a real inspect, search, edit, run, test, and retry loop. Every fresh writer or fixer invocation starts with no repository-grounding credit: first call workspace_list, then workspace_read on at least one observed file, and read each existing target before changing it. Design text, prior-cycle reads, findings, and diff text do not replace these current-turn tool observations. Do not call workspace_write, workspace_patch, workspace_remove, or run_command before that sequence. {delivery} Harness-owned peritus-internal gates are unavailable as workspace commands and run independently after your turn. Make substantial maintainable changes and preserve unrelated work. Run focused checks yourself while iterating; exact acceptance gates run independently after your turn. Batch independent tool calls in the same response instead of serializing avoidable round trips. A successful workspace_write with changed=false means the requested content already matches; move on instead of repeating it. Use workspace_remove for an intentional regular file or listed empty directory; directory removal is non-recursive. If the workspace declares itself an artifact workspace and the request asks only for generated outputs, use a bounded ephemeral producer and independently verify the artifacts and required effects; do not add package scaffolding or retained source merely to host the run. Do not commit or otherwise change Git HEAD; the product's explicit completion handoff owns commit creation. Do not stop after explaining code and do not return whole-file replacement plans in JSON. When the implementation is ready for independent gates, return only {{\"kind\":\"complete\",\"summary\":\"what this task-level deliverable now does\",\"run_instructions\":\"exact command or concise steps for the user to run it\"}}. Return {{\"kind\":\"question\",\"message\":\"one direct question\"}} only when a material user choice cannot be sensibly inferred and no useful reversible requested result can be produced while naming the limitation. Do not invent obscure concerns.\n\n{}",
        crate::engineering_workflow::developer(),
    )
}

fn writer_user(
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

fn correction_prompt(error: &ProductRunnerError) -> String {
    format!(
        "The harness rejected the previous terminal response during {}: {}. Inspect the current workspace with `workspace_list` and targeted `workspace_read` calls, address the reported contract failure, and only then return the required terminal JSON. If no code change is needed, still ground that conclusion in the current repository and exact evidence.",
        error.operation(),
        error.detail(),
    )
}

fn question_confirmation_prompt(question: &str) -> String {
    format!(
        "The previous turn stopped without changing the workspace and asked: {question}\n\nThe harness independently confirms that this managed workspace is writable and that `workspace_write`, `workspace_patch`, `workspace_remove`, and `run_command` are available host functions even when the provider has no native filesystem tools. Re-ground with `workspace_list` and targeted `workspace_read`, then continue implementing with those host functions. If a material user choice still remains after using the available capabilities, return the same direct question unchanged; otherwise complete the requested work."
    )
}

fn retry_unverified_question(
    question: &str,
    workspace_unchanged: bool,
    pending_question: Option<&str>,
    unproductive_terminals: &mut u8,
) -> bool {
    if !workspace_unchanged || pending_question == Some(question) {
        return false;
    }
    *unproductive_terminals = unproductive_terminals.saturating_add(1);
    *unproductive_terminals < MAX_UNPRODUCTIVE_TERMINALS
}

pub fn developer_error(error: &DeveloperLoopError) -> ProductRunnerError {
    let kind = match error {
        DeveloperLoopError::Cancelled => ProductRunnerErrorKind::Cancelled,
        DeveloperLoopError::Trace(_) => ProductRunnerErrorKind::Repository,
        DeveloperLoopError::Tool(_) => ProductRunnerErrorKind::Apply,
        _ => ProductRunnerErrorKind::Provider,
    };
    ProductRunnerError::new(kind, "execute D0 developer loop", error.to_string())
}

#[cfg(test)]
mod tests;
