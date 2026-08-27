//! Subscription, artifact, prompt, and terminal service observations.

/// Stable terminal result of one subscription exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonSubscriptionOutcome {
    /// Delivery remains active after the exercise.
    Active,
    /// A delivered prefix was acknowledged.
    Acknowledged,
    /// Retention requires an explicit snapshot before resumption.
    SnapshotRequired,
    /// The negotiated in-flight ceiling applied backpressure.
    Backpressured,
}

/// Direct subscription delivery-window and journal facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonSubscriptionObservation {
    outcome: DaemonSubscriptionOutcome,
    supplied_cursor: u64,
    first_source_cursor: Option<u64>,
    redeliveries: u64,
    stable_event_identity: bool,
    distinct_attempt_identity: bool,
    acknowledgement_contiguous: bool,
    released_capacity: u64,
    journal_records_deleted: u64,
    peak_in_flight: u64,
}

impl DaemonSubscriptionObservation {
    /// Creates one complete subscription observation.
    #[allow(
        clippy::too_many_arguments,
        reason = "the contract keeps independent cursor, identity, ack, and bound facts visible"
    )]
    #[must_use]
    pub const fn new(
        outcome: DaemonSubscriptionOutcome,
        supplied_cursor: u64,
        first_source_cursor: Option<u64>,
        redeliveries: u64,
        stable_event_identity: bool,
        distinct_attempt_identity: bool,
        acknowledgement_contiguous: bool,
        released_capacity: u64,
        journal_records_deleted: u64,
        peak_in_flight: u64,
    ) -> Self {
        Self {
            outcome,
            supplied_cursor,
            first_source_cursor,
            redeliveries,
            stable_event_identity,
            distinct_attempt_identity,
            acknowledgement_contiguous,
            released_capacity,
            journal_records_deleted,
            peak_in_flight,
        }
    }

    /// Returns the stable subscription outcome.
    #[must_use]
    pub const fn outcome(self) -> DaemonSubscriptionOutcome {
        self.outcome
    }

    /// Returns the resume cursor supplied to the daemon.
    #[must_use]
    pub const fn supplied_cursor(self) -> u64 {
        self.supplied_cursor
    }

    /// Returns the first delivered source cursor, when delivery occurred.
    #[must_use]
    pub const fn first_source_cursor(self) -> Option<u64> {
        self.first_source_cursor
    }

    /// Returns the number of repeated delivery attempts observed.
    #[must_use]
    pub const fn redeliveries(self) -> u64 {
        self.redeliveries
    }

    /// Returns whether redelivery retained the original event identity and bytes.
    #[must_use]
    pub const fn stable_event_identity(self) -> bool {
        self.stable_event_identity
    }

    /// Returns whether each redelivery received a distinct attempt identity.
    #[must_use]
    pub const fn distinct_attempt_identity(self) -> bool {
        self.distinct_attempt_identity
    }

    /// Returns whether acknowledgement named and closed exactly a delivered prefix.
    #[must_use]
    pub const fn acknowledgement_contiguous(self) -> bool {
        self.acknowledgement_contiguous
    }

    /// Returns delivery-window slots released by acknowledgement.
    #[must_use]
    pub const fn released_capacity(self) -> u64 {
        self.released_capacity
    }

    /// Returns immutable journal records deleted by acknowledgement.
    #[must_use]
    pub const fn journal_records_deleted(self) -> u64 {
        self.journal_records_deleted
    }

    /// Returns the maximum simultaneously in-flight deliveries observed.
    #[must_use]
    pub const fn peak_in_flight(self) -> u64 {
        self.peak_in_flight
    }
}

/// Stable artifact transfer outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonArtifactOutcome {
    /// An immutable artifact download completed.
    Downloaded,
    /// An upload finalized and published its catalog fact.
    Uploaded,
    /// Corrupt or mismatched content was rejected.
    CorruptRejected,
}

/// Direct artifact transfer and catalog facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonArtifactObservation {
    outcome: DaemonArtifactOutcome,
    transferred_bytes: u64,
    integrity: DaemonArtifactIntegrity,
    publication: DaemonArtifactPublication,
}

/// Exactness of the observed artifact identity, byte offsets, and digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonArtifactIntegrity {
    /// Identity, total bytes, contiguous offsets, and completion digest all matched.
    Exact,
    /// At least one identity, offset, byte-count, or digest fact differed.
    Mismatched,
}

/// Catalog-authority result observed after an artifact exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonArtifactPublication {
    /// Download used an already-available immutable catalog entry.
    Available,
    /// Exact upload finalization published one available catalog entry.
    Published,
    /// Rejected content published no availability or partial fact.
    Withheld,
    /// Incomplete or corrupt content incorrectly became authoritative.
    Partial,
}

