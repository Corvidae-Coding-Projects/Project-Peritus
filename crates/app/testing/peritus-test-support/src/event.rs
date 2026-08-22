//! Protocol-neutral event identifier and sequence fixture contexts.

use crate::{DeterministicIdSource, IdSourceError};
use peritus_types::{EventId, EventSequence};
use std::error::Error;
use std::fmt;

/// The A1 primitives allocated for one caller-owned event fixture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EventFixtureContext {
    event_id: EventId,
    sequence: EventSequence,
}

impl EventFixtureContext {
    /// Returns the deterministic event identifier.
    #[must_use]
    pub const fn event_id(self) -> EventId {
        self.event_id
    }

    /// Returns the one-based sequence within this builder's aggregate.
    #[must_use]
    pub const fn sequence(self) -> EventSequence {
        self.sequence
    }
}

/// Failure to allocate an event fixture context.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EventFixtureError {
    /// The deterministic identifier source failed.
    Identifier(IdSourceError),
    /// The maximum event sequence was already emitted.
    SequenceExhausted,
}

impl EventFixtureError {
    /// Returns a stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Identifier(_) => "PERITUS-TEST-EVENT-001",
            Self::SequenceExhausted => "PERITUS-TEST-EVENT-002",
        }
    }
}

impl fmt::Display for EventFixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier(error) => {
                write!(formatter, "event identifier allocation failed: {error}")
            }
            Self::SequenceExhausted => formatter.write_str("event fixture sequence is exhausted"),
        }
    }
}

impl Error for EventFixtureError {}

/// A non-cloneable allocator for one aggregate's event fixture contexts.
///
/// This type does not define an event envelope. Callers own all aggregate, causality, revision,
/// timestamp, schema, and payload fields.
#[derive(Debug)]
pub struct EventFixtureBuilder {
    ids: DeterministicIdSource,
    next_sequence: Option<EventSequence>,
}

impl EventFixtureBuilder {
    /// Creates a builder starting at event sequence one.
    #[must_use]
    pub const fn new(ids: DeterministicIdSource) -> Self {
        Self { ids, next_sequence: Some(EventSequence::first()) }
    }

    /// Creates a builder starting at an exact valid event sequence.
    #[must_use]
    pub const fn starting_at(ids: DeterministicIdSource, sequence: EventSequence) -> Self {
        Self { ids, next_sequence: Some(sequence) }
    }

    /// Returns the next sequence without allocating an identifier.
    ///
    /// # Errors
    ///
    /// Returns [`EventFixtureError::SequenceExhausted`] after the maximum sequence was emitted.
    pub fn peek_sequence(&self) -> Result<EventSequence, EventFixtureError> {
        self.next_sequence.ok_or(EventFixtureError::SequenceExhausted)
    }

    /// Allocates the next deterministic event identifier and aggregate sequence.
    ///
    /// Identifier failure does not advance the event sequence. The maximum sequence is returned
    /// once and then marks the builder exhausted.
    ///
    /// # Errors
    ///
    /// Returns a typed identifier or sequence exhaustion error.
    pub fn next_context(&mut self) -> Result<EventFixtureContext, EventFixtureError> {
        let sequence = self.peek_sequence()?;
        let event_id = self.ids.next(EventId::new).map_err(EventFixtureError::Identifier)?;
        self.next_sequence = sequence.checked_next().ok();
        Ok(EventFixtureContext { event_id, sequence })
    }

    /// Allocates a context and gives it to a caller-owned event constructor.
    ///
    /// # Errors
    ///
    /// Returns the same allocation failures as [`Self::next_context`].
    pub fn build<T>(
        &mut self,
        constructor: impl FnOnce(EventFixtureContext) -> T,
    ) -> Result<T, EventFixtureError> {
        self.next_context().map(constructor)
    }

    /// Consumes the builder and returns its identifier source at the next unissued value.
    #[must_use]
    pub const fn into_id_source(self) -> DeterministicIdSource {
        self.ids
    }
}
