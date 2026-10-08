//! Deterministic projection payload and invariant encoding.

use peritus_codec::{CodecLimits, decode_message, sha256};
use peritus_projection::{ProjectionError, ProjectionErrorKind, ProjectionState, RecoveryClass};
use peritus_types::Sha256Digest;

use super::{projection_error, state::TraceProjectionState};
use crate::{Observation, SpanId, SpanOutcome};

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

    fn decode(payload: &[u8]) -> Result<Self, ProjectionError> {
        let mut reader = Reader::new(payload, b"peritus-trace-projection-v1\0")?;
        let declared_count = reader.u64()?;
        let declared_last_position = reader.u64()?;
        let trace_count = reader.count(48)?;
        let mut observations = Vec::new();
        for _ in 0..trace_count {
            reader.skip(16 + 16)?;
            let span_count = reader.count(67)?;
            for _ in 0..span_count {
                reader.skip(8)?;
                match reader.u8()? {
                    0 => {}
                    1 => reader.skip(8)?,
                    _ => return Err(restore_error("trace checkpoint parent tag is invalid")),
                }
                reader.skip(1 + 8 + 8 + 8 + 8 + 8 + 16 + 1)?;
            }
            let observation_count = reader.count(48)?;
            for _ in 0..observation_count {
                let position = reader.u64()?;
                let digest = Sha256Digest::new(reader.array()?);
                let length = usize::try_from(reader.u64()?)
                    .map_err(|_| restore_error("trace checkpoint frame length overflows"))?;
                let frame = reader.bytes(length)?.to_vec();
                if sha256(&frame) != digest {
                    return Err(restore_error("trace checkpoint frame digest is invalid"));
                }
                observations.push((position, frame));
            }
        }
        reader.finish()?;
        observations.sort_by_key(|(position, _)| *position);
        let mut state = TraceProjectionState::default();
        for (position, frame) in observations {
            let observation = decode_message::<Observation>(&frame, CodecLimits::PRODUCTION)
                .map_err(|_| restore_error("trace checkpoint observation is invalid"))?;
            state
                .apply(observation, position)
                .map_err(|_| restore_error("trace checkpoint causal state is invalid"))?;
        }
        state.validate()?;
        if state.observation_count != declared_count
            || state.last_journal_position != declared_last_position
            || state.encode() != payload
        {
            return Err(restore_error("trace checkpoint is not canonical"));
        }
        Ok(state)
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

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], prefix: &[u8]) -> Result<Self, ProjectionError> {
        if !bytes.starts_with(prefix) {
            return Err(restore_error("trace checkpoint prefix is invalid"));
        }
        Ok(Self { bytes, offset: prefix.len() })
    }

    fn u8(&mut self) -> Result<u8, ProjectionError> {
        Ok(self.array::<1>()?[0])
    }

    fn u64(&mut self) -> Result<u64, ProjectionError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn count(&mut self, minimum_bytes: usize) -> Result<usize, ProjectionError> {
        let count = usize::try_from(self.u64()?)
            .map_err(|_| restore_error("trace checkpoint item count overflows"))?;
        if minimum_bytes == 0 || count > self.remaining() / minimum_bytes {
            return Err(restore_error("trace checkpoint item count exceeds its payload"));
        }
        Ok(count)
    }

    fn skip(&mut self, length: usize) -> Result<(), ProjectionError> {
        self.bytes(length).map(|_| ())
    }

    fn bytes(&mut self, length: usize) -> Result<&'a [u8], ProjectionError> {
        let end = self
            .offset
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| restore_error("trace checkpoint is truncated"))?;
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProjectionError> {
        self.bytes(N)?
            .try_into()
            .map_err(|_| restore_error("trace checkpoint field has an invalid length"))
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn finish(self) -> Result<(), ProjectionError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(restore_error("trace checkpoint has trailing bytes"))
        }
    }
}

fn restore_error(detail: &'static str) -> ProjectionError {
    ProjectionError::fold(
        ProjectionErrorKind::CorruptCatalog,
        RecoveryClass::Rebuild,
        "decode trace projection checkpoint",
        detail,
    )
}
