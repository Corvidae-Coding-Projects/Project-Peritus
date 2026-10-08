use super::*;
use serde_json::json;

fn owner(workspace: u8) -> Value {
    json!({
        "generation": 1,
        "store": "01".repeat(16),
        "config": "/unavailable/native-target/config.toml",
        "config_sha256": "a".repeat(64),
        "product_state": "/unavailable/native-target/product-state.json",
        "product_state_sha256": "b".repeat(64),
        "endpoint": "/unavailable/peritus.sock",
        "workspace": format!("{workspace:02x}").repeat(16),
        "native_session": "05".repeat(16)
    })
}

#[test]
fn cached_owner_decode_checks_shape_without_reading_snapshot_files() {
    let decoded = NativeOwner::decode(&owner(4)).expect("shape-only cached owner");
    assert_eq!(decoded.workspace(), "04".repeat(16));
    assert!(decoded.validate().is_err());
}

#[test]
fn prepared_message_rejects_an_owner_for_another_workspace() {
    let value = json!({
        "version": 2,
        "conversation": "02".repeat(16),
        "workspace": "03".repeat(16),
        "run": "04".repeat(16),
        "title": "Retained message",
        "providers": {
            "writer": "06".repeat(16),
            "reviewer": "07".repeat(16),
            "fixer": "08".repeat(16)
        },
        "mode": "chat",
        "models": {},
        "text": "retained input",
        "owner": owner(9)
    });
    let error = crate::daemon::PreparedChat::from_retained(&value)
        .err()
        .expect("workspace mismatch");
    assert!(error.0.contains("workspace does not match"));
}
