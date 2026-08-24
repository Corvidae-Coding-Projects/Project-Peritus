//! Direct capability, redaction, and adapter-isolation qualification probes.

use std::sync::Arc;

use peritus_conformance::{
    ProviderCapability, ProviderCapabilityObservation, ProviderConformanceError,
    ProviderConformanceFixture, ProviderConformanceObservation, ProviderIsolationObservation,
    ProviderRedactionObservation, ReportText,
};
use peritus_model_protocol::{
    BoundedText, CachePolicy, Capability, ContentBlock, GenerationConfig, JsonBounds, JsonSchema,
    Message, ModelRequest, ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderProfile,
    ReasoningPolicy, RequestId, RequestOptions, RequestedCapabilities, Role, SchemaDialect,
    StructuredOutput, ToolChoice, ToolDefinition, ToolName, negotiate,
};

use super::Probe;
use crate::AnthropicClient;
use crate::test_support::{TestCredentials, TestTransport, TransportState, config_at};

pub(super) fn capabilities(
    probe: &Probe,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    let profile = crate::test_support::profile();
    let streaming_advertised = profile.capabilities().supports(Capability::Streaming);
    let streaming_succeeded = probe.events.iter().any(|event| {
        matches!(event.event(), peritus_model_protocol::ModelEvent::ResponseCompleted)
    }) && probe.transport_requests == 1
        && probe.exchange_matched
        && probe.credential_resolutions == 1;
    let unsupported = RequestedCapabilities::new(&[Capability::AudioInput], &[], profile.limits())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let audio_rejected = negotiate(&profile, unsupported).is_err();

    Ok(ProviderConformanceObservation::Capabilities(ProviderCapabilityObservation::new(
        streaming_advertised.then_some(ProviderCapability::Streaming).into_iter().collect(),
        streaming_succeeded.then_some(ProviderCapability::Streaming).into_iter().collect(),
        audio_rejected.then_some(ProviderCapability::AudioInput).into_iter().collect(),
        probe.encoded_streaming.then_some(ProviderCapability::Streaming).into_iter().collect(),
        probe.transport_requests,
    )))
}

pub(super) fn redaction(
    probe: &Probe,
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    if fixture_artifacts_contain(fixture.canary())?
        || normalized_events_contain(&probe.events, fixture.canary())
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
        ReportText::new("Anthropic fixture manifest and artifacts contain no injected canary")
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
    );
    Ok(ProviderConformanceObservation::Redaction(ProviderRedactionObservation::new(5, surfaces)))
}

pub(super) fn isolation(
    probe: &Probe,
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    let foreign_state = TransportState::with_responses(Vec::new());
    let foreign_credentials = TestCredentials::default();
    let foreign_resolutions = foreign_credentials.counter();
    let foreign = AnthropicClient::with_transport(
        config_at("https://foreign.invalid", 1, Vec::new()),
        Box::new(foreign_credentials),
        Box::new(TestTransport(Arc::clone(&foreign_state))),
    );
    let configured_selected = foreign.profile().provider().as_str() == "anthropic"
        && probe.exchange_matched
        && probe.transport_requests == 1;
    drop(foreign);
    let foreign_requests = foreign_state.captures().len();
    let foreign_credential_resolutions =
        foreign_resolutions.load(std::sync::atomic::Ordering::SeqCst);
    let selected = fixture.selected_adapter();
    let foreign_label = fixture.foreign_adapter();
    let label = |condition: bool| {
        ReportText::new(if condition { selected } else { foreign_label })
            .map_err(|_| ProviderConformanceError::Infrastructure)
    };
    Ok(ProviderConformanceObservation::Isolation(ProviderIsolationObservation::new(
        label(configured_selected)?,
        label(probe.encoded_streaming)?,
        label(probe.credential_resolutions == 1 && foreign_credential_resolutions == 0)?,
        label(probe.exchange_matched && foreign_requests == 0)?,
        u64::try_from(foreign_requests).map_err(|_| ProviderConformanceError::Infrastructure)?,
    )))
}

