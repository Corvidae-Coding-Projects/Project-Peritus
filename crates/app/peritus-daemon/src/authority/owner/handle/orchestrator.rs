//! E0 claimed-directive settlement through the sole writable authority owner.

use tokio::sync::oneshot;

use super::{AuthorityHandle, AuthorityMessage};
use crate::{DaemonError, outbox::OrchestratorDirectiveClaim};

impl AuthorityHandle {
    /// Durably admits one supported exact D1-D3 child directive before settling E0 and C0.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle, child-head, replay, stale-fence, or durable commit failure.
    pub(crate) async fn deliver_orchestrator_child(
        &self,
        claim: OrchestratorDirectiveClaim,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::DeliverOrchestratorChild { claim, respond }, receive).await
    }

    /// Atomically acknowledges one exact E0 directive and its claimed C0 outbox row.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle, claim-binding, replay, stale-fence, or durable commit failure.
    pub(crate) async fn settle_orchestrator_directive(
        &self,
        claim: OrchestratorDirectiveClaim,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::SettleOrchestratorDirective { claim, respond }, receive).await
    }
}
