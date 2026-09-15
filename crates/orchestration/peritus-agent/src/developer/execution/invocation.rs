//! Current executor lifecycle projected independently of replayed recovery instructions.

use peritus_model_protocol::{BoundedText, ContentBlock, Message, ProtocolLimits, Role};

use super::super::{DeveloperLoopError, DeveloperLoopRequest};

pub(super) fn retry_policy(
    request: &DeveloperLoopRequest,
    step: u16,
    attempt: u8,
    required_tool: &str,
) -> Result<Message, DeveloperLoopError> {
    let policy = policy(request, step, Some(required_tool))?;
    let [ContentBlock::Text(text)] = policy.content() else {
        return Err(DeveloperLoopError::Context(
            "current invocation policy is not one text block".to_owned(),
        ));
    };
    let text = format!(
        "{}\n\nCURRENT PROVIDER RETRY\nattempt={attempt}; required_tool={required_tool}\nThis is a fresh provider retry of the same host step. Return exactly one call to the declared `{required_tool}` tool now and no terminal response. The host will execute the call and return its result on the next turn.",
        text.expose_for_wire(),
    );
    Ok(Message::new(
        Role::System,
        vec![ContentBlock::Text(BoundedText::new(text, ProtocolLimits::PRODUCTION)?)],
        ProtocolLimits::PRODUCTION,
    )?)
}

pub(super) fn policy(
    request: &DeveloperLoopRequest,
    step: u16,
    required_tool: Option<&str>,
) -> Result<Message, DeveloperLoopError> {
    let position = if step == 1 {
        "This is the first provider step of this host invocation."
    } else {
        "This is a continuation of the SAME host invocation, not a fresh invocation."
    };
    let prerequisite = required_tool.map_or_else(
        || "The live executor requires no specific prerequisite tool now. Continue from the latest completed tool results; do not repeat initial grounding merely because a recovery instruction is replayed. This does not waive target-specific reads, permissions, or acceptance checks.".to_owned(),
        |name| format!("The live executor requires `{name}` next. Return exactly one call to that declared tool now and no terminal response; the host will execute it and return its result on the next turn. This prerequisite comes from current executor state, not from archived claims of earlier grounding."),
    );
    let text = format!(
        "{}\n\nCURRENT HOST INVOCATION STATE\nprovider_step={step}; required_tool={}\n{position} A provider request, transport retry, or context reconstruction does not start a new host invocation. Recovery/startup instructions in the task describe this invocation's entry, not a command to restart it on every provider step. Successful tools already executed in this invocation remain completed. Reinspect when new evidence or a changed target requires it, not solely to replay setup. {prerequisite}",
        request.system,
        required_tool.unwrap_or("none"),
    );
    Ok(Message::new(
        Role::System,
        vec![ContentBlock::Text(BoundedText::new(text, ProtocolLimits::PRODUCTION)?)],
        ProtocolLimits::PRODUCTION,
    )?)
}
