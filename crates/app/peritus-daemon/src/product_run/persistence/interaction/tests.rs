use super::*;

fn options() -> InteractionOptions {
    InteractionOptions::new(ProductInteractionMode::Chat, ProductRoleModels::default())
}

#[test]
fn missing_efforts_preserves_legacy_default_and_unknown_values_reject() {
    let legacy = serde_json::to_value(PersistedInteraction::capture(&options())).expect("capture");
    assert!(legacy.get("efforts").is_none(), "default records keep the legacy stored shape");
    let restored: PersistedInteraction = serde_json::from_value(legacy.clone()).expect("legacy");
    assert!(!restored.restore().expect("restore").models.has_effort());
    let mut corrupt = legacy;
    corrupt["efforts"] = serde_json::Value::from(vec![99, 0, 0]);
    let restored: PersistedInteraction = serde_json::from_value(corrupt).expect("typed");
    assert!(restored.restore().is_err(), "unknown effort must not become default");
}

#[test]
fn mixed_role_efforts_roundtrip_without_changing_model_provenance_or_activities() {
    let mut value = options();
    value.models = ProductRoleModels::new(
        ProductModelChoice::default().with_effort(ProductModelEffort::Low),
        ProductModelChoice::new("review-model".to_owned(), true)
            .expect("model")
            .with_effort(ProductModelEffort::XHigh),
        ProductModelChoice::default().with_effort(ProductModelEffort::Max),
    );
    let bytes = serde_json::to_vec(&PersistedInteraction::capture(&value)).expect("capture");
    let restored: PersistedInteraction = serde_json::from_slice(&bytes).expect("decode");
    let restored = restored.restore().expect("restore");
    assert_eq!(restored.models, value.models);
    assert_eq!(restored.activities, value.activities);
    assert_eq!(restored.next_sequence, value.next_sequence);
}
