//! Deterministic projection payload and invariant encoding.

use peritus_codec::sha256;
use peritus_projection::{ProjectionError, ProjectionErrorKind, ProjectionState};
use peritus_types::Sha256Digest;

use super::{projection_error, state::TraceProjectionState};
use crate::{SpanId, SpanOutcome};

impl ProjectionState for TraceProjectionState {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = b"peritus-trace-projection-v1\0".to_vec();
        put_u64(&mut bytes, self.observation_count);
        put_u64(&mut bytes, self.last_journal_position);
        put_u64(&mut bytes, self.traces.len() as u64);
        for (trace_id, trace) in &self.traces {
            bytes.extend_from_slice(trace_id.as_bytes());
            bytes.extend_from_slice(&trace.session);
            put_u64(&mut bytes, trace.spans.len() as u64);
            for span in trace.spans.values() {
                bytes.extend_from_slice(span.span_id.as_bytes());
                put_option_span(&mut bytes, span.parent_span_id);
                bytes.push(span.kind.tag());
                put_u64(&mut bytes, span.sequence);
                put_u64(&mut bytes, span.start.unix_nanos());
                put_u64(&mut bytes, span.start.monotonic_tick());
                put_u64(&mut bytes, span.latest.unix_nanos());
                put_u64(&mut bytes, span.latest.monotonic_tick());
                bytes.extend_from_slice(span.latest_event.as_bytes());
                bytes.push(span.outcome.map_or(0, SpanOutcome::tag));
            }
            put_u64(&mut bytes, trace.observations.len() as u64);
            for observation in &trace.observations {
                put_u64(&mut bytes, observation.journal_position);
                bytes.extend_from_slice(observation.frame_digest.as_bytes());
                put_u64(&mut bytes, observation.frame.len() as u64);
                bytes.extend_from_slice(&observation.frame);
            }
        }
        bytes
    }

    fn validate(&self) -> Result<(), ProjectionError> {
        let counted = self
            .traces
            .values()
            .try_fold(0_u64, |sum, trace| sum.checked_add(trace.observations.len() as u64));
        let valid = counted == Some(self.observation_count)
            && usize::try_from(self.observation_count).ok() == Some(self.seen_events.len())
            && self.traces.values().all(|trace| {
                trace.spans.values().all(|span| {
                    span.sequence > 0
                        && span.start.monotonic_tick() <= span.latest.monotonic_tick()
                        && span.start.unix_nanos() <= span.latest.unix_nanos()
                })
            });
        if valid {
            Ok(())
        } else {
            Err(projection_error(
                ProjectionErrorKind::FoldInvariant,
                "trace projection state invariant failed",
            ))
        }
    }

    fn invariant_digest(&self) -> Sha256Digest {
        let mut bytes = b"peritus-trace-projection-invariants-v1\0".to_vec();
        bytes.extend_from_slice(&self.encode());
        sha256(&bytes)
    }
}

fn put_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn put_option_span(bytes: &mut Vec<u8>, value: Option<SpanId>) {
    match value {
        Some(span) => {
            bytes.push(1);
            bytes.extend_from_slice(span.as_bytes());
        }
        None => bytes.push(0),
    }
}
