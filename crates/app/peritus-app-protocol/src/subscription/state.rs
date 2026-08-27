//! Pure subscription state and correlated transition values.

use crate::{CorrelationId, DeliveryAttemptId, SubscriptionId};
use peritus_types::EventId;

use super::{RegisteredEventFrame, SubscriptionError, SubscriptionErrorKind, error::reject};

/// Zero-based event-stream cursor; zero denotes the origin before any delivery.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EventCursor(u64);

impl EventCursor {
    /// Returns the origin cursor.
    #[must_use]
    pub const fn origin() -> Self {
        Self(0)
    }

    /// Creates an exact cursor, including origin.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the primitive cursor value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Canonical, sorted, nonempty topic filter.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SubscriptionFilter {
    topics: Vec<String>,
}

impl SubscriptionFilter {
    /// Creates a sorted unique filter under explicit count and UTF-8 byte bounds.
    ///
    /// # Errors
    ///
    /// Rejects zero limits, an empty filter, empty/oversized topics, duplicates, or noncanonical
    /// topic order.
    pub fn new(
        topics: Vec<String>,
        maximum_topics: usize,
        maximum_topic_bytes: usize,
    ) -> Result<Self, SubscriptionError> {
        if maximum_topics == 0 || maximum_topic_bytes == 0 {
            return Err(reject(SubscriptionErrorKind::InvalidLimit, "topic limit is zero"));
        }
        if topics.is_empty() || topics.len() > maximum_topics {
            return Err(reject(
                SubscriptionErrorKind::InvalidInput,
                "topic count is empty or exceeds its negotiated bound",
            ));
        }
        if topics.iter().any(|topic| topic.is_empty() || topic.len() > maximum_topic_bytes) {
            return Err(reject(
                SubscriptionErrorKind::InvalidInput,
                "topic text is empty or exceeds its negotiated bound",
            ));
        }
        if topics.windows(2).any(|pair| pair[0].as_str() >= pair[1].as_str()) {
            return Err(reject(
                SubscriptionErrorKind::InvalidInput,
                "topics must be strictly sorted and unique",
            ));
        }
        Ok(Self { topics })
    }

    /// Borrows the canonical topic list.
    #[must_use]
    pub fn topics(&self) -> &[String] {
        &self.topics
    }
}

/// One exact event delivery attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delivery {
    pub(super) subscription_id: SubscriptionId,
    pub(super) event_id: EventId,
    pub(super) cursor: EventCursor,
    pub(super) attempt_id: DeliveryAttemptId,
    pub(super) attempt: u32,
    pub(super) frame: RegisteredEventFrame,
}

impl Delivery {
    /// Reconstructs one checked delivery value from canonical fields.
    ///
    /// This validates local representation only. Cross-attempt identity preservation is enforced
    /// by [`SubscriptionState::redeliver`].
    ///
    /// # Errors
    ///
    /// Rejects the origin cursor or a zero attempt number.
    pub fn new(
        subscription_id: SubscriptionId,
        event_id: EventId,
        cursor: EventCursor,
        attempt_id: DeliveryAttemptId,
        attempt: u32,
        frame: RegisteredEventFrame,
    ) -> Result<Self, SubscriptionError> {
        if cursor == EventCursor::origin() || attempt == 0 {
            return Err(reject(
                SubscriptionErrorKind::InvalidInput,
                "delivery cursor and attempt number must be positive",
            ));
        }
        Ok(Self { subscription_id, event_id, cursor, attempt_id, attempt, frame })
    }

    /// Returns the subscription identity.
    #[must_use]
    pub const fn subscription_id(&self) -> SubscriptionId {
        self.subscription_id
    }
    /// Returns the stable event identity used for client deduplication.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the stable delivery cursor.
    #[must_use]
    pub const fn cursor(&self) -> EventCursor {
        self.cursor
    }
    /// Returns this attempt's distinct identity.
    #[must_use]
    pub const fn attempt_id(&self) -> DeliveryAttemptId {
        self.attempt_id
    }
    /// Returns the one-based delivery attempt number.
    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }
    /// Borrows the exact immutable B3 event frame.
    #[must_use]
    pub const fn frame(&self) -> &RegisteredEventFrame {
        &self.frame
    }
}

/// Result of attempting to admit a new distinct event delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeliveryAdmission {
    /// The exact event was admitted and occupies one in-flight slot.
    Delivered(Delivery),
    /// The event was not admitted because the negotiated window is full.
    Backpressured,
}

/// Cumulative acknowledgement scoped to one subscription.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Acknowledgement {
    pub(super) subscription_id: SubscriptionId,
    pub(super) cursor: EventCursor,
}

impl Acknowledgement {
    /// Creates a cumulative acknowledgement.
    #[must_use]
    pub const fn new(subscription_id: SubscriptionId, cursor: EventCursor) -> Self {
        Self { subscription_id, cursor }
    }
    /// Returns the target subscription.
    #[must_use]
    pub const fn subscription_id(self) -> SubscriptionId {
        self.subscription_id
    }
    /// Returns the cumulative cursor.
    #[must_use]
    pub const fn cursor(self) -> EventCursor {
        self.cursor
    }
}

/// Retention gap that requires snapshot/resubscription recovery.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SubscriptionGap {
    pub(super) requested: EventCursor,
    earliest: EventCursor,
    latest: EventCursor,
}

