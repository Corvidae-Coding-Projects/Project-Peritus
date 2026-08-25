//! Projection state values and observation application.

use std::{
    collections::{BTreeMap, btree_map::Values},
    iter::Copied,
};

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_journal::{AggregateKind, CommittedRecord};
use peritus_types::{EventId, Sha256Digest};

use super::fold::apply_span;
use crate::{
    ApplyOutcome, CausalBinding, Observation, ObservedTime, SpanId, SpanKind, SpanOutcome,
    TraceError, TraceErrorKind, TraceId, trace_schema_digest,
};

/// One immutable projected observation and its C0 provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedObservation {
    pub(super) observation: Observation,
    pub(super) frame_digest: Sha256Digest,
    pub(super) frame: Vec<u8>,
    pub(super) journal_position: u64,
}

impl ProjectedObservation {
    /// Borrows the canonical observation.
    #[must_use]
    pub const fn observation(&self) -> &Observation {
        &self.observation
    }
    /// Returns the exact canonical frame digest.
    #[must_use]
    pub const fn frame_digest(&self) -> Sha256Digest {
        self.frame_digest
    }
    /// Borrows the exact family-60 frame.
    #[must_use]
    pub fn frame_bytes(&self) -> &[u8] {
        &self.frame
    }
    /// Returns the one-based global C0 position.
    #[must_use]
    pub const fn journal_position(&self) -> u64 {
        self.journal_position
    }
}

/// Latest validated state of one span.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpanSnapshot {
    pub(super) span_id: SpanId,
    pub(super) parent_span_id: Option<SpanId>,
    pub(super) kind: SpanKind,
    pub(super) binding: CausalBinding,
    pub(super) sequence: u64,
    pub(super) start: ObservedTime,
    pub(super) latest: ObservedTime,
    pub(super) latest_event: EventId,
    pub(super) outcome: Option<SpanOutcome>,
}

impl SpanSnapshot {
    /// Returns the span identity.
    #[must_use]
    pub const fn span_id(self) -> SpanId {
        self.span_id
    }
    /// Returns the structural parent span.
    #[must_use]
    pub const fn parent_span_id(self) -> Option<SpanId> {
        self.parent_span_id
    }
    /// Returns the span role.
    #[must_use]
    pub const fn kind(self) -> SpanKind {
        self.kind
    }
    /// Returns immutable entity correlation.
    #[must_use]
    pub const fn binding(self) -> CausalBinding {
        self.binding
    }
    /// Returns the latest one-based span sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Returns the start time.
    #[must_use]
    pub const fn start(self) -> ObservedTime {
        self.start
    }
    /// Returns the latest observed time.
    #[must_use]
    pub const fn latest(self) -> ObservedTime {
        self.latest
    }
    /// Returns the latest event identity.
    #[must_use]
    pub const fn latest_event(self) -> EventId {
        self.latest_event
    }
    /// Returns the terminal outcome, or `None` while open.
    #[must_use]
    pub const fn outcome(self) -> Option<SpanOutcome> {
        self.outcome
    }
    /// Returns whether the span remains open.
    #[must_use]
    pub const fn is_open(self) -> bool {
        self.outcome.is_none()
    }
}

/// Complete projected state for one trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceSnapshot {
    pub(super) trace_id: TraceId,
    pub(super) session: [u8; 16],
    pub(super) spans: BTreeMap<SpanId, SpanSnapshot>,
    pub(super) observations: Vec<ProjectedObservation>,
}

impl TraceSnapshot {
    /// Returns the trace identity.
    #[must_use]
    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }
    /// Returns the number of spans.
    #[must_use]
    pub fn span_count(&self) -> usize {
        self.spans.len()
    }
    /// Looks up one span.
    #[must_use]
    pub fn span(&self, span_id: SpanId) -> Option<SpanSnapshot> {
        self.spans.get(&span_id).copied()
    }
    /// Iterates spans in canonical span-identity order.
    pub fn spans(&self) -> Copied<Values<'_, SpanId, SpanSnapshot>> {
        self.spans.values().copied()
    }
    /// Borrows observations in global journal order.
    #[must_use]
    pub fn observations(&self) -> &[ProjectedObservation] {
        &self.observations
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SeenEvent {
    trace_id: TraceId,
    frame_digest: Sha256Digest,
}

/// Deterministic complete trace projection state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TraceProjectionState {
    pub(super) traces: BTreeMap<TraceId, TraceSnapshot>,
    pub(super) seen_events: BTreeMap<EventId, SeenEvent>,
    pub(super) observation_count: u64,
    pub(super) last_journal_position: u64,
}

