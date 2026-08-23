#![allow(dead_code, reason = "shared fixture vocabulary spans several integration targets")]

mod budget;
mod capability;
pub mod lifecycle;

use peritus_kernel::{
    CommandEnvelope, KernelAggregate, KernelCommand, KernelOutcome, KernelTransition, ReducerInputs,
};
use peritus_policy::{AuthorityInstant, Permission, PermissionSet};
use peritus_quality_policy::{
    AcceptanceEvidence, ApprovalObservation, ApprovalOutcome, ApprovalSubject, EvidenceObservation,
    FindingDisposition, FindingObservation, GateAttemptOrdinal, GateObservation, GateOutcome,
    ReviewCycleOrdinal, ReviewObservation, ReviewerIdentity, WaiverObservation,
};
use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContentReference, ContractDocuments,
    EvidenceRequirement, EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification,
    FindingSeverity, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    GateSuccessRule, HumanApprovalPolicy, Requirement, RequirementId, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, ApprovalRequestId, AttemptId, BudgetId, CapabilityName,
    CommandId, EnvironmentId, EventId, FindingId, GateExecutionId, GateId, Generation, HarnessId,
    PolicyId, ProjectId, ProviderProfileId, ResourceId, ReviewCycleId, RevisionNumber,
    RevisionTuple, RunId, SessionId, Sha256Digest, TurnId, WorkspaceId,
};

pub struct Fixture {
    pub revision: RevisionTuple,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub run_id: RunId,
    pub attempt_id: AttemptId,
    pub turn_id: TurnId,
    pub action_id: ActionId,
    pub actor_id: ActorId,
    pub environment_id: EnvironmentId,
    pub resource_id: ResourceId,
    pub root_budget_id: BudgetId,
    pub child_budget_id: BudgetId,
    pub gate_id: GateId,
    pub review_id: ReviewCycleId,
    pub category: ReviewCategory,
    pub evidence_id: EvidenceRequirementId,
}

impl Fixture {
    pub fn new() -> Self {
        let acceptance = AcceptanceSpecId::new(bytes(1)).expect("acceptance id");
        Self {
            revision: RevisionTuple::new(
                acceptance,
                HarnessId::new(bytes(2)).expect("harness id"),
                WorkspaceId::new(bytes(3)).expect("workspace id"),
                Generation::first(),
                RevisionNumber::first(),
                PolicyId::new(bytes(4)).expect("policy id"),
                ProviderProfileId::new(bytes(5)).expect("provider id"),
            ),
            project_id: ProjectId::new(bytes(6)).expect("project id"),
            session_id: SessionId::new(bytes(7)).expect("session id"),
            run_id: RunId::new(bytes(8)).expect("run id"),
            attempt_id: AttemptId::new(bytes(9)).expect("attempt id"),
            turn_id: TurnId::new(bytes(10)).expect("turn id"),
            action_id: ActionId::new(bytes(11)).expect("action id"),
            actor_id: ActorId::new(bytes(12)).expect("actor id"),
            environment_id: EnvironmentId::new(bytes(13)).expect("environment id"),
            resource_id: ResourceId::new(bytes(14)).expect("resource id"),
            root_budget_id: BudgetId::new(bytes(15)).expect("root budget id"),
            child_budget_id: BudgetId::new(bytes(16)).expect("child budget id"),
            gate_id: GateId::new(bytes(17)).expect("gate id"),
            review_id: ReviewCycleId::new(bytes(18)).expect("review id"),
            category: ReviewCategory::new(digest(19)),
            evidence_id: EvidenceRequirementId::new(digest(20)),
        }
    }

    pub fn contract(&self) -> AcceptanceContract {
        self.contract_with_waiver(WaiverPolicy::Forbidden, digest(25))
    }

    pub fn waiver_contract(&self) -> AcceptanceContract {
        self.contract_with_waiver(
            WaiverPolicy::Allowed { authority: content(53), evidence: waiver_evidence_id() },
            digest(55),
        )
    }

    fn contract_with_waiver(
        &self,
        waiver_policy: WaiverPolicy,
        contract_digest: Sha256Digest,
    ) -> AcceptanceContract {
        let gate = GateDefinition::new(
            self.gate_id,
            GateExecutionPlan::new(
                content(21),
                self.environment_id,
                content(22),
                content(23),
                GateSuccessRule::ExitCodeZero,
                60_000,
                content(24),
                GateFreshnessScope::ExactRevisionTuple,
            )
            .expect("gate plan"),
            Vec::new(),
            vec![self.evidence_id],
        )
        .expect("gate");
        let mut evidence_requirements = vec![EvidenceRequirement::new(
            self.evidence_id,
            content(38),
            EvidenceSource::Gate(self.gate_id),
            ExportClassification::Internal,
        )];
        if let WaiverPolicy::Allowed { evidence, .. } = waiver_policy {
            evidence_requirements.push(EvidenceRequirement::new(
                evidence,
                content(54),
                EvidenceSource::WaiverAuthorization,
                ExportClassification::Restricted,
            ));
        }
        AcceptanceContract::new(
            self.revision.acceptance_spec_id(),
            contract_digest,
            ContractDocuments::new(
                content(26),
                content(27),
                content(28),
                content(29),
                content(30),
                content(31),
                content(32),
                content(33),
            ),
            vec![Requirement::new(RequirementId::new(digest(34)), content(35))],
            vec![Exclusion::new(content(36))],
            vec![Assumption::new(content(37))],
            GateGraph::new(vec![gate]).expect("gate graph"),
            ReviewPolicy::new(
                vec![self.category],
                1,
                ReviewerIndependence::new(true, true, false, false, false, false),
                FindingSeverity::High,
            )
            .expect("review policy"),
            evidence_requirements,
            CompletionPolicy::new(3, 4).expect("completion policy"),
            HumanApprovalPolicy::NotRequired,
            waiver_policy,
        )
        .expect("contract")
    }

