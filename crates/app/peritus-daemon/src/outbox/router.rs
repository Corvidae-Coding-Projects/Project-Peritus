//! Exact destination-to-handler routing for claimed outbox rows.

use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

use peritus_journal::OutboxMessage;

use super::{CLAIM_DESTINATIONS, DurableOutboxPort, TypedOutboxClaim, decode_claim, handlers};
use crate::{AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery};

/// One exact typed outbox destination adapter.
pub(crate) trait OutboxHandler: Send + Sync + 'static {
    /// Performs one idempotent delivery attempt.
    ///
    /// `true` means the generic pump must acknowledge the exact claim after return. `false` means
    /// the domain handler already settled it through its durable, idempotent owning API.
    fn deliver<'a>(
        &'a self,
        message: &'a OutboxMessage,
        claim: &'a TypedOutboxClaim,
    ) -> Pin<Box<dyn Future<Output = Result<bool, DaemonError>> + Send + 'a>>;
}

/// Immutable destination inventory shared by the sole outbox pump.
#[derive(Default)]
pub(crate) struct DestinationRouter {
    handlers: BTreeMap<String, Arc<dyn OutboxHandler>>,
}

impl DestinationRouter {
    pub(crate) fn empty(maximum: usize) -> Result<Self, DaemonError> {
        Self::new(std::iter::empty(), maximum)
    }

    pub(crate) fn production_children(
        authority: &AuthorityHandle,
        maximum: usize,
    ) -> Result<Self, DaemonError> {
        Self::new(handlers::child_directive_handlers(authority), maximum)
    }

    /// Builds the complete closed production destination registry.
    pub(crate) fn production(
        authority: &AuthorityHandle,
        durable_port: Arc<dyn DurableOutboxPort>,
        maximum: usize,
    ) -> Result<Self, DaemonError> {
        let mut handlers = handlers::child_directive_handlers(authority);
        handlers.extend(handlers::durable_domain_handlers(durable_port));
        Self::new(handlers, maximum)
    }

    pub(crate) fn new(
        handlers: impl IntoIterator<Item = (String, Arc<dyn OutboxHandler>)>,
        maximum: usize,
    ) -> Result<Self, DaemonError> {
        let mut entries = BTreeMap::new();
        for (destination, handler) in handlers {
            if !CLAIM_DESTINATIONS.contains(&destination.as_str()) {
                return Err(DaemonError::new(
                    DaemonErrorCode::InvalidInput,
                    DaemonRecovery::CorrectRequest,
                    "construct outbox destination registry",
                    "outbox handler destination is outside the closed production inventory",
                ));
            }
            if entries.len() >= maximum || entries.insert(destination, handler).is_some() {
                return Err(DaemonError::new(
                    DaemonErrorCode::InvalidInput,
                    DaemonRecovery::CorrectRequest,
                    "construct outbox destination registry",
                    "outbox destination inventory is duplicate or exceeds its configured bound",
                ));
            }
        }
        Ok(Self { handlers: entries })
    }

    pub(crate) async fn deliver(&self, message: &OutboxMessage) -> Result<bool, DaemonError> {
        let claim = decode_claim(message)?;
        let handler = self.handlers.get(message.destination()).ok_or_else(|| {
            DaemonError::new(
                DaemonErrorCode::Unsupported,
                DaemonRecovery::Operator,
                "route outbox delivery",
                "outbox destination has no configured production handler",
            )
        })?;
        handler.deliver(message, &claim).await
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, destination: &str) -> bool {
        self.handlers.contains_key(destination)
    }
}
