//! Move-only permit and public dispatch outcomes.

use peritus_policy::{ActorRole, AuthorityInstant};
use peritus_tool_protocol::{PreparedToolCall, ReplayIdentity, ToolResult};
use peritus_types::{
    ActionId, ActorId, EnvironmentId, EventId, ResourceId, RevisionTuple, SessionId, Sha256Digest,
};

use crate::{ReplayDisposition, ReplayReservation, RouterError};

/// Dispatch failure that returns an unconsumed durable reservation only before any effect.
#[derive(Debug)]
pub struct ReservedDispatchError {
    error: RouterError,
    reservation: Option<ReplayReservation>,
}

impl ReservedDispatchError {
    pub(crate) const fn before_effect(
        error: RouterError,
        reservation: ReplayReservation,
    ) -> Self {
        Self { error, reservation: Some(reservation) }
    }

    pub(crate) const fn after_effect(error: RouterError) -> Self {
        Self { error, reservation: None }
    }

    /// Borrows the underlying router failure.
    #[must_use]
    pub const fn error(&self) -> &RouterError {
        &self.error
    }

    /// Returns the failure and a reservation that remains safe to retry, when no effect occurred.
    #[must_use]
    pub fn into_parts(self) -> (RouterError, Option<ReplayReservation>) {
        (self.error, self.reservation)
    }
}

impl core::fmt::Display for ReservedDispatchError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for ReservedDispatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Copyable exact actor/role/target binding authenticated by the complete router authority gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorizedToolBinding {
    actor_id: ActorId,
    role: ActorRole,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
    revision: RevisionTuple,
    session_id: SessionId,
}

impl AuthorizedToolBinding {
    pub(crate) const fn new(
        actor_id: ActorId,
        role: ActorRole,
        environment_id: EnvironmentId,
        resource_id: ResourceId,
        revision: RevisionTuple,
        session_id: SessionId,
    ) -> Self {
        Self { actor_id, role, environment_id, resource_id, revision, session_id }
    }
    /// Returns the authenticated actor.
    #[must_use]
    pub const fn actor_id(self) -> ActorId {
        self.actor_id
    }
    /// Returns the compiled authenticated role.
    #[must_use]
    pub const fn role(self) -> ActorRole {
        self.role
    }
    /// Returns the isolated environment identity.
    #[must_use]
    pub const fn environment_id(self) -> EnvironmentId {
        self.environment_id
    }
    /// Returns the exact target resource.
    #[must_use]
    pub const fn resource_id(self) -> ResourceId {
        self.resource_id
    }
    /// Returns the complete authority revision.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    /// Returns the owning session.
    #[must_use]
    pub const fn session_id(self) -> SessionId {
        self.session_id
    }
}

/// Router-created, move-only, one-use invocation permit.
///
/// Fields and construction remain private; this type is public only for [`crate::ToolDispatcher`].
pub struct AuthorizedInvocation {
    prepared: PreparedToolCall,
    intent_digest: Sha256Digest,
    dispatch_event: EventId,
    observed_at: AuthorityInstant,
    binding: AuthorizedToolBinding,
}

impl AuthorizedInvocation {
    pub(crate) const fn new(
        prepared: PreparedToolCall,
        intent_digest: Sha256Digest,
        dispatch_event: EventId,
        observed_at: AuthorityInstant,
        binding: AuthorizedToolBinding,
    ) -> Self {
        Self { prepared, intent_digest, dispatch_event, observed_at, binding }
    }

    /// Borrows the exact prepared call authorized for this one effect.
    #[must_use]
    pub const fn prepared(&self) -> &PreparedToolCall {
        &self.prepared
    }
    /// Returns the exact canonical B3 action-intent digest.
    #[must_use]
    pub const fn intent_digest(&self) -> Sha256Digest {
        self.intent_digest
    }
    /// Returns the one exact committed B0 dispatch event.
    #[must_use]
    pub const fn dispatch_event(&self) -> EventId {
        self.dispatch_event
    }
    /// Returns the validated current authority instant at permit creation.
    #[must_use]
    pub const fn observed_at(&self) -> AuthorityInstant {
        self.observed_at
    }
    /// Returns the exact authenticated actor/role/environment/resource binding.
    #[must_use]
    pub const fn binding(&self) -> AuthorizedToolBinding {
        self.binding
    }
    /// Returns the authorized action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.prepared.call().action_id()
    }
    /// Returns the exact prepared-call digest.
    #[must_use]
    pub const fn prepared_digest(&self) -> Sha256Digest {
        self.prepared.prepared_digest()
    }
    /// Returns the exact replay identity consumed before dispatch.
    #[must_use]
    pub const fn replay_identity(&self) -> ReplayIdentity {
        self.prepared.replay_identity()
    }
    /// Consumes the permit into the exact prepared call for the lower adapter.
    #[must_use]
    pub fn into_prepared(self) -> PreparedToolCall {
        self.prepared
    }
}

/// Stable handle for router-owned active execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvocationHandle {
    action_id: ActionId,
    replay_identity: ReplayIdentity,
}

impl InvocationHandle {
    pub(crate) const fn new(action_id: ActionId, replay_identity: ReplayIdentity) -> Self {
        Self { action_id, replay_identity }
    }
    /// Returns the active action identity.
    #[must_use]
    pub const fn action_id(self) -> ActionId {
        self.action_id
    }
    /// Returns its exact replay identity.
    #[must_use]
    pub const fn replay_identity(self) -> ReplayIdentity {
        self.replay_identity
    }
}

/// Router-owned classification after a dispatch call was interrupted by unwinding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptedDispatch {
    /// The router never adopted the externally durable reservation, so no invocation permit was
    /// exposed to the dispatcher.
    Unadopted,
    /// The dispatcher returned an active execution and the router retains its exact owner.
    Active(InvocationHandle),
    /// The adopted attempt is terminal or indeterminate and cannot be dispatched again.
    Settled,
}

/// Dispatch/replay observation without effect ambiguity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchOutcome {
    /// Dispatcher returned one terminal result.
    Completed(ToolResult),
    /// Router owns a running execution.
    Active(InvocationHandle),
    /// Exact idempotent replay returned the prior terminal envelope.
    Replayed(ToolResult),
    /// Exact replay exists but cannot be repeated or returned as success.
    PriorOutcome(ReplayDisposition),
}
