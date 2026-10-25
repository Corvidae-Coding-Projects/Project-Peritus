//! Product-run behavior selected explicitly by the local user.

use serde::Deserialize;

/// Product-run provider recovery policy.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ProductRunPolicy {
    automatic_provider_failover: bool,
    #[serde(default, rename = "provider_turn_timeout_seconds")]
    legacy_provider_turn_timeout_seconds: Option<u64>,
}

impl ProductRunPolicy {
    pub(super) const fn runtime_policy(mut self) -> Self {
        self.legacy_provider_turn_timeout_seconds = None;
        self
    }

    /// Returns whether a role may try another configured provider after ordinary recovery ends.
    #[must_use]
    pub const fn automatic_provider_failover(self) -> bool {
        self.automatic_provider_failover
    }

    pub(super) const fn validate(self) -> Result<(), crate::DaemonError> {
        Ok(())
    }
}
