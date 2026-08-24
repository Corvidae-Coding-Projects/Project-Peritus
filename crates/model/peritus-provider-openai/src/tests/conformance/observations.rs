//! Conversion of normalized production events into A2 direct observations.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use peritus_conformance::{
    ProviderCapability, ProviderCapabilityObservation, ProviderConformanceError,
    ProviderConformanceFixture, ProviderConformanceObservation, ProviderEventKind,
    ProviderEventObservation, ProviderFailureKind, ProviderFailureObservation,
    ProviderIsolationObservation, ProviderStreamObservation, ProviderTerminal,
    ProviderUsageObservation, ProviderUsageSnapshot, ReportText,
};
use peritus_model_protocol::{
    Capability, EventEnvelope, ModelEvent, RequestedCapabilities, negotiate,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Credential, CredentialReference, CredentialSource, Endpoint,
    HttpRequest, HttpTransport, ModelProvider, ProviderCoreError,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpFault, FakeHttpHeader, FakeHttpLimits, FakeHttpServer,
    HeaderMatchMode, ScriptedHttpResponse,
};

use super::super::support::{
    credential_reference, fixture, minimal_request, profile_minimal, profile_streaming_only,
};
use super::Probe;
use crate::{OpenAiConfig, OpenAiProvider};

pub(super) fn capabilities(
    probe: &Probe,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    let profile = profile_streaming_only();
    let streaming_advertised = profile.capabilities().supports(Capability::Streaming);
    let streaming_succeeded =
        probe.events.iter().any(|event| matches!(event.event(), ModelEvent::ResponseCompleted))
            && probe.transport_requests == 1
            && probe.facts.contains(super::ProbeFacts::EXCHANGE_MATCHED)
            && probe.credential_resolutions == 1;
    let unsupported = RequestedCapabilities::new(&[Capability::AudioInput], &[], profile.limits())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let audio_rejected = negotiate(&profile, unsupported).is_err();
    Ok(ProviderConformanceObservation::Capabilities(ProviderCapabilityObservation::new(
        streaming_advertised.then_some(ProviderCapability::Streaming).into_iter().collect(),
        streaming_succeeded.then_some(ProviderCapability::Streaming).into_iter().collect(),
        audio_rejected.then_some(ProviderCapability::AudioInput).into_iter().collect(),
        probe
            .facts
            .contains(super::ProbeFacts::ENCODED_STREAMING)
            .then_some(ProviderCapability::Streaming)
            .into_iter()
            .collect(),
        probe.transport_requests,
    )))
}

pub(super) fn ordered(events: &[EventEnvelope]) -> ProviderConformanceObservation {
    let duplicates = duplicate_provider_sequences(events);
    let events = observed_events(events);
    let received = u64::try_from(events.len()).expect("bounded events") + duplicates;
    ProviderConformanceObservation::Stream(ProviderStreamObservation::new(
        events, received, duplicates, 1, None, None, None,
    ))
}

pub(super) fn fragmented(
    events: &[EventEnvelope],
    digest: [u8; 32],
) -> ProviderConformanceObservation {
    let events = observed_events(events);
    let final_fragment = events
        .iter()
        .rev()
        .find(|event| event.kind() == ProviderEventKind::ToolArgumentDelta)
        .map(|event| event.sequence());
    let closed = events
        .iter()
        .rev()
        .find(|event| event.kind() == ProviderEventKind::ItemCompleted)
        .map(|event| event.sequence());
    let received = u64::try_from(events.len()).expect("bounded events");
    ProviderConformanceObservation::Stream(ProviderStreamObservation::new(
        events,
        received,
        0,
        1,
        Some(digest),
        final_fragment,
        closed,
    ))
}

pub(super) fn failure(
    events: &[EventEnvelope],
    kind: ProviderFailureKind,
    requests: u64,
) -> ProviderConformanceObservation {
    let partial = events.iter().filter(|event| !is_terminal(event.event())).count();
    ProviderConformanceObservation::Failure(ProviderFailureObservation::new(
        kind,
        ProviderTerminal::Failed,
        requests,
        u64::try_from(partial).expect("bounded events"),
    ))
}

pub(super) fn usage(events: &[EventEnvelope]) -> ProviderConformanceObservation {
    let snapshots = events
        .iter()
        .filter_map(|event| match event.event() {
            ModelEvent::Usage(usage) => {
                let counters = usage.counters();
                Some(ProviderUsageSnapshot::new(
                    counters.input_tokens(),
                    counters.cached_input_tokens(),
                    counters.output_tokens(),
                    counters.total_tokens(),
                ))
            }
            _ => None,
        })
        .collect();
    ProviderConformanceObservation::Usage(ProviderUsageObservation::new(snapshots))
}

