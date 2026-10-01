//! Product-run behavior selected explicitly by the local user.

use serde::Deserialize;

/// Product-run provider recovery policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ProductRunPolicy {
    automatic_provider_failover: bool,
    provider_turn_timeout_seconds: u64,
}

impl ProductRunPolicy {
    /// Returns whether a role may try another configured provider after ordinary recovery ends.
    #[must_use]
    pub const fn automatic_provider_failover(self) -> bool {
        self.automatic_provider_failover
    }

    /// Returns the wall-clock deadline for one complete provider turn.
    #[must_use]
    pub const fn provider_turn_timeout_seconds(self) -> u64 {
        self.provider_turn_timeout_seconds
    }

    pub(super) fn validate(self) -> Result<(), crate::DaemonError> {
        if !(1..=600).contains(&self.provider_turn_timeout_seconds) {
            return Err(super::invalid(
                "product provider_turn_timeout_seconds must be between 1 and 600",
            ));
        }
        Ok(())
    }
}

impl Default for ProductRunPolicy {
    fn default() -> Self {
        Self {
            automatic_provider_failover: false,
            provider_turn_timeout_seconds: default_provider_turn_timeout_seconds(),
        }
    }
}

const fn default_provider_turn_timeout_seconds() -> u64 {
    600
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
        assert_eq!(enabled.provider_turn_timeout_seconds(), 600);
        let bounded: ProductRunPolicy = toml::from_str(
            "automatic_provider_failover = false\nprovider_turn_timeout_seconds = 45",
        )
        .expect("bounded provider deadline");
        assert_eq!(bounded.provider_turn_timeout_seconds(), 45);
        assert!(bounded.validate().is_ok());
        let invalid: ProductRunPolicy =
            toml::from_str("provider_turn_timeout_seconds = 0").expect("parse policy");
        assert!(invalid.validate().is_err());
    }
}
