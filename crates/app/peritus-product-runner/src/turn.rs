//! Tool-capable writer/fixer turns and independent reviewer prompts.

use std::{sync::Arc, time::Duration};

use peritus_agent::{
    DeveloperLoopError, DeveloperLoopLimits, DeveloperLoopOutcome, DeveloperLoopRequest,
};
use peritus_provider_core::ModelProvider;
use peritus_types::RunId;

use crate::budget::RunAccounting;
use crate::developer_tools::{WorkspaceDeveloperTools, WorkspaceOwnership, merge_rendered};
use crate::execution::CandidateRecorder;
use crate::execution::{AppliedTurn, AppliedWrite, ProductRunInput, check_cancelled};
use crate::local_context::LocalContextHandle;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

mod correction;
mod evidence;
mod provider;
mod request_name;
mod reviewer_prompt;
pub use reviewer_prompt::{ReviewDelivery, reviewer_system, reviewer_user};
mod terminal;

pub use evidence::ReviewerPrompt;
use terminal::TerminalTurn;
const MAX_UNPRODUCTIVE_TERMINALS: u8 = 3;

pub fn request_name(run_id: RunId, role: &str, cycle: u32) -> String {
    request_name::format(run_id, role, cycle)
}

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
    recorder: &CandidateRecorder,
) -> Result<AppliedTurn, ProductRunnerError> {
    let mut providers = crate::failover::ProviderCursor::new(primary, &input.providers.fallbacks);
    let memory = input.working_memory(role)?;
    let mut checkpoint = input.checkpoint()?;
    let mut invocation = 0_u32;
    let mut unproductive_terminals = 0_u8;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut verification_evidence = String::new();
    let mut successful_commands = Vec::new();
    let (mut correction, mut pending_question) = (None, None);
    loop {
        check_cancelled(input)?;
        crate::failover::bypass_open_circuit(input, role, cycle, accounting, &mut providers)?;
        invocation = invocation.saturating_add(1);
        let revision = input.conversation.revision();
        let identity = DeveloperInvocation { role, cycle, invocation };
        let remaining = accounting.remaining();
        let Some((result, tools)) = run_selected_invocation(
            input,
            &mut providers,
            identity,
            InvocationContext {
                design,
                findings,
                correction: correction.as_deref(),
                ownership,
                remaining,
                recorder,
                memory: memory.as_ref(),
            },
            accounting,
        )
        .await?
        else {
            continue;
        };
        accounting.check()?;
        *ownership = tools.ownership().clone();
        merge_rendered(&mut verification_evidence, &tools.verification_evidence());
        crate::developer_tools::merge_successful(
            &mut successful_commands,
            &tools.successful_commands(),
        );
        check_cancelled(input)?;
        if input.conversation.revision() != revision {
            checkpoint = input.checkpoint()?;
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
        let terminal = match parse_grounded_terminal(&tools, &result).and_then(|terminal| {
            terminal::validate_run_instructions(input.delivery_scope, terminal)
        }) {
            Ok(terminal) => terminal,
            Err(error) => {
                let current = input.checkpoint()?;
                if current != checkpoint {
                    checkpoint = current;
                    unproductive_terminals = 0;
                    (correction, pending_question) = (None, None);
                    continue;
                }
                unproductive_terminals = unproductive_terminals.saturating_add(1);
                if unproductive_terminals < MAX_UNPRODUCTIVE_TERMINALS {
                    correction = Some(correction::rejected_terminal(&error));
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
                let current = input.checkpoint()?;
                if retry_unverified_question(
                    &question,
                    current == checkpoint,
                    pending_question.as_deref(),
                    &mut unproductive_terminals,
                ) {
                    correction = Some(correction::unverified_question(&question));
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
    memory: Option<&'a LocalContextHandle>,
    design: &'a str,
    findings: Option<&'a str>,
    correction: Option<&'a str>,
    ownership: &'a WorkspaceOwnership,
    remaining: Duration,
    recorder: &'a CandidateRecorder,
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
    let result =
        run_developer_invocation(input, providers.current(), identity, context, accounting).await;
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
    context: InvocationContext<'_>,
    accounting: &mut RunAccounting,
) -> Result<
    (Result<DeveloperLoopOutcome, DeveloperLoopError>, WorkspaceDeveloperTools),
    ProductRunnerError,
> {
    let transcript = input.conversation.render();
    let media = input.media(&transcript, model.profile())?;
    let prompt = writer_user(
        &input.conversation.stable_request_context(),
        context.design,
        context.findings,
        context.correction,
    );
    let (prompt, attachments) = media.into_parts(prompt);
    let prefix = request_name(input.run_id, identity.role, identity.cycle);
    let request_prefix = format!(
        "{prefix}-revision-{}-invocation-{}",
        input.conversation.revision(),
        identity.invocation,
    );
    let mut tools = input.configure_tools(
        WorkspaceDeveloperTools::with_ownership(
            input.workspace_root.clone(),
            context.ownership.clone(),
            input.trace_path.with_extension("effects.bin"),
            request_prefix.clone(),
            context.remaining,
            input.command_runtime.clone(),
        )
        .with_checkpoint_observer(context.recorder.tool_observer(Arc::clone(&input.conversation)))
        .with_task_contract(&transcript),
    );
    let result = crate::local_context::run_live_invocation(
        model,
        DeveloperLoopRequest {
            request_prefix,
            system: writer_system(
                identity.role,
                input.delivery_scope,
                crate::delivery_requirement::ExternalEffectRequirement::from_task(
                    input.delivery_scope,
                    &input.task,
                ),
                context.remaining,
            ) + input.delivery_instructions(),
            prompt,
            attachments,
            tools: input.developer_definitions()?,
            limits: DeveloperLoopLimits::new(48, 512).map_err(|error| developer_error(&error))?,
            cancellation: input.provider_cancellation.clone(),
        },
        &mut tools,
        crate::local_context::InvocationAccounting { trace_path: &input.trace_path, accounting },
        context.memory,
        input.conversation.interaction(),
        if identity.role == "fixer" {
            peritus_agent::DeveloperModelRole::Fixer
        } else {
            peritus_agent::DeveloperModelRole::Writer
        },
    )
    .await;
    Ok((result, tools))
}

fn writer_system(
    role: &str,
    delivery_scope: super::ProductDeliveryScope,
    effect_requirement: crate::delivery_requirement::ExternalEffectRequirement,
    remaining: Duration,
) -> String {
    let delivery = match delivery_scope {
        super::ProductDeliveryScope::WorkspaceChanges => {
            "This run accepts exact workspace changes. Label build, test, lint, and other inspection commands with purpose `verification`; external effects are not an alternate completion path. The workspace_list result gives the exact workspace_root and declares workspace tool paths to be relative to it. When the task names an absolute path below that exact root, remove the root prefix once; never repeat the root directory inside itself. For `run_instructions`, return exactly one direct command such as `cargo run --quiet`: use whitespace-separated executable and arguments only, with no prose, Markdown, quotes, environment assignments, redirections, expansions, or shell operators."
        }
        super::ProductDeliveryScope::AuthorizedExternalEffects => {
            if effect_requirement.is_required() {
                "The caller explicitly authorizes external-effect delivery, and the original operational imperative requires the live configured result even when supporting workspace files change. A setup script, README, or instructions alone cannot complete this request. Within the requested external subject, attempt ordinary prerequisites needed for the result before asking the user again. This includes installing normal build or runtime dependencies when the task clearly requests software or system work in a disposable environment. First try the available scoped installation mechanism; escalate only after a concrete failure or when a material choice exceeds the request. Do not extend this authority to the user's durable host or to unrelated systems. Label commands that perform the requested action with purpose `external_effect`, then run at least one fresh deterministic state inspection or end-to-end check labeled `verification`. Both successful forms are required. The workspace_list result gives the exact workspace_root and declares workspace tool paths to be relative to it. When the task names an absolute path below that exact root, remove the root prefix once; never repeat the root directory inside itself."
            } else {
                "The caller explicitly authorizes external-effect delivery. Within the requested external subject, attempt ordinary prerequisites needed for the result before asking the user again. This includes installing normal build or runtime dependencies when the task clearly requests software or system work in a disposable environment. First try the available scoped installation mechanism; escalate only after a concrete failure or when a material choice exceeds the request. Do not extend this authority to the user's durable host or to unrelated systems. If the requested result lives outside the workspace, label commands that perform the requested action with purpose `external_effect`, then run at least one fresh deterministic state inspection or end-to-end check labeled `verification`. Both successful forms are required; do not create a synthetic workspace file merely to produce a diff. The workspace_list result gives the exact workspace_root and declares workspace tool paths to be relative to it. When the task names an absolute path below that exact root, remove the root prefix once; never repeat the root directory inside itself."
            }
        }
    };
    let instructions = format!(
        "You are the {role} developer in a production coding harness. Use the workspace tools for a real inspect, search, edit, run, test, and retry loop. Every fresh writer or fixer invocation starts with no repository-grounding credit (one invocation spans multiple provider requests): first call workspace_list, then workspace_read on at least one observed file, and read each existing target before changing an existing file. Design text, prior-cycle reads, findings, and diff text do not replace tool observations from this host invocation. Treat workspace_list.execution_resources as the authoritative command envelope and keep build/test worker counts at or below its recommended_parallelism. Do not call workspace_write, workspace_patch, workspace_remove, run_command, or command_start before that grounding sequence. Use run_command for ordinary finite commands. Use command_start plus command_poll and the handle-based stdin, resize, signal, cancel, or recover tools only for interactive or genuinely long-lived commands. {delivery} Harness-owned peritus-internal gates are unavailable as workspace commands and run independently after your turn. Make substantial maintainable changes and preserve unrelated work. Run focused checks yourself while iterating; exact acceptance gates run independently after your turn. Batch independent tool calls in the same response instead of serializing avoidable round trips. A successful workspace_write with changed=false means the requested content already matches; move on instead of repeating it. Use workspace_remove for an intentional regular file or listed empty directory; directory removal is non-recursive. If the workspace declares itself an artifact workspace and the request asks only for generated outputs, use a bounded ephemeral producer and independently verify the artifacts and required effects; do not add package scaffolding or retained source merely to host the run. Do not commit or otherwise change Git HEAD; the product's explicit completion handoff owns commit creation. Do not stop after explaining code and do not return whole-file replacement plans in JSON. When the implementation is ready for independent gates, return only {{\"kind\":\"complete\",\"summary\":\"what this task-level deliverable now does\",\"run_instructions\":\"exact command or concise steps for the user to run it\"}}. Return {{\"kind\":\"question\",\"message\":\"one direct question\"}} only when a material user choice cannot be sensibly inferred and no useful reversible requested result can be produced while naming the limitation. Do not invent obscure concerns.\n\n{}",
        crate::engineering_workflow::developer(),
    );
    format!(
        "This role begins with approximately {} seconds left in the caller's product-run window, shared with deterministic gates, independent review, and any required fix. Use the available time for substantial work, but stop open-ended exploration or optimization early enough to return the strongest tested candidate for those downstream phases.\n\n{instructions}",
        remaining.as_secs()
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
