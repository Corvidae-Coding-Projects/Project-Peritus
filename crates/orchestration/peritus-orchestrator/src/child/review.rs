//! D2 review observations with quorum and finding conservation.

use peritus_review::{
    DispositionKind, FixerResponse, ReviewRunPhase, ReviewRunState, ReviewTerminalKind,
};
use peritus_types::{ActorId, FindingId, RevisionTuple, RunId, Sha256Digest};

use super::{ChildAggregateKind, ChildHead, ChildTerminalClass, binding, stale};
use crate::{Handoff, HandoffId, HandoffKind, OrchestratorError};

/// Exact D2 disposition fact proving one D0 fixer response was durably recorded.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReviewFixerRecord {
    finding_id: FindingId,
    kind: DispositionKind,
    actor: ActorId,
    response_digest: Sha256Digest,
}

impl ReviewFixerRecord {
    pub(crate) const fn from_wire(
        finding_id: FindingId,
        kind: DispositionKind,
        actor: ActorId,
        response_digest: Sha256Digest,
    ) -> Self {
        Self { finding_id, kind, actor, response_digest }
    }

    #[must_use]
    /// Returns the D2 finding covered by this durable record.
    pub const fn finding_id(self) -> FindingId {
        self.finding_id
    }
    #[must_use]
    /// Returns the recorded D2 disposition kind.
    pub const fn kind(self) -> DispositionKind {
        self.kind
    }
    #[must_use]
    /// Returns the fixer actor recorded by D2.
    pub const fn actor(self) -> ActorId {
        self.actor
    }
    #[must_use]
    /// Returns the exact fixer-response record digest.
    pub const fn response_digest(self) -> Sha256Digest {
        self.response_digest
    }
}

/// Checked D2 head proving every handed-off fixer response is durable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewFixerObservation {
    handoff_id: HandoffId,
    run_id: RunId,
    revision: RevisionTuple,
    binding_digest: Sha256Digest,
    records: Vec<ReviewFixerRecord>,
    head: ChildHead,
}

impl ReviewFixerObservation {
    /// Projects exact current D2 disposition records for all handed-off findings.
    ///
    /// # Errors
    ///
    /// Returns an error when disposition records do not exactly cover the fixer handoff.
    pub fn from_state(
        state: &ReviewRunState,
        handoff: &Handoff,
        responses: &[(FindingId, FixerResponse)],
    ) -> Result<Self, OrchestratorError> {
        if handoff.kind() != HandoffKind::Fixer
            || state.binding().revision() != handoff.candidate().revision()
            || responses.len() != handoff.blocking_findings().len()
        {
            return Err(binding("D2 fixer observation differs from its handoff"));
        }
        let mut records = Vec::with_capacity(responses.len());
        for (expected, (finding_id, response)) in handoff.blocking_findings().iter().zip(responses)
        {
            let finding = state
                .finding(*finding_id)
                .ok_or_else(|| binding("D2 fixer response finding is absent"))?;
            let disposition = finding
                .dispositions()
                .last()
                .ok_or_else(|| binding("D2 finding lacks the fixer disposition"))?;
            let kind = response_kind(response);
            if expected != finding_id
                || (response.actor() != handoff.destination_actor()
                    || response.revision() != handoff.candidate().revision())
                || disposition.kind() != kind
                || disposition.actor() != Some(response.actor())
                || disposition.revision() != response.revision()
                || disposition.record_digest() != response.digest()
            {
                return Err(binding("D2 disposition differs from the exact fixer response"));
            }
            records.push(ReviewFixerRecord::from_wire(
                *finding_id,
                kind,
                response.actor(),
                response.digest(),
            ));
        }
        Self::from_wire(
            handoff.id(),
            state.run_id(),
            state.binding().revision(),
            state.binding().digest(),
            records,
            ChildHead::new(
                ChildAggregateKind::Review,
                state.sequence(),
                state.last_event_id(),
                state.state_digest(),
                None,
            )?,
        )
    }

    pub(crate) fn from_wire(
        handoff_id: HandoffId,
        run_id: RunId,
        revision: RevisionTuple,
        binding_digest: Sha256Digest,
        records: Vec<ReviewFixerRecord>,
        head: ChildHead,
    ) -> Result<Self, OrchestratorError> {
        if records.is_empty()
            || records.windows(2).any(|pair| pair[0].finding_id >= pair[1].finding_id)
            || records.iter().any(|record| {
                !matches!(
                    record.kind,
                    DispositionKind::Fixed
                        | DispositionKind::Disputed
                        | DispositionKind::SupersessionProposed
                        | DispositionKind::WaiverRequested
                ) || record.response_digest.as_bytes().iter().all(|byte| *byte == 0)
            })
            || head.aggregate() != ChildAggregateKind::Review
            || head.terminal().is_some()
        {
            return Err(binding("decoded D2 fixer observation is inconsistent"));
        }
        Ok(Self { handoff_id, run_id, revision, binding_digest, records, head })
    }

    #[must_use]
    /// Returns the fixer handoff whose responses were recorded.
    pub const fn handoff_id(&self) -> HandoffId {
        self.handoff_id
    }
    #[must_use]
    /// Returns the persistent D2 review run identity.
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    #[must_use]
    /// Returns the candidate revision of the recorded responses.
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns the exact D2 binding digest at this head.
    pub const fn binding_digest(&self) -> Sha256Digest {
        self.binding_digest
    }
    #[must_use]
    /// Returns the canonical ordered fixer-response records.
    pub fn records(&self) -> &[ReviewFixerRecord] {
        &self.records
    }
    #[must_use]
    /// Returns the nonterminal D2 head containing the dispositions.
    pub const fn head(&self) -> ChildHead {
        self.head
    }
}

