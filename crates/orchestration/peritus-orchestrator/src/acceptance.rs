//! B2-derived certificate data that cannot itself accept a B0 run.

use peritus_quality_policy::{AcceptanceDecision, AcceptanceEvidence, evaluate_acceptance};
use peritus_spec::AcceptanceContract;
use peritus_types::{AcceptanceSpecId, CommandId, EventId, RevisionTuple, Sha256Digest};

use crate::{
    CandidateBinding, OrchestratorBinding, OrchestratorError, OrchestratorErrorKind,
    OrchestratorRecoveryAction,
};

/// Exact two-envelope B0 acceptance plan committed with a B2 certificate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KernelAcceptancePlan {
    begin_command_id: CommandId,
    begin_event_id: EventId,
    expected_previous_kernel_event: Option<EventId>,
    evaluate_command_id: CommandId,
    evaluate_event_id: EventId,
}

impl KernelAcceptancePlan {
    /// Creates a collision-free Begin then Evaluate B0 acceptance plan.
    ///
    /// # Errors
    /// Rejects reused command or event identities.
    pub fn new(
        begin_command_id: CommandId,
        begin_event_id: EventId,
        expected_previous_kernel_event: Option<EventId>,
        evaluate_command_id: CommandId,
        evaluate_event_id: EventId,
    ) -> Result<Self, OrchestratorError> {
        let value = Self {
            begin_command_id,
            begin_event_id,
            expected_previous_kernel_event,
            evaluate_command_id,
            evaluate_event_id,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(self) -> Result<(), OrchestratorError> {
        if self.begin_command_id == self.evaluate_command_id
            || self.begin_event_id == self.evaluate_event_id
        {
            Err(binding("B0 acceptance plan reuses a command or event identity"))
        } else {
            Ok(())
        }
    }

    #[must_use]
    /// Returns the planned B0 Begin command identity.
    pub const fn begin_command_id(self) -> CommandId {
        self.begin_command_id
    }
    #[must_use]
    /// Returns the planned B0 Begin event identity.
    pub const fn begin_event_id(self) -> EventId {
        self.begin_event_id
    }
    #[must_use]
    /// Returns the kernel head that must precede the planned Begin event.
    pub const fn expected_previous_kernel_event(self) -> Option<EventId> {
        self.expected_previous_kernel_event
    }
    #[must_use]
    /// Returns the planned B0 Evaluate command identity.
    pub const fn evaluate_command_id(self) -> CommandId {
        self.evaluate_command_id
    }
    #[must_use]
    /// Returns the planned B0 Evaluate event identity.
    pub const fn evaluate_event_id(self) -> EventId {
        self.evaluate_event_id
    }
    #[must_use]
    /// Returns the Begin event that must immediately precede Evaluate.
    pub const fn evaluate_previous_event_id(self) -> EventId {
        self.begin_event_id
    }
}

/// Exact acceptable B2 evaluation retained before any B0 acceptance request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptanceCertificate {
    contract_id: AcceptanceSpecId,
    contract_digest: Sha256Digest,
    orchestrator_binding_digest: Sha256Digest,
    revision: RevisionTuple,
    candidate_binding_digest: Sha256Digest,
    gate_state_digest: Sha256Digest,
    review_state_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
    evaluation_request_digest: Sha256Digest,
    decision_digest: Sha256Digest,
    maximum_gate_attempts: u16,
    maximum_review_cycles: u16,
    kernel_plan: KernelAcceptancePlan,
    digest: Sha256Digest,
}

impl AcceptanceCertificate {
    /// Computes the stable payload digest for a B2 evaluation request before evaluation.
    #[must_use]
    pub fn request_digest(
        contract: &AcceptanceContract,
        orchestrator: &OrchestratorBinding,
        candidate: &CandidateBinding,
        evidence: &AcceptanceEvidence,
        gate_state_digest: Sha256Digest,
        review_state_digest: Sha256Digest,
    ) -> Sha256Digest {
        crate::canonical::evaluation_request_digest(
            contract.id(),
            contract.content_digest(),
            orchestrator.digest(),
            candidate.revision(),
            candidate.digest(),
            gate_state_digest,
            review_state_digest,
            crate::canonical::acceptance_evidence_digest(evidence),
        )
    }

