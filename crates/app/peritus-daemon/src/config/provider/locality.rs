//! Literal-loopback route admission; DNS and subprocesses are not offline guarantees.
use super::{Endpoint, ProviderRoute, ProviderRouteKind};

impl ProviderRoute {
    pub(in crate::config) fn is_local_task_route(&self) -> bool {
        matches!(
            self.kind,
            ProviderRouteKind::CompatibleResponses | ProviderRouteKind::CompatibleChatCompletions
        ) && self
            .endpoint
            .as_ref()
            .and_then(|endpoint| Endpoint::new(endpoint.clone()).ok())
            .is_some_and(|endpoint| endpoint.is_literal_loopback())
    }
}
