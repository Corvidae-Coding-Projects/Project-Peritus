//! Complete borrowed request and exact committed authority validation.

use peritus_budget::{BudgetDimension, BudgetOperation, BudgetReceiptKind, ReservationPhase};
use peritus_codec::{CodecLimits, decode_message};
use peritus_journal::{
    CommittedBudgetTransition, CommittedCapabilityUse, CommittedKernelTransition,
    CommittedLeaseTransition, CurrentAuthorityEpoch,
};
use peritus_kernel::{ActionPhase, KernelEventKind};
use peritus_leases::{LeasePhase, LeaseTransitionKind};
use peritus_policy::AuthorityInstant;
use peritus_protocol::{ActionIntentDto, KernelEventDto, KernelSubjectDto};
use peritus_tool_protocol::{LeaseRequirement, PreparedToolCall};
use peritus_types::{
    ActionId, EventId, Generation, RevisionNumber, RevisionTuple, SessionId, Sha256Digest,
};

use crate::{
    AuthorizedToolBinding, RouterError, RouterErrorKind, TOOL_INTENT_MEDIA_TYPE, ToolIntentPayload,
    verified::{ToolAuthorityFacts, tool_authority_complete},
};

/// Exact C0 observations and current facts required for one C4 dispatch decision.
pub struct ToolAuthorizationRequest<'a> {
    intent: &'a ActionIntentDto,
    kernel: &'a CommittedKernelTransition,
    capability: &'a CommittedCapabilityUse,
    budget: &'a CommittedBudgetTransition,
    lease: Option<&'a CommittedLeaseTransition>,
    current_epoch: &'a CurrentAuthorityEpoch,
    revision: RevisionTuple,
    session_id: SessionId,
    observed_at: AuthorityInstant,
    expected_generation: Generation,
    expected_revision: RevisionNumber,
    expected_prepared_digest: Sha256Digest,
}

impl<'a> ToolAuthorizationRequest<'a> {
    /// Creates a complete unprivileged request; the router independently checks every field.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        intent: &'a ActionIntentDto,
        kernel: &'a CommittedKernelTransition,
        capability: &'a CommittedCapabilityUse,
        budget: &'a CommittedBudgetTransition,
        lease: Option<&'a CommittedLeaseTransition>,
        current_epoch: &'a CurrentAuthorityEpoch,
        revision: RevisionTuple,
        session_id: SessionId,
        observed_at: AuthorityInstant,
        expected_generation: Generation,
        expected_revision: RevisionNumber,
        expected_prepared_digest: Sha256Digest,
    ) -> Self {
        Self {
            intent,
            kernel,
            capability,
            budget,
            lease,
            current_epoch,
            revision,
            session_id,
            observed_at,
            expected_generation,
            expected_revision,
            expected_prepared_digest,
        }
    }

    pub(crate) const fn observed_at(&self) -> AuthorityInstant {
        self.observed_at
    }
}

pub struct AuthorizationEvidence {
    pub intent_digest: Sha256Digest,
    pub dispatch_event: EventId,
    pub binding: AuthorizedToolBinding,
}

