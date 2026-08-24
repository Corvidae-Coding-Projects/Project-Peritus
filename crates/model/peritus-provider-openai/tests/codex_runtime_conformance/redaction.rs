//! Canary accounting and direct scanning of normalized/reportable runtime surfaces.

use std::path::Path;

use peritus_conformance::{
    ProviderConformanceError, ProviderConformanceFixture, ProviderConformanceObservation,
    ProviderRedactionObservation, ReportText,
};
use peritus_model_protocol::{ContentBlock, EventEnvelope, ModelEvent, ModelRequest};

use super::support::Probe;

pub(super) fn observe(
    probe: &Probe,
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    if fixture_artifacts_contain(fixture.canary())?
        || events_contain_canary(&probe.events, fixture.canary())
        || probe.surfaces.iter().any(|surface| surface.contains(fixture.canary()))
    {
        return Err(ProviderConformanceError::Infrastructure);
    }
    let mut surfaces = probe
        .surfaces
        .iter()
        .map(|surface| {
            ReportText::new(surface.clone()).map_err(|_| ProviderConformanceError::Infrastructure)
        })
        .collect::<Result<Vec<_>, _>>()?;
    surfaces.push(
        ReportText::new(
            "Codex runtime events, diagnostics, process trace, fixture inventory, and artifacts contain no canary",
        )
        .map_err(|_| ProviderConformanceError::Infrastructure)?,
    );
    Ok(ProviderConformanceObservation::Redaction(ProviderRedactionObservation::new(
        u64::try_from(probe.sensitive_inputs)
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        surfaces,
    )))
}

pub(super) fn request_canary_count(request: &ModelRequest, canary: &str) -> usize {
    let request_id = usize::from(request.request_id().expose_for_wire() == canary);
    let prompts = request
        .messages()
        .iter()
        .flat_map(peritus_model_protocol::Message::content)
        .filter(|content| {
            matches!(content, ContentBlock::Text(text) if text.expose_for_wire() == canary)
        })
        .count();
    let tools = request
        .tools()
        .iter()
        .filter(|tool| {
            tool.description().is_some_and(|description| description.expose_for_wire() == canary)
        })
        .count();
    request_id + prompts + tools
}

fn fixture_artifacts_contain(canary: &str) -> Result<bool, ProviderConformanceError> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/v1");
    let manifest = read(&root.join("MANIFEST"))?;
    let sums = read(&root.join("SHA256SUMS"))?;
    if contains_bytes(&manifest, canary.as_bytes()) || contains_bytes(&sums, canary.as_bytes()) {
        return Ok(true);
    }
    let manifest =
        core::str::from_utf8(&manifest).map_err(|_| ProviderConformanceError::Infrastructure)?;
    for line in manifest.lines().skip_while(|line| !line.starts_with("reviewed=")).skip(1) {
        let (name, _) = line.split_once('=').ok_or(ProviderConformanceError::Infrastructure)?;
        if contains_bytes(&read(&root.join(name))?, canary.as_bytes()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn read(path: &Path) -> Result<Vec<u8>, ProviderConformanceError> {
    std::fs::read(path).map_err(|_| ProviderConformanceError::Infrastructure)
}

fn events_contain_canary(events: &[EventEnvelope], canary: &str) -> bool {
    events.iter().any(|envelope| {
        envelope
            .provider_event_id()
            .is_some_and(|identity| identity.expose_for_wire().contains(canary))
            || event_contains_canary(envelope.event(), canary)
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "the exhaustive scan keeps every sensitive normalized event field visibly audited"
)]
fn event_contains_canary(event: &ModelEvent, canary: &str) -> bool {
    match event {
        ModelEvent::ResponseStarted { response_id, model } => {
            response_id.as_ref().is_some_and(|identity| identity.expose_for_wire().contains(canary))
                || model.as_ref().is_some_and(|name| name.as_str().contains(canary))
        }
        ModelEvent::ResponseIdentity(identity) => identity.expose_for_wire().contains(canary),
        ModelEvent::ItemStarted { item_id, .. } | ModelEvent::ItemCompleted(item_id) => {
            item_id.expose_for_wire().contains(canary)
        }
        ModelEvent::TextDelta { item_id, fragment }
        | ModelEvent::ReasoningSummaryDelta { item_id, fragment }
        | ModelEvent::ReasoningReplayDelta { item_id, fragment }
        | ModelEvent::RefusalDelta { item_id, fragment } => {
            item_id.expose_for_wire().contains(canary)
                || contains_bytes(fragment.expose(), canary.as_bytes())
        }
        ModelEvent::ToolCallStarted { item_id, call_id, name } => {
            item_id.expose_for_wire().contains(canary)
                || call_id.expose_for_wire().contains(canary)
                || name.as_str().contains(canary)
        }
        ModelEvent::ToolArgumentDelta { call_id, fragment } => {
            call_id.expose_for_wire().contains(canary)
                || contains_bytes(fragment.expose(), canary.as_bytes())
        }
        ModelEvent::ProviderEvent(extension) => {
            extension.name().as_str().contains(canary)
                || contains_bytes(extension.value().canonical_bytes(), canary.as_bytes())
        }
        ModelEvent::ResponseFailed(failure) => {
            failure.provider().as_str().contains(canary)
                || failure
                    .response_id()
                    .is_some_and(|identity| identity.expose_for_wire().contains(canary))
                || failure.diagnostic().code().contains(canary)
        }
        ModelEvent::Usage(_)
        | ModelEvent::RateLimit(_)
        | ModelEvent::Cache(_)
        | ModelEvent::Finish(_)
        | ModelEvent::Heartbeat
        | ModelEvent::ResponseCompleted
        | ModelEvent::ResponseCancelled => false,
        _ => true,
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}
