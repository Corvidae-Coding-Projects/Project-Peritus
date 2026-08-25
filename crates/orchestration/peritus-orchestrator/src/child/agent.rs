//! D0 writer and fixer observations bound to exact E0/D3 handoffs.

use peritus_agent::{AgentPhase, AgentTurnState, TerminalKind};
use peritus_collaboration::CollaborationTaskId;
use peritus_review::FixerResponse;
use peritus_role::HarnessRole;
use peritus_scheduler::WorkId;
use peritus_types::{ActorId, AttemptId, FindingId, RevisionTuple, RunId, Sha256Digest, TurnId};

use super::{ChildAggregateKind, ChildHead, ChildTerminalClass, binding, stale};
use crate::{
    Handoff, HandoffId, HandoffKind, OrchestratorBinding, OrchestratorError, OrchestratorErrorKind,
    OrchestratorRecoveryAction,
};

/// Exact D2 fixer-response identity associated with one handed-off finding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FixerResponseIdentity {
    finding_id: FindingId,
    response_digest: Sha256Digest,
}

impl FixerResponseIdentity {
    pub(crate) const fn from_wire(finding_id: FindingId, response_digest: Sha256Digest) -> Self {
        Self { finding_id, response_digest }
    }

    #[must_use]
    /// Returns the D2 finding answered by the fixer.
    pub const fn finding_id(self) -> FindingId {
        self.finding_id
    }
    #[must_use]
    /// Returns the canonical fixer-response digest.
    pub const fn response_digest(self) -> Sha256Digest {
        self.response_digest
    }
}

/// Checked terminal writer/fixer observation from D0.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentChildObservation {
    handoff_id: HandoffId,
    task_id: CollaborationTaskId,
    work_id: WorkId,
    turn_id: TurnId,
    run_id: RunId,
    actor: ActorId,
    role: HarnessRole,
    attempt_id: AttemptId,
    revision: RevisionTuple,
    proposal_digest: Option<Sha256Digest>,
    fixer_responses: Vec<FixerResponseIdentity>,
    head: ChildHead,
}