#[allow(
    clippy::too_many_lines,
    reason = "the target gate keeps the complete committed authority cross-match contiguous"
)]
pub fn validate(
    prepared: &PreparedToolCall,
    request: &ToolAuthorizationRequest<'_>,
) -> Result<AuthorizationEvidence, RouterError> {
    let intent = request.intent;
    let descriptor = prepared.descriptor();
    let expected_payload = ToolIntentPayload::new(prepared);
    let intent_exact = intent.media_type == TOOL_INTENT_MEDIA_TYPE
        && intent.payload == expected_payload.as_bytes()
        && intent.action_id == prepared.call().action_id()
        && intent.capability_name == *descriptor.name()
        && intent.operation_class == descriptor.operation().operation_class()
        && intent.role.permits_operation(intent.operation_class);
    let intent_digest = intent
        .digest(CodecLimits::PRODUCTION)
        .map_err(|_| mismatch("action intent cannot be encoded canonically"))?;
    let aggregate = request.kernel.aggregate();
    let action = aggregate
        .action(intent.action_id)
        .ok_or_else(|| mismatch("committed B0 aggregate has no exact action"))?;
    let witness = action
        .authorization()
        .ok_or_else(|| mismatch("committed B0 action has no authorization witness"))?;
    let lifecycle_exact = aggregate.session().id() == request.session_id
        && aggregate.revision() == request.revision
        && action.id() == intent.action_id
        && action.digest() == intent_digest
        && action.actor_id() == intent.actor_id
        && action.role() == intent.role
        && action.environment_id() == intent.environment_id
        && action.phase() == ActionPhase::Dispatched;
    let capability = request.capability.transition();
    let scope = capability.scope();
    let capability_exact = capability.action_id() == intent.action_id
        && capability.action_digest() == intent_digest
        && capability.permission().resource_id() == intent.resource_id
        && capability.permission().capability_name() == &intent.capability_name
        && scope.actor_id() == intent.actor_id
        && scope.role() == intent.role
        && scope.environment_id() == intent.environment_id
        && scope.revision() == request.revision
        && witness.transition_digest() == capability.transition_digest()
        && witness.resource_id() == intent.resource_id
        && witness.capability_name() == &intent.capability_name;
    let budget_exact = validate_budget(request, prepared, intent_digest)?;
    let lease_exact = validate_lease(request, prepared, intent_digest, capability)?;
    let dispatch_event = exact_dispatch(request, intent.action_id, request.revision)?;
    let time_current = request.current_epoch.get() == request.observed_at.epoch().get()
        && request.observed_at >= capability.used_at()
        && scope.validity().contains(request.observed_at).unwrap_or(false)
        && capability.successor().time_state().epoch() == request.observed_at.epoch()
        && capability.successor().time_state().greatest_tick_millis()
            <= request.observed_at.tick_millis()
        && request.observed_at.epoch() == prepared.call().deadline().epoch()
        && request.observed_at.tick_millis() < prepared.call().deadline().tick_millis();
    let revision_exact = request.revision == prepared.call().revision()
        && request.expected_generation == request.revision.workspace_generation()
        && request.expected_revision == request.revision.workspace_revision();
    let descriptor_exact = descriptor.operation().name() == descriptor.name()
        && descriptor.schema_digest() == descriptor.schema().digest()
        && descriptor.descriptor_digest().get()
            == peritus_codec::sha256(&descriptor.canonical_bytes());
    let prepared_exact = request.expected_prepared_digest == prepared.prepared_digest()
        && prepared.arguments_digest() == prepared.arguments().digest();
    let facts = ToolAuthorityFacts {
        intent_exact,
        lifecycle_exact,
        capability_exact,
        budget_exact,
        lease_exact,
        dispatch_committed: true,
        time_current,
        revision_exact,
        descriptor_exact,
        prepared_exact,
    };
    if !tool_authority_complete(facts) {
        return Err(mismatch("committed tool authority facts are incomplete"));
    }
    Ok(AuthorizationEvidence {
        intent_digest,
        dispatch_event,
        binding: AuthorizedToolBinding::new(
            intent.actor_id,
            intent.role,
            intent.environment_id,
            intent.resource_id,
            request.revision,
            request.session_id,
        ),
    })
}

fn validate_budget(
    request: &ToolAuthorizationRequest<'_>,
    prepared: &PreparedToolCall,
    intent_digest: Sha256Digest,
) -> Result<bool, RouterError> {
    let transition = request.budget.transition();
    let receipt = transition.receipt();
    if receipt.operation() != BudgetOperation::Begin || receipt.kind() != BudgetReceiptKind::Applied
    {
        return Err(mismatch("budget receipt is not one applied Begin reservation"));
    }
    let reservation_id = receipt
        .reservation_id()
        .ok_or_else(|| mismatch("budget Begin has no reservation identity"))?;
    let snapshot = transition
        .reservation_snapshot(reservation_id)
        .map_err(|_| mismatch("budget transition has no exact reservation snapshot"))?;
    let begin = snapshot.request();
    Ok(snapshot.phase() == ReservationPhase::Held
        && begin.reservation_id() == reservation_id
        && begin.action_id() == prepared.call().action_id()
        && begin.action_digest() == intent_digest
        && begin.revision() == prepared.call().revision()
        && begin.reserve().get(BudgetDimension::ActiveEffectMilliseconds).get()
            >= prepared.call().limits().timeout_millis())
}

