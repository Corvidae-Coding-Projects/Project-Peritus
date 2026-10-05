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
use crate::execution::{
    AppliedTurn, AppliedWrite, HostTurnEvidence, ProductRunInput, check_cancelled,
};
use crate::local_context::LocalContextHandle;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

mod correction;
mod evidence;
mod provider;
mod request_name;
mod reviewer_prompt;
mod writer_prompt;
pub use reviewer_prompt::{ReviewDelivery, reviewer_system, reviewer_user};
use writer_prompt::{writer_system, writer_user};
mod terminal;

pub use evidence::ReviewerPrompt;
use terminal::TerminalTurn;

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
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut host = HostTurnEvidence {
        tool_calls: 0,
        conversation_revision: input.conversation.revision(),
        verification_evidence: String::new(),
        successful_commands: Vec::new(),
    };
    let (mut correction, mut pending_question) = (None, None);
    loop {
        if let Err(error) = check_cancelled(input) {
            return Ok(AppliedTurn::Rejected { error, host });
        }
        if let Err(error) =
            crate::failover::bypass_open_circuit(input, role, cycle, accounting, &mut providers)
        {
            return Ok(AppliedTurn::Rejected { error, host });
        }
        invocation = invocation.saturating_add(1);
        let revision = input.conversation.revision();
        let identity = DeveloperInvocation { role, cycle, invocation };
        let remaining = accounting.remaining();
        let selected = run_selected_invocation(
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
        .await;
        let Some((result, tools)) = (match selected {
            Ok(selected) => selected,
            Err(error) => return Ok(AppliedTurn::Rejected { error, host }),
        }) else {
            continue;
        };
        *ownership = tools.ownership().clone();
        merge_rendered(&mut host.verification_evidence, &tools.verification_evidence());
        crate::developer_tools::merge_successful(
            &mut host.successful_commands,
            &tools.successful_commands(),
        );
        if let Ok(outcome) = &result {
            host.tool_calls = host.tool_calls.saturating_add(outcome.tool_calls);
        }
        if let Err(error) = accounting.check() {
            return Ok(AppliedTurn::Rejected { error, host });
        }
        if let Err(error) = check_cancelled(input) {
            return Ok(AppliedTurn::Rejected { error, host });
        }
        if input.conversation.revision() != revision {
            checkpoint = match input.checkpoint() {
                Ok(checkpoint) => checkpoint,
                Err(error) => return Ok(AppliedTurn::Rejected { error, host }),
            };
            provider_recovery.reset();
            (correction, pending_question) = (None, None);
            host.conversation_revision = input.conversation.revision();
            host.successful_commands.clear();
            merge_rendered(
                &mut host.verification_evidence,
                "Requirements changed during the turn; earlier command observations remain historical and are not current acceptance evidence.",
            );
            continue;
        }
        let resolution = match provider::resolve(
            input,
            &mut providers,
            identity,
            result,
            &mut checkpoint,
            &mut provider_recovery,
            accounting,
        ) {
            Ok(resolution) => resolution,
            Err(error) => return Ok(AppliedTurn::Rejected { error, host }),
        };
        let Some(result) = provider::apply(resolution, &mut correction, &mut pending_question)
        else {
            continue;
        };
        let terminal = match parse_grounded_terminal(&tools, &result).and_then(|terminal| {
            terminal::validate_run_instructions(input.delivery_scope, terminal)
        }) {
            Ok(terminal) => terminal,
            Err(error) => {
                let current = match input.checkpoint() {
                    Ok(current) => current,
                    Err(checkpoint_error) => {
                        return Ok(AppliedTurn::Rejected { error: checkpoint_error, host });
                    }
                };
                if current != checkpoint {
                    checkpoint = current;
                }
                correction = Some(correction::rejected_terminal(&error));
                pending_question = None;
                accounting.record_role_retry()?;
                continue;
            }
        };
        return match terminal {
            TerminalTurn::Complete(summary) => Ok(AppliedTurn::Applied(AppliedWrite {
                summary: summary.0,
                run_instructions: summary.1,
                tool_calls: host.tool_calls,
                conversation_revision: revision,
                verification_evidence: host.verification_evidence,
                successful_commands: host.successful_commands,
            })),
            TerminalTurn::Question(question) => {
                let current = match input.checkpoint() {
                    Ok(current) => current,
                    Err(error) => return Ok(AppliedTurn::Rejected { error, host }),
                };
                if retry_unverified_question(
                    &question,
                    current == checkpoint,
                    pending_question.as_deref(),
                ) {
                    correction = Some(correction::unverified_question(&question));
                    pending_question = Some(question);
                    continue;
                }
                Ok(AppliedTurn::Waiting { question, conversation_revision: revision, host })
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
    remaining: Option<Duration>,
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
    let request_prefix = request_name::invocation(
        input.run_id,
        identity.role,
        identity.cycle,
        input.conversation.revision(),
        identity.invocation,
    )?;
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
            local_session_directory: Some(input.native_session_directory(identity.role)),
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

fn retry_unverified_question(
    question: &str,
    workspace_unchanged: bool,
    pending_question: Option<&str>,
) -> bool {
    workspace_unchanged && pending_question != Some(question)
}

pub fn developer_error(error: &DeveloperLoopError) -> ProductRunnerError {
    let kind = match error {
        DeveloperLoopError::Cancelled => ProductRunnerErrorKind::Cancelled,
        DeveloperLoopError::LimitExceeded | DeveloperLoopError::SegmentExhausted => {
            ProductRunnerErrorKind::Budget
        }
        DeveloperLoopError::Trace(_) => ProductRunnerErrorKind::Repository,
        DeveloperLoopError::Tool(_) | DeveloperLoopError::RecoveryRequired(_) => {
            ProductRunnerErrorKind::Apply
        }
        _ => ProductRunnerErrorKind::Provider,
    };
    ProductRunnerError::new(kind, "execute D0 developer loop", error.to_string())
}

#[cfg(test)]
mod tests;
