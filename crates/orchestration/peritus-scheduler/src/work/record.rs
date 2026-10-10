//! Retained work accessors and verified lifecycle mutations.

use super::{WorkPhase, WorkRecord, WorkSpec, WorkTerminal};
use crate::AttemptNumber;
use peritus_types::Sha256Digest;

impl WorkRecord {
    #[doc = " Borrows immutable work definition."]
    #[must_use]
    pub const fn spec(&self) -> &WorkSpec {
        &self.spec
    }
    #[doc = " Returns current lifecycle."]
    #[must_use]
    pub const fn phase(&self) -> WorkPhase {
        self.phase
    }
    #[doc = " Returns immutable admission order."]
    #[must_use]
    pub const fn enqueue_ordinal(&self) -> u64 {
        self.enqueue_ordinal
    }
    #[doc = " Returns bounded feasible-bypass count."]
    #[must_use]
    pub const fn bypasses(&self) -> u16 {
        self.bypasses
    }
    #[doc = " Returns attempts reserved so far."]
    #[must_use]
    pub const fn attempts_started(&self) -> u16 {
        self.attempts_started
    }
    #[doc = " Borrows the exact retained terminal outcome."]
    #[must_use]
    pub const fn terminal(&self) -> Option<&WorkTerminal> {
        self.terminal.as_ref()
    }
    pub(crate) const fn set_phase(&mut self, phase: WorkPhase) {
        self.phase = phase;
    }
    pub(crate) const fn set_bypasses(&mut self, value: u16) {
        self.bypasses = value;
    }
    pub(crate) const fn set_retry_pending(&mut self, cause: Sha256Digest) {
        self.phase = WorkPhase::RetryPending;
        self.retry_cause = Some(cause);
    }
    pub(crate) const fn queue_retry(&mut self) {
        self.phase = WorkPhase::Queued;
        self.retry_cause = None;
    }
    pub(crate) const fn terminalize(&mut self, terminal: WorkTerminal) {
        self.phase = WorkPhase::Terminal;
        self.retry_cause = None;
        self.terminal = Some(terminal);
    }
    pub(crate) const fn try_begin_attempt(&mut self) -> Option<AttemptNumber> {
        if self.attempts_started == u16::MAX {
            return None;
        }
        let next = self.attempts_started + 1;
        if next > self.spec.maximum_attempts().get() {
            return None;
        }
        self.attempts_started = next;
        self.phase = WorkPhase::Reserved;
        self.retry_cause = None;
        Some(AttemptNumber::from_wire(next))
    }
}

impl WorkRecord {
    pub(crate) const fn new(spec: WorkSpec, phase: WorkPhase, enqueue_ordinal: u64) -> Self {
        Self {
            spec,
            phase,
            enqueue_ordinal,
            bypasses: 0,
            attempts_started: 0,
            retry_cause: None,
            terminal: None,
        }
    }
}

impl WorkRecord {
    /// Returns latest retryable failure cause.
    #[must_use]
    pub const fn retry_cause(&self) -> Option<Sha256Digest> {
        self.retry_cause
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "exact closed-wire work record fields are reconstructed without defaults"
    )]
    pub(crate) const fn from_wire(
        spec: WorkSpec,
        phase: WorkPhase,
        enqueue_ordinal: u64,
        bypasses: u16,
        attempts_started: u16,
        retry_cause: Option<Sha256Digest>,
        terminal: Option<WorkTerminal>,
    ) -> Self {
        Self { spec, phase, enqueue_ordinal, bypasses, attempts_started, retry_cause, terminal }
    }
}
