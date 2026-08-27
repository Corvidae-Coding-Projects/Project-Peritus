//! Production adapters for E0-owned child lifecycle destinations.

use std::{future::Future, pin::Pin, sync::Arc};

use peritus_journal::OutboxMessage;
use peritus_orchestrator::DirectiveDestination;

use super::{OrchestratorDirectiveClaim, TypedOutboxClaim, router::OutboxHandler};
use crate::{AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn child_directive_handlers(
    authority: &AuthorityHandle,
) -> Vec<(String, Arc<dyn OutboxHandler>)> {
    [DirectiveDestination::Gates, DirectiveDestination::Review]
        .into_iter()
        .map(|destination| {
            let handler: Arc<dyn OutboxHandler> =
                Arc::new(ChildDirectiveHandler { authority: authority.clone(), destination });
            (destination.outbox_destination().to_owned(), handler)
        })
        .collect()
}

struct ChildDirectiveHandler {
    authority: AuthorityHandle,
    destination: DirectiveDestination,
}

impl OutboxHandler for ChildDirectiveHandler {
    fn deliver<'a>(
        &'a self,
        _message: &'a OutboxMessage,
        claim: &'a TypedOutboxClaim,
    ) -> Pin<Box<dyn Future<Output = Result<bool, DaemonError>> + Send + 'a>> {
        Box::pin(async move {
            let claim = self.exact_claim(claim)?.clone();
            self.authority.deliver_orchestrator_child(claim).await?;
            Ok(false)
        })
    }
}

impl ChildDirectiveHandler {
    fn exact_claim<'a>(
        &self,
        claim: &'a TypedOutboxClaim,
    ) -> Result<&'a OrchestratorDirectiveClaim, DaemonError> {
        match (self.destination, claim) {
            (DirectiveDestination::Gates, TypedOutboxClaim::OrchestratorGates(claim))
            | (DirectiveDestination::Review, TypedOutboxClaim::OrchestratorReview(claim)) => {
                Ok(claim)
            }
            _ => Err(DaemonError::new(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::ReadOnly,
                "deliver E0 child directive",
                "outbox router supplied a typed claim for another destination",
            )),
        }
    }
}