    /// Creates a certificate only by reproducing the supplied acceptable B2 decision.
    ///
    /// This certificate is evidence, not lifecycle authority. Only a matching durable B0
    /// `AcceptanceAccepted` event can make the orchestrator accepted.
    ///
    /// # Errors
    /// Rejects an unacceptable, unrelated, stale, or non-reproducible decision.
    #[allow(clippy::too_many_arguments, reason = "complete acceptance evidence stays explicit")]
    pub fn from_evaluation(
        contract: &AcceptanceContract,
        orchestrator: &OrchestratorBinding,
        candidate: &CandidateBinding,
        evidence: &AcceptanceEvidence,
        decision: &AcceptanceDecision,
        gate_state_digest: Sha256Digest,
        review_state_digest: Sha256Digest,
        kernel_plan: KernelAcceptancePlan,
    ) -> Result<Self, OrchestratorError> {
        if candidate.revision().acceptance_spec_id() != contract.id()
            || orchestrator.contract_id() != contract.id()
            || orchestrator.contract_digest() != contract.content_digest()
        {
            return Err(binding("candidate revision names another acceptance contract"));
        }
        let reproduced = evaluate_acceptance(contract, candidate.revision(), evidence);
        if !decision.is_acceptable() || &reproduced != decision {
            return Err(OrchestratorError::new(
                OrchestratorErrorKind::InvalidInput,
                OrchestratorRecoveryAction::CorrectInput,
                "acceptance decision is unacceptable or does not reproduce for exact evidence",
            ));
        }
        let policy = contract.completion_policy();
        kernel_plan.validate()?;
        if decision.gate_attempt_limit() != policy.max_gate_attempts()
            || decision.review_cycle_limit() != policy.max_review_cycles()
        {
            return Err(binding("acceptance decision limits differ from the exact contract"));
        }
        let evidence_digest = crate::canonical::acceptance_evidence_digest(evidence);
        let evaluation_request_digest = Self::request_digest(
            contract,
            orchestrator,
            candidate,
            evidence,
            gate_state_digest,
            review_state_digest,
        );
        let mut certificate = Self {
            contract_id: contract.id(),
            contract_digest: contract.content_digest(),
            orchestrator_binding_digest: orchestrator.digest(),
            revision: candidate.revision(),
            candidate_binding_digest: candidate.digest(),
            gate_state_digest,
            review_state_digest,
            evidence_digest,
            evaluation_request_digest,
            decision_digest: crate::canonical::acceptance_decision_digest(decision),
            maximum_gate_attempts: decision.gate_attempt_limit(),
            maximum_review_cycles: decision.review_cycle_limit(),
            kernel_plan,
            digest: Sha256Digest::new([0; 32]),
        };
        certificate.digest = crate::canonical::certificate_digest(&certificate);
        Ok(certificate)
    }

