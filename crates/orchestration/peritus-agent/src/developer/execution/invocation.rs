//! Current executor lifecycle projected independently of replayed recovery instructions.

use peritus_model_protocol::{BoundedText, ContentBlock, Message, ProtocolLimits, Role};

use super::super::{DeveloperLoopError, DeveloperLoopRequest};

pub(super) struct InvocationPolicy {
    pub(super) system: Message,
    pub(super) state: Message,
}

impl InvocationPolicy {
    // The non-local path retains its original single authoritative policy projection.
    pub(super) fn combined(&self) -> Result<Message, DeveloperLoopError> {
        let mut content = self.system.content().to_vec();
        content.extend_from_slice(self.state.content());
        Ok(Message::new(Role::System, content, ProtocolLimits::PRODUCTION)?)
    }
}

pub(super) fn retry_policy(
    request: &DeveloperLoopRequest,
    step: u16,
    attempt: u8,
    required_tool: &str,
    guidance: Option<&str>,
) -> Result<InvocationPolicy, DeveloperLoopError> {
    let mut policy = policy(request, step, Some(required_tool), guidance)?;
    let [ContentBlock::Text(current)] = policy.state.content() else {
        return Err(DeveloperLoopError::Context("current executor state is not text".to_owned()));
    };
    policy.state = text(
        Role::Developer,
        format!(
            "{}\n\nCURRENT PROVIDER RETRY\nattempt={attempt}; required_tool={required_tool}\nThis is a fresh provider retry of the same host step. Return exactly one call to the declared `{required_tool}` tool now and no terminal response. The host will execute the call and return its result on the next turn.",
            current.expose_for_wire(),
        ),
    )?;
    Ok(policy)
}

pub(super) fn policy(
    request: &DeveloperLoopRequest,
    _step: u16,
    required_tool: Option<&str>,
    guidance: Option<&str>,
) -> Result<InvocationPolicy, DeveloperLoopError> {
    let prerequisite = required_tool.map_or_else(
        || "The live executor requires no specific prerequisite tool now. Continue from the latest completed tool results; do not repeat initial grounding merely because a recovery instruction is replayed. This does not waive target-specific reads, permissions, or acceptance checks.".to_owned(),
        |name| format!("The live executor requires `{name}` next. Return exactly one call to that declared tool now and no terminal response; the host will execute it and return its result on the next turn. This prerequisite comes from current executor state, not from archived claims of earlier grounding."),
    );
    let guidance = guidance.unwrap_or("");
    let state = format!(
        "CURRENT HOST INVOCATION STATE\nrequired_tool={}\nThis is the SAME host invocation. A provider request, transport retry, or context reconstruction does not start a new host invocation. Recovery/startup instructions in the task describe this invocation's entry, not a command to restart it on every provider step. Successful tools already executed in this invocation remain completed. Reinspect when new evidence or a changed target requires it, not solely to replay setup. {prerequisite}\n\n{guidance}",
        required_tool.unwrap_or("none"),
    );
    Ok(InvocationPolicy {
        system: text(Role::System, request.system.clone())?,
        state: text(Role::Developer, state)?,
    })
}

fn text(role: Role, value: String) -> Result<Message, DeveloperLoopError> {
    Ok(Message::new(
        role,
        vec![ContentBlock::Text(BoundedText::new(value, ProtocolLimits::PRODUCTION)?)],
        ProtocolLimits::PRODUCTION,
    )?)
}
