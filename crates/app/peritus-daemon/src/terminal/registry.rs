//! Bounded concurrent registry of live processes and exact terminal attachments.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};

use peritus_app_protocol::{
    TerminalBinding, TerminalCancellation, TerminalDetach, TerminalExit, TerminalInput,
    TerminalOutput, TerminalResize, TerminalTransitionDisposition,
};
use peritus_types::{ActorId, ProcessId, SessionId};

use super::{
    bridge::{LiveTerminalRegistration, TerminalBridge},
    error::{TerminalBridgeError, TerminalBridgeErrorKind},
    limits::TerminalRegistryLimits,
};

/// Result of idempotently opening an exact terminal attachment.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum AttachmentDisposition {
    /// A new attachment was opened and its retained output prefix was queued.
    Attached,
    /// The same live attachment binding was already open.
    AlreadyAttached,
}

/// One A3 event produced by the live bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TerminalBridgeEvent {
    /// Ordered opaque process output.
    Output(TerminalOutput),
    /// The unique terminal exit fence.
    Exited(TerminalExit),
}

struct RegistryState {
    limits: TerminalRegistryLimits,
    processes: BTreeMap<ProcessId, TerminalBridge>,
    attachments: BTreeMap<peritus_app_protocol::TerminalAttachmentId, ProcessId>,
}

/// Cloneable access handle to the bounded live terminal registry.
#[derive(Clone)]
pub(crate) struct TerminalRegistry {
    inner: Arc<Mutex<RegistryState>>,
}

