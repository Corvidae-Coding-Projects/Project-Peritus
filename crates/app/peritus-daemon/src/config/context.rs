//! Local working memory and explicit offline provider-route admission.

use super::{ProductRunPolicy, ProviderRoute, invalid};
use crate::DaemonError;
use peritus_product_runner::LocalContextConfig;
use serde::Deserialize;

/// Context settings are local-only; fully offline admission additionally constrains task models.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct ContextPolicy {
    fully_offline: bool,
    local: LocalContextConfig,
}

impl ContextPolicy {
    /// Requires literal-loopback task endpoints and disables automatic provider failover.
    /// The operator must also configure the local model server itself for offline inference.
    #[must_use]
    pub const fn fully_offline(&self) -> bool {
        self.fully_offline
    }

    /// Borrows the default-enabled local-memory configuration (`[context.local]`).
    #[must_use]
    pub const fn local(&self) -> &LocalContextConfig {
        &self.local
    }

    pub(super) fn validate(
        &self,
        providers: &[ProviderRoute],
        product: ProductRunPolicy,
    ) -> Result<(), DaemonError> {
        self.local.validate().map_err(|_| invalid("invalid local context configuration"))?;
        if self.fully_offline
            && (product.automatic_provider_failover()
                || providers.iter().any(|provider| !provider.is_local_task_route()))
        {
            return Err(invalid(
                "fully offline context requires literal-loopback compatible provider routes and disabled automatic provider failover",
            ));
        }
        Ok(())
    }
}
