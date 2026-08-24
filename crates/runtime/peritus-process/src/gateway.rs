//! Sole target-owned authorization and execution effect gateway.

mod native;

use peritus_budget::{BudgetDimension, BudgetOperation, BudgetReceiptKind, ReservationPhase};
use peritus_codec::{CodecLimits, decode_message};
use peritus_kernel::{ActionPhase, KernelEventKind};
use peritus_leases::{LeaseClaim, LeasePhase, LeaseTransitionKind};
use peritus_policy::OperationClass;
use peritus_protocol::{KernelEventDto, KernelSubjectDto};
use peritus_types::{ActionId, RevisionTuple, Sha256Digest};

use crate::{
    ErrorCode, ExecutionAuthorizationRequest, ExecutionIntentPayload, ExecutionPlan, OwnedProcess,
    ProcessError, ProcessOperation, ProcessStore, RecoveryClass, WorkspaceAccess,
    error::mismatch,
    supervisor,
    verified::{ExecutionAuthorityFacts, execution_authority_complete},
};

/// Sole public owner of the protected process registry and execution effect.
pub struct ExecutionGateway {
    store: ProcessStore,
}

impl ExecutionGateway {
    /// Creates a gateway around one protected durable process store.
    #[must_use]
    pub const fn new(store: ProcessStore) -> Self {
        Self { store }
    }

    /// Returns the protected process store for recovery and quiescence inspection.
    #[must_use]
    pub const fn store(&self) -> &ProcessStore {
        &self.store
    }

    /// Validates, durably consumes, and starts exactly one owned execution.
    ///
    /// The sandbox plan/admission were checked while constructing `plan`; the gateway repeats the
    /// exact digests against the committed B3 payload immediately before durable consumption.
    ///
    /// # Errors
    ///
    /// Returns a stable typed error before any process effect for authority or plan mismatch. Once
    /// durable consumption succeeds the same action/process pair cannot be replayed, including
    /// when supervisor-thread or operating-system spawn subsequently fails.
    pub fn launch(
        &self,
        request: &ExecutionAuthorizationRequest<'_>,
        plan: ExecutionPlan,
    ) -> Result<OwnedProcess, ProcessError> {
        if plan.isolation() != crate::ExecutionIsolation::ExplicitRawEffect {
            return Err(ProcessError::new(
                ErrorCode::Unsupported,
                ProcessOperation::Spawn,
                RecoveryClass::SelectBackend,
                "C2 local execution requires an explicitly authorized raw-effect plan; C3 supplies native restricted launchers",
            ));
        }
        let validation = validate_request(request, &plan)?;
        supervisor::validate_launch(&plan)?;
        let permit = ExecutionPermit {
            _action_id: plan.identity().action_id(),
            _process_id: plan.identity().process_id(),
            action_digest: validation.action_digest,
            _plan_digest: plan.digest(),
        };
        self.store.consume(&plan, validation.action_digest, validation.lease_claim)?;
        let launch = AuthorizedLaunch::new(permit, plan);
        supervisor::start(&self.store, launch)
    }
}

struct Validation {
    action_digest: Sha256Digest,
    lease_claim: Option<LeaseClaim>,
}

/// Crate-private move-only proof produced only after complete target validation.
struct ExecutionPermit {
    _action_id: ActionId,
    _process_id: peritus_types::ProcessId,
    action_digest: Sha256Digest,
    _plan_digest: Sha256Digest,
}

/// Opaque single-use launch value accepted by the private local launcher.
pub(crate) struct AuthorizedLaunch {
    permit: ExecutionPermit,
    plan: ExecutionPlan,
}

