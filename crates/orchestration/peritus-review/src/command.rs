//! Closed pure D2 command vocabulary.

use peritus_evidence::EvidenceId;
use peritus_types::{
    CommandId, EventId, FindingId, ReviewCycleId, RevisionTuple, RunId, Sha256Digest,
};

use crate::{
    FixerResponse, ObservedWaiver, ReviewAssignment, ReviewBinding, ReviewLimits, ReviewSubmission,
};

/// Core semantic payload of one fenced review command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewCommandKind {
    /// Starts one run from a checked contract/candidate binding.
    StartRun {
        /// Complete immutable binding.
        binding: ReviewBinding,
        /// Complete independent D2 limits.
        limits: ReviewLimits,
    },
    /// Advances to a different exact candidate freshness binding.
    AdvanceRevision {
        /// Newly checked binding.
        binding: ReviewBinding,
    },
    /// Assigns one fresh-context reviewer cycle.
    AssignReviewer {
        /// Complete immutable assignment.
        assignment: ReviewAssignment,
    },
    /// Admits one complete structured submission.
    SubmitReview {
        /// Complete one-shot submission.
        submission: ReviewSubmission,
    },
    /// Reconciles duplicate identities under one existing canonical finding.
    ReconcileDuplicates {
        /// Existing canonical identity.
        canonical: FindingId,
        /// Nonempty canonical duplicate identity set.
        duplicates: Vec<FindingId>,
        /// Digest of the explicit reconciliation decision.
        reconciliation_digest: Sha256Digest,
    },
    /// Records fixer evidence without closing a finding.
    RecordFixerResponse {
        /// Current finding identity.
        finding_id: FindingId,
        /// Complete structured response.
        response: FixerResponse,
    },
    /// Confirms a pending fixed response.
    ConfirmResolution {
        /// Current finding identity.
        finding_id: FindingId,
        /// Current assigned independent reviewer cycle.
        reviewer_cycle: ReviewCycleId,
        /// Exact pending fixer-response digest.
        pending_response_digest: Sha256Digest,
        /// Canonical confirmation evidence.
        evidence: Vec<EvidenceId>,
        /// Digest of the reviewer confirmation.
        confirmation_digest: Sha256Digest,
    },
    /// Confirms that a current finding is invalid.
    ConfirmInvalidation {
        /// Current finding identity.
        finding_id: FindingId,
        /// Current assigned independent reviewer cycle.
        reviewer_cycle: ReviewCycleId,
        /// Exact pending dispute-response digest.
        pending_response_digest: Sha256Digest,
        /// Canonical confirmation evidence.
        evidence: Vec<EvidenceId>,
        /// Digest of the reviewer confirmation.
        confirmation_digest: Sha256Digest,
    },
    /// Confirms provenance-preserving supersession.
    ConfirmSupersession {
        /// Finding being superseded.
        finding_id: FindingId,
        /// Existing canonical replacement finding.
        superseding: FindingId,
        /// Current assigned independent reviewer cycle.
        reviewer_cycle: ReviewCycleId,
        /// Exact pending proposal digest.
        pending_response_digest: Sha256Digest,
        /// Canonical confirmation evidence.
        evidence: Vec<EvidenceId>,
        /// Digest of the reviewer confirmation.
        confirmation_digest: Sha256Digest,
    },
    /// Records an existing external authority request without granting it.
    RequestWaiver {
        /// Current finding identity.
        finding_id: FindingId,
        /// A [`FixerResponse::WaiverRequested`] payload.
        request: FixerResponse,
    },
    /// Consumes an exact external B2 waiver observation.
    ObserveWaiver {
        /// Existing external observation plus request binding.
        waiver: ObservedWaiver,
    },
    /// Cancels one unsubmitted current cycle.
    CancelCycle {
        /// Assigned cycle identity.
        cycle_id: ReviewCycleId,
    },
    /// Cancels the review run truthfully.
    CancelRun,
    /// Records explicit budget exhaustion as non-success.
    ExhaustBudget {
        /// Inert bounded reason digest.
        reason_digest: Sha256Digest,
    },
    /// Records an explicitly unrecoverable review failure.
    FailRun {
        /// Inert failure observation digest.
        failure_digest: Sha256Digest,
    },
    /// Computes D2 review completion from current state.
    FinalizeRun,
}

/// One syntax-checked but unprivileged D2 reducer command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewCommand {
    command_id: CommandId,
    event_id: EventId,
    run_id: RunId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    revision: RevisionTuple,
    kind: ReviewCommandKind,
}

impl ReviewCommand {
    /// Creates a command with exact genesis/non-genesis predecessor shape.
    ///
    /// # Errors
    /// Rejects inconsistent zero-sequence/predecessor combinations.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: ReviewCommandKind,
    ) -> Result<Self, crate::ReviewError> {
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(crate::error::reject(
                crate::ReviewErrorKind::StaleFence,
                "command predecessor shape is inconsistent",
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
    pub(super) const fn from_wire(
        command_id: CommandId,
        event_id: EventId,
        run_id: RunId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        revision: RevisionTuple,
        kind: ReviewCommandKind,
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

    /// Returns the idempotent command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns the reserved successor event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the run aggregate identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the expected current sequence, zero at genesis.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Returns the expected prior event.
    #[must_use]
    pub const fn expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    /// Returns the expected predecessor-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns the exact current revision fence.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Borrows the closed semantic payload.
    #[must_use]
    pub const fn kind(&self) -> &ReviewCommandKind {
        &self.kind
    }
}
