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

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_app_protocol::{
        CorrelationId, RequestId, TerminalDetach, TerminalTransitionDisposition,
    };
    use peritus_types::ProcessId;

    #[test]
    fn old_receipts_expire_without_losing_recent_exact_facts() {
        let actor = ActorId::new([1; 16]).expect("actor");
        let session = SessionId::new([1; 16]).expect("session");
        let process = ProcessId::new([1; 16]).expect("process");
        let mut receipts = TerminalReceipts::default();
        let mut facts = Vec::new();
        for id in 1..=12 {
            let binding = TerminalBinding::new(
                TerminalAttachmentId::new([id; 16]).expect("attachment"),
                process,
                RequestId::new([id; 16]).expect("request"),
            );
            let fact =
                TerminalDetach::new(binding, CorrelationId::new([id; 16]).expect("correlation"));
            let mut state = TerminalState::new(binding, 8192).expect("state");
            state.detach(fact).expect("detach");
            receipts.remember(actor, session, state, 8);
            facts.push(fact);
        }
        assert_eq!(receipts.facts.len(), 8);
        assert_eq!(receipts.order.len(), 8);
        for (index, fact) in facts.into_iter().enumerate() {
            let retained = receipts.exact(actor, session, fact.binding()).expect("ownership");
            if index < 4 {
                assert!(retained.is_none());
            } else {
                assert_eq!(
                    retained.expect("recent receipt").detach(fact).expect("repeat"),
                    TerminalTransitionDisposition::Repeated
                );
            }
        }
    }
}
