//! Exact Anthropic profile and beta-configuration tests.

use peritus_model_protocol::{
    Capability, CapabilityMatrix, ProviderName, ProviderProfile, WireDialect,
};
use peritus_provider_core::ProviderCoreErrorKind;

use super::validate_anthropic_profile;
use crate::AnthropicBeta;
use crate::test_support::{config, profile};

fn altered(provider: &str, dialect: WireDialect, extra: &[Capability]) -> ProviderProfile {
    let base = profile();
    let mut supported = base
        .capabilities()
        .iter()
        .filter_map(|(capability, state)| {
            matches!(state, peritus_model_protocol::CapabilityState::Supported)
                .then_some(capability)
        })
        .collect::<Vec<_>>();
    supported.extend_from_slice(extra);
    ProviderProfile::new(
        base.profile_id(),
        base.revision(),
        ProviderName::new(provider.to_owned()).expect("provider"),
        base.model().clone(),
        dialect,
        CapabilityMatrix::new(&supported, &[]).expect("capabilities"),
        base.provenance(),
        base.limits(),
        base.output_limit_enforcement(),
        base.state_mode(),
        base.resume_kind(),
        base.cancellation_kind(),
    )
    .expect("altered profile")
}

#[test]
fn only_the_exact_messages_dialect_and_lifecycle_are_accepted() {
    validate_anthropic_profile(&profile()).expect("exact Anthropic profile");
    for invalid in [
        altered("anthropic-compatible", WireDialect::AnthropicMessages, &[]),
        altered("anthropic", WireDialect::CompatibleChatCompletions, &[]),
        altered("anthropic", WireDialect::AnthropicMessages, &[Capability::AudioInput]),
    ] {
        let error = validate_anthropic_profile(&invalid).expect_err("profile drift");
        assert_eq!(error.kind(), ProviderCoreErrorKind::Configuration);
    }
}

#[test]
fn beta_header_configuration_is_unique_canonical_and_redacted() {
    let config = config(
        1,
        vec![AnthropicBeta::StructuredOutputs20251113, AnthropicBeta::PromptCaching20240731],
    );
    assert_eq!(
        config.betas(),
        &[AnthropicBeta::PromptCaching20240731, AnthropicBeta::StructuredOutputs20251113,]
    );
    let duplicate = crate::AnthropicConfig::new(
        config.endpoint().clone(),
        peritus_provider_core::CredentialReference::new("duplicate-secret".to_owned())
            .expect("credential reference"),
        profile(),
        vec![AnthropicBeta::PromptCaching20240731, AnthropicBeta::PromptCaching20240731],
        peritus_provider_core::HttpLimits::PRODUCTION,
        peritus_provider_core::FramingLimits::PRODUCTION,
        config.retry_policy(),
    )
    .expect_err("duplicate beta");
    assert_eq!(duplicate.kind(), ProviderCoreErrorKind::Configuration);
    assert!(!format!("{config:?}").contains("duplicate-secret"));
}
