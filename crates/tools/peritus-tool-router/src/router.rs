//! Stateful growing dispatch, active ownership, control, deadline, and recovery router.

use std::collections::HashMap;

use peritus_policy::{ActorRole, AuthorityInstant, CapabilityScope};
use peritus_tool_protocol::{
    CancellationReason, PreparedToolCall, ProgressContract, ToolCall, ToolControl,
};
use peritus_types::ActionId;

use crate::{
    AuthorizedInvocation, ControlRetryability, DispatchFailure, DispatchOutcome, ExecutionUpdate,
    ExposedTools,
    InterruptedDispatch, InvocationHandle, RecoveryObservation, RecoveryOutcome,
    ReservedDispatchError, RouterError, RouterErrorKind, ToolAuthorizationRequest, ToolDispatcher,
    ToolRegistry, ToolStart,
    authorization,
    execution::{ActiveEntry, ensure_supported, validate_result},
    normalization::normalize_failure,
    replay::{
        PendingReplayReceipt, PublishedReplayReceipt, ReplayLedger, ReplayReservation, ReplayStore,
    },
};

/// Initial allocation targets for dynamically growing live router indexes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterLimits {
    active: usize,
    replay: usize,
}

impl RouterLimits {
    /// Creates nonzero allocation targets.
    ///
    /// # Errors
    ///
    /// Rejects a zero-sized initial allocation target.
    pub const fn new(active: usize, replay: usize) -> Result<Self, RouterError> {
        if active == 0 || replay == 0 {
            return Err(RouterError::new(
                RouterErrorKind::Capacity,
                "configure tool router",
                "router initial allocation targets must be nonzero",
            ));
        }
        Ok(Self { active, replay })
    }
    /// Returns the initial active-index allocation target.
    #[must_use]
    pub const fn active(self) -> usize {
        self.active
    }
    /// Returns the initial live replay-index allocation target.
    #[must_use]
    pub const fn replay(self) -> usize {
        self.replay
    }
}

/// Sole C4 authorization, dispatch, control, and replay owner.
pub struct ToolRouter {
    registry: ToolRegistry,
    replay: ReplayLedger,
    active: HashMap<ActionId, ActiveEntry>,
}

impl ToolRouter {
    /// Creates an empty stateful router over one immutable registry.
    #[must_use]
    pub fn new(registry: ToolRegistry, limits: RouterLimits) -> Self {
        Self {
            registry,
            replay: ReplayLedger::new(limits.replay),
            active: HashMap::with_capacity(limits.active),
        }
    }

    /// Creates an empty stateful router backed by a durable indexed replay store.
    #[must_use]
    pub fn with_replay_store<S>(registry: ToolRegistry, limits: RouterLimits, store: S) -> Self
    where
        S: ReplayStore + 'static,
    {
        Self {
            registry,
            replay: ReplayLedger::with_store(limits.replay, Box::new(store)),
            active: HashMap::with_capacity(limits.active),
        }
    }

    /// Borrows the immutable canonical registry.
    #[must_use]
    pub const fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Reports whether this router retains the exact active invocation owner.
    #[must_use]
    pub fn owns_active(&self, handle: InvocationHandle) -> bool {
        self.active.get(&handle.action_id()).is_some_and(|entry| {
            entry.prepared().replay_identity() == handle.replay_identity()
        })
    }

    /// Reads an exact durable V2 progress page from an invocation frontier.
    ///
    /// This remains available after terminal settlement because the page store is independent of
    /// the router's live owner table.
    pub fn progress_page(
        &mut self,
        handle: InvocationHandle,
        start: u64,
    ) -> Result<Option<crate::ProgressPage>, RouterError> {
        self.replay.progress_page(
            handle.action_id(),
            handle.replay_identity().digest(),
            start,
        )
    }

    /// Reconciles router state after unwinding interrupted one exact reserved dispatch call.
    ///
    /// An adopted reservation with no active owner or settled receipt is advanced to
    /// indeterminate before this method returns. The caller may then publish that receipt without
    /// ever invoking the dispatcher again.
    ///
    /// # Errors
    ///
    /// Rejects a retained action whose replay identity differs from the prepared call.
    pub fn reconcile_interrupted_dispatch(
        &mut self,
        prepared: &PreparedToolCall,
    ) -> Result<InterruptedDispatch, RouterError> {
        let action_id = prepared.call().action_id();
        if let Some(entry) = self.active.get(&action_id) {
            if entry.prepared().replay_identity() != prepared.replay_identity() {
                return Err(replay_mismatch());
            }
            return Ok(InterruptedDispatch::Active(InvocationHandle::new(
                action_id,
                prepared.replay_identity(),
            )));
        }
        let Some(reservation_owner) = self.replay.retained_reservation_owner(prepared)? else {
            return Ok(InterruptedDispatch::Unadopted);
        };
        if self.replay.pending_publication(action_id).is_none() {
            self.replay.indeterminate(prepared, reservation_owner);
        }
        Ok(InterruptedDispatch::Settled)
    }

