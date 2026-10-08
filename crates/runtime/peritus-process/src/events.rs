//! Ordered bounded process events.

use std::collections::VecDeque;

use peritus_types::{ProcessId, Sha256Digest};

use crate::{CancellationReason, OutputStream, ProcessSignal, TerminalSize};

mod retained;

/// Stable kind of one execution observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessEventKind {
    /// Durable execution intent was accepted.
    IntentPersisted,
    /// The operating-system spawn attempt began.
    SpawnAttempt,
    /// Startup and tree identity were observed.
    Started {
        /// Root operating-system process identifier observed after spawn.
        root_pid: u32,
    },
    /// Process stream bytes were observed.
    Output(OutputStream),
    /// A bounded stdin write was accepted.
    StdinAccepted {
        /// Number of input bytes accepted by this event.
        bytes: u64,
    },
    /// Process input was closed.
    StdinClosed,
    /// A PTY resize was applied.
    Resized(TerminalSize),
    /// A portable non-cancelling signal was delivered.
    Signalled(ProcessSignal),
    /// The first cancellation trigger was accepted.
    Cancellation(CancellationReason),
    /// Forced process-tree termination was applied.
    Escalated,
    /// One resource sample was observed.
    ResourceSample,
    /// A resource ceiling was crossed.
    ResourceLimit,
    /// The sandbox backend emitted a bounded observation.
    SandboxObservation,
    /// The operating-system root exit was observed.
    OsExit,
    /// The owned process tree became quiescent.
    TreeQuiescent,
    /// Output completeness was fixed.
    OutputClosed,
    /// A retained stream was finalized as an artifact.
    ArtifactPublished,
    /// The unique terminal result was accepted.
    TerminalPublished,
}

/// One ordered bounded process observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessEvent {
    process_id: ProcessId,
    plan_digest: Sha256Digest,
    sequence: u64,
    stream_offset: Option<u64>,
    stream_offsets_before: Option<[u64; 3]>,
    stream_offsets_after: Option<[u64; 3]>,
    loss: Option<ProcessEventLoss>,
    kind: ProcessEventKind,
    data: Vec<u8>,
}

impl ProcessEvent {
    pub(crate) const fn new(
        process_id: ProcessId,
        plan_digest: Sha256Digest,
        sequence: u64,
        stream_offset: Option<u64>,
        stream_offsets_before: Option<[u64; 3]>,
        stream_offsets_after: Option<[u64; 3]>,
        kind: ProcessEventKind,
        data: Vec<u8>,
    ) -> Self {
        Self {
            process_id,
            plan_digest,
            sequence,
            stream_offset,
            stream_offsets_before,
            stream_offsets_after,
            loss: None,
            kind,
            data,
        }
    }

    /// Returns the process identity.
    #[must_use]
    pub const fn process_id(&self) -> ProcessId {
        self.process_id
    }
    /// Returns the exact execution-plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns the monotonic nonzero event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Returns the stream byte offset when applicable.
    #[must_use]
    pub const fn stream_offset(&self) -> Option<u64> {
        self.stream_offset
    }
    /// Returns the exact cursor gap preceding this event, when retained history was evicted.
    ///
    /// The marker is attached only to the first event returned for a cursor that fell behind the
    /// bounded event window. Its sequence range names every omitted event. Output ranges are also
    /// exact when the cursor was advanced from previously returned events.
    #[must_use]
    pub const fn loss(&self) -> Option<ProcessEventLoss> {
        self.loss
    }
    /// Returns the stable event kind.
    #[must_use]
    pub const fn kind(&self) -> &ProcessEventKind {
        &self.kind
    }
    /// Returns bounded event data.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub(crate) fn select_for_retained_cursor(&mut self, cursor: ProcessCursor) {
        let Some(first_missing) = cursor.sequence().checked_add(1) else {
            self.loss = None;
            return;
        };
        if self.sequence() <= first_missing {
            self.loss = None;
            return;
        }
        let output_offsets = match (cursor.stream_offsets, self.stream_offsets_before) {
            (Some(cursor), Some(resume))
                if cursor.iter().zip(resume).all(|(cursor, resume)| *cursor <= resume) =>
            {
                Some([
                    [cursor[0], resume[0]],
                    [cursor[1], resume[1]],
                    [cursor[2], resume[2]],
                ])
            }
            _ => None,
        };
        self.loss = Some(ProcessEventLoss::new(
            cursor.sequence(),
            self.sequence().saturating_sub(1),
            output_offsets,
        ));
    }
}

/// Exact bounded-history gap attached to the event that resumes observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessEventLoss {
    cursor_sequence: u64,
    through_sequence: u64,
    output_offsets: Option<[[u64; 2]; 3]>,
}

