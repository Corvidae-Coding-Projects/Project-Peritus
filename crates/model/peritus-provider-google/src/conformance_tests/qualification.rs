//! Direct profile, redaction, and adapter-isolation qualification probes.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;

use peritus_conformance::{
    ProviderCapability, ProviderCapabilityObservation, ProviderConformanceError,
    ProviderConformanceFixture, ProviderConformanceObservation, ProviderIsolationObservation,
    ProviderRedactionObservation, ReportText,
};
use peritus_model_protocol::{
    BoundedText, CachePolicy, Capability, ContentBlock, GenerationConfig, JsonBounds, JsonSchema,
    Message, ModelRequest, ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderProfile,
    ReasoningPolicy, RequestId, RequestOptions, RequestedCapabilities, Role, SchemaDialect,
    StructuredOutput, ToolChoice, ToolDefinition, ToolName, WireDialect, negotiate,
};

use super::Probe;
use crate::GoogleClient;
use crate::test_support::{TestCredentials, config_at, streaming_profile};

pub(super) fn capabilities(
    probe: &Probe,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    let profile = streaming_profile(WireDialect::GeminiInteractionsV1);
    let advertised = profile.capabilities().supports(Capability::Streaming);
    let completed = probe.events.iter().any(|event| {
        matches!(event.event(), peritus_model_protocol::ModelEvent::ResponseCompleted)
    });
    let succeeded = completed
        && probe.transport_requests == 1
        && probe.exchange_matched.observed()
        && probe.credential_resolutions == 1;
    let unsupported =
        RequestedCapabilities::new(&[Capability::ResumableResponse], &[], profile.limits())
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let rejected = negotiate(&profile, unsupported).is_err();
    Ok(ProviderConformanceObservation::Capabilities(ProviderCapabilityObservation::new(
        advertised.then_some(ProviderCapability::Streaming).into_iter().collect(),
        succeeded.then_some(ProviderCapability::Streaming).into_iter().collect(),
        rejected.then_some(ProviderCapability::ExactResume).into_iter().collect(),
        probe
            .encoded_streaming
            .observed()
            .then_some(ProviderCapability::Streaming)
            .into_iter()
            .collect(),
        probe.transport_requests,
    )))
}

pub(super) fn redaction(
    probe: &Probe,
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    if probe.sensitive_inputs < 4
        || probe.surfaces.iter().any(|surface| surface.contains(fixture.canary()))
    {
        return Err(ProviderConformanceError::Infrastructure);
    }
    let surfaces = probe
        .surfaces
        .iter()
        .map(|surface| {
            ReportText::new(surface.clone()).map_err(|_| ProviderConformanceError::Infrastructure)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProviderConformanceObservation::Redaction(ProviderRedactionObservation::new(
        probe.sensitive_inputs,
        surfaces,
    )))
}

pub(super) fn isolation(
    probe: &Probe,
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    let foreign_server = ForeignCounter::start()?;
    let foreign_credentials = TestCredentials::default();
    let foreign_resolutions = foreign_credentials.counter();
    let foreign = GoogleClient::new(
        config_at(
            &format!("http://{}", foreign_server.address()),
            WireDialect::GeminiInteractionsV1,
            1,
        ),
        Box::new(foreign_credentials),
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let foreign_profile_is_google = foreign.profile().provider().as_str() == "google";
    drop(foreign);
    let foreign_requests = foreign_server.finish()?;
    let foreign_credential_resolutions = foreign_resolutions.load(Ordering::SeqCst);
    let selected = fixture.selected_adapter();
    let foreign_label = fixture.foreign_adapter();
    let label = |condition: bool| {
        ReportText::new(if condition { selected } else { foreign_label })
            .map_err(|_| ProviderConformanceError::Infrastructure)
    };
    Ok(ProviderConformanceObservation::Isolation(ProviderIsolationObservation::new(
        label(probe.configured_google.observed() && foreign_profile_is_google)?,
        label(probe.request_google.observed())?,
        label(probe.credential_resolutions == 1 && foreign_credential_resolutions == 0)?,
        label(
            probe.exchange_matched.observed()
                && probe.transport_requests == 1
                && foreign_requests == 0,
        )?,
        u64::try_from(foreign_requests).map_err(|_| ProviderConformanceError::Infrastructure)?,
    )))
}

pub(super) fn redaction_request(
    profile: &ProviderProfile,
    canary: &str,
) -> Result<(ModelRequest, u64), ProviderConformanceError> {
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
    let markers = [
        format!("{canary}-prompt"),
        format!("{canary}-tool-description"),
        format!("{canary}-request"),
        format!("{canary}-stop"),
    ];
    let sensitive_inputs =
        u64::try_from(markers.iter().filter(|value| value.contains(canary)).count())
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let prompt = BoundedText::new(markers[0].clone(), ProtocolLimits::PRODUCTION)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let message =
        Message::new(Role::User, vec![ContentBlock::Text(prompt)], ProtocolLimits::PRODUCTION)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let schema = JsonSchema::parse(
        r#"{"additionalProperties":false,"properties":{},"type":"object"}"#,
        SchemaDialect::GeminiSubset,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let tool = ToolDefinition::new(
        ToolName::new("redaction_probe".to_owned())
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        Some(
            BoundedText::new(markers[1].clone(), ProtocolLimits::PRODUCTION)
                .map_err(|_| ProviderConformanceError::Infrastructure)?,
        ),
        schema,
        true,
    );
    let request = ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(markers[2].clone()).map_err(|_| ProviderConformanceError::Infrastructure)?,
        vec![message],
        vec![tool],
        ToolChoice::Auto,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(
                128,
                vec![
                    BoundedText::new(markers[3].clone(), ProtocolLimits::PRODUCTION)
                        .map_err(|_| ProviderConformanceError::Infrastructure)?,
                ],
                None,
                None,
                None,
            )
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        ProtocolLimits::PRODUCTION,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    Ok((request, sensitive_inputs))
}

struct ForeignCounter {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    requests: Arc<AtomicUsize>,
    worker: Option<JoinHandle<()>>,
}

impl ForeignCounter {
    fn start() -> Result<Self, ProviderConformanceError> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let address =
            listener.local_addr().map_err(|_| ProviderConformanceError::Infrastructure)?;
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let worker_stop = Arc::clone(&stop);
        let worker_requests = Arc::clone(&requests);
        let worker = std::thread::Builder::new()
            .name("google-foreign-server".to_owned())
            .spawn(move || {
                while let Ok((_stream, _peer)) = listener.accept() {
                    if worker_stop.load(Ordering::SeqCst) {
                        break;
                    }
                    worker_requests.fetch_add(1, Ordering::SeqCst);
                }
            })
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        Ok(Self { address, stop, requests, worker: Some(worker) })
    }

    const fn address(&self) -> SocketAddr {
        self.address
    }

    fn finish(mut self) -> Result<usize, ProviderConformanceError> {
        self.stop.store(true, Ordering::SeqCst);
        let _wake = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| ProviderConformanceError::Infrastructure)?;
        }
        Ok(self.requests.load(Ordering::SeqCst))
    }
}

impl Drop for ForeignCounter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _wake = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            let _joined = worker.join();
        }
    }
}