    /// Returns a move-only copy of one settled receipt awaiting durable publication.
    #[must_use]
    pub fn pending_replay_publication(
        &self,
        action_id: ActionId,
    ) -> Option<PendingReplayReceipt> {
        self.replay.pending_publication(action_id)
    }

    /// Retires a local settled receipt after an external durable writer accepted it.
    ///
    /// # Errors
    ///
    /// Rejects a publication proof that differs from the router-owned pending receipt.
    pub fn acknowledge_replay_publication(
        &mut self,
        published: PublishedReplayReceipt,
    ) -> Result<(), RouterError> {
        self.replay.acknowledge_publication(published)
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
        self.registry.prepare(call)
    }

    /// Validates exact authority, consumes replay identity, and invokes one matching dispatcher.
    ///
    /// # Errors
    ///
    /// Rejects incomplete authority, identity mismatch, replay conflict, or an invalid
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
        Self::validate_dispatch_target(&prepared, dispatcher)?;
        let evidence = authorization::validate(&prepared, request)?;
        self.replay.reserve(&prepared)?;
        let retained = prepared.clone();
        let observed_at = request.observed_at();
        let invocation = AuthorizedInvocation::new(
            prepared,
            evidence.intent_digest,
            evidence.dispatch_event,
            observed_at,
            evidence.binding,
        );
        self.start_dispatch(&retained, invocation, observed_at, dispatcher)
    }

    /// Consumes an externally persisted exact reservation and invokes its matching dispatcher.
    ///
    /// A failure before permit construction returns the reservation for operation-owned retry.
    /// Once the dispatcher can observe the permit, the reservation is consumed permanently.
    ///
    /// # Errors
    ///
    /// Rejects incomplete authority, identity mismatch, reservation conflict, or
    /// an invalid dispatcher observation without ever returning a post-effect reservation.
    pub fn dispatch_reserved(
        &mut self,
        prepared: &PreparedToolCall,
        request: &ToolAuthorizationRequest<'_>,
        dispatcher: &mut dyn ToolDispatcher,
        reservation: ReplayReservation,
    ) -> Result<DispatchOutcome, ReservedDispatchError> {
        if let Err(error) = Self::validate_dispatch_target(prepared, dispatcher) {
            return Err(ReservedDispatchError::before_effect(error, reservation));
        }
        let evidence = match authorization::validate(prepared, request) {
            Ok(evidence) => evidence,
            Err(error) => {
                return Err(ReservedDispatchError::before_effect(error, reservation));
            }
        };
        if let Err(error) = self.replay.adopt(prepared, &reservation) {
            return Err(ReservedDispatchError::before_effect(error, reservation));
        }
        let retained = prepared.clone();
        let observed_at = request.observed_at();
        let invocation = AuthorizedInvocation::new(
            prepared.clone(),
            evidence.intent_digest,
            evidence.dispatch_event,
            observed_at,
            evidence.binding,
        );
        self.start_dispatch(&retained, invocation, observed_at, dispatcher)
            .map_err(ReservedDispatchError::after_effect)
    }

    /// Adopts an independently checkpointed owner for an already reserved or active invocation.
    ///
    /// The durable replay receipt and paged progress chain select the exact accepted frontier
    /// before `adopter` is called. The adopter must reconstruct the same execution from its own
    /// authoritative checkpoint; this path never invokes a dispatcher or repeats the effect.
    ///
    /// # Errors
    /// Rejects settled, mismatched, ownerless, legacy-progress, or already live invocations and
    /// preserves the durable reservation when reconstruction fails.
    pub fn adopt_active<F>(
        &mut self,
        prepared: PreparedToolCall,
        observed_at: AuthorityInstant,
        adopter: F,
    ) -> Result<InvocationHandle, RouterError>
    where
        F: FnOnce(&PreparedToolCall, u64) -> Result<Box<dyn crate::ToolExecution>, DispatchFailure>,
    {
        let action_id = prepared.call().action_id();
        if self.active.contains_key(&action_id) {
            return Err(RouterError::new(
                RouterErrorKind::ReplayConflict,
                "adopt durable tool execution",
                "action already has a live execution owner",
            ));
        }
        let handle = InvocationHandle::new(action_id, prepared.replay_identity());
        let (reservation_owner, next_sequence) = self
            .replay
            .active_adoption(&prepared, observed_at)
            .map_err(|error| error.retaining_invocation(handle))?;
        let execution = adopter(&prepared, next_sequence).map_err(|failure| {
            RouterError::active_failure(
                handle,
                "adopt durable tool execution",
                failure,
                None,
            )
        })?;
        self.active.insert(
            action_id,
            ActiveEntry::adopted(prepared.clone(), execution, observed_at, next_sequence),
        );
        self.replay.mark_active(&prepared, reservation_owner);
        Ok(handle)
    }

    fn validate_dispatch_target(
        prepared: &PreparedToolCall,
        dispatcher: &dyn ToolDispatcher,
    ) -> Result<(), RouterError> {
        if dispatcher.implementation_identity() != prepared.descriptor().implementation_identity()
            || dispatcher.descriptor_digest() != prepared.descriptor_digest()
        {
            return Err(RouterError::new(
                RouterErrorKind::DispatcherIdentity,
                "dispatch tool invocation",
                "dispatcher identity or descriptor digest differs before permit construction",
            ));
        }
        Ok(())
    }

    fn start_dispatch(
        &mut self,
        retained: &PreparedToolCall,
        invocation: AuthorizedInvocation,
        observed_at: AuthorityInstant,
        dispatcher: &mut dyn ToolDispatcher,
    ) -> Result<DispatchOutcome, RouterError> {
        let reservation_owner = self.replay.reservation_owner(retained)?;
        match dispatcher.start(invocation) {
            Ok(ToolStart::Completed(result)) => {
                if let Err(error) = validate_result(retained, &result, 0) {
                    self.replay.indeterminate(retained, reservation_owner);
                    return Err(error);
                }
                self.replay.complete(retained, reservation_owner, result.clone());
                Ok(DispatchOutcome::Completed(result))
            }
            Ok(ToolStart::Active(execution)) => {
                let handle =
                    InvocationHandle::new(retained.call().action_id(), retained.replay_identity());
                self.active.insert(
                    retained.call().action_id(),
                    ActiveEntry::new(retained.clone(), execution, observed_at),
                );
                self.replay.mark_active(retained, reservation_owner);
                Ok(DispatchOutcome::Active(handle))
            }
            Err(failure) => {
                let result = match normalize_failure(
                    retained,
                    observed_at,
                    observed_at,
                    &failure,
                    0,
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        self.replay.indeterminate(retained, reservation_owner);
                        return Err(error);
                    }
                };
                self.replay.complete(retained, reservation_owner, result.clone());
                Ok(DispatchOutcome::Completed(result))
            }
        }
    }

    /// Polls an active invocation, enforcing its deadline first.
    ///
    /// # Errors
    ///
    /// Rejects unknown/mismatched handles or failed/malformed observations. An observation
    /// failure retains the exact active owner and never synthesizes a terminal result.
    pub fn poll(
        &mut self,
        handle: InvocationHandle,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, RouterError> {
        self.drive(handle, observed_at, "poll tool execution", false, |entry| {
            entry.poll(observed_at)
        })
    }

    /// Applies one descriptor-supported active control.
    ///
    /// # Errors
    ///
    /// Rejects unknown/mismatched handles, unsupported controls, or failed/malformed observations.
    /// Rejection preserves ownership; only an explicitly unadmitted request permits retry.
    pub fn control(
        &mut self,
        handle: InvocationHandle,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, RouterError> {
        let entry = self.exact_entry(handle)?;
        ensure_supported(entry.prepared().descriptor(), &control).map_err(|error| {
            error
                .retaining_invocation(handle)
                .rejecting_control(ControlRetryability::CorrectRequest)
        })?;
        self.drive(handle, observed_at, "control tool execution", true, |entry| {
            entry.control(control, observed_at)
        })
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
        self.drive(handle, observed_at, "cancel tool execution", true, |entry| {
            entry.cancel(reason, observed_at)
        })
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
        let action_id = handle.action_id();
        let prepared = self.exact_entry(handle)?.prepared().clone();
        let reservation_owner = self
            .replay
            .reservation_owner(&prepared)
            .map_err(|error| error.retaining_invocation(handle))?;
        let resolution = {
            let entry = self.exact_entry_mut(handle)?;
            entry
                .observe_time(observed_at)
                .map_err(|error| error.retaining_invocation(handle))?;
            recover_entry(entry, handle, observed_at)
        };
        match resolution {
            Ok(RecoveredEntry::Active(mut update)) => {
                self.accept_update(handle, &prepared, &mut update)?;
                Ok(RecoveryOutcome::Active(update))
            }
            Ok(RecoveredEntry::Completed(mut result)) => {
                self.accept_update(handle, &prepared, &mut result)?;
                let terminal = result
                    .terminal()
                    .cloned()
                    .ok_or_else(|| invalid("completed recovery observation has no terminal result"))?;
                self.replay.complete(&prepared, reservation_owner, terminal.clone());
                self.active.remove(&action_id);
                if prepared.call().limits().progress_contract() == ProgressContract::PagedV2 {
                    Ok(RecoveryOutcome::CompletedUpdate(result))
                } else {
                    Ok(RecoveryOutcome::Completed(terminal))
                }
            }
            Err(error) => Err(error.retaining_invocation(handle)),
        }
    }

    fn drive(
        &mut self,
        handle: InvocationHandle,
        observed_at: AuthorityInstant,
        operation_name: &'static str,
        control_operation: bool,
        operation: impl FnOnce(&mut ActiveEntry) -> Result<ExecutionUpdate, DispatchFailure>,
    ) -> Result<ExecutionUpdate, RouterError> {
        let action_id = handle.action_id();
        let prepared = self.exact_entry(handle)?.prepared().clone();
        let reservation_owner = self
            .replay
            .reservation_owner(&prepared)
            .map_err(|error| error.retaining_invocation(handle))?;
        let mut update = {
            let entry = self.exact_entry_mut(handle)?;
            entry
                .observe_time(observed_at)
                .map_err(|error| {
                    let error = error.retaining_invocation(handle);
                    if control_operation {
                        error.rejecting_control(ControlRetryability::CorrectRequest)
                    } else {
                        error
                    }
                })?;
            drive_entry(entry, handle, operation_name, control_operation, operation)?
        };
        self.accept_update(handle, &prepared, &mut update)?;
        if let Some(result) = update.terminal() {
            self.replay.complete(&prepared, reservation_owner, result.clone());
            self.active.remove(&action_id);
        }
        Ok(update)
    }

    fn accept_update(
        &mut self,
        handle: InvocationHandle,
        prepared: &PreparedToolCall,
        update: &mut ExecutionUpdate,
    ) -> Result<(), RouterError> {
        let (start, next_sequence) = {
            let entry = self.exact_entry(handle)?;
            (entry.next_sequence(), entry.validate_update(update)?)
        };
        let page = if prepared.call().limits().progress_contract() == ProgressContract::PagedV2 {
            self.replay
                .append_progress(prepared, start, update.progress())
                .map_err(|error| error.retaining_invocation(handle))?
        } else {
            None
        };
        self.exact_entry_mut(handle)?
            .acknowledge_update(next_sequence)
            .map_err(|failure| {
                RouterError::active_failure(
                    handle,
                    "acknowledge durable tool progress",
                    failure,
                    None,
                )
            })?;
        update.bind_progress_page(page);
        Ok(())
    }

    fn exact_entry(&self, handle: InvocationHandle) -> Result<&ActiveEntry, RouterError> {
        let entry = self.active.get(&handle.action_id()).ok_or_else(unknown_active)?;
        if entry.prepared().replay_identity() != handle.replay_identity() {
            return Err(replay_mismatch());
        }
        Ok(entry)
    }

    fn exact_entry_mut(
        &mut self,
        handle: InvocationHandle,
    ) -> Result<&mut ActiveEntry, RouterError> {
        let entry = self.active.get_mut(&handle.action_id()).ok_or_else(unknown_active)?;
        if entry.prepared().replay_identity() != handle.replay_identity() {
            return Err(replay_mismatch());
        }
        Ok(entry)
    }
}

fn drive_entry(
    entry: &mut ActiveEntry,
    handle: InvocationHandle,
    operation_name: &'static str,
    control_operation: bool,
    operation: impl FnOnce(&mut ActiveEntry) -> Result<ExecutionUpdate, DispatchFailure>,
) -> Result<ExecutionUpdate, RouterError> {
    let update = match operation(entry) {
        Ok(update) => update,
        Err(failure) => {
            let retryability = if control_operation { failure.control_retryability() } else { None };
            return Err(RouterError::active_failure(handle, operation_name, failure, retryability));
        }
    };
    Ok(update)
}

enum RecoveredEntry {
    Active(ExecutionUpdate),
    Completed(ExecutionUpdate),
}

fn recover_entry(
    entry: &mut ActiveEntry,
    handle: InvocationHandle,
    observed_at: AuthorityInstant,
) -> Result<RecoveredEntry, RouterError> {
    match entry.recover(observed_at) {
        Ok(RecoveryObservation::Active(update)) => {
            if update.terminal().is_some() {
                return Err(invalid(
                    "active recovery observation unexpectedly contains a terminal result",
                ));
            }
            Ok(RecoveredEntry::Active(update))
        }
        Ok(RecoveryObservation::Completed(update)) => {
            if update.terminal().is_none() {
                return Err(invalid("completed recovery observation has no terminal result"));
            }
            Ok(RecoveredEntry::Completed(update))
        }
        Ok(RecoveryObservation::Lost(failure)) | Err(failure) => {
            Err(RouterError::active_failure(handle, "recover tool execution", failure, None))
        }
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