impl DaemonArtifactObservation {
    /// Creates one complete artifact observation.
    #[must_use]
    pub const fn new(
        outcome: DaemonArtifactOutcome,
        transferred_bytes: u64,
        integrity: DaemonArtifactIntegrity,
        publication: DaemonArtifactPublication,
    ) -> Self {
        Self { outcome, transferred_bytes, integrity, publication }
    }

    /// Returns the stable transfer outcome.
    #[must_use]
    pub const fn outcome(self) -> DaemonArtifactOutcome {
        self.outcome
    }

    /// Returns the exact number of transferred content bytes.
    #[must_use]
    pub const fn transferred_bytes(self) -> u64 {
        self.transferred_bytes
    }

    /// Returns exactness across identity, size, offsets, and digest.
    #[must_use]
    pub const fn integrity(self) -> DaemonArtifactIntegrity {
        self.integrity
    }

    /// Returns the observed catalog-authority result.
    #[must_use]
    pub const fn publication(self) -> DaemonArtifactPublication {
        self.publication
    }
}

/// One negative prompt settlement attempt that the daemon rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonPromptRejection {
    /// The responding actor or durable session differed from the prompt owner.
    ActorSessionMismatch,
    /// The prompt revision or cancellation generation was stale.
    StaleRevisionGeneration,
    /// An approve or deny response lacked a valid current-registry signature.
    UnsignedApproval,
}

/// Direct prompt freshness and authority facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonPromptObservation {
    current_response_settled: bool,
    rejected_attempts: Vec<DaemonPromptRejection>,
    terminal_settlements: u64,
}

impl DaemonPromptObservation {
    /// Creates one complete prompt observation.
    #[must_use]
    pub const fn new(
        current_response_settled: bool,
        rejected_attempts: Vec<DaemonPromptRejection>,
        terminal_settlements: u64,
    ) -> Self {
        Self { current_response_settled, rejected_attempts, terminal_settlements }
    }

    /// Returns whether one current response settled durably.
    #[must_use]
    pub const fn current_response_settled(&self) -> bool {
        self.current_response_settled
    }

    /// Returns negative settlement attempts in adapter-observed order.
    #[must_use]
    pub fn rejected_attempts(&self) -> &[DaemonPromptRejection] {
        &self.rejected_attempts
    }

    /// Returns durable terminal settlements observed for the prompt.
    #[must_use]
    pub const fn terminal_settlements(&self) -> u64 {
        self.terminal_settlements
    }
}

/// Direct combined-PTY stream and exit facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonTerminalObservation {
    output_bytes: u64,
    sequence_strictly_increasing: bool,
    offsets_conserved: bool,
    combined_stream_only: bool,
    exit_records: u64,
    peak_buffered_bytes: u64,
    configured_buffer_limit: u64,
}

impl DaemonTerminalObservation {
    /// Creates one complete terminal observation.
    #[must_use]
    pub const fn new(
        output_bytes: u64,
        sequence_strictly_increasing: bool,
        offsets_conserved: bool,
        combined_stream_only: bool,
        exit_records: u64,
        peak_buffered_bytes: u64,
        configured_buffer_limit: u64,
    ) -> Self {
        Self {
            output_bytes,
            sequence_strictly_increasing,
            offsets_conserved,
            combined_stream_only,
            exit_records,
            peak_buffered_bytes,
            configured_buffer_limit,
        }
    }

    /// Returns terminal output bytes observed through the bridge.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        self.output_bytes
    }

    /// Returns whether global terminal sequence numbers strictly increased.
    #[must_use]
    pub const fn sequence_strictly_increasing(self) -> bool {
        self.sequence_strictly_increasing
    }

    /// Returns whether offsets exactly conserved the output byte count.
    #[must_use]
    pub const fn offsets_conserved(self) -> bool {
        self.offsets_conserved
    }

    /// Returns whether PTY output used only the combined terminal stream.
    #[must_use]
    pub const fn combined_stream_only(self) -> bool {
        self.combined_stream_only
    }

    /// Returns terminal exit records observed by the client.
    #[must_use]
    pub const fn exit_records(self) -> u64 {
        self.exit_records
    }

    /// Returns the peak buffered terminal bytes.
    #[must_use]
    pub const fn peak_buffered_bytes(self) -> u64 {
        self.peak_buffered_bytes
    }

    /// Returns the configured terminal buffering ceiling.
    #[must_use]
    pub const fn configured_buffer_limit(self) -> u64 {
        self.configured_buffer_limit
    }
}
