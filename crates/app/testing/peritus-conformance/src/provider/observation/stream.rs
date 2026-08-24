//! Normalized stream observations.

/// Normalized semantic class of one observed event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderEventKind {
    /// Response start.
    ResponseStarted,
    /// Output item start.
    ItemStarted,
    /// Text fragment.
    TextDelta,
    /// Tool call start.
    ToolCallStarted,
    /// Tool arguments fragment.
    ToolArgumentDelta,
    /// Item close.
    ItemCompleted,
    /// Usage snapshot.
    Usage,
    /// Finish reason.
    Finish,
    /// Success terminal.
    ResponseCompleted,
    /// Failure terminal.
    ResponseFailed,
    /// Cancellation terminal.
    ResponseCancelled,
}

/// Ordering and identity facts for one normalized event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderEventObservation {
    sequence: u64,
    provider_sequence: Option<u64>,
    identity_digest: [u8; 32],
    payload_digest: [u8; 32],
    kind: ProviderEventKind,
    fragment_bytes: u64,
}

impl ProviderEventObservation {
    /// Creates one direct event observation.
    #[must_use]
    pub const fn new(
        sequence: u64,
        provider_sequence: Option<u64>,
        identity_digest: [u8; 32],
        payload_digest: [u8; 32],
        kind: ProviderEventKind,
        fragment_bytes: u64,
    ) -> Self {
        Self { sequence, provider_sequence, identity_digest, payload_digest, kind, fragment_bytes }
    }

    /// Returns adapter-local sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Returns provider sequence when documented.
    #[must_use]
    pub const fn provider_sequence(self) -> Option<u64> {
        self.provider_sequence
    }
    /// Returns the redacted provider-event identity digest.
    #[must_use]
    pub const fn identity_digest(self) -> [u8; 32] {
        self.identity_digest
    }
    /// Returns the exact raw-event payload digest.
    #[must_use]
    pub const fn payload_digest(self) -> [u8; 32] {
        self.payload_digest
    }
    /// Returns normalized event kind.
    #[must_use]
    pub const fn kind(self) -> ProviderEventKind {
        self.kind
    }
    /// Returns fragment bytes carried by this event.
    #[must_use]
    pub const fn fragment_bytes(self) -> u64 {
        self.fragment_bytes
    }
}

/// Complete direct stream observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderStreamObservation {
    events: Vec<ProviderEventObservation>,
    received_events: u64,
    duplicate_events: u64,
    provider_deduplication_applicable: bool,
    terminal_count: u64,
    completed_tool_digest: Option<[u8; 32]>,
    final_fragment_sequence: Option<u64>,
    tool_closed_sequence: Option<u64>,
}

impl ProviderStreamObservation {
    /// Creates one stream observation.
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "stream facts are independently falsifiable")]
    pub const fn new(
        events: Vec<ProviderEventObservation>,
        received_events: u64,
        duplicate_events: u64,
        terminal_count: u64,
        completed_tool_digest: Option<[u8; 32]>,
        final_fragment_sequence: Option<u64>,
        tool_closed_sequence: Option<u64>,
    ) -> Self {
        Self {
            events,
            received_events,
            duplicate_events,
            provider_deduplication_applicable: true,
            terminal_count,
            completed_tool_digest,
            final_fragment_sequence,
            tool_closed_sequence,
        }
    }

    /// Returns emitted normalized events.
    #[must_use]
    pub fn events(&self) -> &[ProviderEventObservation] {
        &self.events
    }
    /// Returns raw provider events received before exact deduplication.
    #[must_use]
    pub const fn received_events(&self) -> u64 {
        self.received_events
    }
    /// Returns exact duplicates ignored.
    #[must_use]
    pub const fn duplicate_events(&self) -> u64 {
        self.duplicate_events
    }
    /// Returns whether the selected wire dialect exposes provider event identity for deduplication.
    #[must_use]
    pub const fn provider_deduplication_applicable(&self) -> bool {
        self.provider_deduplication_applicable
    }
    /// Marks provider-event deduplication inapplicable for a final-result-only wire dialect.
    ///
    /// The observation must report no synthetic duplicate events. Ordering and terminal checks
    /// remain fully applicable.
    #[must_use]
    pub const fn without_provider_event_deduplication(mut self) -> Self {
        self.provider_deduplication_applicable = false;
        self
    }
    /// Returns normalized terminal count.
    #[must_use]
    pub const fn terminal_count(&self) -> u64 {
        self.terminal_count
    }
    /// Returns completed canonical tool-argument digest.
    #[must_use]
    pub const fn completed_tool_digest(&self) -> Option<[u8; 32]> {
        self.completed_tool_digest
    }
    /// Returns the final argument-fragment sequence.
    #[must_use]
    pub const fn final_fragment_sequence(&self) -> Option<u64> {
        self.final_fragment_sequence
    }
    /// Returns the tool-close sequence.
    #[must_use]
    pub const fn tool_closed_sequence(&self) -> Option<u64> {
        self.tool_closed_sequence
    }
}