impl SubscriptionGap {
    /// Creates a retained interval that cannot satisfy the requested cursor.
    ///
    /// # Errors
    ///
    /// Rejects an inverted retained interval or a request inside the retained interval.
    pub fn new(
        requested: EventCursor,
        earliest: EventCursor,
        latest: EventCursor,
    ) -> Result<Self, SubscriptionError> {
        if earliest > latest || requested >= earliest {
            return Err(reject(
                SubscriptionErrorKind::InvalidInput,
                "gap must place the request before a non-inverted retained interval",
            ));
        }
        Ok(Self { requested, earliest, latest })
    }
    /// Returns the unsatisfied requested cursor.
    #[must_use]
    pub const fn requested(self) -> EventCursor {
        self.requested
    }
    /// Returns the earliest retained cursor.
    #[must_use]
    pub const fn earliest(self) -> EventCursor {
        self.earliest
    }
    /// Returns the latest retained cursor.
    #[must_use]
    pub const fn latest(self) -> EventCursor {
        self.latest
    }
}

/// Explicit reason that ordinary delivery is paused.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PauseReason {
    /// The client explicitly paused delivery.
    Client,
    /// The server identified a slow consumer.
    SlowConsumer,
}

/// Party that initiated terminal subscription cancellation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SubscriptionCancellationSource {
    /// Cancellation originated at the client.
    Client,
    /// Cancellation originated at the server.
    Server,
}

/// Correlated terminal cancellation fact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SubscriptionCancellation {
    pub(super) subscription_id: SubscriptionId,
    correlation_id: CorrelationId,
    source: SubscriptionCancellationSource,
}

impl SubscriptionCancellation {
    /// Creates an exact correlated cancellation fact.
    #[must_use]
    pub const fn new(
        subscription_id: SubscriptionId,
        correlation_id: CorrelationId,
        source: SubscriptionCancellationSource,
    ) -> Self {
        Self { subscription_id, correlation_id, source }
    }
    /// Returns the target subscription.
    #[must_use]
    pub const fn subscription_id(self) -> SubscriptionId {
        self.subscription_id
    }
    /// Returns the request/response correlation identity.
    #[must_use]
    pub const fn correlation_id(self) -> CorrelationId {
        self.correlation_id
    }
    /// Returns the initiating party.
    #[must_use]
    pub const fn source(self) -> SubscriptionCancellationSource {
        self.source
    }
}

/// Observable subscription phase.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SubscriptionPhase {
    /// New event delivery is permitted when the in-flight window has capacity.
    Active,
    /// Delivery is explicitly paused.
    Paused(PauseReason),
    /// A retention gap requires a replacement subscription from a snapshot.
    SnapshotRequired(SubscriptionGap),
    /// Cancellation is terminal.
    Cancelled(SubscriptionCancellation),
}

/// Result of applying an idempotent cancellation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CancellationDisposition {
    /// The cancellation fact caused the terminal transition.
    Applied,
    /// The exact retained cancellation fact was repeated.
    Repeated,
}

/// Bounded pure subscription delivery state machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionState {
    pub(super) id: SubscriptionId,
    filter: SubscriptionFilter,
    origin: EventCursor,
    pub(super) requested: EventCursor,
    pub(super) scanned: EventCursor,
    pub(super) last_delivered: EventCursor,
    pub(super) last_acknowledged: EventCursor,
    pub(super) maximum_in_flight: usize,
    pub(super) in_flight: Vec<Delivery>,
    pub(super) phase: SubscriptionPhase,
}

impl SubscriptionState {
    /// Creates an active subscription at origin or a resume cursor.
    ///
    /// # Errors
    ///
    /// Rejects a zero negotiated in-flight limit.
    pub fn new(
        id: SubscriptionId,
        filter: SubscriptionFilter,
        requested: EventCursor,
        maximum_in_flight: usize,
    ) -> Result<Self, SubscriptionError> {
        if maximum_in_flight == 0 {
            return Err(reject(
                SubscriptionErrorKind::InvalidLimit,
                "maximum in-flight delivery count is zero",
            ));
        }
        Ok(Self {
            id,
            filter,
            origin: requested,
            requested,
            scanned: requested,
            last_delivered: requested,
            last_acknowledged: requested,
            maximum_in_flight,
            in_flight: Vec::new(),
            phase: SubscriptionPhase::Active,
        })
    }

    /// Returns the subscription identity.
    #[must_use]
    pub const fn id(&self) -> SubscriptionId {
        self.id
    }
    /// Borrows the canonical topic filter.
    #[must_use]
    pub const fn filter(&self) -> &SubscriptionFilter {
        &self.filter
    }
    /// Returns the immutable creation/resume origin.
    #[must_use]
    pub const fn origin(&self) -> EventCursor {
        self.origin
    }
    /// Returns the cursor requested when this subscription was created.
    #[must_use]
    pub const fn requested_cursor(&self) -> EventCursor {
        self.requested
    }
    /// Returns the greatest source cursor examined by the subscription pump.
    #[must_use]
    pub const fn scanned_cursor(&self) -> EventCursor {
        self.scanned
    }
    /// Returns the last distinct delivered cursor.
    #[must_use]
    pub const fn last_delivered(&self) -> EventCursor {
        self.last_delivered
    }
    /// Returns the cumulative acknowledged cursor.
    #[must_use]
    pub const fn last_acknowledged(&self) -> EventCursor {
        self.last_acknowledged
    }
    /// Returns the negotiated maximum in-flight count.
    #[must_use]
    pub const fn maximum_in_flight(&self) -> usize {
        self.maximum_in_flight
    }
    /// Borrows unacknowledged deliveries in ascending cursor order.
    #[must_use]
    pub fn in_flight(&self) -> &[Delivery] {
        &self.in_flight
    }
    /// Returns the current phase.
    #[must_use]
    pub const fn phase(&self) -> SubscriptionPhase {
        self.phase
    }
}
