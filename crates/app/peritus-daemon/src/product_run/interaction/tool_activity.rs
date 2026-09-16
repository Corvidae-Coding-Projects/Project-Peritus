//! Bounded observations of actual tool calls, not model prose or raw provider events.

use super::{InteractionOptions, ProductRunServiceError, bounded};
use peritus_app_protocol::{ProductActivity, ProductActivityKind};
use serde_json::Value;

mod display;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct PendingTool {
    sequence: u64,
    name: String,
    summary: String,
}

pub(super) fn started(
    options: &mut InteractionOptions,
    name: &str,
    arguments: &str,
) -> Result<(), ProductRunServiceError> {
    let summary = display::summary(name, arguments);
    let sequence = options.next_sequence;
    options.append(
        ProductActivityKind::Tool,
        &format!("Calling {summary}"),
        "Awaiting tool result",
    )?;
    options.pending_tool = Some(PendingTool { sequence, name: name.to_owned(), summary });
    Ok(())
}

pub(super) fn finished(
    options: &mut InteractionOptions,
    name: &str,
    output: &str,
    is_error: bool,
) -> Result<(), ProductRunServiceError> {
    let result = serde_json::from_str::<Value>(output).ok();
    let state = result.as_ref().and_then(|value| value["state"].as_str());
    let verb = if is_error {
        "Failed"
    } else if state == Some("running") {
        "Started"
    } else if matches!(name, "run_command" | "command_start") {
        "Ran"
    } else {
        "Called"
    };
    let detail = display::result(name, output);
    if let Some(pending) = options.pending_tool.take()
        && pending.name == name
        && let Some(activity) =
            options.activities.iter_mut().find(|item| item.sequence() == pending.sequence)
    {
        *activity = ProductActivity::new(
            pending.sequence,
            ProductActivityKind::Tool,
            bounded(&format!("{verb} {}", pending.summary)),
            detail,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        options.streaming_text = false;
        return Ok(());
    }
    // A missing start (e.g. a restored observation) must not be paired to a different call.
    options.append(ProductActivityKind::Tool, &format!("{verb} {name}"), &detail)
}
