//! Bounded exact completion facts, separate from live attachment capacity and ownership.

use peritus_app_protocol::{TerminalAttachmentId, TerminalBinding, TerminalState};
use peritus_types::{ActorId, SessionId};
use std::collections::{BTreeMap, VecDeque};

use super::{TerminalBridgeError, TerminalBridgeErrorKind, rejected};

#[derive(Default)]
pub(super) struct TerminalReceipts {
    order: VecDeque<TerminalAttachmentId>,
    facts: BTreeMap<TerminalAttachmentId, Receipt>,
}

struct Receipt {
    actor: ActorId,
    session: SessionId,
    state: TerminalState,
}

impl TerminalReceipts {
    pub(super) fn contains(&self, id: TerminalAttachmentId) -> bool {
        self.facts.contains_key(&id)
    }

    pub(super) fn exact(
        &mut self,
        actor: ActorId,
        session: SessionId,
        binding: TerminalBinding,
    ) -> Result<Option<&mut TerminalState>, TerminalBridgeError> {
        let Some(receipt) = self.facts.get_mut(&binding.attachment_id()) else {
            return Ok(None);
        };
        if receipt.actor != actor || receipt.session != session {
            return Err(rejected(
                TerminalBridgeErrorKind::OwnershipMismatch,
                "authenticated actor/session does not own the terminal completion receipt",
            ));
        }
        if receipt.state.binding() != binding {
            return Err(rejected(
                TerminalBridgeErrorKind::RegistrationConflict,
                "terminal binding differs from its retained completion receipt",
            ));
        }
        Ok(Some(&mut receipt.state))
    }

    pub(super) fn remember(
        &mut self,
        actor: ActorId,
        session: SessionId,
        state: TerminalState,
        maximum: usize,
    ) {
        let id = state.binding().attachment_id();
        if !self.facts.contains_key(&id) {
            self.order.push_back(id);
        }
        self.facts.insert(id, Receipt { actor, session, state });
        while self.facts.len() > maximum {
            let Some(oldest) = self.order.pop_front() else { break };
            self.facts.remove(&oldest);
        }
    }
}
