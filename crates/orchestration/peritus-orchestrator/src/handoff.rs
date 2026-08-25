//! Exact bounded role handoffs without free-form hidden reasoning.
use peritus_collaboration::CollaborationTaskId;
use peritus_role::HarnessRole;
use peritus_scheduler::WorkId;
use peritus_types::{ActorId, FindingId, Sha256Digest, TurnId};
use sha2::{Digest, Sha256};

use crate::{
    ActivePhase, CandidateBinding, HandoffId, OrchestratorError, OrchestratorErrorKind,
    OrchestratorLimits, OrchestratorRecoveryAction,
};

/// Closed role-bearing handoff class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HandoffKind {
    /// Initial candidate-production work.
    Writer,
    /// Fresh independent review work.
    Reviewer,
    /// Current-finding remediation work.
    Fixer,
}

impl HandoffKind {
    /// Returns the only permitted source phase.
    #[must_use]
    pub const fn source_phase(self) -> Option<ActivePhase> {
        match self {
            Self::Writer => None,
            Self::Reviewer => Some(ActivePhase::GatesActive),
            Self::Fixer => Some(ActivePhase::ReviewActive),
        }
    }

    /// Returns the only permitted source role.
    #[must_use]
    pub const fn source_role(self) -> HandoffRole {
        match self {
            Self::Writer | Self::Reviewer => HandoffRole::Orchestrator,
            Self::Fixer => HandoffRole::Reviewer,
        }
    }

    /// Returns the only permitted destination role.
    #[must_use]
    pub const fn destination_role(self) -> HandoffRole {
        match self {
            Self::Writer => HandoffRole::Writer,
            Self::Reviewer => HandoffRole::Reviewer,
            Self::Fixer => HandoffRole::Fixer,
        }
    }

    /// Returns the only permitted destination phase.
    #[must_use]
    pub const fn destination_phase(self) -> ActivePhase {
        match self {
            Self::Writer => ActivePhase::WriterPending,
            Self::Reviewer => ActivePhase::ReviewPending,
            Self::Fixer => ActivePhase::FixerPending,
        }
    }
}

/// Exact role retained on a handoff endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HandoffRole {
    /// E0 service coordination without raw effects.
    Orchestrator,
    /// Candidate-producing writer.
    Writer,
    /// Fresh independent reviewer.
    Reviewer,
    /// Finding-remediating fixer.
    Fixer,
}

impl HandoffRole {
    /// Returns the C6 harness role when the endpoint directly executes agent work.
    #[must_use]
    pub const fn harness_role(self) -> Option<HarnessRole> {
        match self {
            Self::Orchestrator => None,
            Self::Writer => Some(HarnessRole::Writer),
            Self::Reviewer => Some(HarnessRole::Reviewer),
            Self::Fixer => Some(HarnessRole::Fixer),
        }
    }
}

/// One immutable causal role handoff bound to exact D3 ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Handoff {
    id: HandoffId,
    kind: HandoffKind,
    source_phase: Option<ActivePhase>,
    source_role: HandoffRole,
    destination_phase: ActivePhase,
    source_actor: ActorId,
    destination_actor: ActorId,
    destination_role: HandoffRole,
    candidate: CandidateBinding,
    turn_id: Option<TurnId>,
    task_id: CollaborationTaskId,
    work_id: WorkId,
    artifact_inputs: Vec<Sha256Digest>,
    evidence_inputs: Vec<Sha256Digest>,
    blocking_findings: Vec<FindingId>,
    digest: Sha256Digest,
}

