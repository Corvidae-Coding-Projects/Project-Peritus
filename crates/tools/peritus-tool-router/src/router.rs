//! Stateful bounded dispatch, active ownership, control, deadline, and recovery router.

use std::collections::BTreeMap;

use peritus_policy::{ActorRole, AuthorityInstant, CapabilityScope};
use peritus_tool_protocol::{CancellationReason, PreparedToolCall, ToolCall, ToolControl};
use peritus_types::ActionId;

use crate::{
    AuthorizedInvocation, DispatchFailure, DispatchOutcome, ExecutionUpdate, ExposedTools,
    InvocationHandle, RecoveryObservation, RecoveryOutcome, RouterError, RouterErrorKind,
    ToolAuthorizationRequest, ToolDispatcher, ToolRegistry, ToolStart, authorization,
    execution::{ActiveEntry, ensure_supported, validate_result},
    normalization::normalize_failure,
    replay::ReplayLedger,
};

/// Fixed active and replay-state capacities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterLimits {
    active: usize,
    replay: usize,
}

impl RouterLimits {
    /// Creates nonzero limits with replay capacity at least active capacity.
    ///
    /// # Errors
    ///
    /// Rejects zero active capacity or replay capacity below the active capacity.
    pub const fn new(active: usize, replay: usize) -> Result<Self, RouterError> {
        if active == 0 || replay < active {
            return Err(RouterError::new(
                RouterErrorKind::Capacity,
                "configure tool router",
                "router capacities are zero or replay capacity is below active capacity",
            ));
        }
        Ok(Self { active, replay })
    }
    /// Returns active capacity.
    #[must_use]
    pub const fn active(self) -> usize {
        self.active
    }
    /// Returns replay capacity.
    #[must_use]
    pub const fn replay(self) -> usize {
        self.replay
    }
}

/// Sole C4 authorization, dispatch, control, and replay owner.
pub struct ToolRouter {
    registry: ToolRegistry,
    limits: RouterLimits,
    replay: ReplayLedger,
    active: BTreeMap<ActionId, ActiveEntry>,
}

impl ToolRouter {
    /// Creates an empty stateful router over one immutable registry.
    #[must_use]
    pub const fn new(registry: ToolRegistry, limits: RouterLimits) -> Self {
        Self { registry, limits, replay: ReplayLedger::new(limits.replay), active: BTreeMap::new() }
    }

    /// Borrows the immutable canonical registry.
    #[must_use]
    pub const fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Computes canonical role/capability exposure.
    ///
    /// # Errors
    ///
    /// Rejects malformed or excessive exposure state.
    pub fn exposed(
        &self,
        role: ActorRole,
        scope: &CapabilityScope,
    ) -> Result<ExposedTools, RouterError> {
        ExposedTools::plan(&self.registry, role, scope)
    }

    /// Performs effect-free lookup, schema validation, and digest-bound preparation.
    ///
    /// # Errors
    ///
    /// Rejects unknown tools, widened limits, or schema-invalid arguments.
    pub fn prepare(&self, call: ToolCall) -> Result<PreparedToolCall, RouterError> {
        crate::preparation::prepare(&self.registry, call)
    }

