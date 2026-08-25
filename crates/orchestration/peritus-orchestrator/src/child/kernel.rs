//! Exact B0 acceptance event observations.

use peritus_kernel::{KernelEvent, KernelEventKind, KernelSubject};
use peritus_types::{CommandId, EventId, EventSequence, RevisionTuple, RunId};

use super::binding;
use crate::{AcceptanceCertificate, OrchestratorError};

/// B0 acceptance outcome that E0 may observe but cannot mint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum KernelAcceptanceOutcome {
    /// B0 durably began the exact planned acceptance request.
    Begun,
    /// B0 durably accepted the exact revision.
    Accepted,
    /// B0 durably requested another candidate cycle.
    NeedsChanges,
    /// B0 durably cancelled the run while acceptance was active.
    Cancelled,
}

/// Exact durable B0 acceptance event observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KernelAcceptanceObservation {
    event_id: EventId,
    command_id: CommandId,
    sequence: EventSequence,
    previous_event_id: Option<EventId>,
    run_id: RunId,
    revision: RevisionTuple,
    outcome: KernelAcceptanceOutcome,
}

impl KernelAcceptanceObservation {
    /// Checks an authoritative B0 event against the exact causal request and revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the event is not an allowed outcome or differs from the plan.
    pub fn from_event(
        event: KernelEvent,
        certificate: &AcceptanceCertificate,
        expected_run: RunId,
    ) -> Result<Self, OrchestratorError> {
        let plan = certificate.kernel_plan();
        let outcome = match event.kind() {
            KernelEventKind::AcceptanceBegun => KernelAcceptanceOutcome::Begun,
            KernelEventKind::AcceptanceAccepted => KernelAcceptanceOutcome::Accepted,
            KernelEventKind::AcceptanceNeedsChanges => KernelAcceptanceOutcome::NeedsChanges,
            KernelEventKind::RunCancelled => KernelAcceptanceOutcome::Cancelled,
            _ => return Err(binding("B0 event is not an acceptance lifecycle outcome")),
        };
        let envelope_matches = match outcome {
            KernelAcceptanceOutcome::Begun => {
                event.command_id() == plan.begin_command_id()
                    && event.id() == plan.begin_event_id()
                    && event.previous_event_id() == plan.expected_previous_kernel_event()
            }
            KernelAcceptanceOutcome::Accepted | KernelAcceptanceOutcome::NeedsChanges => {
                event.command_id() == plan.evaluate_command_id()
                    && event.id() == plan.evaluate_event_id()
                    && event.previous_event_id() == Some(plan.evaluate_previous_event_id())
            }
            KernelAcceptanceOutcome::Cancelled => true,
        };
        let subject_matches = match outcome {
            KernelAcceptanceOutcome::Cancelled => {
                event.subject() == KernelSubject::Run(expected_run)
            }
            _ => event.subject() == KernelSubject::Acceptance(expected_run),
        };
        if !subject_matches || event.revision() != certificate.revision() || !envelope_matches {
            return Err(binding("B0 acceptance event differs from the exact E0 request"));
        }
        Ok(Self {
            event_id: event.id(),
            command_id: event.command_id(),
            sequence: event.sequence(),
            previous_event_id: event.previous_event_id(),
            run_id: expected_run,
            revision: certificate.revision(),
            outcome,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_wire(
        event_id: EventId,
        command_id: CommandId,
        sequence: EventSequence,
        previous_event_id: Option<EventId>,
        run_id: RunId,
        revision: RevisionTuple,
        outcome: KernelAcceptanceOutcome,
    ) -> Self {
        Self { event_id, command_id, sequence, previous_event_id, run_id, revision, outcome }
    }

    #[must_use]
    /// Returns the exact B0 event identity.
    pub const fn event_id(self) -> EventId {
        self.event_id
    }
    #[must_use]
    /// Returns the exact B0 command identity.
    pub const fn command_id(self) -> CommandId {
        self.command_id
    }
    #[must_use]
    /// Returns the B0 aggregate sequence of the observed event.
    pub const fn sequence(self) -> EventSequence {
        self.sequence
    }
    #[must_use]
    /// Returns the preceding B0 aggregate event identity.
    pub const fn previous_event_id(self) -> Option<EventId> {
        self.previous_event_id
    }
    #[must_use]
    /// Returns the overall E0/B0 run identity.
    pub const fn run_id(self) -> RunId {
        self.run_id
    }
    #[must_use]
    /// Returns the exact candidate revision observed by B0.
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns the normalized B0 acceptance-lifecycle outcome.
    pub const fn outcome(self) -> KernelAcceptanceOutcome {
        self.outcome
    }
}