impl Handoff {
    /// Creates one canonical exact role handoff.
    ///
    /// # Errors
    /// Rejects role/phase mismatch, oversized or noncanonical inputs, or invalid candidates.
    #[allow(clippy::too_many_arguments, reason = "causal handoff bindings remain explicit")]
    pub fn new(
        id: HandoffId,
        kind: HandoffKind,
        source_actor: ActorId,
        destination_actor: ActorId,
        candidate: CandidateBinding,
        turn_id: Option<TurnId>,
        task_id: CollaborationTaskId,
        work_id: WorkId,
        artifact_inputs: Vec<Sha256Digest>,
        evidence_inputs: Vec<Sha256Digest>,
        blocking_findings: Vec<FindingId>,
        limits: OrchestratorLimits,
    ) -> Result<Self, OrchestratorError> {
        let mut value = Self::from_wire(
            id,
            kind,
            kind.source_phase(),
            kind.source_role(),
            kind.destination_phase(),
            source_actor,
            destination_actor,
            kind.destination_role(),
            candidate,
            turn_id,
            task_id,
            work_id,
            artifact_inputs,
            evidence_inputs,
            blocking_findings,
            Sha256Digest::new([0; 32]),
        );
        value.validate_shape(limits)?;
        value.digest = handoff_digest(&value)?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments, reason = "exact closed-wire handoff reconstruction")]
    pub(crate) const fn from_wire(
        id: HandoffId,
        kind: HandoffKind,
        source_phase: Option<ActivePhase>,
        source_role: HandoffRole,
        destination_phase: ActivePhase,
        source_actor: ActorId,
        destination_actor: ActorId,
        destination_role: HandoffRole,
        candidate: CandidateBinding,
        turn_id: Option<TurnId>,
        task_id: CollaborationTaskId,
        work_id: WorkId,
        artifact_inputs: Vec<Sha256Digest>,
        evidence_inputs: Vec<Sha256Digest>,
        blocking_findings: Vec<FindingId>,
        digest: Sha256Digest,
    ) -> Self {
        Self {
            id,
            kind,
            source_phase,
            source_role,
            destination_phase,
            source_actor,
            destination_actor,
            destination_role,
            candidate,
            turn_id,
            task_id,
            work_id,
            artifact_inputs,
            evidence_inputs,
            blocking_findings,
            digest,
        }
    }

    pub(crate) fn validate(&self, limits: OrchestratorLimits) -> Result<(), OrchestratorError> {
        self.validate_shape(limits)?;
        if self.digest != handoff_digest(self)? {
            return Err(reject(
                OrchestratorErrorKind::BindingMismatch,
                "handoff digest differs from its canonical fields",
            ));
        }
        Ok(())
    }

    fn validate_shape(&self, limits: OrchestratorLimits) -> Result<(), OrchestratorError> {
        self.candidate.validate(limits)?;
        let cap = usize::from(limits.artifact_references());
        let collections_valid = self.artifact_inputs.len() <= cap
            && self.evidence_inputs.len() <= cap
            && self.blocking_findings.len() <= cap
            && ordered(&self.artifact_inputs)
            && ordered(&self.evidence_inputs)
            && ordered(&self.blocking_findings);
        let turn_binding_valid = match self.kind {
            HandoffKind::Reviewer => self.turn_id.is_none(),
            HandoffKind::Writer | HandoffKind::Fixer => self.turn_id.is_some(),
        };
        let finding_binding_valid = match self.kind {
            HandoffKind::Fixer => !self.blocking_findings.is_empty(),
            HandoffKind::Writer | HandoffKind::Reviewer => self.blocking_findings.is_empty(),
        };
        let role_phase_valid = self.destination_role == self.kind.destination_role()
            && self.source_role == self.kind.source_role()
            && self.source_phase == self.kind.source_phase()
            && self.destination_phase == self.kind.destination_phase()
            && turn_binding_valid
            && finding_binding_valid;
        if collections_valid && role_phase_valid && self.source_actor != self.destination_actor {
            Ok(())
        } else {
            Err(reject(
                OrchestratorErrorKind::NonCanonical,
                "handoff role, phase, findings, or inputs are invalid or noncanonical",
            ))
        }
    }

    /// Returns stable idempotency identity.
    #[must_use]
    pub const fn id(&self) -> HandoffId {
        self.id
    }
    /// Returns role-bearing handoff class.
    #[must_use]
    pub const fn kind(&self) -> HandoffKind {
        self.kind
    }
    /// Returns source phase, absent only at genesis.
    #[must_use]
    pub const fn source_phase(&self) -> Option<ActivePhase> {
        self.source_phase
    }
    /// Returns exact source role.
    #[must_use]
    pub const fn source_role(&self) -> HandoffRole {
        self.source_role
    }
    /// Returns destination phase.
    #[must_use]
    pub const fn destination_phase(&self) -> ActivePhase {
        self.destination_phase
    }
    /// Returns source actor.
    #[must_use]
    pub const fn source_actor(&self) -> ActorId {
        self.source_actor
    }
    /// Returns destination actor.
    #[must_use]
    pub const fn destination_actor(&self) -> ActorId {
        self.destination_actor
    }
    /// Returns destination role.
    #[must_use]
    pub const fn destination_role(&self) -> HandoffRole {
        self.destination_role
    }
    /// Borrows exact candidate binding.
    #[must_use]
    pub const fn candidate(&self) -> &CandidateBinding {
        &self.candidate
    }
    /// Returns the exact D0 turn for writer/fixer handoffs.
    #[must_use]
    pub const fn turn_id(&self) -> Option<TurnId> {
        self.turn_id
    }
    /// Returns D3 collaboration task.
    #[must_use]
    pub const fn task_id(&self) -> CollaborationTaskId {
        self.task_id
    }
    /// Returns D3 scheduler work.
    #[must_use]
    pub const fn work_id(&self) -> WorkId {
        self.work_id
    }
    /// Borrows canonical artifact inputs.
    #[must_use]
    pub fn artifact_inputs(&self) -> &[Sha256Digest] {
        &self.artifact_inputs
    }
    /// Borrows canonical evidence inputs.
    #[must_use]
    pub fn evidence_inputs(&self) -> &[Sha256Digest] {
        &self.evidence_inputs
    }
    /// Borrows canonical blocking findings.
    #[must_use]
    pub fn blocking_findings(&self) -> &[FindingId] {
        &self.blocking_findings
    }
    /// Returns canonical complete handoff digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn handoff_digest(value: &Handoff) -> Result<Sha256Digest, OrchestratorError> {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.orchestrator.handoff.v1\0");
    hasher.update(value.id.as_bytes());
    hasher.update([handoff_kind_tag(value.kind)]);
    hasher.update([value.source_phase.map_or(0, active_phase_tag)]);
    hasher.update([role_tag(value.source_role)]);
    hasher.update([active_phase_tag(value.destination_phase)]);
    hasher.update(value.source_actor.as_bytes());
    hasher.update(value.destination_actor.as_bytes());
    hasher.update([role_tag(value.destination_role)]);
    hasher.update(value.candidate.digest().as_bytes());
    hasher.update([u8::from(value.turn_id.is_some())]);
    if let Some(turn_id) = value.turn_id {
        hasher.update(turn_id.as_bytes());
    }
    hasher.update(value.task_id.as_bytes());
    hasher.update(value.work_id.as_bytes());
    hash_digests(&mut hasher, &value.artifact_inputs)?;
    hash_digests(&mut hasher, &value.evidence_inputs)?;
    let count = u16::try_from(value.blocking_findings.len()).map_err(|_| {
        reject(OrchestratorErrorKind::LimitExceeded, "handoff finding count is unrepresentable")
    })?;
    hasher.update(count.to_be_bytes());
    for finding in &value.blocking_findings {
        hasher.update(finding.as_bytes());
    }
    Ok(Sha256Digest::new(hasher.finalize().into()))
}

