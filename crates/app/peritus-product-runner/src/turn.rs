//! Tool-capable writer/fixer turns and independent reviewer prompts.

use peritus_agent::{DeveloperLoop, DeveloperLoopLimits, DeveloperLoopRequest};
use peritus_provider_core::ModelProvider;
use peritus_types::RunId;
use serde::Deserialize;

use crate::developer_tools::{WorkspaceDeveloperTools, WorkspaceOwnership, definitions};
use crate::execution::{AppliedTurn, AppliedWrite, ProductRunInput, check_cancelled};
use crate::progress::WorkspaceCheckpoint;
use crate::trace::FileDeveloperTrace;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAX_UNPRODUCTIVE_TERMINALS: u8 = 3;

pub async fn complete_developer_turn(
    input: &ProductRunInput,
    model: &dyn ModelProvider,
    role: &str,
    cycle: u32,
    design: &str,
    findings: Option<&str>,
    ownership: &mut WorkspaceOwnership,
) -> Result<AppliedTurn, ProductRunnerError> {
    let mut checkpoint = WorkspaceCheckpoint::capture(&input.workspace_root)?;
    let mut invocation = 0_u32;
    let mut unproductive_terminals = 0_u8;
    let mut correction = None;
    loop {
        check_cancelled(input)?;
        invocation = invocation.saturating_add(1);
        let revision = input.conversation.revision();
        let transcript = input.conversation.render();
        let media =
            crate::workspace_media::discover(&input.workspace_root, &transcript, model.profile())?;
        let prompt = writer_user(&transcript, design, findings, correction.as_deref());
        let (prompt, attachments) = media.into_parts(prompt);
        let mut tools = WorkspaceDeveloperTools::with_ownership(
            input.workspace_root.clone(),
            ownership.clone(),
        );
        let mut trace = FileDeveloperTrace::new(input.trace_path.clone());
        let prefix = request_name(input.run_id, role, cycle);
        let result = DeveloperLoop::run(
            model,
            DeveloperLoopRequest {
                request_prefix: format!("{prefix}-invocation-{invocation}"),
                system: writer_system(role),
                prompt,
                attachments,
                tools: definitions()?,
                limits: DeveloperLoopLimits::new(48, 512)
                    .map_err(|error| developer_error(&error))?,
                cancellation: input.provider_cancellation.clone(),
            },
            &mut tools,
            &mut trace,
        )
        .await;
        *ownership = tools.ownership().clone();
        check_cancelled(input)?;
        if input.conversation.revision() != revision {
            checkpoint = WorkspaceCheckpoint::capture(&input.workspace_root)?;
            unproductive_terminals = 0;
            correction = None;
            continue;
        }
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let current = WorkspaceCheckpoint::capture(&input.workspace_root)?;
                if current != checkpoint {
                    checkpoint = current;
                    unproductive_terminals = 0;
                    correction = None;
                    continue;
                }
                return Err(developer_error(&error));
            }
        };
        let grounded = tools.grounding().validate().map_err(|detail| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidModelOutput,
                "ground developer turn in repository evidence",
                detail,
            )
        });
        let terminal = grounded.and_then(|()| parse_terminal(&result.text));
        let terminal = match terminal {
            Ok(terminal) => terminal,
            Err(error) => {
                let current = WorkspaceCheckpoint::capture(&input.workspace_root)?;
                if current != checkpoint {
                    checkpoint = current;
                    unproductive_terminals = 0;
                    correction = None;
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
            })),
            TerminalTurn::Question(question) => {
                Ok(AppliedTurn::Waiting { question, conversation_revision: revision })
            }
        };
    }
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
        "You are the independent D2 reviewer in a coding harness. Inspect the original conversation, exact diff, and exact-target gate evidence; the design is a proposal, not authority. Verify every explicit requested path, field, value, operation, and scoped rule against the result. Reject self-authored checks that merely prove the implementation agrees with its own interpretation. A non-advisory finding must identify an unmet explicit requirement, a failed deterministic gate, or a concrete contradiction. Treat optional richer traces, duplicated corroboration, and evidence-presentation improvements as advisory, never as reasons for repeated fixer cycles. Accept contemporaneous process metrics unless contradicted; do not rerun stateful external operations merely to reproduce one-shot transient failures. Return only one JSON object with this shape: {{\"summary\":\"...\",\"findings\":[{{\"category\":\"correctness|requested_behavior|build_coverage|test_coverage|security|maintainability|documentation\",\"severity\":\"advisory|low|medium|high|critical\",\"title\":\"stable concise identity\",\"description\":\"specific observed problem\",\"location\":\"path:line or empty\",\"reproduction\":\"exact evidence or command\",\"remediation\":\"specific required fix\"}}]}}. Do not return a blocking Boolean; policy derives blocker status from typed fields. Repeat every still-present finding using the same title and location. Omit a prior finding only after independently confirming its fix in the fresh diff and evidence. Do not invent obscure hypothetical threats or demand unrelated redesign. Do not use markdown fences.\n\n{}",
        crate::engineering_workflow::reviewer(),
    )
}

pub fn reviewer_user(transcript: &str, diff: &str, gates: &str, prior: &str) -> String {
    format!(
        "Conversation:\n{transcript}\n\nCurrent diff:\n{diff}\n\nExact-target checks:\n{gates}\n\nConserved finding history:\n{prior}"
    )
}

