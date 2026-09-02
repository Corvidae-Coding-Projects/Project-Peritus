//! Bounded combined-stream reading and deterministic PTY event reduction.

use std::io::Read;

use super::{
    BUFFER_LIMIT_BYTES_U64, MAXIMUM_EVENTS, PtyQualificationError, PtyQualificationErrorKind,
    PtyQualificationObservation,
};

#[derive(Clone, Copy)]
pub(super) enum QualifiedStream {
    Terminal,
}

#[derive(Clone, Copy)]
pub(super) enum QualifiedEvent {
    Output { sequence: u64, offset: u64, bytes: u64, stream: QualifiedStream },
    Exited { sequence: u64, offset: u64 },
}

impl QualifiedEvent {
    const fn sequence(self) -> u64 {
        match self {
            Self::Output { sequence, .. } | Self::Exited { sequence, .. } => sequence,
        }
    }
}

pub(super) fn read_terminal(
    mut reader: Box<dyn Read + Send>,
    mut events: Vec<QualifiedEvent>,
    mut read_buffer: Vec<u8>,
) -> Result<(Vec<QualifiedEvent>, u64, u64), PtyQualificationError> {
    let mut output_offset = 0_u64;
    let mut peak_buffered_bytes = 0_u64;
    loop {
        let count = reader.read(&mut read_buffer).map_err(|source| {
            PtyQualificationError::with_source(
                PtyQualificationErrorKind::Read,
                "read combined terminal stream",
                "the PTY reader failed before its end fence",
                Box::new(source),
            )
        })?;
        if count == 0 {
            break;
        }
        let bytes = u64::try_from(count).map_err(|_| {
            PtyQualificationError::rejected(
                PtyQualificationErrorKind::Arithmetic,
                "convert terminal read length",
                "the observed byte count cannot be represented",
            )
        })?;
        peak_buffered_bytes = peak_buffered_bytes.max(bytes);
        let sequence = next_sequence(&events)?;
        push_event(
            &mut events,
            QualifiedEvent::Output {
                sequence,
                offset: output_offset,
                bytes,
                stream: QualifiedStream::Terminal,
            },
        )?;
        output_offset = output_offset.checked_add(bytes).ok_or_else(|| {
            PtyQualificationError::rejected(
                PtyQualificationErrorKind::Arithmetic,
                "advance terminal output offset",
                "the observed terminal offset overflowed",
            )
        })?;
    }
    Ok((events, output_offset, peak_buffered_bytes))
}

pub(super) fn push_event(
    events: &mut Vec<QualifiedEvent>,
    event: QualifiedEvent,
) -> Result<(), PtyQualificationError> {
    if events.len() >= MAXIMUM_EVENTS {
        return Err(PtyQualificationError::rejected(
            PtyQualificationErrorKind::EventLimit,
            "record terminal observation",
            "the real PTY stream exceeded its event ceiling",
        ));
    }
    events.push(event);
    Ok(())
}

pub(super) fn next_sequence(events: &[QualifiedEvent]) -> Result<u64, PtyQualificationError> {
    u64::try_from(events.len()).map_err(|_| {
        PtyQualificationError::rejected(
            PtyQualificationErrorKind::Arithmetic,
            "assign terminal event sequence",
            "the event sequence cannot be represented",
        )
    })
}

pub(super) fn summarize(
    events: &[QualifiedEvent],
    peak_buffered_bytes: u64,
) -> PtyQualificationObservation {
    let mut previous_sequence = None;
    let mut next_offset = 0_u64;
    let mut sequence_strictly_increasing = true;
    let mut offsets_conserved = true;
    let mut combined_stream_only = true;
    let mut exit_records = 0_u64;
    let mut exit_seen = false;
    for event in events {
        let sequence = event.sequence();
        if previous_sequence.is_some_and(|previous| sequence <= previous) {
            sequence_strictly_increasing = false;
        }
        previous_sequence = Some(sequence);
        match *event {
            QualifiedEvent::Output { offset, bytes, stream, .. } => {
                offsets_conserved &= !exit_seen && offset == next_offset;
                next_offset = next_offset.checked_add(bytes).unwrap_or_else(|| {
                    offsets_conserved = false;
                    u64::MAX
                });
                combined_stream_only &= matches!(stream, QualifiedStream::Terminal);
            }
            QualifiedEvent::Exited { offset, .. } => {
                exit_records = exit_records.saturating_add(1);
                offsets_conserved &= !exit_seen && offset == next_offset;
                exit_seen = true;
            }
        }
    }
    PtyQualificationObservation {
        output_bytes: next_offset,
        sequence_strictly_increasing,
        offsets_conserved,
        combined_stream_only,
        exit_records,
        peak_buffered_bytes,
        configured_buffer_limit: BUFFER_LIMIT_BYTES_U64,
    }
}