    pub fn evidence(
        &self,
        contract: &AcceptanceContract,
        revision: RevisionTuple,
        review_id: ReviewCycleId,
    ) -> AcceptanceEvidence {
        self.build_evidence(contract, revision, review_id, true)
    }

    pub fn incomplete_evidence(
        &self,
        contract: &AcceptanceContract,
        revision: RevisionTuple,
        review_id: ReviewCycleId,
    ) -> AcceptanceEvidence {
        self.build_evidence(contract, revision, review_id, false)
    }

    pub fn waiver_evidence(
        &self,
        contract: &AcceptanceContract,
        finding_id: FindingId,
    ) -> AcceptanceEvidence {
        let request_id = ApprovalRequestId::new(bytes(92)).expect("waiver approval request");
        let review = ReviewObservation::new(
            self.review_id,
            ReviewCycleOrdinal::new(1).expect("review ordinal"),
            self.revision,
            ReviewerIdentity::new(
                ActorId::new(bytes(40)).expect("review actor"),
                digest(41),
                digest(42),
                digest(43),
                digest(44),
                digest(45),
                true,
            ),
            vec![self.category],
            vec![FindingObservation::new(
                finding_id,
                FindingSeverity::Critical,
                FindingDisposition::WaiverRequested,
                digest(91),
            )],
            digest(46),
        )
        .expect("waiver review observation");
        let approval = ApprovalObservation::new(
            request_id,
            self.revision,
            ApprovalSubject::FindingWaiver(finding_id),
            ActorId::new(bytes(93)).expect("waiver approver"),
            content(53),
            ApprovalOutcome::Approved,
            digest(94),
        );
        let waiver = WaiverObservation::new(
            finding_id,
            self.revision,
            request_id,
            content(53),
            waiver_evidence_id(),
            digest(95),
        );
        let required = contract
            .evidence_requirements()
            .iter()
            .map(|requirement| {
                EvidenceObservation::new(requirement.id(), self.revision, requirement.id().digest())
            })
            .collect();
        AcceptanceEvidence::new(
            vec![GateObservation::new(
                GateExecutionId::new(bytes(47)).expect("gate execution"),
                self.gate_id,
                GateAttemptOrdinal::new(1).expect("gate ordinal"),
                self.revision,
                GateOutcome::Passed,
                digest(48),
            )],
            vec![review],
            required,
            vec![approval],
            vec![waiver],
        )
        .expect("waiver acceptance evidence")
    }

    fn build_evidence(
        &self,
        contract: &AcceptanceContract,
        revision: RevisionTuple,
        review_id: ReviewCycleId,
        include_gate: bool,
    ) -> AcceptanceEvidence {
        let review = ReviewObservation::new(
            review_id,
            ReviewCycleOrdinal::new(1).expect("review ordinal"),
            revision,
            ReviewerIdentity::new(
                ActorId::new(bytes(40)).expect("review actor"),
                digest(41),
                digest(42),
                digest(43),
                digest(44),
                digest(45),
                true,
            ),
            vec![self.category],
            Vec::new(),
            digest(46),
        )
        .expect("review observation");
        let required = contract
            .evidence_requirements()
            .iter()
            .map(|requirement| {
                EvidenceObservation::new(requirement.id(), revision, requirement.id().digest())
            })
            .collect();
        let gate = GateObservation::new(
            GateExecutionId::new(bytes(47)).expect("gate execution"),
            self.gate_id,
            GateAttemptOrdinal::new(1).expect("gate ordinal"),
            revision,
            GateOutcome::Passed,
            digest(48),
        );
        let gates = if include_gate { vec![gate] } else { Vec::new() };
        AcceptanceEvidence::new(gates, vec![review], required, Vec::new(), Vec::new())
            .expect("acceptance evidence")
    }

    pub fn genesis(&self, contract: &AcceptanceContract) -> KernelAggregate {
        KernelAggregate::open(
            self.project_id,
            self.session_id,
            contract,
            self.revision,
            CommandEnvelope::new(
                CommandId::new(bytes(60)).expect("genesis command"),
                EventId::new(bytes(61)).expect("genesis event"),
                None,
                self.revision,
            ),
        )
        .expect("kernel genesis")
        .into_parts()
        .0
    }
}

pub fn envelope(state: &KernelAggregate, value: u8) -> CommandEnvelope {
    CommandEnvelope::new(
        CommandId::new(bytes(value)).expect("command id"),
        EventId::new(bytes(value.wrapping_add(80))).expect("event id"),
        Some(state.head_event_id()),
        state.revision(),
    )
}

pub fn execute(
    state: KernelAggregate,
    value: u8,
    command: KernelCommand,
    inputs: ReducerInputs<'_>,
) -> KernelOutcome {
    let command_envelope = envelope(&state, value);
    state.reduce(command_envelope, command, inputs)
}

pub fn applied(outcome: KernelOutcome) -> KernelTransition {
    outcome.into_result().expect("applied command")
}

pub const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
pub const fn waiver_evidence_id() -> EvidenceRequirementId {
    EvidenceRequirementId::new(digest(54))
}
pub const fn content(value: u8) -> ContentReference {
    ContentReference::new(digest(value))
}
pub fn instant(epoch: u64, tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::new(epoch).expect("epoch"), tick)
}
pub fn permission(resource: ResourceId, name: &str) -> Permission {
    Permission::new(resource, CapabilityName::new(name.to_owned()).expect("capability name"))
}
pub fn permission_set(resource: ResourceId, name: &str) -> PermissionSet {
    PermissionSet::new(vec![permission(resource, name)]).expect("permission set")
}