impl AuthorizedLaunch {
    const fn new(permit: ExecutionPermit, plan: ExecutionPlan) -> Self {
        Self { permit, plan }
    }
    pub(crate) fn into_parts(self) -> (ExecutionPlan, Sha256Digest) {
        (self.plan, self.permit.action_digest)
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the target gate keeps its complete committed authority cross-match auditable"
)]
fn validate_request(
    request: &ExecutionAuthorizationRequest<'_>,
    plan: &ExecutionPlan,
) -> Result<Validation, ProcessError> {
    let intent = request.intent();
    let identity = plan.identity();
    let payload = ExecutionIntentPayload::decode(&intent.payload)?;
    let expected_payload = ExecutionIntentPayload::new(
        identity.process_id(),
        plan.digest(),
        plan.sandbox_digest(),
        plan.backend().descriptor_digest(),
    );
    let expected_class = plan.isolation().operation_class();
    let intent_exact = intent.media_type == crate::EXECUTION_INTENT_MEDIA_TYPE
        && intent.operation_class == expected_class
        && matches!(expected_class, OperationClass::Execution | OperationClass::RawEffect)
        && intent.role.permits_operation(expected_class)
        && payload == expected_payload
        && intent.payload == expected_payload.encode()
        && intent.action_id == identity.action_id()
        && intent.actor_id == identity.actor_id()
        && intent.environment_id == identity.environment_id()
        && intent.resource_id == identity.resource_id();
    let action_digest = intent
        .digest(CodecLimits::PRODUCTION)
        .map_err(|_| mismatch("action intent cannot be encoded canonically"))?;
    let aggregate = request.kernel().aggregate();
    let action = aggregate
        .action(intent.action_id)
        .ok_or_else(|| mismatch("committed B0 aggregate has no exact action"))?;
    let witness = action
        .authorization()
        .ok_or_else(|| mismatch("committed B0 action has no authorization witness"))?;
    let turn = aggregate
        .turn(identity.turn_id())
        .ok_or_else(|| mismatch("committed B0 aggregate has no exact parent turn"))?;
    let attempt = aggregate
        .attempt(identity.attempt_id())
        .ok_or_else(|| mismatch("committed B0 aggregate has no exact parent attempt"))?;
    let run = aggregate
        .run(identity.run_id())
        .ok_or_else(|| mismatch("committed B0 aggregate has no exact parent run"))?;
    let lifecycle_exact = aggregate.project_id() == identity.project_id()
        && aggregate.session().id() == identity.session_id()
        && request.session_id() == identity.session_id()
        && aggregate.revision() == identity.revision()
        && action.id() == identity.action_id()
        && action.turn_id() == identity.turn_id()
        && action.digest() == action_digest
        && action.actor_id() == identity.actor_id()
        && action.role() == intent.role
        && action.environment_id() == identity.environment_id()
        && action.phase() == ActionPhase::Dispatched
        && turn.id() == identity.turn_id()
        && turn.attempt_id() == identity.attempt_id()
        && attempt.id() == identity.attempt_id()
        && attempt.run_id() == identity.run_id()
        && run.id() == identity.run_id()
        && run.revision() == identity.revision();
    let capability = request.capability().transition();
    let scope = capability.scope();
    let capability_exact = capability.action_id() == intent.action_id
        && capability.action_digest() == action_digest
        && capability.permission().resource_id() == identity.resource_id()
        && capability.permission().capability_name() == &intent.capability_name
        && scope.actor_id() == identity.actor_id()
        && scope.role() == intent.role
        && scope.environment_id() == identity.environment_id()
        && scope.revision() == identity.revision()
        && witness.transition_digest() == capability.transition_digest()
        && witness.resource_id() == identity.resource_id()
        && witness.capability_name() == &intent.capability_name;
    let budget_exact = validate_budget(request, action_digest, plan)?;
    let (lease_exact, lease_claim) = validate_lease(request, action_digest, plan, capability)?;
    let dispatch_committed = exact_dispatch(request, intent.action_id, request.revision())?;
    let time_current = request.current_epoch().get() == request.observed_at().epoch().get()
        && request.observed_at() >= capability.used_at()
        && scope.validity().contains(request.observed_at()).unwrap_or(false)
        && capability.successor().time_state().epoch() == request.observed_at().epoch()
        && capability.successor().time_state().greatest_tick_millis()
            <= request.observed_at().tick_millis();
    let revision = identity.revision();
    let revision_exact = request.revision() == revision
        && request.expected_generation() == revision.workspace_generation()
        && request.expected_revision() == revision.workspace_revision()
        && plan.working_directory().generation() == request.expected_generation()
        && plan.working_directory().revision() == request.expected_revision();
    let plan_exact = request.expected_plan_digest() == plan.digest()
        && payload.execution_plan_digest() == plan.digest()
        && payload.sandbox_plan_digest() == plan.sandbox_digest()
        && payload.backend_descriptor_digest() == plan.backend().descriptor_digest();
    let facts = ExecutionAuthorityFacts {
        intent_exact,
        lifecycle_exact,
        capability_exact,
        budget_exact,
        lease_exact,
        dispatch_committed,
        time_current,
        revision_exact,
        plan_exact,
    };
    if !execution_authority_complete(facts) {
        return Err(mismatch("committed execution authority facts are incomplete"));
    }
    Ok(Validation { action_digest, lease_claim })
}

fn validate_budget(
    request: &ExecutionAuthorizationRequest<'_>,
    action_digest: Sha256Digest,
    plan: &ExecutionPlan,
) -> Result<bool, ProcessError> {
    let transition = request.budget().transition();
    let receipt = transition.receipt();
    if receipt.operation() != BudgetOperation::Begin || receipt.kind() != BudgetReceiptKind::Applied
    {
        return Err(ProcessError::new(
            ErrorCode::BudgetMismatch,
            ProcessOperation::Authorize,
            RecoveryClass::Reauthorize,
            "committed budget receipt is not one applied Begin reservation",
        ));
    }
    let reservation_id = receipt.reservation_id().ok_or_else(|| {
        ProcessError::new(
            ErrorCode::BudgetMismatch,
            ProcessOperation::Authorize,
            RecoveryClass::Reauthorize,
            "committed budget Begin has no reservation identity",
        )
    })?;
    let snapshot = transition.reservation_snapshot(reservation_id).map_err(|_| {
        ProcessError::new(
            ErrorCode::BudgetMismatch,
            ProcessOperation::Authorize,
            RecoveryClass::Reauthorize,
            "committed budget transition has no exact reservation snapshot",
        )
    })?;
    let begin = snapshot.request();
    Ok(snapshot.phase() == ReservationPhase::Held
        && begin.reservation_id() == reservation_id
        && begin.action_id() == plan.identity().action_id()
        && begin.action_digest() == action_digest
        && begin.revision() == plan.identity().revision()
        && begin.reserve().get(BudgetDimension::ActiveEffectMilliseconds).get()
            >= plan.resource_policy().wall_millis())
}

