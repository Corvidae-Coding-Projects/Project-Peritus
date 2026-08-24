//! Target-owned authorization gateway and private permit construction.

use peritus_codec::{CodecLimits, decode_message};
use peritus_kernel::{ActionPhase, KernelEventKind};
use peritus_leases::{LeasePhase, LeaseTransitionKind};
use peritus_policy::OperationClass;
use peritus_protocol::{KernelEventDto, KernelSubjectDto};
use peritus_types::{ActionId, RevisionTuple, Sha256Digest};

use crate::{
    WorkspaceAuthorizationRequest, WorkspaceCondition, WorkspaceError, WorkspaceState,
    error::{ErrorCode, RecoveryClass, WorkspaceOperation, mismatch},
    verified::{AuthorityFacts, ResourceFacts, authority_complete, resource_identity_exact},
    writable::WritableWorkspace,
};

/// Sole public owner of a live writable workspace and its receipt replay guard.
pub struct WorkspaceGateway {
    workspace: WritableWorkspace,
}

impl WorkspaceGateway {
    /// Wraps one checked move-only writable workspace.
    #[must_use]
    pub const fn new(workspace: WritableWorkspace) -> Self {
        Self { workspace }
    }

    /// Returns current immutable state for inspection.
    #[must_use]
    pub const fn state(&self) -> &WorkspaceState {
        self.workspace.state()
    }

    /// Returns the canonical target-owned transaction namespace for diagnostics.
    #[must_use]
    pub fn transaction_namespace(&self) -> &std::path::Path {
        self.workspace.transaction_namespace()
    }

    /// Consumes the gateway and returns the writable handle.
    #[must_use]
    pub fn into_workspace(self) -> WritableWorkspace {
        self.workspace
    }

    pub(crate) fn authorize(
        &mut self,
        request: &WorkspaceAuthorizationRequest<'_>,
        expected_payload: &[u8],
    ) -> Result<MutationPermit, WorkspaceError> {
        self.authorize_in_condition(request, expected_payload, WorkspaceCondition::Clean)
    }

    pub(crate) fn authorize_in_condition(
        &mut self,
        request: &WorkspaceAuthorizationRequest<'_>,
        expected_payload: &[u8],
        required_condition: WorkspaceCondition,
    ) -> Result<MutationPermit, WorkspaceError> {
        if self.workspace.state().condition() != required_condition {
            return Err(WorkspaceError::new(
                ErrorCode::WorkspaceUnavailable,
                WorkspaceOperation::Authorize,
                RecoveryClass::Reconcile,
                "workspace is not in the clean writable state",
            ));
        }
        let action_id = request.intent().action_id;
        if self.workspace.state().action_consumed(action_id) {
            return Err(WorkspaceError::new(
                ErrorCode::ReceiptReused,
                WorkspaceOperation::Authorize,
                RecoveryClass::Reauthorize,
                "action receipts were already consumed by this workspace revision",
            ));
        }
        let facts = validate_request(self.workspace.state(), request, expected_payload)?;
        if !authority_complete(facts) {
            return Err(mismatch("committed authority facts are incomplete"));
        }
        let action_digest = request
            .intent()
            .digest(CodecLimits::PRODUCTION)
            .map_err(|_| mismatch("action intent cannot be encoded canonically"))?;
        if let Err(error) = self.workspace.commit_action_consumption(action_id, action_digest) {
            if error.code() == ErrorCode::Indeterminate {
                self.workspace.state_mut().set_condition(WorkspaceCondition::Indeterminate);
            }
            return Err(error);
        }
        Ok(MutationPermit {
            action_id,
            action_digest,
            generation: self.workspace.state().generation(),
            revision: self.workspace.state().revision(),
            dispatch_event: request.kernel().batch().records()[0].event_id(),
        })
    }

    pub(crate) const fn workspace_mut(&mut self) -> &mut WritableWorkspace {
        &mut self.workspace
    }
}

/// Crate-private, move-only, operation-scoped authorization proof.
pub struct MutationPermit {
    action_id: ActionId,
    action_digest: Sha256Digest,
    generation: peritus_types::Generation,
    revision: peritus_types::RevisionNumber,
    dispatch_event: peritus_types::EventId,
}