pub(super) fn isolation(
    probe: &Probe,
    fixture_data: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    let foreign_limits = FakeHttpLimits::default();
    let foreign_profile = profile_minimal();
    let foreign_request = minimal_request(&foreign_profile);
    let foreign_body = crate::request::encode(&foreign_request)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let foreign_expected =
        ExpectedHttpRequest::new("POST", "/v1/responses", Vec::new(), foreign_body, foreign_limits)
            .map_err(|_| ProviderConformanceError::Infrastructure)?
            .header_match_mode(HeaderMatchMode::AllowAdditional);
    let foreign_response = ScriptedHttpResponse::new(
        200,
        vec![
            FakeHttpHeader::new("content-type", "text/event-stream")
                .map_err(|_| ProviderConformanceError::Infrastructure)?,
        ],
        vec![fixture("success.sse")],
        FakeHttpFault::Complete,
        None,
        foreign_limits,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let foreign_server = FakeHttpServer::start(foreign_expected, foreign_response, foreign_limits)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let foreign_config = OpenAiConfig::for_test(
        Endpoint::new(foreign_server.base_url())
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        credential_reference(),
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let foreign_configured = foreign_config.endpoint().as_str().trim_end_matches('/')
        == foreign_server.base_url().trim_end_matches('/');
    let foreign_credentials = Arc::new(CountingCredential::default());
    let foreign_transport = Arc::new(CountingTransport::default());
    let foreign = OpenAiProvider::with_transport(
        foreign_config,
        foreign_profile,
        foreign_credentials.clone(),
        foreign_transport.clone(),
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    if !foreign_configured || foreign.profile().provider().as_str() != "openai" {
        return Err(ProviderConformanceError::Infrastructure);
    }
    drop(foreign);
    drop(foreign_server);
    let foreign_requests = foreign_transport.count.load(Ordering::SeqCst);
    let foreign_resolutions = foreign_credentials.count.load(Ordering::SeqCst);
    let selected = fixture_data.selected_adapter();
    let foreign_label = fixture_data.foreign_adapter();
    let label = |condition: bool| {
        ReportText::new(if condition { selected } else { foreign_label })
            .map_err(|_| ProviderConformanceError::Infrastructure)
    };
    Ok(ProviderConformanceObservation::Isolation(ProviderIsolationObservation::new(
        label(probe.facts.contains(super::ProbeFacts::CONFIGURED_SELECTED))?,
        label(probe.facts.contains(super::ProbeFacts::REQUEST_BOUND_SELECTED))?,
        label(
            probe.credential_adapter.as_deref() == Some(selected)
                && probe.credential_resolutions == 1
                && foreign_resolutions == 0,
        )?,
        label(
            probe.transport_adapter.as_deref() == Some(selected)
                && probe.facts.contains(super::ProbeFacts::EXCHANGE_MATCHED)
                && foreign_requests == 0,
        )?,
        foreign_requests,
    )))
}

#[derive(Default)]
struct CountingCredential {
    count: AtomicU64,
}

impl CredentialSource for CountingCredential {
    fn resolve(&self, _reference: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Credential::new(b"foreign-openai-key".to_vec())
    }
}

#[derive(Default)]
struct CountingTransport {
    count: AtomicU64,
}

impl HttpTransport for CountingTransport {
    fn send<'a>(
        &'a self,
        _request: HttpRequest,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<peritus_provider_core::HttpResponse, ProviderCoreError>> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(ProviderCoreError::transport("openai_test", "foreign transport")) })
    }
}

fn observed_events(events: &[EventEnvelope]) -> Vec<ProviderEventObservation> {
    events.iter().filter_map(observed_event).collect()
}

fn observed_event(event: &EventEnvelope) -> Option<ProviderEventObservation> {
    let (kind, bytes) = match event.event() {
        ModelEvent::ResponseStarted { .. } => (ProviderEventKind::ResponseStarted, 0),
        ModelEvent::ItemStarted { .. } => (ProviderEventKind::ItemStarted, 0),
        ModelEvent::TextDelta { fragment, .. } => (ProviderEventKind::TextDelta, fragment.len()),
        ModelEvent::ToolCallStarted { .. } => (ProviderEventKind::ToolCallStarted, 0),
        ModelEvent::ToolArgumentDelta { fragment, .. } => {
            (ProviderEventKind::ToolArgumentDelta, fragment.len())
        }
        ModelEvent::ItemCompleted(_) => (ProviderEventKind::ItemCompleted, 0),
        ModelEvent::Usage(_) => (ProviderEventKind::Usage, 0),
        ModelEvent::Finish(_) => (ProviderEventKind::Finish, 0),
        ModelEvent::ResponseCompleted => (ProviderEventKind::ResponseCompleted, 0),
        ModelEvent::ResponseFailed(_) => (ProviderEventKind::ResponseFailed, 0),
        ModelEvent::ResponseCancelled => (ProviderEventKind::ResponseCancelled, 0),
        _ => return None,
    };
    let digest = event.provider_digest().into_bytes();
    Some(ProviderEventObservation::new(
        event.sequence(),
        event.provider_sequence(),
        digest,
        digest,
        kind,
        u64::try_from(bytes).expect("bounded fragment"),
    ))
}

fn duplicate_provider_sequences(events: &[EventEnvelope]) -> u64 {
    let mut seen = BTreeSet::new();
    u64::try_from(
        events
            .iter()
            .filter_map(EventEnvelope::provider_sequence)
            .filter(|sequence| !seen.insert(*sequence))
            .count(),
    )
    .expect("bounded duplicates")
}

pub(super) fn terminal_count(events: &[EventEnvelope]) -> u64 {
    u64::try_from(events.iter().filter(|event| is_terminal(event.event())).count())
        .expect("bounded events")
}

pub(super) const fn is_terminal(event: &ModelEvent) -> bool {
    matches!(
        event,
        ModelEvent::ResponseCompleted
            | ModelEvent::ResponseFailed(_)
            | ModelEvent::ResponseCancelled
    )
}