impl TerminalRegistry {
    /// Creates an empty registry with fixed limits.
    ///
    /// # Errors
    ///
    /// Rejects zero or inconsistent limits.
    pub(crate) fn new(limits: TerminalRegistryLimits) -> Result<Self, TerminalBridgeError> {
        if !limits.valid() {
            return Err(rejected(
                TerminalBridgeErrorKind::InvalidLimit,
                "terminal registry limits are zero or inconsistent",
            ));
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(RegistryState {
                limits,
                processes: BTreeMap::new(),
                attachments: BTreeMap::new(),
            })),
        })
    }

    /// Registers one exact live C2 process control.
    ///
    /// # Errors
    ///
    /// Rejects capacity exhaustion or conflicting process reuse.
    pub(crate) fn register(
        &self,
        registration: LiveTerminalRegistration,
    ) -> Result<(), TerminalBridgeError> {
        let mut state = self.lock();
        let process_id = registration.process_id();
        if state.processes.contains_key(&process_id) {
            return Err(rejected(
                TerminalBridgeErrorKind::RegistrationConflict,
                "process identity already has a live terminal registration",
            ));
        }
        if state.processes.len() >= state.limits.maximum_processes {
            return Err(capacity("live terminal process registry is full"));
        }
        state.processes.insert(process_id, TerminalBridge::new(registration));
        Ok(())
    }

    /// Opens or idempotently confirms one exact actor/session/process attachment.
    ///
    /// # Errors
    ///
    /// Rejects unknown or terminal processes, ownership mismatch, unavailable replay, conflicts,
    /// or attachment capacity exhaustion.
    pub(crate) fn attach(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        binding: TerminalBinding,
        maximum_chunk_bytes: usize,
    ) -> Result<AttachmentDisposition, TerminalBridgeError> {
        let mut state = self.lock();
        let process_id = binding.process_id();
        if let Some(indexed_process) = state.attachments.get(&binding.attachment_id()) {
            if *indexed_process != process_id {
                return Err(rejected(
                    TerminalBridgeErrorKind::RegistrationConflict,
                    "attachment identity is indexed to another process",
                ));
            }
        }
        let limits = state.limits;
        let global_full = state.attachments.len() >= limits.maximum_attachments;
        let bridge = process_mut(&mut state, process_id, actor_id, session_id)?;
        let existing = bridge.attachment_matches(binding);
        if !existing
            && (global_full
                || bridge.attachment_count() >= limits.maximum_attachments_per_process())
        {
            return Err(capacity("live terminal attachment registry is full"));
        }
        let disposition = bridge.attach(binding, maximum_chunk_bytes, limits)?;
        state.attachments.insert(binding.attachment_id(), process_id);
        Ok(disposition)
    }

    /// Forwards one bounded input write through the exact live attachment.
    ///
    /// # Errors
    ///
    /// Rejects a mismatched binding, actor/session ownership, A3 phase, or C2 input operation.
    pub(crate) fn input(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        input: &TerminalInput,
    ) -> Result<(), TerminalBridgeError> {
        let mut state = self.lock();
        exact_bridge_mut(&mut state, actor_id, session_id, input.binding())?.input(input)
    }

    /// Forwards one checked PTY resize through the exact live attachment.
    ///
    /// # Errors
    ///
    /// Rejects a mismatched binding, actor/session ownership, A3 phase, or C2 resize operation.
    pub(crate) fn resize(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        resize: TerminalResize,
    ) -> Result<(), TerminalBridgeError> {
        let mut state = self.lock();
        exact_bridge_mut(&mut state, actor_id, session_id, resize.binding())?.resize(resize)
    }

    /// Detaches without terminating the underlying process.
    ///
    /// # Errors
    ///
    /// Rejects a mismatched binding, actor/session ownership, or conflicting terminal fact.
    pub(crate) fn detach(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        detach: TerminalDetach,
    ) -> Result<TerminalTransitionDisposition, TerminalBridgeError> {
        let mut state = self.lock();
        exact_bridge_mut(&mut state, actor_id, session_id, detach.binding())?.detach(detach)
    }

    /// Propagates one idempotent user cancellation through C2.
    ///
    /// # Errors
    ///
    /// Rejects a mismatched binding, actor/session ownership, conflicting fact, or C2 control
    /// refusal.
    pub(crate) fn cancel(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        cancellation: TerminalCancellation,
    ) -> Result<TerminalTransitionDisposition, TerminalBridgeError> {
        let mut state = self.lock();
        exact_bridge_mut(&mut state, actor_id, session_id, cancellation.binding())?
            .cancel(cancellation)
    }

    /// Observes C2 and drains one bounded A3 event page for an exact attachment.
    ///
    /// # Errors
    ///
    /// Rejects ownership/binding mismatch, C2 observation gaps, backpressure, or projection
    /// failure.
    pub(crate) fn poll(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        binding: TerminalBinding,
    ) -> Result<Vec<TerminalBridgeEvent>, TerminalBridgeError> {
        let mut state = self.lock();
        let limits = state.limits;
        exact_bridge_mut(&mut state, actor_id, session_id, binding)?.poll(binding, limits)
    }

    /// Observes and removes a terminal process after all attachment deliveries are settled.
    ///
    /// Returns `false` while the process is live or an attachment still has pending output.
    ///
    /// # Errors
    ///
    /// Rejects an unknown process or an unobservable C2 event stream.
    pub(crate) fn retire(&self, process_id: ProcessId) -> Result<bool, TerminalBridgeError> {
        let mut state = self.lock();
        let limits = state.limits;
        let bridge = state.processes.get_mut(&process_id).ok_or_else(|| {
            rejected(
                TerminalBridgeErrorKind::ProcessNotRegistered,
                "process has no live terminal control in this daemon",
            )
        })?;
        bridge.observe(limits)?;
        if !bridge.can_retire() {
            return Ok(false);
        }
        let attachment_ids = bridge.attachment_ids();
        state.processes.remove(&process_id);
        for attachment_id in attachment_ids {
            state.attachments.remove(&attachment_id);
        }
        Ok(true)
    }

    /// Releases one connection's exact attachment set without killing the underlying processes.
    pub(crate) fn release_attachments(
        &self,
        actor_id: ActorId,
        session_id: SessionId,
        bindings: &[TerminalBinding],
    ) {
        let mut state = self.lock();
        for binding in bindings {
            if state.attachments.get(&binding.attachment_id()) != Some(&binding.process_id()) {
                continue;
            }
            let removable = state.processes.get(&binding.process_id()).is_some_and(|bridge| {
                bridge.owner_matches(actor_id, session_id) && bridge.attachment_matches(*binding)
            });
            if removable {
                if let Some(bridge) = state.processes.get_mut(&binding.process_id()) {
                    bridge.remove_attachment(binding.attachment_id());
                }
                state.attachments.remove(&binding.attachment_id());
            }
        }
    }

    /// Returns exact live process and attachment counts from the owned registry.
    pub(crate) fn counts(&self) -> (usize, usize) {
        let state = self.lock();
        (state.processes.len(), state.attachments.len())
    }

    fn lock(&self) -> MutexGuard<'_, RegistryState> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn exact_bridge_mut(
    state: &mut RegistryState,
    actor_id: ActorId,
    session_id: SessionId,
    binding: TerminalBinding,
) -> Result<&mut TerminalBridge, TerminalBridgeError> {
    if state.attachments.get(&binding.attachment_id()) != Some(&binding.process_id()) {
        return Err(rejected(
            TerminalBridgeErrorKind::RegistrationConflict,
            "terminal attachment is not indexed to its exact process",
        ));
    }
    let bridge = process_mut(state, binding.process_id(), actor_id, session_id)?;
    if !bridge.attachment_matches(binding) {
        return Err(rejected(
            TerminalBridgeErrorKind::RegistrationConflict,
            "terminal attachment binding does not match its retained registration",
        ));
    }
    Ok(bridge)
}

fn process_mut(
    state: &mut RegistryState,
    process_id: ProcessId,
    actor_id: ActorId,
    session_id: SessionId,
) -> Result<&mut TerminalBridge, TerminalBridgeError> {
    let bridge = state.processes.get_mut(&process_id).ok_or_else(|| {
        rejected(
            TerminalBridgeErrorKind::ProcessNotRegistered,
            "process has no live terminal control in this daemon",
        )
    })?;
    if !bridge.owner_matches(actor_id, session_id) {
        return Err(rejected(
            TerminalBridgeErrorKind::OwnershipMismatch,
            "authenticated actor/session does not own the terminal process",
        ));
    }
    Ok(bridge)
}

const fn capacity(detail: &'static str) -> TerminalBridgeError {
    rejected(TerminalBridgeErrorKind::Capacity, detail)
}

const fn rejected(kind: TerminalBridgeErrorKind, detail: &'static str) -> TerminalBridgeError {
    TerminalBridgeError::rejected(kind, detail)
}
