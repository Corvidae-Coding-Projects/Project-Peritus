//! Checked compound records embedded in E0 commands.

use peritus_types::Sha256Digest;

use crate::{
    AgentChildObservation, CandidateBinding, Handoff, OrchestratorBinding, OrchestratorError,
    OrchestratorErrorKind, OrchestratorRecoveryAction, RoleOwnership,
};

/// Complete immutable payload shared by the genesis command and accepted event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorGenesis {
    binding: OrchestratorBinding,
    candidate: CandidateBinding,
    ownership: RoleOwnership,
    writer_handoff: Handoff,
}

impl OrchestratorGenesis {
    /// Groups the four independently checked genesis values for reducer validation.
    #[must_use]
    pub const fn new(
        binding: OrchestratorBinding,
        candidate: CandidateBinding,
        ownership: RoleOwnership,
        writer_handoff: Handoff,
    ) -> Self {
        Self { binding, candidate, ownership, writer_handoff }
    }

    /// Borrows the complete immutable orchestrator binding.
    #[must_use]
    pub const fn binding(&self) -> &OrchestratorBinding {
        &self.binding
    }

    /// Borrows the exact initial candidate.
    #[must_use]
    pub const fn candidate(&self) -> &CandidateBinding {
        &self.candidate
    }

    /// Borrows the role-separated ownership binding.
    #[must_use]
    pub const fn ownership(&self) -> &RoleOwnership {
        &self.ownership
    }

    /// Borrows the genesis writer handoff.
    #[must_use]
    pub const fn writer_handoff(&self) -> &Handoff {
        &self.writer_handoff
    }
}

/// Exact child-head checkpoint required to pause and resume in-flight owned work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeReconciliation {
    checkpoint_state_digest: Sha256Digest,
    child_heads: Vec<crate::ChildHead>,
}

impl ResumeReconciliation {
    /// Binds canonical current child heads to one exact E0 checkpoint.
    ///
    /// # Errors
    /// Rejects a stale checkpoint or a missing, duplicate, or unexpected active child head.
    pub fn from_checkpoint(
        state: &crate::OrchestratorState,
        child_heads: Vec<crate::ChildHead>,
    ) -> Result<Self, OrchestratorError> {
        let value = Self { checkpoint_state_digest: state.state_digest(), child_heads };
        value.validate_for_state(state)?;
        Ok(value)
    }

    pub(crate) fn from_wire(
        checkpoint_state_digest: Sha256Digest,
        child_heads: Vec<crate::ChildHead>,
    ) -> Result<Self, OrchestratorError> {
        let value = Self { checkpoint_state_digest, child_heads };
        value.validate_shape()?;
        Ok(value)
    }

    pub(crate) fn validate_for_state(
        &self,
        state: &crate::OrchestratorState,
    ) -> Result<(), OrchestratorError> {
        self.validate_shape()?;
        let aggregates: Vec<_> =
            self.child_heads.iter().copied().map(crate::ChildHead::aggregate).collect();
        if self.checkpoint_state_digest == state.state_digest()
            && aggregates == state.active_children()
        {
            Ok(())
        } else {
            Err(stale("pause/resume child heads differ from the exact E0 checkpoint"))
        }
    }

    fn validate_shape(&self) -> Result<(), OrchestratorError> {
        if self.checkpoint_state_digest.as_bytes().iter().any(|byte| *byte != 0)
            && self.child_heads.windows(2).all(|pair| pair[0].aggregate() < pair[1].aggregate())
            && self.child_heads.iter().all(|head| {
                head.state_digest().as_bytes().iter().any(|byte| *byte != 0)
                    && head.terminal().is_none()
            })
        {
            Ok(())
        } else {
            Err(binding("pause/resume reconciliation is invalid or noncanonical"))
        }
    }

    /// Returns the exact E0 checkpoint digest.
    #[must_use]
    pub const fn checkpoint_state_digest(&self) -> Sha256Digest {
        self.checkpoint_state_digest
    }

    /// Borrows canonical active child heads.
    #[must_use]
    pub fn child_heads(&self) -> &[crate::ChildHead] {
        &self.child_heads
    }
}

