//! OpenTelemetry-compatible and metric projection from checked trace state.

use peritus_trace::{
    CausalBinding, DiagnosticCode, ObservationKind, ObservedTime, SafeAttribute, SpanId, SpanKind,
    SpanOutcome, TraceId, TraceProjectionState, TraceSnapshot,
};
use peritus_types::EventId;

use crate::{ExportRecord, MetricState, TelemetryError, TelemetryErrorKind};

/// OpenTelemetry-compatible safe event value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtelEvent {
    event_id: EventId,
    trace_id: TraceId,
    span_id: SpanId,
    time: ObservedTime,
    code: DiagnosticCode,
    attributes: Vec<SafeAttribute>,
}

impl OtelEvent {
    /// Returns the durable event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the 16-byte trace identity.
    #[must_use]
    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }
    /// Returns the 8-byte span identity.
    #[must_use]
    pub const fn span_id(&self) -> SpanId {
        self.span_id
    }
    /// Returns caller-observed event time.
    #[must_use]
    pub const fn time(&self) -> ObservedTime {
        self.time
    }
    /// Returns the stable content-free event code.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }
    /// Borrows closed safe attributes.
    #[must_use]
    pub fn attributes(&self) -> &[SafeAttribute] {
        &self.attributes
    }
}

/// OpenTelemetry-compatible safe span value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtelSpan {
    trace_id: TraceId,
    span_id: SpanId,
    parent_span_id: Option<SpanId>,
    kind: SpanKind,
    binding: CausalBinding,
    start: ObservedTime,
    end: ObservedTime,
    outcome: SpanOutcome,
    attributes: Vec<SafeAttribute>,
    events: Vec<OtelEvent>,
}

impl OtelSpan {
    /// Returns the 16-byte OpenTelemetry trace identity.
    #[must_use]
    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }
    /// Returns the 8-byte OpenTelemetry span identity.
    #[must_use]
    pub const fn span_id(&self) -> SpanId {
        self.span_id
    }
    /// Returns the optional 8-byte parent span identity.
    #[must_use]
    pub const fn parent_span_id(&self) -> Option<SpanId> {
        self.parent_span_id
    }
    /// Returns the closed span role.
    #[must_use]
    pub const fn kind(&self) -> SpanKind {
        self.kind
    }
    /// Returns exact Peritus domain correlation.
    #[must_use]
    pub const fn binding(&self) -> CausalBinding {
        self.binding
    }
    /// Returns observed start time.
    #[must_use]
    pub const fn start(&self) -> ObservedTime {
        self.start
    }
    /// Returns observed terminal time.
    #[must_use]
    pub const fn end(&self) -> ObservedTime {
        self.end
    }
    /// Returns the explicit terminal outcome.
    #[must_use]
    pub const fn outcome(&self) -> SpanOutcome {
        self.outcome
    }
    /// Borrows start-span safe attributes.
    #[must_use]
    pub fn attributes(&self) -> &[SafeAttribute] {
        &self.attributes
    }
    /// Borrows diagnostic events in span sequence order.
    #[must_use]
    pub fn events(&self) -> &[OtelEvent] {
        &self.events
    }
}

/// Complete deterministic telemetry projection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TelemetryProjection {
    records: Vec<ExportRecord>,
    metrics: MetricState,
}

impl TelemetryProjection {
    /// Borrows export records in deterministic journal/record order.
    #[must_use]
    pub fn records(&self) -> &[ExportRecord] {
        &self.records
    }
    /// Borrows cumulative stable metrics.
    #[must_use]
    pub const fn metrics(&self) -> &MetricState {
        &self.metrics
    }
}

/// Derives safe export records and metrics from a checked trace projection.
///
/// # Errors
///
/// Returns a checked metric overflow. The input projection has already validated all causality.
pub fn project_telemetry(
    state: &TraceProjectionState,
) -> Result<TelemetryProjection, TelemetryError> {
    let mut ordered = state
        .traces()
        .flat_map(|trace| trace.observations().iter().map(move |observation| (trace, observation)))
        .collect::<Vec<_>>();
    ordered.sort_unstable_by_key(|(_, observation)| observation.journal_position());
    let mut projection = TelemetryProjection::default();
    for (trace, projected) in ordered {
        let observation = projected.observation();
        match observation.kind() {
            ObservationKind::Diagnostic(code) => {
                let event = OtelEvent {
                    event_id: observation.event_id(),
                    trace_id: observation.trace_id(),
                    span_id: observation.span_id(),
                    time: observation.time(),
                    code,
                    attributes: observation.attributes().to_vec(),
                };
                projection.records.push(ExportRecord::Event(event));
                if let Some(point) =
                    projection.metrics.observe(code, observation.time(), observation.trace_id())?
                {
                    projection.records.push(ExportRecord::Metric(point));
                }
            }
            ObservationKind::SpanEnded(outcome) => {
                projection.records.push(ExportRecord::Span(project_span(
                    trace,
                    observation.span_id(),
                    outcome,
                )?));
            }
            ObservationKind::SpanStarted(_) => {}
        }
    }
    Ok(projection)
}

fn project_span(
    trace: &TraceSnapshot,
    span_id: SpanId,
    outcome: SpanOutcome,
) -> Result<OtelSpan, TelemetryError> {
    let span = trace.span(span_id).ok_or_else(|| {
        TelemetryError::new(
            TelemetryErrorKind::RecoveryMismatch,
            "project telemetry span",
            "checked trace projection is missing a terminal span",
        )
    })?;
    let mut attributes = Vec::new();
    let mut events = Vec::new();
    for projected in trace.observations() {
        let observation = projected.observation();
        if observation.span_id() != span_id {
            continue;
        }
        match observation.kind() {
            ObservationKind::SpanStarted(_) => attributes = observation.attributes().to_vec(),
            ObservationKind::Diagnostic(code) => events.push(OtelEvent {
                event_id: observation.event_id(),
                trace_id: observation.trace_id(),
                span_id,
                time: observation.time(),
                code,
                attributes: observation.attributes().to_vec(),
            }),
            ObservationKind::SpanEnded(_) => {}
        }
    }
    Ok(OtelSpan {
        trace_id: trace.trace_id(),
        span_id,
        parent_span_id: span.parent_span_id(),
        kind: span.kind(),
        binding: span.binding(),
        start: span.start(),
        end: span.latest(),
        outcome,
        attributes,
        events,
    })
}
