//! Exact immutable views used on both sides of the independent oracle comparison.

use peritus_approval::{
    ActionDigest, AmendmentIdentity, ApprovalAggregate, ApprovalChoice, ApprovalDecisionDigest,
    ApprovalError, ApprovalPhase, ApprovalRequest, ApprovalRequestDigest, ApprovalTransition,
    ApprovalTransitionKind, ApprovedActionTransition, ApprovedPolicyAmendment,
    AuthenticatedApprovalObservation, ConsumedApproval,
};
use peritus_policy::{
    ActorRole, AuthorityInstant, AuthorityTier, IndependenceRequirement, RiskClass, UseLimit,
    ValidityWindow,
};
use peritus_types::{
    ActionId, ActorId, ApprovalRequestId, CommandId, EnvironmentId, Generation, ResourceId,
    RevisionNumber, RevisionTuple, Sha256Digest,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PermissionView {
    pub(super) resource: ResourceId,
    pub(super) capability: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestView {
    pub(super) request_id: ApprovalRequestId,
    pub(super) action_id: ActionId,
    pub(super) action_digest: ActionDigest,
    pub(super) requester: ActorId,
    pub(super) requester_role: ActorRole,
    pub(super) scope_actor: ActorId,
    pub(super) scope_role: ActorRole,
    pub(super) environment: EnvironmentId,
    pub(super) permissions: Vec<PermissionView>,
    pub(super) revision: RevisionTuple,
    pub(super) scope_validity: ValidityWindow,
    pub(super) use_limit: UseLimit,
    pub(super) minimum_tier: AuthorityTier,
    pub(super) approver_roles: Vec<ActorRole>,
    pub(super) independence: Vec<IndependenceRequirement>,
    pub(super) requirement_validity: ValidityWindow,
    pub(super) evaluated_at: AuthorityInstant,
    pub(super) challenge_epoch: Generation,
    pub(super) challenge_tick: u64,
    pub(super) authority_epoch: Generation,
    pub(super) authority_tick: u64,
    pub(super) risks: Vec<RiskClass>,
    pub(super) risk_details_digest: Sha256Digest,
    pub(super) producing: Vec<ActorId>,
    pub(super) review: Vec<ActorId>,
    pub(super) validity: ValidityWindow,
    pub(super) digest: ApprovalRequestDigest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolutionView {
    pub(super) decision_digest: ApprovalDecisionDigest,
    pub(super) command_id: CommandId,
    pub(super) choice: ApprovalChoice,
    pub(super) registry_revision: RevisionNumber,
    pub(super) credential_generation: Generation,
    pub(super) valid_until: AuthorityInstant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregateView {
    pub(super) request: RequestView,
    pub phase: ApprovalPhase,
    pub(super) resolution: Option<ResolutionView>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservationView {
    pub(super) request_id: ApprovalRequestId,
    pub(super) request_digest: ApprovalRequestDigest,
    pub(super) decision_digest: ApprovalDecisionDigest,
    pub(super) command_id: CommandId,
    pub(super) responder: ActorId,
    pub(super) role: ActorRole,
    pub(super) choice: ApprovalChoice,
    pub(super) key_id: peritus_approval::ApprovalKeyId,
    pub(super) credential_generation: Generation,
    pub(super) registry_revision: RevisionNumber,
    pub(super) credential_validity: ValidityWindow,
    pub(super) decision_expires_at: AuthorityInstant,
    pub(super) observed_at: AuthorityInstant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionView {
    pub(super) kind: ApprovalTransitionKind,
    pub(super) from: ApprovalPhase,
    pub(super) to: ApprovalPhase,
    pub(super) decision_digest: Option<ApprovalDecisionDigest>,
    pub(super) registry_revision: Option<RevisionNumber>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UseView {
    pub(super) request_id: ApprovalRequestId,
    pub(super) request_digest: ApprovalRequestDigest,
    pub(super) action_id: ActionId,
    pub(super) action_digest: ActionDigest,
    pub(super) revision: RevisionTuple,
    pub(super) decision_digest: ApprovalDecisionDigest,
    pub(super) command_id: CommandId,
    pub(super) registry_revision: RevisionNumber,
    pub(super) valid_until: AuthorityInstant,
    pub(super) consumed_request_id: ApprovalRequestId,
    pub(super) consumed_decision_digest: ApprovalDecisionDigest,
    pub(super) consumed_action_id: ActionId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AmendmentView {
    pub(super) identity: AmendmentIdentity,
    pub(super) registry_revision: RevisionNumber,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcceptedView {
    Transition(TransitionView),
    Use(Box<UseView>),
    Amendment(AmendmentView),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectedView {
    pub error: ApprovalError,
    pub observation: Option<ObservationView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepView {
    pub after: AggregateView,
    pub result: Result<AcceptedView, RejectedView>,
}

pub fn request_view(request: &ApprovalRequest) -> RequestView {
    let scope = request.scope();
    let requirement = request.requirement();
    RequestView {
        request_id: request.request_id(),
        action_id: request.action_id(),
        action_digest: request.action_digest(),
        requester: request.requester(),
        requester_role: request.requester_role(),
        scope_actor: scope.actor_id(),
        scope_role: scope.role(),
        environment: scope.environment_id(),
        permissions: scope
            .permissions()
            .as_slice()
            .iter()
            .map(|permission| PermissionView {
                resource: permission.resource_id(),
                capability: permission.capability_name().as_str().to_owned(),
            })
            .collect(),
        revision: scope.revision(),
        scope_validity: scope.validity(),
        use_limit: scope.use_limit(),
        minimum_tier: requirement.minimum_tier(),
        approver_roles: requirement.approver_roles().to_vec(),
        independence: requirement.independence().as_slice().to_vec(),
        requirement_validity: requirement.validity(),
        evaluated_at: request.evaluated_at(),
        challenge_epoch: request.challenge_epoch(),
        challenge_tick: request.challenge_tick_millis(),
        authority_epoch: request.authority_time().epoch(),
        authority_tick: request.authority_time().greatest_tick_millis(),
        risks: request.risks().as_slice().to_vec(),
        risk_details_digest: request.risk_details_digest(),
        producing: request.producing_participants().as_slice().to_vec(),
        review: request.review_participants().as_slice().to_vec(),
        validity: request.validity(),
        digest: request.digest(),
    }
}

pub fn aggregate_view(aggregate: &ApprovalAggregate) -> AggregateView {
    let resolution = aggregate.resolution().map(|facts| ResolutionView {
        decision_digest: facts.decision_digest(),
        command_id: facts.command_id(),
        choice: facts.choice(),
        registry_revision: facts.registry_revision(),
        credential_generation: facts.credential_generation(),
        valid_until: facts.valid_until(),
    });
    AggregateView {
        request: request_view(aggregate.request()),
        phase: aggregate.phase(),
        resolution,
    }
}

pub fn initial_view(request: &ApprovalRequest) -> AggregateView {
    AggregateView {
        request: request_view(request),
        phase: ApprovalPhase::Pending,
        resolution: None,
    }
}

pub const fn observation_view(observation: &AuthenticatedApprovalObservation) -> ObservationView {
    ObservationView {
        request_id: observation.request_id(),
        request_digest: observation.request_digest(),
        decision_digest: observation.decision_digest(),
        command_id: observation.command_id(),
        responder: observation.responder(),
        role: observation.approver_role(),
        choice: observation.choice(),
        key_id: observation.key_id(),
        credential_generation: observation.credential_generation(),
        registry_revision: observation.registry_revision(),
        credential_validity: observation.credential_validity(),
        decision_expires_at: observation.decision_expires_at(),
        observed_at: observation.observed_at(),
    }
}

pub const fn transition_view(transition: &ApprovalTransition) -> TransitionView {
    TransitionView {
        kind: transition.kind(),
        from: transition.from(),
        to: transition.to(),
        decision_digest: transition.decision_digest(),
        registry_revision: transition.registry_revision(),
    }
}

pub const fn use_view(
    transition: &ApprovedActionTransition,
    consumed: &ConsumedApproval,
) -> UseView {
    UseView {
        request_id: transition.request_id(),
        request_digest: transition.request_digest(),
        action_id: transition.action_id(),
        action_digest: transition.action_digest(),
        revision: transition.revision(),
        decision_digest: transition.decision_digest(),
        command_id: transition.command_id(),
        registry_revision: transition.registry_revision(),
        valid_until: transition.valid_until(),
        consumed_request_id: consumed.request_id(),
        consumed_decision_digest: consumed.decision_digest(),
        consumed_action_id: consumed.action_id(),
    }
}

pub const fn amendment_view(approval: &ApprovedPolicyAmendment) -> AmendmentView {
    AmendmentView { identity: approval.identity(), registry_revision: approval.registry_revision() }
}
