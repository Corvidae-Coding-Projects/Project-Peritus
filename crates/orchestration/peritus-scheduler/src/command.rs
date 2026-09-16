//! Closed pure scheduler command vocabulary.

use peritus_types::{CommandId, EventId, RevisionTuple, RunId, Sha256Digest};
use vstd::prelude::*;

use crate::{SchedulerError, SchedulerErrorKind, SchedulerSemantics, SchedulerState};

mod kind;

pub use kind::{FailureDisposition, SchedulerCommandKind};

verus! {

/// Syntax-checked but unprivileged scheduler reducer command.
#[derive(Debug, Eq, PartialEq)]
pub struct SchedulerCommand {
    semantics: SchedulerSemantics,
    command_id: CommandId,
    event_id: EventId,
    run_id: RunId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    revision: RevisionTuple,
    kind: SchedulerCommandKind,
}

impl SchedulerCommand {
    /// Relates every command fence and the exact semantic payload across a clone.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        &&& left.semantics == right.semantics
        &&& left.command_id == right.command_id
        &&& left.event_id == right.event_id
        &&& left.run_id == right.run_id
        &&& left.expected_sequence == right.expected_sequence
        &&& left.expected_previous_event == right.expected_previous_event
        &&& left.prior_state_digest == right.prior_state_digest
        &&& left.revision == right.revision
        &&& SchedulerCommandKind::clone_equivalent(&left.kind, &right.kind)
    }
    /// Returns the mathematical immutable scheduler semantics.
    pub closed spec fn spec_semantics(&self) -> SchedulerSemantics { self.semantics }
    /// Returns the mathematical idempotency identity.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }
    /// Returns the mathematical reserved event identity.
    pub closed spec fn spec_event_id(&self) -> EventId { self.event_id }
    /// Returns the mathematical run fence.
    pub closed spec fn spec_run_id(&self) -> RunId { self.run_id }
    /// Returns the mathematical predecessor sequence fence.
    pub closed spec fn spec_expected_sequence(&self) -> u64 { self.expected_sequence }
    /// Returns the mathematical predecessor event fence.
    pub closed spec fn spec_expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    /// Returns the mathematical predecessor-state digest fence.
    pub closed spec fn spec_prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns the mathematical immutable revision fence.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }
    /// Returns the mathematical semantic command payload.
    pub closed spec fn spec_kind(&self) -> &SchedulerCommandKind { &self.kind }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_wire(
        semantics: SchedulerSemantics,
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: SchedulerCommandKind,
    ) -> (result: Self)
        ensures
            result.spec_semantics() == semantics,
            result.spec_command_id() == command_id,
            result.spec_event_id() == event_id,
            result.spec_run_id() == run_id,
            result.spec_expected_sequence() == expected_sequence,
            result.spec_expected_previous_event() == expected_previous_event,
            result.spec_prior_state_digest() == prior_state_digest,
            result.spec_revision() == revision,
            result.spec_kind() == &kind,
    {
        Self {
            semantics,
            command_id,
            event_id,
            run_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            revision,
            kind,
        }
    }
    /// Returns the immutable queue and recovery semantics.
    #[must_use]
    pub const fn semantics(&self) -> (result: SchedulerSemantics)
        ensures result == self.spec_semantics(),
    {
        self.semantics
    }
    /// Returns idempotent command identity.
    #[must_use]
    pub const fn command_id(&self) -> (result: CommandId)
        ensures result == self.spec_command_id(),
    {
        self.command_id
    }
    /// Returns reserved successor event identity.
    #[must_use]
    pub const fn event_id(&self) -> (result: EventId)
        ensures result == self.spec_event_id(),
    {
        self.event_id
    }
    /// Returns bound run.
    #[must_use]
    pub const fn run_id(&self) -> (result: RunId)
        ensures result == self.spec_run_id(),
    {
        self.run_id
    }
    /// Returns expected prior sequence, zero at genesis.
    #[must_use]
    pub const fn expected_sequence(&self) -> (result: u64)
        ensures result == self.spec_expected_sequence(),
    {
        self.expected_sequence
    }
    /// Returns exact prior event identity.
    #[must_use]
    pub const fn expected_previous_event(&self) -> (result: Option<EventId>)
        ensures result == self.spec_expected_previous_event(),
    {
        self.expected_previous_event
    }
    /// Returns exact predecessor-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_prior_state_digest(),
    {
        self.prior_state_digest
    }
    /// Returns exact immutable revision fence.
    #[must_use]
    pub const fn revision(&self) -> (result: RevisionTuple)
        ensures result == self.spec_revision(),
    {
        self.revision
    }
    /// Borrows closed semantic payload.
    #[must_use]
    pub const fn kind(&self) -> (result: &SchedulerCommandKind)
        ensures result == self.spec_kind(),
    {
        &self.kind
    }
}

impl Clone for SchedulerCommand {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            semantics: self.semantics,
            command_id: self.command_id,
            event_id: self.event_id,
            run_id: self.run_id,
            expected_sequence: self.expected_sequence,
            expected_previous_event: self.expected_previous_event,
            prior_state_digest: self.prior_state_digest,
            revision: self.revision,
            kind: self.kind.clone(),
        }
    }
}

} // verus!

impl SchedulerCommand {
    /// Creates one exact fenced command.
    ///
    /// # Errors
    /// Rejects inconsistent genesis/non-genesis predecessor shape.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: SchedulerCommandKind,
    ) -> Result<Self, SchedulerError> {
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(crate::error::reject(
                SchedulerErrorKind::StaleFence,
                "scheduler command predecessor shape is inconsistent",
            ));
        }
        if let SchedulerCommandKind::StartScheduler { binding } = &kind
            && binding.semantics() != SchedulerSemantics::StrictRecoveryQueueV2
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::BindingMismatch,
                "new scheduler genesis must use strict recovery queue semantics",
            ));
        }
        Ok(Self::from_wire(
            SchedulerSemantics::StrictRecoveryQueueV2,
            command_id,
            event_id,
            run_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            revision,
            kind,
        ))
    }

    /// Creates an exact non-genesis command from authoritative scheduler state.
    ///
    /// # Errors
    /// Rejects a genesis payload because an existing state cannot be started again.
    pub fn from_state(
        state: &SchedulerState,
        command_id: CommandId,
        event_id: EventId,
        kind: SchedulerCommandKind,
    ) -> Result<Self, SchedulerError> {
        if matches!(kind, SchedulerCommandKind::StartScheduler { .. }) {
            return Err(crate::error::reject(
                SchedulerErrorKind::IllegalTransition,
                "existing scheduler state cannot create another genesis command",
            ));
        }
        Ok(Self::from_wire(
            state.binding().semantics(),
            command_id,
            event_id,
            state.run_id(),
            state.sequence().get(),
            Some(state.last_event_id()),
            state.state_digest(),
            state.binding().revision(),
            kind,
        ))
    }
}