fn hash_digests(hasher: &mut Sha256, values: &[Sha256Digest]) -> Result<(), OrchestratorError> {
    let count = u16::try_from(values.len()).map_err(|_| {
        reject(OrchestratorErrorKind::LimitExceeded, "handoff input count is unrepresentable")
    })?;
    hasher.update(count.to_be_bytes());
    for value in values {
        hasher.update(value.as_bytes());
    }
    Ok(())
}

const fn handoff_kind_tag(value: HandoffKind) -> u8 {
    match value {
        HandoffKind::Writer => 1,
        HandoffKind::Reviewer => 2,
        HandoffKind::Fixer => 3,
    }
}

const fn active_phase_tag(value: ActivePhase) -> u8 {
    match value {
        ActivePhase::WriterPending => 1,
        ActivePhase::WriterActive => 2,
        ActivePhase::GatesPending => 3,
        ActivePhase::GatesActive => 4,
        ActivePhase::ReviewPending => 5,
        ActivePhase::ReviewActive => 6,
        ActivePhase::FixerPending => 7,
        ActivePhase::FixerActive => 8,
        ActivePhase::RevisionAdvancing => 9,
        ActivePhase::EvaluatingAcceptance => 10,
        ActivePhase::KernelAcceptancePending => 11,
    }
}

const fn role_tag(value: HandoffRole) -> u8 {
    match value {
        HandoffRole::Orchestrator => 1,
        HandoffRole::Writer => 2,
        HandoffRole::Reviewer => 3,
        HandoffRole::Fixer => 4,
    }
}

fn ordered<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

const fn reject(kind: OrchestratorErrorKind, detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(kind, OrchestratorRecoveryAction::CorrectInput, detail)
}