    /// Validates exact authority, consumes replay identity, and invokes one matching dispatcher.
    ///
    /// # Errors
    ///
    /// Rejects incomplete authority, identity mismatch, capacity, replay conflict, or an invalid
    /// dispatcher observation. Rejection before permit construction never calls the dispatcher.
    pub fn dispatch(
        &mut self,
        prepared: PreparedToolCall,
        request: &ToolAuthorizationRequest<'_>,
        dispatcher: &mut dyn ToolDispatcher,
    ) -> Result<DispatchOutcome, RouterError> {
        if let Some(outcome) = self.replay.inspect(&prepared)? {
            authorization::validate(&prepared, request)?;
            return Ok(outcome);
        }
        if dispatcher.implementation_identity() != prepared.descriptor().implementation_identity()
            || dispatcher.descriptor_digest() != prepared.descriptor_digest()
        {
            return Err(RouterError::new(
                RouterErrorKind::DispatcherIdentity,
                "dispatch tool invocation",
                "dispatcher identity or descriptor digest differs before permit construction",
            ));
        }
        if self.active.len() >= self.limits.active {
            return Err(RouterError::new(
                RouterErrorKind::Capacity,
                "dispatch tool invocation",
                "bounded active invocation table is full",
            ));
        }
        let evidence = authorization::validate(&prepared, request)?;
        self.replay.reserve(&prepared)?;
        let retained = prepared.clone();
        let invocation = AuthorizedInvocation::new(
            prepared,
            evidence.intent_digest,
            evidence.dispatch_event,
            request.observed_at(),
            evidence.binding,
        );
        match dispatcher.start(invocation) {
            Ok(ToolStart::Completed(result)) => {
                if let Err(error) = validate_result(&retained, &result, 0) {
                    self.replay.indeterminate(&retained);
                    return Err(error);
                }
                self.replay.complete(&retained, result.clone());
                Ok(DispatchOutcome::Completed(result))
            }
            Ok(ToolStart::Active(execution)) => {
                self.replay.mark_active(&retained);
                let handle =
                    InvocationHandle::new(retained.call().action_id(), retained.replay_identity());
                self.active.insert(
                    retained.call().action_id(),
                    ActiveEntry::new(retained, execution, request.observed_at()),
                );
                Ok(DispatchOutcome::Active(handle))
            }
            Err(failure) => {
                let result = match normalize_failure(
                    &retained,
                    request.observed_at(),
                    request.observed_at(),
                    &failure,
                    0,
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        self.replay.indeterminate(&retained);
                        return Err(error);
                    }
                };
                self.replay.complete(&retained, result.clone());
                Ok(DispatchOutcome::Completed(result))
            }
        }
    }

    /// Polls an active invocation, enforcing its deadline first.
    ///
    /// # Errors
    ///
    /// Rejects unknown/mismatched handles or malformed execution observations.
    pub fn poll(
        &mut self,
        handle: InvocationHandle,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, RouterError> {
        self.drive(handle, observed_at, |entry| entry.poll(observed_at))
    }

    /// Applies one descriptor-supported active control.
    ///
    /// # Errors
    ///
    /// Rejects unknown/mismatched handles, unsupported controls, or malformed observations.
    pub fn control(
        &mut self,
        handle: InvocationHandle,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, RouterError> {
        let entry = self.active.get(&handle.action_id()).ok_or_else(unknown_active)?;
        if entry.prepared().replay_identity() != handle.replay_identity() {
            return Err(replay_mismatch());
        }
        ensure_supported(entry.prepared().descriptor(), &control)?;
        self.drive(handle, observed_at, |entry| entry.control(control, observed_at))
    }

    /// Requests cancellation while retaining execution ownership until terminal observation.
    ///
    /// # Errors
    ///
    /// Rejects unknown/mismatched handles or malformed cancellation observations.
    pub fn cancel(
        &mut self,
        handle: InvocationHandle,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, RouterError> {
        self.drive(handle, observed_at, |entry| entry.cancel(reason, observed_at))
    }

    /// Reconciles one active invocation after observation loss.
    ///
    /// # Errors
    ///
    /// Rejects unknown/mismatched handles or malformed recovered observations.
    pub fn recover(
        &mut self,
        handle: InvocationHandle,
        observed_at: AuthorityInstant,
    ) -> Result<RecoveryOutcome, RouterError> {
        let mut entry = self.take_exact(handle)?;
        match entry.recover(observed_at) {
            Ok(RecoveryObservation::Active(update)) => {
                if update.terminal().is_some() {
                    self.replay.indeterminate(entry.prepared());
                    return Err(invalid(
                        "active recovery observation unexpectedly contains a terminal result",
                    ));
                }
                if let Err(error) = entry.accept_update(&update) {
                    self.replay.indeterminate(entry.prepared());
                    return Err(error);
                }
                self.active.insert(entry.prepared().call().action_id(), entry);
                Ok(RecoveryOutcome::Active(update))
            }
            Ok(RecoveryObservation::Completed(update)) => {
                if update.terminal().is_none() {
                    self.replay.indeterminate(entry.prepared());
                    return Err(invalid("completed recovery observation has no terminal result"));
                }
                if let Err(error) = entry.accept_update(&update) {
                    self.replay.indeterminate(entry.prepared());
                    return Err(error);
                }
                let Some(result) = update.terminal().cloned() else {
                    self.replay.indeterminate(entry.prepared());
                    return Err(invalid("completed recovery observation has no terminal result"));
                };
                self.replay.complete(entry.prepared(), result.clone());
                Ok(RecoveryOutcome::Completed(result))
            }
            Ok(RecoveryObservation::Lost(failure)) | Err(failure) => {
                self.replay.indeterminate(entry.prepared());
                Ok(RecoveryOutcome::Indeterminate(failure))
            }
        }
    }

    fn drive(
        &mut self,
        handle: InvocationHandle,
        observed_at: AuthorityInstant,
        operation: impl FnOnce(&mut ActiveEntry) -> Result<ExecutionUpdate, DispatchFailure>,
    ) -> Result<ExecutionUpdate, RouterError> {
        let mut entry = self.take_exact(handle)?;
        let update = match operation(&mut entry) {
            Ok(update) => update,
            Err(failure) => {
                let progress_count = entry.progress_count();
                let result = match normalize_failure(
                    entry.prepared(),
                    entry.started_at(),
                    observed_at,
                    &failure,
                    progress_count,
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        self.replay.indeterminate(entry.prepared());
                        return Err(error);
                    }
                };
                if let Ok(update) = ExecutionUpdate::new(entry.prepared(), Vec::new(), Some(result))
                {
                    update
                } else {
                    self.replay.indeterminate(entry.prepared());
                    return Err(invalid(
                        "normalized dispatcher failure is not a valid terminal update",
                    ));
                }
            }
        };
        self.finish_or_restore(entry, update)
    }

    fn take_exact(&mut self, handle: InvocationHandle) -> Result<ActiveEntry, RouterError> {
        let entry = self.active.remove(&handle.action_id()).ok_or_else(unknown_active)?;
        if entry.prepared().replay_identity() != handle.replay_identity() {
            self.active.insert(entry.prepared().call().action_id(), entry);
            return Err(replay_mismatch());
        }
        Ok(entry)
    }

    fn finish_or_restore(
        &mut self,
        mut entry: ActiveEntry,
        update: ExecutionUpdate,
    ) -> Result<ExecutionUpdate, RouterError> {
        if let Err(error) = entry.accept_update(&update) {
            self.replay.indeterminate(entry.prepared());
            return Err(error);
        }
        if let Some(result) = update.terminal() {
            self.replay.complete(entry.prepared(), result.clone());
        } else {
            self.active.insert(entry.prepared().call().action_id(), entry);
        }
        Ok(update)
    }
}

const fn invalid(detail: &'static str) -> RouterError {
    RouterError::new(RouterErrorKind::InvalidObservation, "accept tool observation", detail)
}

const fn unknown_active() -> RouterError {
    RouterError::new(
        RouterErrorKind::Control,
        "control tool execution",
        "active invocation handle is unknown",
    )
}

const fn replay_mismatch() -> RouterError {
    RouterError::new(
        RouterErrorKind::ReplayConflict,
        "control tool execution",
        "active handle replay identity differs",
    )
}
