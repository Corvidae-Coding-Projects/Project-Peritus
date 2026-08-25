//! Complete bounded observation envelope and canonical collection checks.

use peritus_types::EventId;

use super::{
    MAX_CAUSAL_EVENTS, MAX_REDACTED_VALUES, MAX_SAFE_ATTRIBUTES, ObservationKind, SafeAttribute,
};
use crate::{
    ArtifactVaultReference, CausalBinding, RedactedValue, SpanId, TraceError, TraceErrorKind,
    TraceId,
};

/// Caller-observed wall and monotonic time used for projection only.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObservedTime {
    unix_nanos: u64,
    monotonic_tick: u64,
}

impl ObservedTime {
    /// Creates nonzero observed timestamps.
    ///
    /// # Errors
    ///
    /// Rejects a zero wall-clock or monotonic value.
    pub const fn new(unix_nanos: u64, monotonic_tick: u64) -> Result<Self, TraceError> {
        if unix_nanos == 0 || monotonic_tick == 0 {
            return Err(TraceError::static_error(
                TraceErrorKind::InvalidTransition,
                "validate observation time",
                "wall and monotonic observations must be nonzero",
            ));
        }
        Ok(Self { unix_nanos, monotonic_tick })
    }

    /// Returns Unix time in nanoseconds.
    #[must_use]
    pub const fn unix_nanos(self) -> u64 {
        self.unix_nanos
    }
    /// Returns the caller's monotonic tick.
    #[must_use]
    pub const fn monotonic_tick(self) -> u64 {
        self.monotonic_tick
    }
}

/// Complete canonical inert trace observation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Observation {
    event_id: EventId,
    trace_id: TraceId,
    span_id: SpanId,
    span_sequence: u64,
    parent_span_id: Option<SpanId>,
    causal_events: Vec<EventId>,
    binding: CausalBinding,
    time: ObservedTime,
    kind: ObservationKind,
    attributes: Vec<SafeAttribute>,
    redactions: Vec<RedactedValue>,
}

impl Observation {
    /// Validates and owns a complete observation.
    ///
    /// Collections must be strictly ordered and duplicate-free. The projection validates that
    /// declared predecessors exist and that the span transition is legal.
    ///
    /// # Errors
    ///
    /// Returns a typed bound, canonical-order, sequence, or structural-parent error.
    #[allow(clippy::too_many_arguments, reason = "the canonical observation envelope is explicit")]
    pub fn new(
        event_id: EventId,
        trace_id: TraceId,
        span_id: SpanId,
        span_sequence: u64,
        parent_span_id: Option<SpanId>,
        causal_events: Vec<EventId>,
        binding: CausalBinding,
        time: ObservedTime,
        kind: ObservationKind,
        attributes: Vec<SafeAttribute>,
        redactions: Vec<RedactedValue>,
    ) -> Result<Self, TraceError> {
        if span_sequence == 0 {
            return Err(sequence_error("span sequence must be one-based"));
        }
        if causal_events.len() > MAX_CAUSAL_EVENTS
            || attributes.len() > MAX_SAFE_ATTRIBUTES
            || redactions.len() > MAX_REDACTED_VALUES
        {
            return Err(TraceError::static_error(
                TraceErrorKind::LimitExceeded,
                "validate trace observation",
                "observation collection bound exceeded",
            ));
        }
        if parent_span_id == Some(span_id) {
            return Err(causal_error("a span cannot be its own structural parent"));
        }
        validate_order(&causal_events, "causal events must be strictly ordered")?;
        if causal_events.binary_search(&event_id).is_ok() {
            return Err(causal_error("an observation cannot causally precede itself"));
        }
        validate_attributes(&attributes)?;
        validate_redactions(&redactions)?;
        let start = matches!(kind, ObservationKind::SpanStarted(_));
        if start != (span_sequence == 1) {
            return Err(sequence_error(
                "only a span start may use sequence one and every start must use it",
            ));
        }
        Ok(Self {
            event_id,
            trace_id,
            span_id,
            span_sequence,
            parent_span_id,
            causal_events,
            binding,
            time,
            kind,
            attributes,
            redactions,
        })
    }

    /// Returns the C0 event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the trace identity.
    #[must_use]
    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }
    /// Returns the span identity.
    #[must_use]
    pub const fn span_id(&self) -> SpanId {
        self.span_id
    }
    /// Returns the one-based sequence within the span.
    #[must_use]
    pub const fn span_sequence(&self) -> u64 {
        self.span_sequence
    }
    /// Returns the structural parent span.
    #[must_use]
    pub const fn parent_span_id(&self) -> Option<SpanId> {
        self.parent_span_id
    }
    /// Borrows canonical prior event identities.
    #[must_use]
    pub fn causal_events(&self) -> &[EventId] {
        &self.causal_events
    }
    /// Returns immutable entity correlation.
    #[must_use]
    pub const fn binding(&self) -> CausalBinding {
        self.binding
    }
    /// Returns caller-observed time.
    #[must_use]
    pub const fn time(&self) -> ObservedTime {
        self.time
    }
    /// Returns the lifecycle or diagnostic kind.
    #[must_use]
    pub const fn kind(&self) -> ObservationKind {
        self.kind
    }
    /// Borrows closed safe attributes.
    #[must_use]
    pub fn attributes(&self) -> &[SafeAttribute] {
        &self.attributes
    }
    /// Borrows explicit redaction decisions.
    #[must_use]
    pub fn redactions(&self) -> &[RedactedValue] {
        &self.redactions
    }

    /// Returns finalized encrypted vault references in canonical sensitivity order.
    #[must_use]
    pub fn vault_references(&self) -> Vec<ArtifactVaultReference> {
        self.redactions.iter().filter_map(|value| value.vault_reference()).collect()
    }
}

fn validate_attributes(attributes: &[SafeAttribute]) -> Result<(), TraceError> {
    if attributes.windows(2).any(|pair| pair[0].key() >= pair[1].key()) {
        Err(noncanonical("safe attribute keys must be strictly ordered"))
    } else {
        Ok(())
    }
}

fn validate_redactions(redactions: &[RedactedValue]) -> Result<(), TraceError> {
    if redactions.windows(2).any(|pair| pair[0].class() >= pair[1].class()) {
        Err(noncanonical("redaction classes must be strictly ordered"))
    } else {
        Ok(())
    }
}

fn validate_order<T: Ord>(values: &[T], detail: &'static str) -> Result<(), TraceError> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        Err(noncanonical(detail))
    } else {
        Ok(())
    }
}

const fn noncanonical(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::NonCanonical, "validate trace observation", detail)
}

const fn sequence_error(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::Sequence, "validate trace observation", detail)
}

const fn causal_error(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::CausalIntegrity, "validate trace observation", detail)
}
