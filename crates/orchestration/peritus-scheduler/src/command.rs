//! Closed pure scheduler command vocabulary.

use peritus_types::{CommandId, EventId, RevisionTuple, RunId, Sha256Digest};

use crate::{SchedulerError, SchedulerErrorKind, SchedulerSemantics, SchedulerState};

mod kind;

pub use kind::{FailureDisposition, SchedulerCommandKind};

#[doc = " Syntax-checked but unprivileged scheduler reducer command."]
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
    ) -> Self {
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
    #[doc = " Returns the immutable queue and recovery semantics."]
    #[must_use]
    pub const fn semantics(&self) -> SchedulerSemantics {
        self.semantics
    }
    #[doc = " Returns idempotent command identity."]
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[doc = " Returns reserved successor event identity."]
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    #[doc = " Returns bound run."]
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    #[doc = " Returns expected prior sequence, zero at genesis."]
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    #[doc = " Returns exact prior event identity."]
    #[must_use]
    pub const fn expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    #[doc = " Returns exact predecessor-state digest."]
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    #[doc = " Returns exact immutable revision fence."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Borrows closed semantic payload."]
    #[must_use]
    pub const fn kind(&self) -> &SchedulerCommandKind {
        &self.kind
    }
}
impl Clone for SchedulerCommand {
    fn clone(&self) -> Self {
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
