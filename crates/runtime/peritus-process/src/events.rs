//! Ordered bounded process events.

use std::collections::VecDeque;

use peritus_types::{ProcessId, Sha256Digest};

use crate::{CancellationReason, OutputStream, ProcessSignal, TerminalSize};

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
    kind: ProcessEventKind,
    data: Vec<u8>,
}

impl ProcessEvent {
    pub(crate) const fn new(
        process_id: ProcessId,
        plan_digest: Sha256Digest,
        sequence: u64,
        stream_offset: Option<u64>,
        kind: ProcessEventKind,
        data: Vec<u8>,
    ) -> Self {
        Self { process_id, plan_digest, sequence, stream_offset, kind, data }
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
}

/// Cursor into a retained bounded event window.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessCursor {
    after_sequence: u64,
}

impl ProcessCursor {
    /// Creates a cursor after the supplied sequence; zero reads from the oldest retained event.
    #[must_use]
    pub const fn after(after_sequence: u64) -> Self {
        Self { after_sequence }
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
}

impl EventLog {
    pub(crate) fn new(limit: u64) -> Self {
        Self {
            events: VecDeque::new(),
            limit: usize::try_from(limit).unwrap_or(usize::MAX),
            next_sequence: 1,
            dropped: 0,
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
        if self.events.len() == self.limit {
            self.events.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.events.push_back(ProcessEvent::new(
            process_id,
            plan_digest,
            sequence,
            stream_offset,
            kind,
            data,
        ));
        sequence
    }

    pub(crate) fn read(&self, cursor: ProcessCursor, max_events: usize) -> Vec<ProcessEvent> {
        self.events
            .iter()
            .filter(|event| event.sequence() > cursor.sequence())
            .take(max_events)
            .cloned()
            .collect()
    }

    pub(crate) const fn dropped(&self) -> u64 {
        self.dropped
    }
}
