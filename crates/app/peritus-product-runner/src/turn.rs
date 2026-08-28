//! Conversational writer/fixer model turns and bounded JSON correction.

use peritus_provider_core::ModelProvider;
use peritus_types::RunId;

use crate::execution::{AppliedTurn, AppliedWrite, ProductRunInput, check_cancelled};
use crate::{ProductRunnerError, ProductRunnerErrorKind, bundle, plan, provider};

pub async fn complete_plan(
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
        let context = bundle::build(&input.workspace_root, &transcript)?;
        let user = writer_user(&transcript, &context.prompt, findings);
        let response = provider::complete(
            model,
            request_name(input.run_id, role, cycle),
            writer_system(),
            user,
            input.provider_cancellation.clone(),
        )
        .await?;
        check_cancelled(input)?;
        if input.conversation.revision() != revision {
            continue;
        }
        let parsed = match plan::parse(&response) {
            Ok(parsed) => parsed,
            Err(error) if error.kind() == ProductRunnerErrorKind::InvalidModelOutput => {
                let repaired = provider::complete(
                    model,
                    request_name(input.run_id, &format!("{role}-json-repair"), cycle),
                    writer_system(),
                    repair_user(&response, error.detail()),
                    input.provider_cancellation.clone(),
                )
                .await?;
                check_cancelled(input)?;
                if input.conversation.revision() != revision {
                    continue;
                }
                plan::parse(&repaired)?
            }
            Err(error) => return Err(error),
        };
        return match parsed {
            plan::ParsedPlan::Apply(_) => {
                let applied = plan::apply(&input.workspace_root, parsed)?;
                Ok(AppliedTurn::Applied(AppliedWrite {
                    summary: applied.summary,
                    changed_files: applied.changed_files,
                    conversation_revision: revision,
                }))
            }
            plan::ParsedPlan::Question(question) => {
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
    "You are an independent code reviewer. Return only one JSON object with this exact shape: {\"summary\":\"...\",\"blocking\":false,\"findings\":[\"specific finding\"]}. Mark blocking true only for correctness, requested-behavior, build, or test failures that should prevent accepting the implementation. Do not invent obscure hypothetical threats or demand unrelated redesign. Do not use markdown fences.".to_owned()
}

pub fn reviewer_user(transcript: &str, diff: &str, gates: &str) -> String {
    format!("Conversation:\n{transcript}\n\nDiff:\n{diff}\n\nChecks:\n{gates}")
}

fn writer_system() -> String {
    "You are the implementation role in a conversational coding harness. Return only one JSON object. When you can proceed, use exactly {\"kind\":\"plan\",\"summary\":\"...\",\"files\":[{\"path\":\"relative/path\",\"content\":\"complete replacement contents\"}],\"deletions\":[\"relative/path\"]}. Only when blocked by a material user choice that cannot be sensibly inferred, use exactly {\"kind\":\"question\",\"message\":\"one direct question\"}. Make a substantial, maintainable implementation. Preserve unrelated code. Do not invent obscure concerns. Do not use markdown fences or merely explain the work. Encode newlines and other control characters correctly inside JSON strings.".to_owned()
}

fn writer_user(transcript: &str, bundle: &str, findings: Option<&str>) -> String {
    let findings = findings.map_or(String::new(), |value| format!("\n\nFailures to fix:\n{value}"));
    format!("Conversation:\n{transcript}\n\nRepository context:\n{bundle}{findings}")
}

fn repair_user(response: &str, error: &str) -> String {
    const MAX_REPAIR_RESPONSE_BYTES: usize = 256 * 1024;
    let start = response.len().saturating_sub(MAX_REPAIR_RESPONSE_BYTES);
    let start = response.floor_char_boundary(start);
    format!(
        "Your previous response was not valid plan JSON ({error}). Return the same intended answer again as one corrected JSON object only. Previous response:\n{}",
        &response[start..]
    )
}
