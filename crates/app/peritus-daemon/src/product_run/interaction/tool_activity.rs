//! Bounded observations of actual tool calls, not model prose or raw provider events.

use super::{InteractionOptions, ProductRunServiceError, bounded};
use peritus_app_protocol::{ProductActivity, ProductActivityKind};
use serde_json::Value;

mod display;

#[derive(Clone)]
pub(super) struct PendingTool {
    sequence: u64,
    name: String,
    summary: String,
    change: Option<display::ChangePreview>,
}

pub(super) fn started(
    options: &mut InteractionOptions,
    name: &str,
    arguments: &str,
) -> Result<(), ProductRunServiceError> {
    let summary = display::summary(name, arguments);
    let change = display::ChangePreview::capture(name, arguments);
    let sequence = options.next_sequence;
    options.append(
        ProductActivityKind::Tool,
        &format!("Calling {summary}"),
        "Awaiting tool result",
    )?;
    options.pending_tool = Some(PendingTool { sequence, name: name.to_owned(), summary, change });
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
    let failed = is_error
        || result.as_ref().is_some_and(|value| {
            value["exit_code"].as_i64().is_some_and(|code| code != 0)
                || value["timed_out"].as_bool() == Some(true)
        });
    let verb = if failed {
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
        let observed_change = if failed {
            None
        } else {
            pending.change.as_ref().and_then(|change| change.finished(result.as_ref()?))
        };
        let (text, detail) = observed_change.unwrap_or_else(|| {
            let label = if !failed
                && name == "context_read"
                && result.as_ref().is_some_and(peritus_product_runner::tool_output::is_markdown)
            {
                "Read document from saved context".to_owned()
            } else {
                format!("{verb} {}", pending.summary)
            };
            (label, detail)
        });
        let mut pages = output_pages(&detail);
        *activity = ProductActivity::new(
            pending.sequence,
            ProductActivityKind::Tool,
            bounded(&text),
            pages.next().unwrap_or_default().to_owned(),
        )
        .map_err(|error| {
            ProductRunServiceError::invalid_data("complete public tool activity", error)
        })?;
        for page in pages {
            options.append(ProductActivityKind::Tool, &format!("{text} · continued"), page)?;
        }
        options.streaming_text = false;
        return Ok(());
    }
    // A missing start (e.g. a restored observation) must not be paired to a different call.
    for page in output_pages(&detail) {
        options.append(ProductActivityKind::Tool, &format!("{verb} {name}"), page)?;
    }
    Ok(())
}

// Keep every returned body byte readable without exceeding a protocol activity's byte bound.
// This partitions presentation only; tool results and model context are unchanged.
fn output_pages(mut text: &str) -> impl Iterator<Item = &str> {
    std::iter::from_fn(move || {
        if text.is_empty() {
            return None;
        }
        let mut end = text.floor_char_boundary(peritus_app_protocol::MAX_PRODUCT_ACTIVITY_BYTES);
        if end < text.len()
            && let Some(newline) = text[..end].rfind('\n').filter(|index| *index > end / 2)
        {
            end = newline + 1;
        }
        let (page, rest) = text.split_at(end);
        text = rest;
        Some(page)
    })
}
