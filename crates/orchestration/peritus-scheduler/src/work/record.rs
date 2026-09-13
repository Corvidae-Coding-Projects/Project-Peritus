//! Retained work accessors and verified lifecycle mutations.

use super::{WorkPhase, WorkRecord, WorkSpec, WorkTerminal};
use crate::AttemptNumber;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

impl WorkRecord {
    /// Returns the mathematical immutable work definition.
    pub closed spec fn spec_definition(&self) -> &WorkSpec { &self.spec }
    /// Returns the mathematical work lifecycle.
    pub closed spec fn spec_phase(&self) -> WorkPhase { self.phase }
    /// Returns the mathematical number of started attempts.
    pub closed spec fn spec_attempts_started(&self) -> u16 { self.attempts_started }
    /// Returns the mathematical latest retryable failure cause.
    pub closed spec fn spec_retry_cause(&self) -> Option<Sha256Digest> { self.retry_cause }
    /// Returns the mathematical terminal outcome, when present.
    pub closed spec fn spec_terminal(&self) -> Option<WorkTerminal> { self.terminal }

    /// Relates every field that phase and terminal updates must leave unchanged.
    pub closed spec fn lifecycle_update_stable(left: &Self, right: &Self) -> bool {
        left.spec == right.spec
            && left.enqueue_ordinal == right.enqueue_ordinal
            && left.bypasses == right.bypasses
            && left.attempts_started == right.attempts_started
    }

    /// Borrows immutable work definition.
    #[must_use]
    pub const fn spec(&self) -> (result: &WorkSpec)
        ensures result == self.spec_definition(),
    {
        &self.spec
    }

    /// Returns immutable admission order.
    pub closed spec fn spec_enqueue_ordinal(&self) -> u64 { self.enqueue_ordinal }
    /// Returns the mathematical bounded feasible-bypass count.
    pub closed spec fn spec_bypasses(&self) -> u16 { self.bypasses }

    /// Returns current lifecycle.
    #[must_use]
    pub const fn phase(&self) -> (result: WorkPhase)
        ensures result == self.spec_phase(),
    {
        self.phase
    }

    /// Returns immutable admission order.
    #[must_use]
    pub const fn enqueue_ordinal(&self) -> (result: u64)
        ensures result == self.spec_enqueue_ordinal(),
    {
        self.enqueue_ordinal
    }

    /// Returns bounded feasible-bypass count.
    #[must_use]
    pub const fn bypasses(&self) -> (result: u16)
        ensures result == self.spec_bypasses(),
    {
        self.bypasses
    }

    /// Returns attempts reserved so far.
    #[must_use]
    pub const fn attempts_started(&self) -> (result: u16)
        ensures result == self.spec_attempts_started(),
    {
        self.attempts_started
    }

    /// Borrows the exact retained terminal outcome.
    #[must_use]
    pub const fn terminal(&self) -> (result: Option<&WorkTerminal>)
        ensures match result {
            Some(value) => self.spec_terminal() == Some(*value),
            None => self.spec_terminal().is_none(),
        },
    {
        self.terminal.as_ref()
    }

    /// Relates fields that bind an active reservation to this work attempt.
    pub closed spec fn reservation_binding_equivalent(left: &Self, right: &Self) -> bool {
        left.spec.id == right.spec.id
            && left.spec.owner == right.spec.owner
            && left.spec.revision == right.spec.revision
            && left.spec.request.spec_entries() == right.spec.request.spec_entries()
            && left.attempts_started == right.attempts_started
    }

    /// Relates the immutable reservation subject while allowing a new attempt.
    pub closed spec fn reservation_subject_equivalent(left: &Self, right: &Self) -> bool {
        left.spec.id == right.spec.id
            && left.spec.owner == right.spec.owner
            && left.spec.revision == right.spec.revision
            && left.spec.request.spec_entries() == right.spec.request.spec_entries()
    }

    /// Relates the reservation binding and lifecycle across bookkeeping-only updates.
    pub closed spec fn reservation_lifecycle_equivalent(left: &Self, right: &Self) -> bool {
        Self::reservation_binding_equivalent(left, right)
            && left.spec_phase() == right.spec_phase()
    }


    pub(crate) proof fn reservation_subject_fields(left: &Self, right: &Self)
        requires Self::reservation_subject_equivalent(left, right),
        ensures
            left.spec_definition().spec_id() == right.spec_definition().spec_id(),
            left.spec_definition().spec_owner() == right.spec_definition().spec_owner(),
            left.spec_definition().spec_revision() == right.spec_definition().spec_revision(),
            left.spec_definition().spec_request().spec_entries()
                == right.spec_definition().spec_request().spec_entries(),
    {
    }

    pub(crate) proof fn reservation_binding_fields(left: &Self, right: &Self)
        requires Self::reservation_binding_equivalent(left, right),
        ensures
            left.spec_definition().spec_id() == right.spec_definition().spec_id(),
            left.spec_definition().spec_owner() == right.spec_definition().spec_owner(),
            left.spec_definition().spec_revision() == right.spec_definition().spec_revision(),
            left.spec_definition().spec_request().spec_entries()
                == right.spec_definition().spec_request().spec_entries(),
            left.spec_attempts_started() == right.spec_attempts_started(),
    {
    }

    pub(crate) proof fn reservation_lifecycle_fields(left: &Self, right: &Self)
        requires Self::reservation_lifecycle_equivalent(left, right),
        ensures
            Self::reservation_binding_equivalent(left, right),
            left.spec_definition().spec_id() == right.spec_definition().spec_id(),
            left.spec_phase() == right.spec_phase(),
    {
    }

