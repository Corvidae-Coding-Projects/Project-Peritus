//! Stable read-only E0 lifecycle projection.

use peritus_types::{EventId, EventSequence, RevisionTuple, RunId, Sha256Digest};

use crate::{ChildAggregateKind, OrchestratorPhase, OrchestratorState, OrchestratorTerminalKind};

/// Compact externally consumable truth derived from one complete E0 checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorProjection {
    run_id: RunId,
    revision: RevisionTuple,
    phase: OrchestratorPhase,
    sequence: EventSequence,
    last_event_id: EventId,
    state_digest: Sha256Digest,
    active_children: Vec<ChildAggregateKind>,
    pending_directive: Option<crate::DirectiveId>,
    acceptance_certificate: Option<Sha256Digest>,
    terminal: Option<OrchestratorTerminalKind>,
}

impl OrchestratorProjection {
    /// Derives a projection without granting mutation authority.
    #[must_use]
    pub fn from_state(state: &OrchestratorState) -> Self {
        Self {
            run_id: state.binding().run_id(),
            revision: state.current_candidate().revision(),
            phase: state.phase(),
            sequence: state.sequence(),
            last_event_id: state.last_event_id(),
            state_digest: state.state_digest(),
            active_children: state.active_children().to_vec(),
            pending_directive: state.pending_directive().map(crate::PendingDirective::id),
            acceptance_certificate: state
                .acceptance_certificate()
                .map(crate::AcceptanceCertificate::digest),
            terminal: state.terminal().map(|value| value.kind()),
        }
    }

    #[must_use]
    /// Returns the projected E0 run identity.
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    #[must_use]
    /// Returns the projected current candidate revision.
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns the projected lifecycle phase.
    pub const fn phase(&self) -> OrchestratorPhase {
        self.phase
    }
    #[must_use]
    /// Returns the projected aggregate event sequence.
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }
    #[must_use]
    /// Returns the projected aggregate head event identity.
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    #[must_use]
    /// Returns the digest of the complete source checkpoint.
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    #[must_use]
    /// Returns the child aggregates still owned by E0.
    pub fn active_children(&self) -> &[ChildAggregateKind] {
        &self.active_children
    }
    #[must_use]
    /// Returns the stable pending directive identity, if any.
    pub const fn pending_directive(&self) -> Option<crate::DirectiveId> {
        self.pending_directive
    }
    #[must_use]
    /// Returns the retained acceptance certificate digest, if evaluated.
    pub const fn acceptance_certificate(&self) -> Option<Sha256Digest> {
        self.acceptance_certificate
    }
    #[must_use]
    /// Returns the terminal classification, if the run is terminal.
    pub const fn terminal(&self) -> Option<OrchestratorTerminalKind> {
        self.terminal
    }
}
