//! Tool-capable writer/fixer turns and independent reviewer prompts.

use std::{sync::Arc, time::Duration};

use peritus_agent::{
    DeveloperLoopError, DeveloperLoopLimits, DeveloperLoopOutcome, DeveloperLoopRequest,
};
use peritus_provider_core::ModelProvider;
use peritus_types::RunId;

use crate::budget::RunAccounting;
use crate::developer_tools::{
    GroundingEvidence, WorkspaceDeveloperTools, WorkspaceOwnership, merge_rendered,
};
use crate::execution::CandidateRecorder;
use crate::execution::{
    AppliedTurn, AppliedWrite, HostTurnEvidence, ProductRunInput, check_cancelled,
};
use crate::local_context::LocalContextHandle;
use crate::{ProductRunnerError, ProductRunnerErrorKind, ProductRunnerFailureCause};

mod correction;
mod evidence;
mod provider;
mod request_name;
mod reviewer_prompt;
mod writer_prompt;
pub use reviewer_prompt::{ReviewDelivery, reviewer_system, reviewer_user, reviewer_user_with_catalog};
use writer_prompt::{writer_system, writer_user};
mod terminal;

pub use evidence::ReviewerPrompt;
use terminal::TerminalTurn;

pub fn request_name(run_id: RunId, role: &str, cycle: u32) -> String {
    request_name::format(run_id, role, cycle)
}

pub(crate) fn request_scope(run_id: RunId, role: &str) -> String {
    request_name::scope(run_id, role)
}

pub(crate) fn invocation_request_name(
    run_id: RunId,
    role: &str,
    cycle: u32,
    revision: u64,
    invocation: u64,
) -> Result<String, ProductRunnerError> {
    request_name::invocation(run_id, role, cycle, revision, invocation)
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
    let memory = input.working_memory_async(role).await?;
    let mut checkpoint = input.checkpoint()?;
    let mut grounding_revision = input.conversation.revision();
    let grounding_scope = request_scope(input.run_id, role);
    let mut grounding = memory
        .as_ref()
        .map(|memory| memory.recover_grounding_scope(&grounding_scope, grounding_revision))
        .transpose()
        .map_err(|error| developer_error(&error))?
        .unwrap_or_default();
    let mut invocation = 0_u64;
    let mut segment = 0_u64;
    let mut reuse_invocation = false;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut host = HostTurnEvidence {
        tool_calls: 0,
        conversation_revision: grounding_revision,
        verification_evidence: String::new(),
        successful_commands: Vec::new(),
    };
    let (mut correction, mut pending_question) = (None, None);
    loop {
        if let Err(error) = check_cancelled(input) {
            return Ok(AppliedTurn::Rejected { error, host });
        }
        if reuse_invocation {
            reuse_invocation = false;
        } else {
            invocation = invocation
                .checked_add(1)
                .ok_or_else(|| developer_error(&DeveloperLoopError::LimitExceeded))?;
            segment = 0;
        }
        let revision = input.conversation.revision();
        if revision != grounding_revision {
            grounding.clear_repository_evidence();
            grounding_revision = revision;
        }
        let identity = DeveloperInvocation { role, cycle, invocation, segment };
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
                grounding: &grounding,
            },
            accounting,
        )
        .await;
        let Some((result, tools, executed_segment)) = (match selected {
            Ok(selected) => selected,
            Err(error) => return Ok(AppliedTurn::Rejected { error, host }),
        }) else {
            continue;
        };
        let retained_grounding = tools.grounding().clone();
        *ownership = tools.ownership().clone();
        merge_rendered(&mut host.verification_evidence, &tools.verification_evidence());
        crate::developer_tools::merge_successful(
            &mut host.successful_commands,
            &tools.successful_commands(),
        );
        if let Ok(outcome) = &result {
            host.tool_calls = host
                .tool_calls
                .checked_add(outcome.tool_calls)
                .ok_or_else(|| developer_error(&DeveloperLoopError::LimitExceeded))?;
        }
        if let Err(error) = accounting.check() {
            return Ok(AppliedTurn::Rejected { error, host });
        }
        if let Err(error) = check_cancelled(input) {
            return Ok(AppliedTurn::Rejected { error, host });
        }
        if let Err(error) = &result
            && crate::failover::requires_reconciliation_before_new_request(error)
            && input.conversation.revision() != revision
        {
            return Ok(AppliedTurn::Rejected { error: developer_error(error), host });
        }
        if input.conversation.revision() != revision {
            checkpoint = match input.checkpoint() {
                Ok(checkpoint) => checkpoint,
                Err(error) => return Ok(AppliedTurn::Rejected { error, host }),
            };
            provider_recovery.reset();
            providers.reopen();
            (correction, pending_question) = (None, None);
            grounding = retained_grounding;
            grounding.clear_repository_evidence();
            grounding_revision = input.conversation.revision();
            host.conversation_revision = grounding_revision;
            host.successful_commands.clear();
            merge_rendered(
                &mut host.verification_evidence,
                "Requirements changed during the turn; earlier command observations remain historical and are not current acceptance evidence.",
            );
            continue;
        }
        grounding = retained_grounding;
        let segment_continuation = matches!(&result, Err(DeveloperLoopError::SegmentContinuation));
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
            if segment_continuation {
                reuse_invocation = true;
                segment = executed_segment
                    .checked_add(1)
                    .ok_or_else(|| developer_error(&DeveloperLoopError::LimitExceeded))?;
            }
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
    invocation: u64,
    segment: u64,
}