impl MutationPermit {
    pub(crate) const fn action_id(&self) -> ActionId {
        self.action_id
    }
    pub(crate) const fn action_digest(&self) -> Sha256Digest {
        self.action_digest
    }
    pub(crate) const fn generation(&self) -> peritus_types::Generation {
        self.generation
    }
    pub(crate) const fn revision(&self) -> peritus_types::RevisionNumber {
        self.revision
    }
    pub(crate) const fn dispatch_event(&self) -> peritus_types::EventId {
        self.dispatch_event
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the target gate intentionally keeps the complete receipt cross-match contiguous"
)]
fn validate_request(
    state: &WorkspaceState,
    request: &WorkspaceAuthorizationRequest<'_>,
    expected_payload: &[u8],
) -> Result<AuthorityFacts, WorkspaceError> {
    let intent = request.intent();
    if intent.operation_class != OperationClass::WorkspaceMutation
        || !intent.role.permits_operation(OperationClass::WorkspaceMutation)
        || intent.payload != expected_payload
    {
        return Err(mismatch("action intent does not describe this exact workspace mutation"));
    }
    let digest = intent
        .digest(CodecLimits::PRODUCTION)
        .map_err(|_| mismatch("action intent cannot be encoded canonically"))?;
    let action = request
        .kernel()
        .aggregate()
        .action(intent.action_id)
        .ok_or_else(|| mismatch("committed B0 aggregate has no exact action"))?;
    let witness = action
        .authorization()
        .ok_or_else(|| mismatch("committed B0 action has no authorization witness"))?;
    let capability = request.capability().transition();
    let scope = capability.scope();
    let lease_transition = request.lease().transition();
    let record = lease_transition.record();
    let lease_use = record
        .binding()
        .as_use()
        .ok_or_else(|| mismatch("committed lease transition is not a capability use"))?;
    let claim = lease_use.claim();
    let lease_scope = claim.scope();
    let binding = state.binding();
    let resources = ResourceFacts {
        workspace: binding.workspace_id(),
        target: binding.resource_id(),
        intent: intent.resource_id,
        witness: witness.resource_id(),
        capability: capability.permission().resource_id(),
        lease_workspace: lease_scope.workspace_id(),
        lease_resource: lease_scope.resource_id(),
        lease_environment: lease_scope.environment_id(),
        environment: binding.environment_id(),
    };
    if !resource_identity_exact(resources) {
        return Err(WorkspaceError::new(
            ErrorCode::ResourceMismatch,
            WorkspaceOperation::Authorize,
            RecoveryClass::Reauthorize,
            "resolved target differs from one or more authorized nominal identities",
        ));
    }
    let dispatch_committed = exact_dispatch(request, intent.action_id, request.revision())?;
    let revision_matches = revision_exact(state, request.revision());
    let action_matches = action.id() == intent.action_id
        && action.digest() == digest
        && action.phase() == ActionPhase::Dispatched
        && capability.action_id() == intent.action_id
        && capability.action_digest() == digest
        && lease_use.action_id() == intent.action_id
        && lease_use.action_digest() == digest
        && caller_matches(state, request, intent);
    let actor_matches = action.actor_id() == intent.actor_id
        && action.role() == intent.role
        && action.environment_id() == intent.environment_id
        && scope.actor_id() == intent.actor_id
        && scope.role() == intent.role
        && scope.environment_id() == intent.environment_id
        && lease_use.actor_id() == intent.actor_id
        && lease_use.role() == intent.role
        && lease_use.environment_id() == intent.environment_id
        && claim.holder().actor_id() == intent.actor_id
        && claim.holder().session_id() == request.session_id()
        && claim.holder() == state.lease_holder();
    let capability_matches = witness.transition_digest() == capability.transition_digest()
        && witness.capability_name() == &intent.capability_name
        && capability.permission().capability_name() == &intent.capability_name
        && lease_use.permission().capability_name() == &intent.capability_name
        && lease_binding_matches_capability(lease_use, capability);
    let next = lease_transition.next();
    let lease_matches = capability_matches
        && matches!(record.kind(), LeaseTransitionKind::Used { action_id, action_digest }
            if action_id == intent.action_id && action_digest == digest)
        && record.scope() == lease_scope
        && record.after_generation() == claim.generation()
        && record.after_version() == next.version()
        && record.after_phase() == LeasePhase::Active
        && next.scope() == lease_scope
        && next.generation() == claim.generation()
        && next.phase() == LeasePhase::Active
        && next.active().is_some_and(|active| active.claim() == claim)
        && claim.generation() == state.generation();
    let time_current = request.current_epoch().get() == request.observed_at().epoch().get()
        && request.observed_at() >= lease_use.observed_at()
        && request.observed_at() < claim.expires_at()
        && scope.validity().contains(request.observed_at()).unwrap_or(false);
    Ok(AuthorityFacts {
        action: intent.action_id,
        action_matches,
        actor: intent.actor_id,
        actor_matches,
        resource_matches: true,
        revision_matches,
        lease_matches,
        dispatch_committed,
        time_current,
        generation: state.generation(),
        expected_generation: request.expected_generation(),
        revision: state.revision(),
        expected_revision: request.expected_revision(),
    })
}

fn caller_matches(
    state: &WorkspaceState,
    request: &WorkspaceAuthorizationRequest<'_>,
    intent: &peritus_protocol::ActionIntentDto,
) -> bool {
    request.caller_binding().is_none_or(|caller| {
        caller.actor_id() == intent.actor_id
            && caller.role() == intent.role
            && caller.workspace_id() == state.binding().workspace_id()
            && caller.environment_id() == intent.environment_id
            && caller.environment_id() == state.binding().environment_id()
            && caller.resource_id() == intent.resource_id
            && caller.resource_id() == state.binding().resource_id()
    })
}

fn revision_exact(state: &WorkspaceState, revision: RevisionTuple) -> bool {
    revision.workspace_id() == state.binding().workspace_id()
        && revision.workspace_generation() == state.generation()
        && revision.workspace_revision() == state.revision()
}

fn exact_dispatch(
    request: &WorkspaceAuthorizationRequest<'_>,
    action_id: ActionId,
    revision: RevisionTuple,
) -> Result<bool, WorkspaceError> {
    let records = request.kernel().batch().records();
    if records.len() != 1 {
        return Err(WorkspaceError::new(
            ErrorCode::MissingDispatch,
            WorkspaceOperation::Authorize,
            RecoveryClass::Reauthorize,
            "kernel receipt is not one exact dispatch event",
        ));
    }
    let event = decode_message::<KernelEventDto>(records[0].frame_bytes(), CodecLimits::PRODUCTION)
        .map_err(|_| {
            WorkspaceError::new(
                ErrorCode::MissingDispatch,
                WorkspaceOperation::Authorize,
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
    reason = "the lease binding and capability scope intentionally use distinct accessor names"
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
        && (binding.scope_use_limit() == scope.use_limit())
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
