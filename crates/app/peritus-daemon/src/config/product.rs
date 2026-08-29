//! Product-run behavior selected explicitly by the local user.

use serde::Deserialize;

/// Product-run provider recovery policy.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProductRunPolicy {
    #[serde(default)]
    automatic_provider_failover: bool,
}

impl ProductRunPolicy {
    /// Returns whether a role may try another configured provider after ordinary recovery ends.
    #[must_use]
    pub const fn automatic_provider_failover(self) -> bool {
        self.automatic_provider_failover
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
    }
}