/// Complete fixer result; its proposal remains inert until candidate advancement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixerCompletion {
    observation: AgentChildObservation,
    proposed_candidate: Option<CandidateBinding>,
    review_observation: Option<crate::ReviewFixerObservation>,
}

impl FixerCompletion {
    /// Checks exact D0 completion and D2 response coverage against its handoff.
    ///
    /// # Errors
    /// Rejects stale actors/revisions, incomplete finding coverage, or a materially unchanged
    /// proposal.
    pub fn new(
        observation: AgentChildObservation,
        proposed_candidate: CandidateBinding,
        review_observation: crate::ReviewFixerObservation,
        handoff: &Handoff,
    ) -> Result<Self, OrchestratorError> {
        let value = Self {
            observation,
            proposed_candidate: Some(proposed_candidate),
            review_observation: Some(review_observation),
        };
        value.validate(handoff)?;
        Ok(value)
    }

    /// Checks a terminal non-success fixer result against its exact handoff.
    ///
    /// # Errors
    /// Rejects a completed result or stale handoff identity.
    pub fn failed(
        observation: AgentChildObservation,
        handoff: &Handoff,
    ) -> Result<Self, OrchestratorError> {
        let value = Self { observation, proposed_candidate: None, review_observation: None };
        value.validate(handoff)?;
        Ok(value)
    }

    pub(crate) const fn from_wire(
        observation: AgentChildObservation,
        proposed_candidate: Option<CandidateBinding>,
        review_observation: Option<crate::ReviewFixerObservation>,
    ) -> Self {
        Self { observation, proposed_candidate, review_observation }
    }

    pub(crate) fn validate(&self, handoff: &Handoff) -> Result<(), OrchestratorError> {
        let findings_match = self.observation.fixer_responses().len()
            == handoff.blocking_findings().len()
            && self
                .observation
                .fixer_responses()
                .iter()
                .copied()
                .map(crate::FixerResponseIdentity::finding_id)
                .eq(handoff.blocking_findings().iter().copied());
        let common = handoff.kind() == crate::HandoffKind::Fixer
            && self.observation.handoff_id() == handoff.id()
            && self.observation.task_id() == handoff.task_id()
            && self.observation.work_id() == handoff.work_id()
            && self.observation.actor() == handoff.destination_actor()
            && self.observation.role() == peritus_role::HarnessRole::Fixer
            && self.observation.revision() == handoff.candidate().revision();
        let completion_valid = self.proposed_candidate.as_ref().map_or_else(
            || {
                !self.observation.is_completed()
                    && self.observation.fixer_responses().is_empty()
                    && self.review_observation.is_none()
            },
            |candidate| {
                self.observation.is_completed()
                    && findings_match
                    && !candidate.materially_equal(handoff.candidate())
                    && self.review_observation.as_ref().is_some_and(|review| {
                        review.handoff_id() == handoff.id()
                            && review.revision() == handoff.candidate().revision()
                            && review.records().len() == self.observation.fixer_responses().len()
                            && review.records().iter().zip(self.observation.fixer_responses()).all(
                                |(record, response)| {
                                    [
                                        record.finding_id() == response.finding_id(),
                                        record.response_digest() == response.response_digest(),
                                        record.actor() == handoff.destination_actor(),
                                    ]
                                    .into_iter()
                                    .all(|exact| exact)
                                },
                            )
                    })
            },
        );
        if common && completion_valid {
            Ok(())
        } else {
            Err(binding("fixer result differs from its handoff or response coverage"))
        }
    }

    /// Borrows the terminal D0 fixer observation.
    #[must_use]
    pub const fn observation(&self) -> &AgentChildObservation {
        &self.observation
    }

    /// Borrows the inert proposed successor candidate.
    #[must_use]
    pub const fn proposed_candidate(&self) -> Option<&CandidateBinding> {
        self.proposed_candidate.as_ref()
    }

    /// Returns the durable D2 fixer-response head required for successful advancement.
    #[must_use]
    pub const fn review_observation(&self) -> Option<&crate::ReviewFixerObservation> {
        self.review_observation.as_ref()
    }
}

const fn binding(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::BindingMismatch,
        OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}

const fn stale(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::StaleState,
        OrchestratorRecoveryAction::Replay,
        detail,
    )
}