pub(super) fn redaction_request(
    profile: &ProviderProfile,
    canary: &str,
) -> Result<ModelRequest, ProviderConformanceError> {
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(
            &[Capability::Streaming, Capability::ToolCalls, Capability::SamplingControls],
            &[],
            profile.limits(),
        )
        .map_err(|_| ProviderConformanceError::Infrastructure)?,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let text = sensitive(canary, "prompt")?;
    let message =
        Message::new(Role::User, vec![ContentBlock::Text(text)], ProtocolLimits::PRODUCTION)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let schema = JsonSchema::parse(
        r#"{"additionalProperties":false,"properties":{},"type":"object"}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let tool = ToolDefinition::new(
        ToolName::new("redaction_probe".to_owned())
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        Some(sensitive(canary, "tool-description")?),
        schema,
        true,
    );
    let stop = sensitive(canary, "stop")?;
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(format!("{canary}-request"))
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        vec![message],
        vec![tool],
        ToolChoice::Auto,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(128, vec![stop], None, None, None)
                .map_err(|_| ProviderConformanceError::Infrastructure)?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        ProtocolLimits::PRODUCTION,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)
}

fn sensitive(canary: &str, suffix: &str) -> Result<BoundedText, ProviderConformanceError> {
    BoundedText::new(format!("{canary}-{suffix}"), ProtocolLimits::PRODUCTION)
        .map_err(|_| ProviderConformanceError::Infrastructure)
}

fn fixture_artifacts_contain(canary: &str) -> Result<bool, ProviderConformanceError> {
    let manifest = crate::test_support::fixture("MANIFEST");
    let digest_inventory = crate::test_support::fixture("SHA256SUMS");
    if manifest.windows(canary.len()).any(|window| window == canary.as_bytes())
        || digest_inventory.windows(canary.len()).any(|window| window == canary.as_bytes())
    {
        return Ok(true);
    }
    let manifest =
        core::str::from_utf8(&manifest).map_err(|_| ProviderConformanceError::Infrastructure)?;
    for line in manifest.lines().filter(|line| {
        line.split_once('=').is_some_and(|(name, _description)| {
            std::path::Path::new(name).extension().and_then(std::ffi::OsStr::to_str).is_some_and(
                |extension| {
                    extension.eq_ignore_ascii_case("json") || extension.eq_ignore_ascii_case("sse")
                },
            )
        })
    }) {
        let (name, _description) =
            line.split_once('=').ok_or(ProviderConformanceError::Infrastructure)?;
        let bytes = crate::test_support::fixture(name);
        if bytes.windows(canary.len()).any(|window| window == canary.as_bytes()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn normalized_events_contain(
    events: &[peritus_model_protocol::EventEnvelope],
    canary: &str,
) -> bool {
    events.iter().any(|envelope| {
        envelope
            .provider_event_id()
            .is_some_and(|identity| identity.expose_for_wire().contains(canary))
            || match envelope.event() {
                peritus_model_protocol::ModelEvent::ResponseStarted { response_id, model } => {
                    response_id
                        .as_ref()
                        .is_some_and(|identity| identity.expose_for_wire().contains(canary))
                        || model.as_ref().is_some_and(|name| name.as_str().contains(canary))
                }
                peritus_model_protocol::ModelEvent::ResponseIdentity(identity) => {
                    identity.expose_for_wire().contains(canary)
                }
                peritus_model_protocol::ModelEvent::ItemStarted { item_id, .. }
                | peritus_model_protocol::ModelEvent::ItemCompleted(item_id) => {
                    item_id.expose_for_wire().contains(canary)
                }
                peritus_model_protocol::ModelEvent::TextDelta { fragment, .. }
                | peritus_model_protocol::ModelEvent::ReasoningSummaryDelta { fragment, .. }
                | peritus_model_protocol::ModelEvent::ReasoningReplayDelta { fragment, .. }
                | peritus_model_protocol::ModelEvent::RefusalDelta { fragment, .. }
                | peritus_model_protocol::ModelEvent::ToolArgumentDelta { fragment, .. } => {
                    fragment
                        .expose()
                        .windows(canary.len())
                        .any(|window| window == canary.as_bytes())
                }
                peritus_model_protocol::ModelEvent::ToolCallStarted { item_id, call_id, name } => {
                    item_id.expose_for_wire().contains(canary)
                        || call_id.expose_for_wire().contains(canary)
                        || name.as_str().contains(canary)
                }
                peritus_model_protocol::ModelEvent::Cache(cache) => {
                    cache.key().is_some_and(|key| key.expose_for_wire().contains(canary))
                }
                peritus_model_protocol::ModelEvent::Finish(
                    peritus_model_protocol::FinishReason::Provider(raw),
                ) => raw.expose_for_wire().contains(canary),
                peritus_model_protocol::ModelEvent::ProviderEvent(extension) => extension
                    .value()
                    .canonical_bytes()
                    .windows(canary.len())
                    .any(|window| window == canary.as_bytes()),
                peritus_model_protocol::ModelEvent::ResponseFailed(failure) => {
                    failure.diagnostic().code().contains(canary)
                }
                _ => false,
            }
    })
}
