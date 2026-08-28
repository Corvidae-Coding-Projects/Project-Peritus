//! Tool-capable writer/fixer turns and independent reviewer prompts.

use peritus_agent::{DeveloperLoop, DeveloperLoopLimits, DeveloperLoopRequest};
use peritus_provider_core::ModelProvider;
use peritus_types::RunId;
use serde::Deserialize;

use crate::developer_tools::{WorkspaceDeveloperTools, definitions};
use crate::execution::{AppliedTurn, AppliedWrite, ProductRunInput, check_cancelled};
use crate::trace::FileDeveloperTrace;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

pub async fn complete_developer_turn(
    input: &ProductRunInput,
    model: &dyn ModelProvider,
    role: &str,
    cycle: u32,
    findings: Option<&str>,
) -> Result<AppliedTurn, ProductRunnerError> {
    loop {
        check_cancelled(input)?;
        let revision = input.conversation.revision();
        let transcript = input.conversation.render();
        let prompt = writer_user(&transcript, findings);
        let mut tools = WorkspaceDeveloperTools::new(input.workspace_root.clone());
        let mut trace = FileDeveloperTrace::new(input.trace_path.clone());
        let result = DeveloperLoop::run(
            model,
            DeveloperLoopRequest {
                request_prefix: request_name(input.run_id, role, cycle),
                system: writer_system(role),
                prompt,
                tools: definitions()?,
                limits: DeveloperLoopLimits::new(48, 512)
                    .map_err(|error| developer_error(&error))?,
                cancellation: input.provider_cancellation.clone(),
            },
            &mut tools,
            &mut trace,
        )
        .await
        .map_err(|error| developer_error(&error))?;
        check_cancelled(input)?;
        if input.conversation.revision() != revision {
            continue;
        }
        let terminal = parse_terminal(&result.text)?;
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
    "You are the independent D2 reviewer in a coding harness. Inspect the exact diff and exact-target gate evidence. Return only one JSON object with this shape: {\"summary\":\"...\",\"findings\":[{\"category\":\"correctness|requested_behavior|build_coverage|test_coverage|security|maintainability|documentation\",\"severity\":\"advisory|low|medium|high|critical\",\"title\":\"stable concise identity\",\"description\":\"specific observed problem\",\"location\":\"path:line or empty\",\"reproduction\":\"exact evidence or command\",\"remediation\":\"specific required fix\"}]}. Do not return a blocking Boolean; policy derives blocker status from typed fields. Repeat every still-present finding using the same title and location. Omit a prior finding only after independently confirming its fix in the fresh diff and evidence. Do not invent obscure hypothetical threats or demand unrelated redesign. Do not use markdown fences."
        .to_owned()
}

pub fn reviewer_user(transcript: &str, diff: &str, gates: &str, prior: &str) -> String {
    format!(
        "Conversation:\n{transcript}\n\nCurrent diff:\n{diff}\n\nExact-target checks:\n{gates}\n\nConserved finding history:\n{prior}"
    )
}

fn writer_system(role: &str) -> String {
    format!(
        "You are the {role} developer in a production coding harness. Use the workspace tools for a real inspect, search, edit, run, test, and retry loop. Read the repository before changing it. Make substantial maintainable changes and preserve unrelated work. Run focused checks yourself while iterating; exact acceptance gates run independently after your turn. Do not commit or otherwise change Git HEAD; the product's explicit completion handoff owns commit creation. Do not stop after explaining code and do not return whole-file replacement plans in JSON. When the implementation is ready for independent gates, return only {{\"kind\":\"complete\",\"summary\":\"what this task-level deliverable now does\",\"run_instructions\":\"exact command or concise steps for the user to run it\"}}. Only when a material user choice cannot be sensibly inferred, return only {{\"kind\":\"question\",\"message\":\"one direct question\"}}. Do not invent obscure concerns."
    )
}

fn writer_user(transcript: &str, findings: Option<&str>) -> String {
    let findings = findings.map_or(String::new(), |value| {
        format!("\n\nExact failed checks and conserved findings to address:\n{value}")
    });
    format!(
        "Conversation and task:\n{transcript}\n\nWork directly in the managed workspace using the available tools.{findings}"
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

fn developer_error(error: &peritus_agent::DeveloperLoopError) -> ProductRunnerError {
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
