use super::*;
use peritus_product_state::ProviderKind;
use std::{collections::BTreeMap, fs};

#[test]
fn direct_reasoning_profiles_enable_supported_display_summaries() {
    use peritus_product_state::{CompatibleProtocol, DirectProviderProfile};
    for kind in [
        ProviderKind::OpenAiApi,
        ProviderKind::AnthropicApi,
        ProviderKind::GoogleGeminiApi,
        ProviderKind::CompatibleEndpoint,
    ] {
        let direct = DirectProviderProfile::new(
            kind,
            format!("peritus-secret-v1:{}:{}", "01".repeat(16), "02".repeat(32)),
            (kind != ProviderKind::OpenAiApi).then(|| "https://example.invalid/v1".to_owned()),
            "fixture-model".to_owned(),
            (kind == ProviderKind::CompatibleEndpoint).then_some(CompatibleProtocol::Responses),
            None,
        )
        .unwrap();
        let rendered = configuration::render_direct_provider(kind, Some(&direct)).unwrap();
        let profile: toml::Value = toml::from_str(&rendered).unwrap();
        let capabilities = profile["providers"][0]["profile"]["capabilities"].as_array().unwrap();
        assert_eq!(
            capabilities.iter().any(|value| value.as_str() == Some("reasoning-summaries")),
            kind != ProviderKind::CompatibleEndpoint
        );
    }
}

#[test]
fn missing_account_model_is_rejected_before_durable_state_changes() {
    let temporary = tempfile::tempdir().expect("root");
    let layout = AppLayout::for_test(temporary.path()).prepare().expect("layout");
    let first = ProductBootstrap::new(layout.clone()).prepare().expect("bootstrap");
    let selection =
        ProviderSelection::new(vec![ProviderKind::CodexAccount], Some(ProviderKind::CodexAccount))
            .expect("legacy-shaped selection");
    assert!(ProductBootstrap::new(layout.clone()).configure_providers(selection).is_err());
    let second = ProductBootstrap::new(layout).prepare().expect("state remains usable");
    assert_eq!(second.state().generation(), first.state().generation());
    assert_eq!(second.daemon_config_path(), first.daemon_config_path());
}

#[test]
fn legacy_account_migration_preserves_the_exact_prior_model_and_configuration() {
    let temporary = tempfile::tempdir().expect("root");
    let layout = AppLayout::for_test(temporary.path()).prepare().expect("layout");
    let legacy =
        ProviderSelection::new(vec![ProviderKind::CodexAccount], Some(ProviderKind::CodexAccount))
            .expect("legacy selection");
    let selected = legacy
        .clone()
        .with_account_models(BTreeMap::from([(
            ProviderKind::CodexAccount,
            "previous-installation-exact-model".to_owned(),
        )]))
        .expect("explicit selection");
    let configured =
        ProductBootstrap::new(layout.clone()).configure_providers(selected).expect("configure");
    let prior_text = fs::read(configured.daemon_config_path()).expect("config");
    let mut state = configured.state().clone();
    assert!(state.configure_providers(legacy));
    let prior_path = layout.daemon_config(state.generation());
    fs::write(&prior_path, &prior_text).expect("pre-upgrade immutable configuration fixture");
    let store = ProductStateStore::open(layout.product_state_root()).expect("store");
    store.commit(&state).expect("legacy state fixture");
    let migrated = ProductBootstrap::new(layout).prepare().expect("migration");
    assert_eq!(
        migrated.state().providers().account_model(ProviderKind::CodexAccount),
        Some("previous-installation-exact-model")
    );
    assert_ne!(migrated.daemon_config_path(), prior_path);
    assert_eq!(fs::read(prior_path).expect("old generation retained"), prior_text);
}
