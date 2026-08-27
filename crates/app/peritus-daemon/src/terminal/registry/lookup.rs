//! Exact process and attachment lookup for the live terminal registry.

use peritus_app_protocol::TerminalBinding;
use peritus_types::{ActorId, ProcessId, SessionId};

use super::RegistryState;
use crate::terminal::{
    bridge::TerminalBridge,
    error::{TerminalBridgeError, TerminalBridgeErrorKind},
};

pub(super) fn exact_bridge_mut(
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

pub(super) fn process_mut(
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

pub(super) const fn capacity(detail: &'static str) -> TerminalBridgeError {
    rejected(TerminalBridgeErrorKind::Capacity, detail)
}

pub(super) const fn rejected(
    kind: TerminalBridgeErrorKind,
    detail: &'static str,
) -> TerminalBridgeError {
    TerminalBridgeError::rejected(kind, detail)
}
