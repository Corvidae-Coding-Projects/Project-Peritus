//! Product-run behavior selected explicitly by the local user.

use serde::Deserialize;

/// Product-run provider recovery policy.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ProductRunPolicy {
    automatic_provider_failover: bool,
    #[serde(default, rename = "provider_turn_timeout_seconds")]
    _legacy_provider_turn_timeout_seconds: Option<u64>,
}

impl ProductRunPolicy {
    /// Returns whether a role may try another configured provider after ordinary recovery ends.
    #[must_use]
    pub const fn automatic_provider_failover(self) -> bool {
        self.automatic_provider_failover
    }

    pub(super) const fn validate(self) -> Result<(), crate::DaemonError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failover_defaults_off_and_requires_an_explicit_true_value() {
        assert!(!ProductRunPolicy::default().automatic_provider_failover());
        let enabled: ProductRunPolicy =
            toml::from_str("automatic_provider_failover = true").expect("explicit policy");
        assert!(enabled.automatic_provider_failover());
        let legacy: ProductRunPolicy = toml::from_str(
            "automatic_provider_failover = false\nprovider_turn_timeout_seconds = 45",
        )
        .expect("legacy provider deadline remains readable");
        assert!(!legacy.automatic_provider_failover());
        assert!(legacy.validate().is_ok());
        let formerly_invalid: ProductRunPolicy =
            toml::from_str("provider_turn_timeout_seconds = 0").expect("parse policy");
        assert!(formerly_invalid.validate().is_ok());
    }
}
