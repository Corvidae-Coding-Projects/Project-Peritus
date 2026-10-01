//! End attachment delivery while retaining bounded exact detach/cancel retry receipts.

use super::{RegistryState, TerminalBridgeError, TerminalRegistry, exact_bridge_mut};
use peritus_app_protocol::{
    TerminalCancellation, TerminalDetach, TerminalState, TerminalTransitionDisposition,
};
use peritus_types::{ActorId, SessionId};

impl TerminalRegistry {
    /// Detaches without terminating the process. Exact retries use bounded completion receipts.
    ///
    /// # Errors
    /// Rejects ownership/binding mismatch, conflicting facts, or an expired receipt.
    pub(crate) fn detach(
        &self,
        actor: ActorId,
        session: SessionId,
        detach: TerminalDetach,
    ) -> Result<TerminalTransitionDisposition, TerminalBridgeError> {
        let mut state = self.lock();
        if let Some(receipt) = state.receipts.exact(actor, session, detach.binding())? {
            return receipt.detach(detach).map_err(Into::into);
        }
        let bridge = exact_bridge_mut(&mut state, actor, session, detach.binding())?;
        let (disposition, receipt) = bridge.detach(detach)?;
        bridge.remove_attachment(detach.binding().attachment_id());
        retain_receipt(&mut state, actor, session, receipt);
        Ok(disposition)
    }

    /// Cancels once through C2 and releases attachment delivery without losing the retry fact.
    ///
    /// # Errors
    /// Rejects ownership/binding mismatch, conflicting facts, expired receipts, or C2 refusal.
    pub(crate) fn cancel(
        &self,
        actor: ActorId,
        session: SessionId,
        cancellation: TerminalCancellation,
    ) -> Result<TerminalTransitionDisposition, TerminalBridgeError> {
        let mut state = self.lock();
        if let Some(receipt) = state.receipts.exact(actor, session, cancellation.binding())? {
            return receipt.cancel(cancellation).map_err(Into::into);
        }
        let bridge = exact_bridge_mut(&mut state, actor, session, cancellation.binding())?;
        let (disposition, receipt) = bridge.cancel(cancellation)?;
        bridge.remove_attachment(cancellation.binding().attachment_id());
        retain_receipt(&mut state, actor, session, receipt);
        Ok(disposition)
    }
}

fn retain_receipt(
    state: &mut RegistryState,
    actor: ActorId,
    session: SessionId,
    receipt: TerminalState,
) {
    state.attachments.remove(&receipt.binding().attachment_id());
    state.receipts.remember(actor, session, receipt, state.limits.maximum_attachments);
}
