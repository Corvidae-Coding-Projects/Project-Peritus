//! Stream sequencing, deduplication, and public reducer observations.

use super::{ReducerTransition, ResponseReducer, SeenEvent};
use crate::{
    CacheObservation, EventEnvelope, FailureCategory, ModelEvent, ProtocolError, ProtocolErrorKind,
    ProtocolLimits, ProviderExtension, ProviderName, RateLimitObservation, ReducerTransitionFacts,
    ResponseId, TerminalOutcome, UsageCounters, UsageTracker,
};

impl ResponseReducer {
    /// Creates an empty reducer for one provider family.
    #[must_use]
    pub const fn new(provider: ProviderName, limits: ProtocolLimits) -> Self {
        Self {
            provider,
            limits,
            last_sequence: 0,
            last_provider_sequence: None,
            event_count: 0,
            output_bytes: 0,
            started: false,
            response_id: None,
            seen: std::collections::BTreeMap::new(),
            indexes: std::collections::BTreeSet::new(),
            items: std::collections::BTreeMap::new(),
            calls: std::collections::BTreeMap::new(),
            completed: Vec::new(),
            usage: UsageTracker::new(),
            rate_limits: Vec::new(),
            cache: Vec::new(),
            extensions: Vec::new(),
            finish: None,
            terminal: None,
        }
    }

    /// Applies one event or irreversibly marks the response malformed.
    ///
    /// # Errors
    ///
    /// Rejects conflicting duplicates, ordering errors, invalid fragments, exceeded bounds,
    /// contradictory terminal state, or any event after terminal.
    pub fn push(&mut self, envelope: EventEnvelope) -> Result<ReducerTransition, ProtocolError> {
        if self.terminal.is_some() {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEvent,
                "stream",
                "event followed the terminal outcome",
            ));
        }
        if let Some(duplicate) = self.duplicate(&envelope)? {
            return Ok(duplicate);
        }
        if !crate::verified::next_sequence_legal(self.last_sequence, envelope.sequence()) {
            return self.reject("local event sequence is reordered or has a gap");
        }
        if let Some(provider_sequence) = envelope.provider_sequence()
            && self.last_provider_sequence.is_some_and(|previous| provider_sequence <= previous)
        {
            return self.reject("provider event sequence did not increase");
        }
        let Some(next_event_count) = self.event_count.checked_add(1) else {
            return self.reject("event count overflowed");
        };
        if next_event_count > self.limits.max_events() {
            return self.reject("event count exceeds its bound");
        }
        self.event_count = next_event_count;
        self.last_sequence = envelope.sequence();
        if envelope.provider_sequence().is_some() {
            self.last_provider_sequence = envelope.provider_sequence();
        }
        if let Some(id) = envelope.provider_event_id().cloned() {
            self.seen.insert(
                id,
                SeenEvent {
                    digest: envelope.provider_digest(),
                    local_sequence: envelope.sequence(),
                    provider_sequence: envelope.provider_sequence(),
                },
            );
        }
        let transition = self.apply(envelope.into_event())?;
        if !crate::verified::reducer_transition_legal(ReducerTransitionFacts {
            ordering_valid: true,
            bounds_valid: true,
            phase_valid: true,
            identity_valid: true,
            terminal_open: true,
        }) {
            return self.reject("accepted reducer transition contradicted its formal projection");
        }
        Ok(transition)
    }

    /// Converts transport EOF into success only if an explicit terminal was already established.
    ///
    /// # Errors
    ///
    /// Returns incomplete-stream failure when EOF precedes terminal.
    pub fn finish_eof(&mut self) -> Result<TerminalOutcome, ProtocolError> {
        if let Some(terminal) = &self.terminal {
            return Ok(terminal.clone());
        }
        let error = ProtocolError::at(
            ProtocolErrorKind::IncompleteStream,
            "stream",
            "provider stream ended without an explicit terminal event",
        );
        self.terminal = Some(TerminalOutcome::Failed(
            self.failure(FailureCategory::IncompleteStream, "incomplete_stream"),
        ));
        Err(error)
    }

    /// Borrows the terminal outcome when established.
    #[must_use]
    pub const fn terminal(&self) -> Option<&TerminalOutcome> {
        self.terminal.as_ref()
    }
    /// Borrows complete validated items.
    #[must_use]
    pub fn completed_items(&self) -> &[super::ReducedItem] {
        &self.completed
    }
    /// Borrows the provider response identity.
    #[must_use]
    pub const fn response_id(&self) -> Option<&ResponseId> {
        self.response_id.as_ref()
    }
    /// Returns cumulative usage high water.
    #[must_use]
    pub const fn usage_high_water(&self) -> UsageCounters {
        self.usage.high_water()
    }
    /// Borrows rate-limit observations.
    #[must_use]
    pub fn rate_limits(&self) -> &[RateLimitObservation] {
        &self.rate_limits
    }
    /// Borrows cache observations.
    #[must_use]
    pub fn cache_observations(&self) -> &[CacheObservation] {
        &self.cache
    }
    /// Borrows ancillary bounded provider events.
    #[must_use]
    pub fn provider_events(&self) -> &[ProviderExtension] {
        &self.extensions
    }

    fn duplicate(
        &mut self,
        envelope: &EventEnvelope,
    ) -> Result<Option<ReducerTransition>, ProtocolError> {
        let Some(id) = envelope.provider_event_id() else { return Ok(None) };
        let Some(seen) = self.seen.get(id) else { return Ok(None) };
        let local_sequence_compatible = envelope.sequence() == seen.local_sequence
            || crate::verified::next_sequence_legal(self.last_sequence, envelope.sequence());
        if !crate::verified::deduplication_legal(crate::verified::DeduplicationFacts {
            identity_matches: true,
            digest_matches: seen.digest == envelope.provider_digest(),
            provider_sequence_matches: seen.provider_sequence == envelope.provider_sequence(),
            local_sequence_compatible,
        }) {
            return self
                .reject("provider event identity was reused with different bytes or sequence");
        }
        if crate::verified::next_sequence_legal(self.last_sequence, envelope.sequence()) {
            self.last_sequence = envelope.sequence();
        }
        Ok(Some(ReducerTransition::DuplicateIgnored))
    }

    fn apply(&mut self, event: ModelEvent) -> Result<ReducerTransition, ProtocolError> {
        match event {
            ModelEvent::Heartbeat => return Ok(ReducerTransition::Applied),
            ModelEvent::ResponseStarted { response_id, .. } => {
                if self.started {
                    return self.reject("response started more than once");
                }
                self.started = true;
                self.response_id = response_id;
            }
            ModelEvent::ResponseFailed(failure) => {
                return Ok(self.set_terminal(TerminalOutcome::Failed(failure)));
            }
            ModelEvent::ResponseCancelled => {
                return Ok(self.set_terminal(TerminalOutcome::Cancelled));
            }
            other => {
                if !self.started {
                    return self.reject("semantic event preceded response start");
                }
                return self.apply_started(other);
            }
        }
        Ok(ReducerTransition::Applied)
    }
}
