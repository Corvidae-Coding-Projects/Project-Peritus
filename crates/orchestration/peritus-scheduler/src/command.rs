//! Closed pure scheduler command vocabulary.

use peritus_types::{CommandId, EventId, RevisionTuple, RunId, Sha256Digest};

use crate::{SchedulerError, SchedulerErrorKind};

mod kind;

pub use kind::{FailureDisposition, SchedulerCommandKind};

/// Syntax-checked but unprivileged scheduler reducer command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerCommand {
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
        Ok(Self::from_wire(
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_wire(
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
    /// Returns idempotent command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns reserved successor event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns bound run.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns expected prior sequence, zero at genesis.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Returns exact prior event identity.
    #[must_use]
    pub const fn expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    /// Returns exact predecessor-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns exact immutable revision fence.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Borrows closed semantic payload.
    #[must_use]
    pub const fn kind(&self) -> &SchedulerCommandKind {
        &self.kind
    }
}