impl ProcessEventLoss {
    const fn new(
        cursor_sequence: u64,
        through_sequence: u64,
        output_offsets: Option<[[u64; 2]; 3]>,
    ) -> Self {
        Self { cursor_sequence, through_sequence, output_offsets }
    }

    /// Returns the caller cursor immediately before the omitted event range.
    #[must_use]
    pub const fn cursor_sequence(self) -> u64 {
        self.cursor_sequence
    }

    /// Returns the final omitted event sequence. The carrying event resumes at the next sequence.
    #[must_use]
    pub const fn through_sequence(self) -> u64 {
        self.through_sequence
    }

    /// Returns whether exact per-stream offsets could be bound to this cursor gap.
    ///
    /// Sequence-only cursors created after a nonzero sequence retain an explicit sequence loss,
    /// but cannot reconstruct output offsets that the caller did not supply.
    #[must_use]
    pub const fn output_offsets_exact(self) -> bool {
        self.output_offsets.is_some()
    }

    /// Returns the inclusive start and exclusive end offsets of omitted bytes for one stream.
    #[must_use]
    pub const fn output_range(self, stream: OutputStream) -> Option<(u64, u64)> {
        let ranges = match self.output_offsets {
            Some(ranges) => ranges,
            None => return None,
        };
        let range = ranges[stream_index(stream)];
        if range[0] == range[1] { None } else { Some((range[0], range[1])) }
    }
}

/// Cursor into a retained bounded event window.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessCursor {
    after_sequence: u64,
    stream_offsets: Option<[u64; 3]>,
}

impl ProcessCursor {
    /// Creates a cursor after the supplied sequence; zero reads from the oldest retained event.
    #[must_use]
    pub const fn after(after_sequence: u64) -> Self {
        Self {
            after_sequence,
            stream_offsets: if after_sequence == 0 { Some([0; 3]) } else { None },
        }
    }

    /// Creates a cursor after a returned event while preserving exact per-stream output offsets.
    #[must_use]
    pub const fn after_event(event: &ProcessEvent) -> Self {
        Self { after_sequence: event.sequence, stream_offsets: event.stream_offsets_after }
    }
    /// Returns the exclusive prior sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.after_sequence
    }
}

pub(crate) struct EventLog {
    events: VecDeque<ProcessEvent>,
    limit: usize,
    next_sequence: u64,
    dropped: u64,
    stream_offsets: Option<[u64; 3]>,
}

impl EventLog {
    pub(crate) fn new(limit: u64) -> Self {
        Self {
            events: VecDeque::new(),
            limit: usize::try_from(limit).unwrap_or(usize::MAX),
            next_sequence: 1,
            dropped: 0,
            stream_offsets: Some([0; 3]),
        }
    }

    pub(crate) fn push(
        &mut self,
        process_id: ProcessId,
        plan_digest: Sha256Digest,
        stream_offset: Option<u64>,
        kind: ProcessEventKind,
        data: Vec<u8>,
    ) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        let stream_offsets_before = self.stream_offsets;
        let stream_offsets_after = advance_stream_offsets(
            stream_offsets_before,
            stream_offset,
            &kind,
            data.len(),
        );
        self.stream_offsets = stream_offsets_after;
        if self.limit == 0 {
            self.dropped = self.dropped.saturating_add(1);
            return sequence;
        }
        if self.events.len() >= self.limit {
            self.events.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.events.push_back(ProcessEvent::new(
            process_id,
            plan_digest,
            sequence,
            stream_offset,
            stream_offsets_before,
            stream_offsets_after,
            kind,
            data,
        ));
        sequence
    }

    pub(crate) fn read(&self, cursor: ProcessCursor, max_events: usize) -> Vec<ProcessEvent> {
        let mut events: Vec<_> = self
            .events
            .iter()
            .filter(|event| event.sequence() > cursor.sequence())
            .take(max_events)
            .cloned()
            .collect();
        let Some(first) = events.first_mut() else { return events };
        first.select_for_retained_cursor(cursor);
        events
    }

    pub(crate) const fn dropped(&self) -> u64 {
        self.dropped
    }
}

fn advance_stream_offsets(
    offsets: Option<[u64; 3]>,
    stream_offset: Option<u64>,
    kind: &ProcessEventKind,
    data_length: usize,
) -> Option<[u64; 3]> {
    let ProcessEventKind::Output(stream) = kind else { return offsets };
    let mut offsets = offsets?;
    let index = stream_index(*stream);
    let offset = stream_offset?;
    if offset != offsets[index] {
        return None;
    }
    let length = u64::try_from(data_length).ok()?;
    offsets[index] = offset.checked_add(length)?;
    Some(offsets)
}

const fn stream_index(stream: OutputStream) -> usize {
    match stream {
        OutputStream::Stdout => 0,
        OutputStream::Stderr => 1,
        OutputStream::Terminal => 2,
    }
}