    pub(crate) proof fn reservation_lifecycle_reflexive(value: &Self)
        ensures Self::reservation_lifecycle_equivalent(value, value),
    {
    }

    /// Projects the complete stable field correspondence for a lifecycle update.
    pub(crate) proof fn lifecycle_update_fields(left: &Self, right: &Self)
        requires Self::lifecycle_update_stable(left, right),
        ensures
            left.spec_definition() == right.spec_definition(),
            left.spec_enqueue_ordinal() == right.spec_enqueue_ordinal(),
            left.spec_bypasses() == right.spec_bypasses(),
            left.spec_attempts_started() == right.spec_attempts_started(),
    {
    }

    pub(crate) proof fn clone_reservation_fields(left: &Self, right: &Self)
        requires Self::clone_equivalent(left, right),
        ensures
            Self::reservation_lifecycle_equivalent(left, right),
            left.spec_definition().spec_id() == right.spec_definition().spec_id(),
            left.spec_definition().spec_owner() == right.spec_definition().spec_owner(),
            left.spec_definition().spec_revision() == right.spec_definition().spec_revision(),
            left.spec_definition().spec_request().spec_entries()
                == right.spec_definition().spec_request().spec_entries(),
            left.spec_attempts_started() == right.spec_attempts_started(),
            left.spec_phase() == right.spec_phase(),
    {
        Self::clone_storage_fields(left, right);
    }

    pub(crate) const fn set_phase(&mut self, phase: WorkPhase)
        ensures
            Self::reservation_binding_equivalent(old(self), final(self)),
            Self::lifecycle_update_stable(old(self), final(self)),
            final(self).spec_phase() == phase,
            final(self).spec_retry_cause() == old(self).spec_retry_cause(),
            final(self).spec_terminal() == old(self).spec_terminal(),
    {
        self.phase = phase;
    }

    pub(crate) const fn set_bypasses(&mut self, value: u16)
        ensures
            Self::reservation_lifecycle_equivalent(old(self), final(self)),
            final(self).spec_retry_cause() == old(self).spec_retry_cause(),
            final(self).spec_terminal() == old(self).spec_terminal(),
    {
        self.bypasses = value;
    }

    pub(crate) const fn set_retry_pending(&mut self, cause: Sha256Digest)
        ensures
            Self::reservation_binding_equivalent(old(self), final(self)),
            Self::lifecycle_update_stable(old(self), final(self)),
            final(self).spec_phase() == WorkPhase::RetryPending,
            final(self).spec_retry_cause() == Some(cause),
            final(self).spec_terminal() == old(self).spec_terminal(),
    {
        self.phase = WorkPhase::RetryPending;
        self.retry_cause = Some(cause);
    }

    pub(crate) const fn queue_retry(&mut self)
        ensures
            Self::reservation_binding_equivalent(old(self), final(self)),
            Self::lifecycle_update_stable(old(self), final(self)),
            final(self).spec_phase() == WorkPhase::Queued,
            final(self).spec_retry_cause().is_none(),
            final(self).spec_terminal() == old(self).spec_terminal(),
    {
        self.phase = WorkPhase::Queued;
        self.retry_cause = None;
    }

    pub(crate) const fn terminalize(&mut self, terminal: WorkTerminal)
        ensures
            Self::reservation_binding_equivalent(old(self), final(self)),
            Self::lifecycle_update_stable(old(self), final(self)),
            final(self).spec_phase() == WorkPhase::Terminal,
            final(self).spec_retry_cause().is_none(),
            final(self).spec_terminal() == Some(terminal),
    {
        self.phase = WorkPhase::Terminal;
        self.retry_cause = None;
        self.terminal = Some(terminal);
    }

    pub(crate) const fn try_begin_attempt(&mut self) -> (result: Option<AttemptNumber>)
        ensures
            result.is_some() <==>
                old(self).spec_attempts_started() < u16::MAX
                    && old(self).spec_attempts_started()
                        < old(self).spec_definition().spec_maximum_attempts().spec_value(),
            match result {
                Some(attempt) =>
                    Self::reservation_subject_equivalent(old(self), final(self))
                        && final(self).spec_attempts_started()
                            == old(self).spec_attempts_started() + 1
                        && attempt.spec_value() == final(self).spec_attempts_started()
                        && final(self).spec_phase() == WorkPhase::Reserved
                        && final(self).spec_retry_cause().is_none()
                        && final(self).spec_terminal() == old(self).spec_terminal(),
                None => {
                    &&& Self::reservation_lifecycle_equivalent(old(self), final(self))
                    &&& final(self).spec_retry_cause() == old(self).spec_retry_cause()
                    &&& final(self).spec_terminal() == old(self).spec_terminal()
                },
            },
    {
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

} // verus!

verus! {

impl WorkRecord {
    pub(crate) const fn new(
        spec: WorkSpec,
        phase: WorkPhase,
        enqueue_ordinal: u64,
    ) -> (result: Self)
        ensures
            *result.spec_definition() == spec,
            result.spec_phase() == phase,
            result.spec_enqueue_ordinal() == enqueue_ordinal,
            result.spec_bypasses() == 0,
            result.spec_attempts_started() == 0,
            result.spec_retry_cause().is_none(),
            result.spec_terminal().is_none(),
    {
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

} // verus!

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
