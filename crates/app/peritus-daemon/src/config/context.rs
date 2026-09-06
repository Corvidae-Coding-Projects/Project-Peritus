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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn route(kind: &str, endpoint: &str) -> ProviderRoute {
        serde_json::from_value(Value::from_iter([
            ("kind", Value::from(kind)),
            ("endpoint", Value::from(endpoint)),
            (
                "profile",
                Value::from_iter([
                    ("profile_id", Value::from("01010101010101010101010101010101")),
                    ("revision", Value::from(1)),
                    ("model", Value::from("fixture")),
                    ("capabilities", Value::Array(vec![Value::from("tool-calls")])),
                    ("max_input_tokens", Value::from(32768)),
                    ("max_output_tokens", Value::from(1024)),
                    ("max_tools", Value::from(32)),
                    ("max_parallel_tool_calls", Value::from(1)),
                    ("max_inline_media_bytes", Value::from(1024)),
                ]),
            ),
        ]))
        .unwrap()
    }

    #[test]
    fn local_is_default_and_legacy_is_explicit_with_strict_bounds() {
        let default: ContextPolicy = toml::from_str("").unwrap();
        assert!(default.local().enabled);
        assert!(!default.fully_offline());
        let legacy: ContextPolicy = toml::from_str("[local]\nenabled = false").unwrap();
        assert!(!legacy.local().enabled);
        assert!(legacy.validate(&[], ProductRunPolicy::default()).is_ok());
        assert!(
            toml::from_str::<ContextPolicy>("[local]\nremote_url = 'https://invalid.test'")
                .is_err()
        );
        assert!(toml::from_str::<ContextPolicy>("[local]\nsemantic_backend = 'remote'").is_err());
        let invalid: ContextPolicy = toml::from_str("[local]\nmax_update_operations = 0").unwrap();
        assert!(invalid.validate(&[], ProductRunPolicy::default()).is_err());
    }

    #[test]
    fn offline_admission_checks_every_route_and_disallows_automatic_failover() {
        let offline: ContextPolicy = toml::from_str("fully_offline = true").unwrap();
        let local = route("compatible-responses", "http://127.0.0.1:8000/v1");
        assert!(
            offline.validate(std::slice::from_ref(&local), ProductRunPolicy::default()).is_ok()
        );
        for (kind, endpoint) in [
            ("compatible-responses", "https://remote.invalid/v1"),
            ("compatible-responses", "http://localhost:8000/v1"),
            ("open-ai", "http://127.0.0.1:8000/v1"),
            ("codex-runtime", "http://127.0.0.1:8000/v1"),
        ] {
            assert!(
                offline
                    .validate(&[local.clone(), route(kind, endpoint)], ProductRunPolicy::default())
                    .is_err()
            );
        }
        let failover: ProductRunPolicy =
            toml::from_str("automatic_provider_failover = true").unwrap();
        assert!(offline.validate(&[local], failover).is_err());
    }
}