impl TraceProjectionState {
    /// Returns the number of distinct traces.
    #[must_use]
    pub fn trace_count(&self) -> usize {
        self.traces.len()
    }
    /// Returns the number of newly applied observations.
    #[must_use]
    pub const fn observation_count(&self) -> u64 {
        self.observation_count
    }
    /// Returns the latest trace-observation journal position, or zero at genesis.
    #[must_use]
    pub const fn last_journal_position(&self) -> u64 {
        self.last_journal_position
    }
    /// Looks up one trace.
    #[must_use]
    pub fn trace(&self, trace_id: TraceId) -> Option<&TraceSnapshot> {
        self.traces.get(&trace_id)
    }
    /// Iterates traces in canonical trace-identity order.
    pub fn traces(&self) -> Values<'_, TraceId, TraceSnapshot> {
        self.traces.values()
    }

    /// Applies one validated observation outside C0 replay, retaining its canonical bytes.
    ///
    /// # Errors
    ///
    /// Returns canonical codec, causal, sequence, lifecycle, duplicate, or time failures.
    pub fn apply(
        &mut self,
        observation: Observation,
        journal_position: u64,
    ) -> Result<ApplyOutcome, TraceError> {
        let frame = encode_message(&observation, CodecLimits::PRODUCTION)
            .map_err(|_| TraceError::codec("encode trace observation"))?;
        let digest = sha256(&frame);
        self.apply_checked(observation, frame, digest, journal_position)
    }

    pub(crate) fn apply_record(
        &mut self,
        record: &CommittedRecord,
    ) -> Result<ApplyOutcome, TraceError> {
        if record.aggregate().kind() != AggregateKind::Trace {
            return Err(integrity("trace projection received a non-trace aggregate"));
        }
        if record.revision_digest() != trace_schema_digest() {
            return Err(integrity("trace record schema digest changed"));
        }
        let observation =
            decode_message::<Observation>(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(|_| TraceError::codec("decode committed trace observation"))?;
        if observation.trace_id().as_bytes() != record.aggregate().id().as_bytes()
            || observation.event_id() != record.event_id()
            || observation.causal_events() != record.causal_parents()
        {
            return Err(integrity("trace frame disagrees with its C0 envelope"));
        }
        self.apply_checked(
            observation,
            record.frame_bytes().to_vec(),
            record.frame_digest(),
            record.global_position(),
        )
    }

    fn apply_checked(
        &mut self,
        observation: Observation,
        frame: Vec<u8>,
        frame_digest: Sha256Digest,
        journal_position: u64,
    ) -> Result<ApplyOutcome, TraceError> {
        if let Some(seen) = self.seen_events.get(&observation.event_id()) {
            return if seen.frame_digest == frame_digest && seen.trace_id == observation.trace_id() {
                Ok(ApplyOutcome::ExactDuplicate)
            } else {
                Err(TraceError::static_error(
                    TraceErrorKind::DuplicateConflict,
                    "apply trace observation",
                    "event identity is already bound to different canonical bytes",
                ))
            };
        }
        if journal_position == 0 || journal_position <= self.last_journal_position {
            return Err(sequence("trace journal positions must strictly increase"));
        }
        self.validate_causal_events(&observation)?;
        let next_observation_count = self
            .observation_count
            .checked_add(1)
            .ok_or_else(|| sequence("trace observation count overflow"))?;
        let trace_id = observation.trace_id();
        let event_id = observation.event_id();
        let session = observation.binding().session_id().into_bytes();
        let mut candidate = self.traces.get(&trace_id).map_or_else(
            || TraceSnapshot {
                trace_id,
                session,
                spans: BTreeMap::new(),
                observations: Vec::new(),
            },
            |snapshot| TraceSnapshot {
                trace_id,
                session: snapshot.session,
                spans: snapshot.spans.clone(),
                observations: Vec::new(),
            },
        );
        if candidate.session != session {
            return Err(causal("a trace cannot cross session identity"));
        }
        apply_span(&mut candidate, &observation)?;
        let projected = ProjectedObservation { observation, frame_digest, frame, journal_position };

        if let Some(snapshot) = self.traces.get_mut(&trace_id) {
            snapshot.spans = candidate.spans;
            snapshot.observations.push(projected);
        } else {
            candidate.observations.push(projected);
            self.traces.insert(trace_id, candidate);
        }
        self.seen_events.insert(event_id, SeenEvent { trace_id, frame_digest });
        self.observation_count = next_observation_count;
        self.last_journal_position = journal_position;
        Ok(ApplyOutcome::Applied)
    }

    fn validate_causal_events(&self, observation: &Observation) -> Result<(), TraceError> {
        for event_id in observation.causal_events() {
            let Some(seen) = self.seen_events.get(event_id) else {
                return Err(causal("causal predecessor has not been observed"));
            };
            if seen.trace_id != observation.trace_id() {
                return Err(causal("causal predecessor belongs to another trace"));
            }
        }
        Ok(())
    }
}

const fn causal(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::CausalIntegrity, "apply trace observation", detail)
}

const fn sequence(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::Sequence, "apply trace observation", detail)
}

const fn integrity(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::Integrity, "replay trace observation", detail)
}