/// Closed D2 observation outcome including the active needs-fix branch.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReviewObservationClass {
    /// Quorum is complete but blocking findings require a fixer.
    NeedsFix,
    /// Quorum and finding conservation are complete.
    Completed,
    /// Review requires human judgment.
    NeedsHuman,
    /// Review ended in deterministic failure.
    Failed,
    /// Review was cancelled without success.
    Cancelled,
}

const fn response_kind(response: &FixerResponse) -> DispositionKind {
    match response {
        FixerResponse::Fixed { .. } => DispositionKind::Fixed,
        FixerResponse::Disputed { .. } => DispositionKind::Disputed,
        FixerResponse::SupersessionProposed { .. } => DispositionKind::SupersessionProposed,
        FixerResponse::WaiverRequested { .. } => DispositionKind::WaiverRequested,
    }
}

/// Checked terminal D2 projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewChildObservation {
    run_id: RunId,
    revision: RevisionTuple,
    binding_digest: Sha256Digest,
    quorum_complete: bool,
    unconserved_findings: Vec<FindingId>,
    class: ReviewObservationClass,
    head: ChildHead,
}

impl ReviewChildObservation {
    /// Projects one terminal D2 state with exact quorum and finding-conservation truth.
    ///
    /// # Errors
    ///
    /// Returns an error when the review state is neither a valid needs-fix state nor terminal.
    pub fn from_state(state: &ReviewRunState) -> Result<Self, OrchestratorError> {
        let (class, normalized) = match (state.phase(), state.terminal()) {
            (ReviewRunPhase::Active, None)
                if state.quorum().complete()
                    && !state.unconserved_current_findings().is_empty()
                    && !state.oscillation().triggered() =>
            {
                (ReviewObservationClass::NeedsFix, None)
            }
            (ReviewRunPhase::Terminal, Some(terminal)) => match terminal.kind() {
                ReviewTerminalKind::Completed => {
                    (ReviewObservationClass::Completed, Some(ChildTerminalClass::Completed))
                }
                ReviewTerminalKind::NeedsHuman => {
                    (ReviewObservationClass::NeedsHuman, Some(ChildTerminalClass::NeedsHuman))
                }
                ReviewTerminalKind::Failed => {
                    (ReviewObservationClass::Failed, Some(ChildTerminalClass::Failed))
                }
                ReviewTerminalKind::Cancelled => {
                    (ReviewObservationClass::Cancelled, Some(ChildTerminalClass::Cancelled))
                }
            },
            _ => return Err(stale("D2 state is neither needs-fix nor terminal")),
        };
        let quorum_complete = state.quorum().complete();
        let findings = state.unconserved_current_findings();
        if class == ReviewObservationClass::Completed && (!quorum_complete || !findings.is_empty())
        {
            return Err(binding("completed D2 projection lacks quorum or finding conservation"));
        }
        let head = ChildHead::new(
            ChildAggregateKind::Review,
            state.sequence(),
            state.last_event_id(),
            state.state_digest(),
            normalized,
        )?;
        Ok(Self {
            run_id: state.run_id(),
            revision: state.binding().revision(),
            binding_digest: state.binding().digest(),
            quorum_complete,
            unconserved_findings: findings,
            class,
            head,
        })
    }

    pub(crate) fn from_wire(
        run_id: RunId,
        revision: RevisionTuple,
        binding_digest: Sha256Digest,
        quorum_complete: bool,
        unconserved_findings: Vec<FindingId>,
        class: ReviewObservationClass,
        head: ChildHead,
    ) -> Result<Self, OrchestratorError> {
        let expected_terminal = match class {
            ReviewObservationClass::NeedsFix => None,
            ReviewObservationClass::Completed => Some(ChildTerminalClass::Completed),
            ReviewObservationClass::NeedsHuman => Some(ChildTerminalClass::NeedsHuman),
            ReviewObservationClass::Failed => Some(ChildTerminalClass::Failed),
            ReviewObservationClass::Cancelled => Some(ChildTerminalClass::Cancelled),
        };
        if head.aggregate() != ChildAggregateKind::Review
            || unconserved_findings.windows(2).any(|pair| pair[0] >= pair[1])
            || head.terminal() != expected_terminal
            || (class == ReviewObservationClass::Completed
                && (!quorum_complete || !unconserved_findings.is_empty()))
            || (class == ReviewObservationClass::NeedsFix
                && (!quorum_complete || unconserved_findings.is_empty()))
        {
            return Err(binding("decoded D2 observation is inconsistent"));
        }
        Ok(Self {
            run_id,
            revision,
            binding_digest,
            quorum_complete,
            unconserved_findings,
            class,
            head,
        })
    }

    #[must_use]
    /// Returns the persistent D2 review run identity.
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    #[must_use]
    /// Returns the candidate revision represented by this observation.
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns the exact D2 binding digest.
    pub const fn binding_digest(&self) -> Sha256Digest {
        self.binding_digest
    }
    #[must_use]
    /// Returns whether the configured reviewer quorum is complete.
    pub const fn quorum_complete(&self) -> bool {
        self.quorum_complete
    }
    #[must_use]
    /// Returns canonical blocking findings not yet conserved by D2.
    pub fn unconserved_findings(&self) -> &[FindingId] {
        &self.unconserved_findings
    }
    #[must_use]
    /// Returns the normalized D2 observation classification.
    pub const fn class(&self) -> ReviewObservationClass {
        self.class
    }
    #[must_use]
    /// Returns the authoritative current D2 head.
    pub const fn head(&self) -> ChildHead {
        self.head
    }
}