fn validate_lease(
    request: &ExecutionAuthorizationRequest<'_>,
    action_digest: Sha256Digest,
    plan: &ExecutionPlan,
    capability: &peritus_policy::CapabilityUseTransition,
) -> Result<(bool, Option<LeaseClaim>), ProcessError> {
    match (plan.working_directory().access(), request.lease()) {
        (WorkspaceAccess::ReadOnly, None) => Ok((true, None)),
        (WorkspaceAccess::ReadOnly, Some(_)) => Err(ProcessError::new(
            ErrorCode::LeaseMismatch,
            ProcessOperation::Authorize,
            RecoveryClass::Reauthorize,
            "read-only execution supplied surplus mutation-lease authority",
        )),
        (WorkspaceAccess::Writable, None) => Err(ProcessError::new(
            ErrorCode::LeaseMismatch,
            ProcessOperation::Authorize,
            RecoveryClass::Reauthorize,
            "writable execution requires a committed lease use",
        )),
        (WorkspaceAccess::Writable, Some(committed)) => {
            let transition = committed.transition();
            let record = transition.record();
            let binding = record
                .binding()
                .as_use()
                .ok_or_else(|| mismatch("committed lease transition is not a capability use"))?;
            let claim = binding.claim();
            let scope = claim.scope();
            let next = transition.next();
            let exact = matches!(record.kind(), LeaseTransitionKind::Used { action_id, action_digest: digest }
                    if action_id == plan.identity().action_id() && digest == action_digest)
                && binding.action_id() == plan.identity().action_id()
                && binding.action_digest() == action_digest
                && binding.actor_id() == plan.identity().actor_id()
                && binding.role() == request.intent().role
                && binding.environment_id() == plan.identity().environment_id()
                && binding.revision() == plan.identity().revision()
                && binding.permission().resource_id() == plan.identity().resource_id()
                && binding.permission().capability_name() == &request.intent().capability_name
                && lease_binding_matches_capability(binding, capability)
                && scope.workspace_id() == plan.identity().workspace_id()
                && scope.resource_id() == plan.identity().resource_id()
                && scope.environment_id() == plan.identity().environment_id()
                && claim.holder().actor_id() == plan.identity().actor_id()
                && claim.holder().session_id() == request.session_id()
                && claim.generation() == request.expected_generation()
                && request.observed_at() >= binding.observed_at()
                && request.observed_at() < claim.expires_at()
                && record.after_generation() == claim.generation()
                && record.after_version() == next.version()
                && record.after_phase() == LeasePhase::Active
                && next.scope() == scope
                && next.generation() == claim.generation()
                && next.phase() == LeasePhase::Active
                && next.active().is_some_and(|active| active.claim() == claim);
            Ok((exact, Some(claim)))
        }
    }
}

fn exact_dispatch(
    request: &ExecutionAuthorizationRequest<'_>,
    action_id: ActionId,
    revision: RevisionTuple,
) -> Result<bool, ProcessError> {
    let records = request.kernel().batch().records();
    if records.len() != 1 {
        return Err(ProcessError::new(
            ErrorCode::MissingDispatch,
            ProcessOperation::Authorize,
            RecoveryClass::Reauthorize,
            "kernel receipt is not one exact dispatch event",
        ));
    }
    let event = decode_message::<KernelEventDto>(records[0].frame_bytes(), CodecLimits::PRODUCTION)
        .map_err(|_| {
            ProcessError::new(
                ErrorCode::MissingDispatch,
                ProcessOperation::Authorize,
                RecoveryClass::Quarantine,
                "committed kernel frame cannot be decoded exactly",
            )
        })?;
    Ok(event.kind == KernelEventKind::ActionDispatched
        && event.subject == KernelSubjectDto::Action(action_id)
        && event.revision == revision
        && event.command_id == records[0].command_id()
        && event.id == records[0].event_id())
}

#[allow(
    clippy::suspicious_operation_groupings,
    reason = "lease projection and capability scope intentionally use distinct accessor names"
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
        && binding.successor_issued_at() == capability.successor().issued_at()
        && binding.successor_issuance_digest() == capability.successor().issuance_digest()
        && binding.successor_issuance_command_id() == capability.successor().issuance_command_id()
        && binding.scope_permissions().len() == permissions.len()
        && binding.scope_permissions().iter().zip(permissions).all(|(left, right)| {
            left.resource_id() == right.resource_id()
                && left.capability_name() == right.capability_name()
        })
}