struct InvocationContext<'a> {
    memory: Option<&'a LocalContextHandle>,
    design: &'a str,
    findings: Option<&'a str>,
    correction: Option<&'a str>,
    ownership: &'a WorkspaceOwnership,
    remaining: Option<Duration>,
    recorder: &'a CandidateRecorder,
    grounding: &'a GroundingEvidence,
}

async fn run_selected_invocation(
    input: &ProductRunInput,
    providers: &mut crate::failover::ProviderCursor<'_>,
    identity: DeveloperInvocation<'_>,
    context: InvocationContext<'_>,
    accounting: &mut RunAccounting,
) -> Result<
    Option<(
        Result<DeveloperLoopOutcome, DeveloperLoopError>,
        WorkspaceDeveloperTools,
        u64,
    )>,
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
    (
        Result<DeveloperLoopOutcome, DeveloperLoopError>,
        WorkspaceDeveloperTools,
        u64,
    ),
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
    let revision = input.conversation.revision();
    let logical_prefix = format!(
        "{}-revision-{revision}-invocation-",
        request_name(input.run_id, identity.role, identity.cycle),
    );
    let reopened = context
        .memory
        .map(|memory| memory.pending_developer_reentry(&logical_prefix))
        .transpose()
        .map_err(|error| developer_error(&error))?
        .flatten();
    let (request_prefix, segment) = match reopened {
        Some((prefix, segment)) => (prefix, segment),
        None => (
            invocation_request_name(
                input.run_id,
                identity.role,
                identity.cycle,
                revision,
                identity.invocation,
            )?,
            identity.segment,
        ),
    };
    let limits = DeveloperLoopLimits::new(48, 512)
        .map_err(|error| developer_error(&error))?
        .with_segment_continuation()
        .with_segment_sequence(segment);
    let execution_prefix = limits.segment_request_prefix(&request_prefix);
    let mut tools = input.configure_tools(
        WorkspaceDeveloperTools::with_ownership(
            input.workspace_root.clone(),
            context.ownership.clone(),
            input.trace_path.with_extension("effects.bin"),
            execution_prefix,
            context.remaining,
            input.command_runtime.clone(),
        )
        .with_grounding(context.grounding.clone())
        .with_checkpoint_observer(context.recorder.tool_observer(Arc::clone(&input.conversation)))
        .with_checkpoint_view(Arc::clone(&input.conversation))
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
            limits,
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
    Ok((result, tools, segment))
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
        DeveloperLoopError::SegmentContinuation => ProductRunnerErrorKind::InternalInvariant,
        DeveloperLoopError::Trace(_) => ProductRunnerErrorKind::Repository,
        DeveloperLoopError::Tool(_) | DeveloperLoopError::RecoveryRequired(_) => {
            ProductRunnerErrorKind::Apply
        }
        _ => ProductRunnerErrorKind::Provider,
    };
    let result = ProductRunnerError::new(kind, "execute D0 developer loop", error.to_string());
    match error {
        DeveloperLoopError::Context(_) => {
            result.with_failure_cause(ProductRunnerFailureCause::ContextPreparation)
        }
        DeveloperLoopError::ProviderFailure(failure) => {
            result.with_provider_failure(failure.clone())
        }
        _ => result,
    }
}

#[cfg(test)]
mod tests;
