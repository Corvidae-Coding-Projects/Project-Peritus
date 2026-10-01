use super::*;

fn options() -> InteractionOptions {
    InteractionOptions::test(ProductInteractionMode::Chat, ProductRoleModels::default())
}

#[test]
fn canonical_efforts_are_required_and_unknown_values_reject() {
    let canonical =
        serde_json::to_value(PersistedInteraction::capture(&options())).expect("capture");
    assert!(canonical.get("efforts").is_some());
    let mut missing = canonical.clone();
    missing.as_object_mut().expect("object").remove("efforts");
    assert!(serde_json::from_value::<PersistedInteraction>(missing).is_err());
    let mut corrupt = canonical;
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