fn writer_system(role: &str) -> String {
    format!(
        "You are the {role} developer in a production coding harness. Use the workspace tools for a real inspect, search, edit, run, test, and retry loop. Read the repository before changing it. Make substantial maintainable changes and preserve unrelated work. Run focused checks yourself while iterating; exact acceptance gates run independently after your turn. Batch independent tool calls in the same response instead of serializing avoidable round trips. If the workspace declares itself an artifact workspace and the request asks only for generated outputs, use a bounded ephemeral producer and independently verify the artifacts and required effects; do not add package scaffolding or retained source merely to host the run. Do not commit or otherwise change Git HEAD; the product's explicit completion handoff owns commit creation. Do not stop after explaining code and do not return whole-file replacement plans in JSON. When the implementation is ready for independent gates, return only {{\"kind\":\"complete\",\"summary\":\"what this task-level deliverable now does\",\"run_instructions\":\"exact command or concise steps for the user to run it\"}}. Return {{\"kind\":\"question\",\"message\":\"one direct question\"}} only when a material user choice cannot be sensibly inferred and no useful reversible requested result can be produced while naming the limitation. Do not invent obscure concerns.\n\n{}",
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalWire {
    kind: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    run_instructions: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

enum TerminalTurn {
    Complete((String, String)),
    Question(String),
}

fn parse_terminal(value: &str) -> Result<TerminalTurn, ProductRunnerError> {
    let start = value.find('{').ok_or_else(|| invalid("developer response contains no JSON"))?;
    let end = value.rfind('}').ok_or_else(|| invalid("developer response has incomplete JSON"))?;
    let wire: TerminalWire = serde_json::from_str(&value[start..=end]).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "parse developer terminal",
            error.to_string(),
        )
    })?;
    match (wire.kind.as_str(), wire.summary, wire.run_instructions, wire.message) {
        ("complete", Some(summary), Some(run_instructions), None)
            if !summary.trim().is_empty() && !run_instructions.trim().is_empty() =>
        {
            Ok(TerminalTurn::Complete((summary, run_instructions)))
        }
        ("question", None, None, Some(message)) if !message.trim().is_empty() => {
            Ok(TerminalTurn::Question(message))
        }
        _ => Err(invalid("developer terminal fields do not match its kind")),
    }
}

pub fn developer_error(error: &peritus_agent::DeveloperLoopError) -> ProductRunnerError {
    let kind = match error {
        peritus_agent::DeveloperLoopError::Cancelled => ProductRunnerErrorKind::Cancelled,
        peritus_agent::DeveloperLoopError::Trace(_) => ProductRunnerErrorKind::Repository,
        peritus_agent::DeveloperLoopError::Tool(_) => ProductRunnerErrorKind::Apply,
        _ => ProductRunnerErrorKind::Provider,
    };
    ProductRunnerError::new(kind, "execute D0 developer loop", error.to_string())
}

fn invalid(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "validate developer terminal",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_terminal_correction_requires_fresh_repository_grounding() {
        let error = ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "ground developer turn in repository evidence",
            "repository grounding requires a successful workspace listing",
        );
        let correction = correction_prompt(&error);
        let prompt = writer_user("task", "design", Some("finding"), Some(&correction));

        assert!(prompt.contains("Harness correction from the previous rejected turn"));
        assert!(prompt.contains("workspace_list"));
        assert!(prompt.contains("workspace_read"));
        assert!(prompt.contains("If no code change is needed"));
        assert!(prompt.contains(error.detail()));
    }

    #[test]
    fn reviewer_checks_literal_request_independently_of_the_design() {
        let prompt = reviewer_system();
        assert!(prompt.contains("design is a proposal, not authority"));
        assert!(prompt.contains("every explicit requested path, field, value"));
        assert!(prompt.contains("agrees with its own interpretation"));
        assert!(prompt.contains("close a non-exhaustive example"));
        assert!(prompt.contains("reverse declared source precedence"));
        assert!(prompt.contains("demotes a matching superseding rule"));
        assert!(prompt.contains("non-advisory finding"));
        assert!(prompt.contains("one-shot transient failures"));
        assert!(prompt.contains("never as reasons for repeated fixer cycles"));
        assert!(prompt.contains("blocking compatibility failure"));
        assert!(prompt.contains("Legitimate mocks for unrelated boundaries"));
    }

    #[test]
    fn writer_batches_tools_and_respects_artifact_workspaces() {
        let prompt = writer_system("writer");
        assert!(prompt.contains("Batch independent tool calls"));
        assert!(prompt.contains("bounded ephemeral producer"));
        assert!(prompt.contains("do not add package scaffolding"));
        assert!(prompt.contains("invented allowlist"));
        assert!(prompt.contains("preserve that precedence"));
        assert!(prompt.contains("owns the primary field"));
        assert!(prompt.contains("opaque contract values"));
        assert!(prompt.contains("no useful reversible requested result"));
        assert!(prompt.contains("real declared dependency"));
        assert!(prompt.contains("Never make tests pass by injecting a substitute"));
        assert!(prompt.contains("same-workload baseline"));
        assert!(prompt.contains("use profiling when the cause is not already evident"));
    }
}
