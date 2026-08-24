//! Canary accounting and direct scans of every reportable normalized surface.

use peritus_conformance::{
    ProviderConformanceError, ProviderConformanceFixture, ProviderConformanceObservation,
    ProviderRedactionObservation, ReportText,
};
use peritus_model_protocol::{ContentBlock, EventEnvelope, ModelEvent, ModelRequest};

use super::super::support::fixture;
use super::Probe;

pub(super) fn observe(
    probe: &Probe,
    fixture_data: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    if fixture_artifacts_contain(fixture_data.canary())?
        || events_contain_canary(&probe.events, fixture_data.canary())
        || probe.surfaces.iter().any(|value| value.contains(fixture_data.canary()))
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
            "compatible fixture inventory, digests, artifacts, events, errors, and exchanges contain no canary",
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
    let identity = usize::from(request.request_id().expose_for_wire() == canary);
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
    identity + prompts + tools
}

fn fixture_artifacts_contain(canary: &str) -> Result<bool, ProviderConformanceError> {
    for inventory in [fixture("MANIFEST"), fixture("SHA256SUMS")] {
        if contains(&inventory, canary.as_bytes()) {
            return Ok(true);
        }
    }
    let manifest = fixture("MANIFEST");
    let manifest =
        core::str::from_utf8(&manifest).map_err(|_| ProviderConformanceError::Infrastructure)?;
    for name in manifest.lines().skip(1).filter(|name| *name != "SHA256SUMS") {
        if contains(&fixture(name), canary.as_bytes()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn events_contain_canary(events: &[EventEnvelope], canary: &str) -> bool {
    events.iter().any(|envelope| {
        envelope
            .provider_event_id()
            .is_some_and(|identity| identity.expose_for_wire().contains(canary))
            || event_contains_canary(envelope.event(), canary)
    })
}

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
                || contains(fragment.expose(), canary.as_bytes())
        }
        ModelEvent::ToolCallStarted { item_id, call_id, name } => {
            item_id.expose_for_wire().contains(canary)
                || call_id.expose_for_wire().contains(canary)
                || name.as_str().contains(canary)
        }
        ModelEvent::ToolArgumentDelta { call_id, fragment } => {
            call_id.expose_for_wire().contains(canary)
                || contains(fragment.expose(), canary.as_bytes())
        }
        ModelEvent::ProviderEvent(extension) => {
            extension.name().as_str().contains(canary)
                || contains(extension.value().canonical_bytes(), canary.as_bytes())
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

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|value| value == needle)
}