    #[allow(clippy::too_many_arguments, reason = "certificate wire fields stay explicit")]
    pub(crate) fn from_wire(
        contract_id: AcceptanceSpecId,
        contract_digest: Sha256Digest,
        orchestrator_binding_digest: Sha256Digest,
        revision: RevisionTuple,
        candidate_binding_digest: Sha256Digest,
        gate_state_digest: Sha256Digest,
        review_state_digest: Sha256Digest,
        evidence_digest: Sha256Digest,
        evaluation_request_digest: Sha256Digest,
        decision_digest: Sha256Digest,
        maximum_gate_attempts: u16,
        maximum_review_cycles: u16,
        kernel_plan: KernelAcceptancePlan,
        digest: Sha256Digest,
    ) -> Result<Self, OrchestratorError> {
        let value = Self {
            contract_id,
            contract_digest,
            orchestrator_binding_digest,
            revision,
            candidate_binding_digest,
            gate_state_digest,
            review_state_digest,
            evidence_digest,
            evaluation_request_digest,
            decision_digest,
            maximum_gate_attempts,
            maximum_review_cycles,
            kernel_plan,
            digest,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<(), OrchestratorError> {
        if self.contract_id != self.revision.acceptance_spec_id()
            || self.maximum_gate_attempts == 0
            || self.maximum_review_cycles == 0
            || self.evaluation_request_digest
                != crate::canonical::evaluation_request_digest(
                    self.contract_id,
                    self.contract_digest,
                    self.orchestrator_binding_digest,
                    self.revision,
                    self.candidate_binding_digest,
                    self.gate_state_digest,
                    self.review_state_digest,
                    self.evidence_digest,
                )
            || self.digest != crate::canonical::certificate_digest(self)
        {
            return Err(binding("decoded acceptance certificate is inconsistent"));
        }
        self.kernel_plan.validate()?;
        Ok(())
    }

    #[must_use]
    /// Returns the acceptance contract identity certified by B2.
    pub const fn contract_id(&self) -> AcceptanceSpecId {
        self.contract_id
    }
    #[must_use]
    /// Returns the exact acceptance contract content digest.
    pub const fn contract_digest(&self) -> Sha256Digest {
        self.contract_digest
    }
    #[must_use]
    /// Returns the immutable E0 binding digest covered by this certificate.
    pub const fn orchestrator_binding_digest(&self) -> Sha256Digest {
        self.orchestrator_binding_digest
    }
    #[must_use]
    /// Returns the exact candidate revision evaluated by B2.
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns the canonical candidate binding digest evaluated by B2.
    pub const fn candidate_binding_digest(&self) -> Sha256Digest {
        self.candidate_binding_digest
    }
    #[must_use]
    /// Returns the durable D1 state digest used by the evaluation.
    pub const fn gate_state_digest(&self) -> Sha256Digest {
        self.gate_state_digest
    }
    #[must_use]
    /// Returns the durable D2 state digest used by the evaluation.
    pub const fn review_state_digest(&self) -> Sha256Digest {
        self.review_state_digest
    }
    #[must_use]
    /// Returns the canonical B2 evidence digest.
    pub const fn evidence_digest(&self) -> Sha256Digest {
        self.evidence_digest
    }
    #[must_use]
    /// Returns the canonical request digest acknowledged by the B2 directive.
    pub const fn evaluation_request_digest(&self) -> Sha256Digest {
        self.evaluation_request_digest
    }
    #[must_use]
    /// Returns the reproduced acceptable B2 decision digest.
    pub const fn decision_digest(&self) -> Sha256Digest {
        self.decision_digest
    }
    #[must_use]
    /// Returns the contract-bound maximum number of gate attempts.
    pub const fn maximum_gate_attempts(&self) -> u16 {
        self.maximum_gate_attempts
    }
    #[must_use]
    /// Returns the contract-bound maximum number of review cycles.
    pub const fn maximum_review_cycles(&self) -> u16 {
        self.maximum_review_cycles
    }
    #[must_use]
    /// Returns the exact two-envelope kernel acceptance plan.
    pub const fn kernel_plan(&self) -> KernelAcceptancePlan {
        self.kernel_plan
    }
    /// Returns the canonical payload digest for the planned B0 Begin envelope.
    #[must_use]
    pub fn begin_payload_digest(&self) -> Sha256Digest {
        crate::canonical::kernel_directive_payload_digest(self, true)
    }
    /// Returns the canonical payload digest for the planned B0 Evaluate envelope.
    #[must_use]
    pub fn evaluate_payload_digest(&self) -> Sha256Digest {
        crate::canonical::kernel_directive_payload_digest(self, false)
    }
    #[must_use]
    /// Returns the canonical digest of the complete certificate.
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

const fn binding(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::BindingMismatch,
        OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}