impl AgentChildObservation {
    /// Observes an exact terminal D0 state and verifies its E0/D3 handoff binding.
    ///
    /// Fixer responses are paired with finding identities in the handoff's canonical order.
    ///
    /// # Errors
    ///
    /// Returns an error when state identity, terminal truth, or fixer response coverage differs
    /// from the checked handoff.
    pub fn from_state(
        state: &AgentTurnState,
        orchestrator: &OrchestratorBinding,
        handoff: &Handoff,
        fixer_responses: &[(FindingId, FixerResponse)],
    ) -> Result<Self, OrchestratorError> {
        let role = handoff
            .destination_role()
            .harness_role()
            .ok_or_else(|| binding("D0 handoff destination is not an agent role"))?;
        if !matches!(handoff.kind(), HandoffKind::Writer | HandoffKind::Fixer)
            || !matches!(role, HarnessRole::Writer | HarnessRole::Fixer)
            || state.binding().actor_id() != handoff.destination_actor()
            || Some(state.binding().turn_id()) != handoff.turn_id()
            || state.binding().role() != role.actor_role()
            || state.binding().attempt_id() != orchestrator.attempt_id()
            || state.binding().revision() != handoff.candidate().revision()
            || handoff.candidate().revision().acceptance_spec_id() != orchestrator.contract_id()
        {
            return Err(binding("D0 state differs from its exact E0/D3 handoff"));
        }
        let terminal = match state.phase() {
            AgentPhase::Terminal(TerminalKind::Completed) => ChildTerminalClass::Completed,
            AgentPhase::Terminal(TerminalKind::Failed) => ChildTerminalClass::Failed,
            AgentPhase::Terminal(TerminalKind::Cancelled) => ChildTerminalClass::Cancelled,
            _ => return Err(stale("D0 observation is not terminal")),
        };
        let identities = if terminal == ChildTerminalClass::Completed {
            validate_responses(handoff, fixer_responses)?
        } else if fixer_responses.is_empty() {
            Vec::new()
        } else {
            return Err(binding("non-completed D0 observation cannot claim fixer responses"));
        };
        let proposal_digest = state.completion().map(crate::canonical::completion_digest);
        if terminal == ChildTerminalClass::Completed && proposal_digest.is_none() {
            return Err(binding("completed D0 state lacks its inert completion proposal"));
        }
        Ok(Self {
            handoff_id: handoff.id(),
            task_id: handoff.task_id(),
            work_id: handoff.work_id(),
            turn_id: state.binding().turn_id(),
            run_id: orchestrator.run_id(),
            actor: handoff.destination_actor(),
            role,
            attempt_id: orchestrator.attempt_id(),
            revision: handoff.candidate().revision(),
            proposal_digest,
            fixer_responses: identities,
            head: ChildHead::new(
                ChildAggregateKind::Agent,
                state.sequence(),
                state.last_event_id(),
                state.state_digest(),
                Some(terminal),
            )?,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "exact child wire binding remains explicit")]
    pub(crate) fn from_wire(
        handoff_id: HandoffId,
        task_id: CollaborationTaskId,
        work_id: WorkId,
        turn_id: TurnId,
        run_id: RunId,
        actor: ActorId,
        role: HarnessRole,
        attempt_id: AttemptId,
        revision: RevisionTuple,
        proposal_digest: Option<Sha256Digest>,
        fixer_responses: Vec<FixerResponseIdentity>,
        head: ChildHead,
    ) -> Result<Self, OrchestratorError> {
        if head.aggregate() != ChildAggregateKind::Agent
            || !matches!(role, HarnessRole::Writer | HarnessRole::Fixer)
            || head.terminal().is_none()
            || (head.terminal() == Some(ChildTerminalClass::Completed) && proposal_digest.is_none())
            || (head.terminal() != Some(ChildTerminalClass::Completed)
                && !fixer_responses.is_empty())
            || fixer_responses.windows(2).any(|pair| pair[0].finding_id >= pair[1].finding_id)
            || (role == HarnessRole::Writer && !fixer_responses.is_empty())
        {
            return Err(binding("decoded D0 observation is inconsistent"));
        }
        Ok(Self {
            handoff_id,
            task_id,
            work_id,
            turn_id,
            run_id,
            actor,
            role,
            attempt_id,
            revision,
            proposal_digest,
            fixer_responses,
            head,
        })
    }

    #[must_use]
    /// Returns the exact E0 handoff completed by D0.
    pub const fn handoff_id(&self) -> HandoffId {
        self.handoff_id
    }
    #[must_use]
    /// Returns the exact D3 collaboration task identity.
    pub const fn task_id(&self) -> CollaborationTaskId {
        self.task_id
    }
    #[must_use]
    /// Returns the exact D3 scheduler work identity.
    pub const fn work_id(&self) -> WorkId {
        self.work_id
    }
    #[must_use]
    /// Returns the exact D0 turn aggregate identity.
    pub const fn turn_id(&self) -> TurnId {
        self.turn_id
    }
    #[must_use]
    /// Returns the overall E0 run identity.
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    #[must_use]
    /// Returns the actor that owned the completed handoff.
    pub const fn actor(&self) -> ActorId {
        self.actor
    }
    #[must_use]
    /// Returns the actor's harness role for the handoff.
    pub const fn role(&self) -> HarnessRole {
        self.role
    }
    #[must_use]
    /// Returns the immutable attempt identity.
    pub const fn attempt_id(&self) -> AttemptId {
        self.attempt_id
    }
    #[must_use]
    /// Returns the exact candidate revision observed by D0.
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns the inert D0 completion proposal digest when present.
    pub const fn proposal_digest(&self) -> Option<Sha256Digest> {
        self.proposal_digest
    }
    #[must_use]
    /// Returns whether D0 completed successfully.
    pub const fn is_completed(&self) -> bool {
        matches!(self.head.terminal(), Some(ChildTerminalClass::Completed))
    }
    #[must_use]
    /// Returns the ordered fixer-response identities retained from D0.
    pub fn fixer_responses(&self) -> &[FixerResponseIdentity] {
        &self.fixer_responses
    }
    #[must_use]
    /// Returns the exact terminal D0 aggregate head.
    pub const fn head(&self) -> ChildHead {
        self.head
    }
}

fn validate_responses(
    handoff: &Handoff,
    responses: &[(FindingId, FixerResponse)],
) -> Result<Vec<FixerResponseIdentity>, OrchestratorError> {
    if handoff.kind() == HandoffKind::Writer {
        return if responses.is_empty() {
            Ok(Vec::new())
        } else {
            Err(binding("writer observation cannot carry fixer responses"))
        };
    }
    if responses.len() != handoff.blocking_findings().len() {
        return Err(binding("fixer responses do not cover every handed-off finding"));
    }
    let mut identities = Vec::with_capacity(responses.len());
    for (expected, (finding, response)) in handoff.blocking_findings().iter().zip(responses) {
        if expected != finding
            || (response.actor() != handoff.destination_actor()
                || response.revision() != handoff.candidate().revision())
        {
            return Err(binding("fixer response differs from its finding, actor, or revision"));
        }
        identities.push(FixerResponseIdentity::from_wire(*finding, response.digest()));
    }
    if identities.iter().any(|item| item.response_digest.as_bytes().iter().all(|byte| *byte == 0)) {
        return Err(OrchestratorError::new(
            OrchestratorErrorKind::InvalidInput,
            OrchestratorRecoveryAction::CorrectInput,
            "fixer response digest must be nonzero",
        ));
    }
    Ok(identities)
}
