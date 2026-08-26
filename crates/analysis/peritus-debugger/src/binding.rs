//! Checked immutable bindings for one production analysis subject.

use crate::{DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery, SubjectId};
use peritus_agent::AgentTurnState;
use peritus_harness::{HarnessProjection, domain::HarnessRevisionIdentity};
use peritus_orchestrator::OrchestratorState;
use peritus_trace::CausalBinding;
use peritus_types::{
    AttemptId, EnvironmentId, EventId, RevisionTuple, RunId, SessionId, Sha256Digest,
};

const SUBJECT_ID_DOMAIN: &[u8] = b"peritus-e2-analysis-subject-id-v1\0";
const SUBJECT_CANONICAL_DOMAIN: &[u8] = b"peritus-e2-analysis-subject-v1\0";

/// Exact cross-slice provenance for one run attempt.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AnalysisSubject {
    id: SubjectId,
    run_id: RunId,
    attempt_id: AttemptId,
    session_id: SessionId,
    environment_id: EnvironmentId,
    revision: RevisionTuple,
    harness_revision: HarnessRevisionIdentity,
    orchestrator_event_id: EventId,
    orchestrator_state_digest: Sha256Digest,
    orchestrator_journal_position: u64,
    agent_event_id: EventId,
    agent_state_digest: Sha256Digest,
    agent_journal_position: u64,
}

impl AnalysisSubject {
    /// Checks recovered E0/D0 state and the exact immutable E1 revision before freezing a subject.
    ///
    /// # Errors
    ///
    /// Rejects any run, attempt, revision, provider, workspace, environment, harness lineage,
    /// full revision, or source-position disagreement.
    pub fn from_recovered(
        orchestrator: &OrchestratorState,
        agent: &AgentTurnState,
        harness: &HarnessProjection,
        harness_revision: HarnessRevisionIdentity,
        orchestrator_journal_position: u64,
        agent_journal_position: u64,
    ) -> Result<Self, DebuggerError> {
        if orchestrator_journal_position == 0 || agent_journal_position == 0 {
            return Err(binding_error("source journal positions must be one-based"));
        }
        let orchestrator_binding = orchestrator.binding();
        let agent_binding = agent.binding();
        let revision = orchestrator.current_candidate().revision();
        if orchestrator_binding.attempt_id() != agent_binding.attempt_id() {
            return Err(binding_error("E0 and D0 attempt identities differ"));
        }
        if revision != agent_binding.revision() {
            return Err(binding_error("E0 and D0 revision tuples differ"));
        }
        if revision.provider_profile_id() != agent_binding.provider_profile_id() {
            return Err(binding_error("D0 provider profile differs from the revision tuple"));
        }
        if revision.harness_id() != harness_revision.harness_id()
            || revision.harness_id() != harness.harness_id()
        {
            return Err(binding_error("E0/D0 and E1 harness identities differ"));
        }
        let selected_revision = harness
            .revision(harness_revision.digest())
            .ok_or_else(|| binding_error("selected full E1 revision is absent"))?;
        if selected_revision.identity() != harness_revision {
            return Err(binding_error("selected E1 revision identity is inconsistent"));
        }

        let mut subject = Self {
            id: SubjectId::new([1; 16])?,
            run_id: orchestrator_binding.run_id(),
            attempt_id: orchestrator_binding.attempt_id(),
            session_id: agent_binding.session_id(),
            environment_id: agent_binding.environment_id(),
            revision,
            harness_revision,
            orchestrator_event_id: orchestrator.last_event_id(),
            orchestrator_state_digest: orchestrator.state_digest(),
            orchestrator_journal_position,
            agent_event_id: agent.last_event_id(),
            agent_state_digest: agent.state_digest(),
            agent_journal_position,
        };
        subject.id = SubjectId::derive(SUBJECT_ID_DOMAIN, &subject.canonical_binding_bytes())?;
        Ok(subject)
    }

    /// Returns the content-derived subject identity.
    #[must_use]
    pub const fn id(&self) -> SubjectId {
        self.id
    }
    /// Returns the governed run.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the governed attempt.
    #[must_use]
    pub const fn attempt_id(&self) -> AttemptId {
        self.attempt_id
    }
    /// Returns the D0 session.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    /// Returns the D0 execution environment.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    /// Returns the exact E0/D0 revision tuple.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the full E1 revision identity.
    #[must_use]
    pub const fn harness_revision(&self) -> HarnessRevisionIdentity {
        self.harness_revision
    }
    /// Returns the E0 state-producing event.
    #[must_use]
    pub const fn orchestrator_event_id(&self) -> EventId {
        self.orchestrator_event_id
    }
    /// Returns the complete recovered E0 state digest.
    #[must_use]
    pub const fn orchestrator_state_digest(&self) -> Sha256Digest {
        self.orchestrator_state_digest
    }
    /// Returns the E0 source journal position.
    #[must_use]
    pub const fn orchestrator_journal_position(&self) -> u64 {
        self.orchestrator_journal_position
    }
    /// Returns the D0 state-producing event.
    #[must_use]
    pub const fn agent_event_id(&self) -> EventId {
        self.agent_event_id
    }
    /// Returns the complete recovered D0 state digest.
    #[must_use]
    pub const fn agent_state_digest(&self) -> Sha256Digest {
        self.agent_state_digest
    }
    /// Returns the D0 source journal position.
    #[must_use]
    pub const fn agent_journal_position(&self) -> u64 {
        self.agent_journal_position
    }

    /// Returns whether a complete production C7 binding belongs to this subject.
    #[must_use]
    pub fn owns(&self, binding: CausalBinding) -> bool {
        binding.session_id() == self.session_id
            && binding.run_id() == Some(self.run_id)
            && binding.attempt_id() == Some(self.attempt_id)
            && binding
                .provider_profile_id()
                .is_none_or(|profile| profile == self.revision.provider_profile_id())
    }

    /// Returns the complete stable canonical subject binding.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = self.canonical_binding_bytes();
        bytes.extend_from_slice(self.id.as_bytes());
        bytes
    }

    fn canonical_binding_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(256);
        bytes.extend_from_slice(SUBJECT_CANONICAL_DOMAIN);
        bytes.extend_from_slice(self.run_id.as_bytes());
        bytes.extend_from_slice(self.attempt_id.as_bytes());
        bytes.extend_from_slice(self.session_id.as_bytes());
        bytes.extend_from_slice(self.environment_id.as_bytes());
        encode_revision(&mut bytes, self.revision);
        bytes.extend_from_slice(self.harness_revision.harness_id().as_bytes());
        bytes.extend_from_slice(&self.harness_revision.number().get().to_be_bytes());
        bytes.extend_from_slice(self.harness_revision.digest().as_bytes());
        bytes.extend_from_slice(self.orchestrator_event_id.as_bytes());
        bytes.extend_from_slice(self.orchestrator_state_digest.as_bytes());
        bytes.extend_from_slice(&self.orchestrator_journal_position.to_be_bytes());
        bytes.extend_from_slice(self.agent_event_id.as_bytes());
        bytes.extend_from_slice(self.agent_state_digest.as_bytes());
        bytes.extend_from_slice(&self.agent_journal_position.to_be_bytes());
        bytes
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "canonical binding encoding is shared by sibling private modules only"
)]
pub(crate) fn encode_revision(bytes: &mut Vec<u8>, revision: RevisionTuple) {
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    bytes.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
    bytes.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
}

fn binding_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Binding,
        DebuggerOperation::ValidateBinding,
        DebuggerRecovery::RepairDependency,
        detail,
    )
}