fn validate_lease(
    request: &ToolAuthorizationRequest<'_>,
    prepared: &PreparedToolCall,
    intent_digest: Sha256Digest,
    capability: &peritus_policy::CapabilityUseTransition,
) -> Result<bool, RouterError> {
    match (prepared.descriptor().lease_requirement(), request.lease) {
        (LeaseRequirement::None, None) => Ok(true),
        (LeaseRequirement::None, Some(_)) => {
            Err(mismatch("surplus mutation-lease authority was supplied"))
        }
        (LeaseRequirement::Required, None) => {
            Err(mismatch("descriptor requires a committed mutation lease use"))
        }
        (LeaseRequirement::Required, Some(committed)) => {
            let transition = committed.transition();
            let record = transition.record();
            let binding = record
                .binding()
                .as_use()
                .ok_or_else(|| mismatch("committed lease transition is not a capability use"))?;
            let claim = binding.claim();
            let scope = claim.scope();
            let next = transition.next();
            Ok(matches!(record.kind(), LeaseTransitionKind::Used { action_id, action_digest }
                    if action_id == prepared.call().action_id() && action_digest == intent_digest)
                && binding.action_id() == prepared.call().action_id()
                && binding.action_digest() == intent_digest
                && binding.actor_id() == request.intent.actor_id
                && binding.role() == request.intent.role
                && binding.environment_id() == request.intent.environment_id
                && binding.revision() == request.revision
                && binding.permission().resource_id() == request.intent.resource_id
                && binding.permission().capability_name() == &request.intent.capability_name
                && lease_binding_matches_capability(binding, capability)
                && scope.workspace_id() == request.revision.workspace_id()
                && scope.resource_id() == request.intent.resource_id
                && scope.environment_id() == request.intent.environment_id
                && claim.holder().actor_id() == request.intent.actor_id
                && claim.holder().session_id() == request.session_id
                && claim.generation() == request.expected_generation
                && request.observed_at >= binding.observed_at()
                && request.observed_at < claim.expires_at()
                && record.after_generation() == claim.generation()
                && record.after_version() == next.version()
                && record.after_phase() == LeasePhase::Active
                && next.scope() == scope
                && next.generation() == claim.generation()
                && next.phase() == LeasePhase::Active
                && next.active().is_some_and(|active| active.claim() == claim))
        }
    }
}

fn exact_dispatch(
    request: &ToolAuthorizationRequest<'_>,
    action_id: ActionId,
    revision: RevisionTuple,
) -> Result<EventId, RouterError> {
    let records = request.kernel.batch().records();
    if records.len() != 1 {
        return Err(mismatch("kernel receipt is not one exact dispatch event"));
    }
    let record = &records[0];
    let event = decode_message::<KernelEventDto>(record.frame_bytes(), CodecLimits::PRODUCTION)
        .map_err(|_| mismatch("committed kernel dispatch frame cannot be decoded exactly"))?;
    if event.kind != KernelEventKind::ActionDispatched
        || event.subject != KernelSubjectDto::Action(action_id)
        || event.revision != revision
        || event.command_id != record.command_id()
        || event.id != record.event_id()
    {
        return Err(mismatch("committed kernel event is not the exact dispatch"));
    }
    Ok(record.event_id())
}

#[allow(
    clippy::suspicious_operation_groupings,
    reason = "lease projection and capability scope intentionally use distinct accessors"
)]
fn lease_binding_matches_capability(
    binding: &peritus_leases::LeaseUseCommandBinding,
    capability: &peritus_policy::CapabilityUseTransition,
) -> bool {
    let scope = capability.scope();
    let permissions = scope.permissions().as_slice();
    binding.action_id() == capability.action_id()
        && binding.action_digest() == capability.action_digest()
        && binding.permission().resource_id() == capability.permission().resource_id()
        && binding.permission().capability_name() == capability.permission().capability_name()
        && binding.actor_id() == scope.actor_id()
        && binding.role() == scope.role()
        && binding.environment_id() == scope.environment_id()
        && binding.revision() == scope.revision()
        && binding.validity() == scope.validity()
        && binding.scope_use_limit() == scope.use_limit()
        && binding.used_at() == capability.used_at()
        && binding.transition_digest() == capability.transition_digest()
        && binding.previous_remaining() == capability.previous_remaining()
        && binding.successor_remaining() == capability.successor().remaining_uses()
        && binding.successor_time_epoch() == capability.successor().time_state().epoch()
        && binding.successor_greatest_tick_millis()
            == capability.successor().time_state().greatest_tick_millis()
        && binding.scope_permissions().len() == permissions.len()
        && binding.scope_permissions().iter().zip(permissions).all(|(left, right)| {
            left.resource_id() == right.resource_id()
                && left.capability_name() == right.capability_name()
        })
}

const fn mismatch(detail: &'static str) -> RouterError {
    RouterError::new(RouterErrorKind::Authorization, "authorize tool invocation", detail)
}
